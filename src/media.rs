//! Local previews and metadata for message attachments.
//!
//! Discord CDN links expire, so every image/video gets a local WebP preview, and its display size
//! (and frame rate for videos) is recorded, letting the frontend lay out media before it loads.
//! Results are cached by attachment id, so only new attachments hit the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Longest edge of generated previews, in pixels.
const PREVIEW_MAX: u32 = 1280;
/// Downloads are latency- rather than CPU-bound, so this exceeds the core count.
const PARALLEL_DOWNLOADS: usize = 24;
/// Network read/write timeout for ffmpeg/ffprobe.
const IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Video,
    File,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Meta {
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
}

#[derive(Serialize)]
pub struct Attachment<'a> {
    pub url: &'a str,
    pub name: &'a str,
    pub kind: Kind,
    /// Preview path relative to the public dir; set for images and videos.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// An attachment as exported by DiscordChatExporter.
#[derive(Deserialize)]
pub struct Source {
    pub id: String,
    pub url: String,
    #[serde(rename = "fileName")]
    pub name: String,
}

impl Source {
    pub fn kind(&self) -> Kind {
        let ext = self
            .name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("png" | "jpg" | "jpeg" | "gif" | "webp") => Kind::Image,
            Some("mp4" | "mov" | "webm") => Kind::Video,
            _ => Kind::File,
        }
    }
}

pub struct Cache {
    meta_path: PathBuf,
    public_dir: PathBuf,
    entries: BTreeMap<String, Meta>,
}

impl Cache {
    pub fn open(meta_path: &Path, public_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let entries = match std::fs::read_to_string(meta_path) {
            Ok(s) => serde_json::from_str(&s)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e.into()),
        };
        std::fs::create_dir_all(public_dir.join("media"))?;
        Ok(Self {
            meta_path: meta_path.to_owned(),
            public_dir: public_dir.to_owned(),
            entries,
        })
    }

    fn preview_rel(id: &str) -> String {
        format!("media/{id}.webp")
    }

    fn is_cached(&self, id: &str) -> bool {
        self.entries.contains_key(id) && self.public_dir.join(Self::preview_rel(id)).exists()
    }

    /// Generates previews for all uncached attachments. Every attachment is attempted; failures are
    /// returned together after the successful ones have been saved to the cache.
    pub fn fill(&mut self, sources: Vec<&Source>) -> Result<(), Vec<String>> {
        let mut todo: Vec<&Source> = sources
            .into_iter()
            .filter(|a| a.kind() != Kind::File && !self.is_cached(&a.id))
            .collect();
        todo.sort_by_key(|a| &a.id);
        todo.dedup_by_key(|a| &a.id);
        if todo.is_empty() {
            return Ok(());
        }
        println!("generating {} previews...", todo.len());

        let done = Mutex::new(0usize);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(PARALLEL_DOWNLOADS)
            .build()
            .expect("thread pool");
        let results: Vec<(&str, Result<Meta, String>)> = pool.install(|| {
            todo.par_iter()
                .map(|a| {
                    let out = self.public_dir.join(Self::preview_rel(&a.id));
                    let result =
                        generate(a, &out).map_err(|e| format!("{} ({}): {e}", a.name, a.id));
                    let mut done = done.lock().expect("progress lock");
                    *done += 1;
                    if *done % 50 == 0 {
                        println!("  {}/{}", *done, todo.len());
                    }
                    (a.id.as_str(), result)
                })
                .collect()
        });

        let mut errors = Vec::new();
        for (id, result) in results {
            match result {
                Ok(meta) => {
                    self.entries.insert(id.to_owned(), meta);
                }
                Err(e) => errors.push(e),
            }
        }
        if let Err(e) = self.save() {
            errors.push(format!("saving {}: {e}", self.meta_path.display()));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn save(&self) -> std::io::Result<()> {
        std::fs::write(
            &self.meta_path,
            serde_json::to_string_pretty(&self.entries).expect("meta serializes"),
        )
    }

    pub fn attachment<'a>(&self, a: &'a Source) -> Attachment<'a> {
        let kind = a.kind();
        let meta = (kind != Kind::File).then(|| {
            self.entries
                .get(&a.id)
                .unwrap_or_else(|| panic!("no cached preview for {}", a.id))
                .clone()
        });
        Attachment {
            url: &a.url,
            name: &a.name,
            kind,
            preview: meta.as_ref().map(|_| Self::preview_rel(&a.id)),
            meta,
        }
    }
}

fn generate(a: &Source, out: &Path) -> Result<Meta, String> {
    let scale = format!(
        "scale='if(gt(iw,ih),min({PREVIEW_MAX},iw),-2)':'if(gt(iw,ih),-2,min({PREVIEW_MAX},ih))'"
    );
    run(Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y"])
        .args(["-rw_timeout", &IO_TIMEOUT.as_micros().to_string()])
        .args(["-i", &a.url, "-frames:v", "1", "-vf", &scale])
        .args(["-c:v", "libwebp", "-quality", "80"])
        .arg(out))?;

    // Probing the preview gives the displayed size, with rotation metadata already applied.
    let preview = probe(out.to_str().ok_or("non-utf8 path")?)?;
    let (width, height) = (
        preview.width.ok_or("preview has no width")?,
        preview.height.ok_or("preview has no height")?,
    );
    let fps = match a.kind() {
        Kind::Video => Some(parse_rate(
            &probe(&a.url)?.avg_frame_rate.ok_or("no frame rate")?,
        )?),
        _ => None,
    };
    Ok(Meta { width, height, fps })
}

#[derive(Deserialize)]
struct Stream {
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
}

fn probe(input: &str) -> Result<Stream, String> {
    #[derive(Deserialize)]
    struct Probe {
        streams: Vec<Stream>,
    }
    let out = run(Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-rw_timeout", &IO_TIMEOUT.as_micros().to_string()])
        .args([
            "-show_entries",
            "stream=width,height,avg_frame_rate",
            "-of",
            "json",
        ])
        .arg(input))?;
    let probe: Probe = serde_json::from_slice(&out).map_err(|e| format!("ffprobe json: {e}"))?;
    probe
        .streams
        .into_iter()
        .next()
        .ok_or_else(|| "no video stream".to_owned())
}

fn parse_rate(rate: &str) -> Result<f64, String> {
    let (num, den) = rate
        .split_once('/')
        .ok_or_else(|| format!("bad frame rate {rate}"))?;
    let (num, den): (f64, f64) = (
        num.parse().map_err(|_| format!("bad frame rate {rate}"))?,
        den.parse().map_err(|_| format!("bad frame rate {rate}"))?,
    );
    if den == 0.0 || num == 0.0 {
        return Err(format!("bad frame rate {rate}"));
    }
    Ok(num / den)
}

fn run(cmd: &mut Command) -> Result<Vec<u8>, String> {
    let output = cmd
        .output()
        .map_err(|e| format!("spawning {:?}: {e}", cmd.get_program()))?;
    if !output.status.success() {
        return Err(format!(
            "{:?} failed ({}): {}",
            cmd.get_program(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_extension() {
        let source = |name: &str| Source {
            id: "1".into(),
            url: String::new(),
            name: name.into(),
        };
        assert_eq!(source("SPOILER_Mip.mp4").kind(), Kind::Video);
        assert_eq!(source("clip.MOV").kind(), Kind::Video);
        assert_eq!(source("image.png").kind(), Kind::Image);
        assert_eq!(source("user1.sav").kind(), Kind::File);
    }

    #[test]
    fn parses_frame_rate() {
        assert_eq!(parse_rate("60/1"), Ok(60.0));
        assert_eq!(parse_rate("30000/1001"), Ok(30000.0 / 1001.0));
        assert!(parse_rate("0/0").is_err());
    }
}
