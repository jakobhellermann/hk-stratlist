//! Exports a game's world map from the game files into the game's data directory:
//!
//! - `game/map-entries.json`: one entry per map sprite, with its world box, color, physics outline
//!   and sprite id
//! - `game/scenes.json`: names of all loadable scenes
//! - `map/<sprite id>-<rrggbb>.webp` in the public dir: each sprite tinted with its room color, over
//!   its full rect
//!
//! Each game has its own binary, which finds the map rooms and loadable scenes in its game files.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use games::Game;
use image::codecs::webp::WebPEncoder;
use image::{ImageEncoder, RgbaImage};
use rabex::objects::pptr::PathId;
use rabex::objects::{ClassId, ClassIdType, TypedPPtr};
use rabex::tpk::TpkTypeTreeBlob;
use rabex::typetree::typetree_cache::sync::TypeTreeCache;
use rabex_env::Environment;
use rabex_env::handle::SerializedFileHandle;
use rabex_env::resolver::GameFiles;
use rabex_env::unity::types::{GameObject, Transform};
use rabex_texture::Texture2D;
use rabex_texture::sprite::{Sprite, SpriteAtlas, render_sprite};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

const STEAM_COMMON_DIR: &str = ".local/share/Steam/steamapps/common";

/// Loads the game's config and its game files, from the directory given as the first argument or
/// else its Steam install.
pub fn open(slug: &str) -> Result<(Game, Environment)> {
    let game = Game::load(slug)?;
    let game_dir = match std::env::args().nth(1) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var("HOME").context("HOME not set")?)
            .join(STEAM_COMMON_DIR)
            .join(&game.steam_dir),
    };
    let env = Environment::new(
        GameFiles::probe(&game_dir)
            .with_context(|| format!("game files in {}", game_dir.display()))?,
        TypeTreeCache::new(TpkTypeTreeBlob::embedded()),
    );
    Ok((game, env))
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct SpriteRenderer {
    m_GameObject: TypedPPtr<GameObject>,
    m_Color: Color,
    m_Sprite: TypedPPtr<Sprite>,
}
impl ClassIdType for SpriteRenderer {
    const CLASS_ID: ClassId = ClassId::SpriteRenderer;
}

#[derive(Deserialize, Clone, Copy)]
struct Color {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

/// The sprite fields only the map needs; rendering uses [`rabex_texture::sprite::Sprite`].
#[derive(Deserialize)]
#[allow(non_snake_case)]
struct SpriteShape {
    m_PhysicsShape: Vec<Vec<(f32, f32)>>,
}

#[derive(Serialize)]
struct Entry {
    name: String,
    area: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: [f64; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    /// File name of the tinted sprite in `public/map/`.
    sprite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scale: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shape: Option<Vec<Vec<[f64; 2]>>>,
}

fn round(v: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (v * f).round() / f
}

/// `<sprite name>-<rrggbb>.webp`; a few sprites are drawn in several colors.
fn sprite_file(name: &str, color: Color) -> String {
    let [r, g, b] = [color.r, color.g, color.b].map(|c| (c * 255.0).round() as u8);
    format!("{name}-{r:02x}{g:02x}{b:02x}.webp")
}

/// Collects map entries and their tinted sprites from the rooms of one serialized file.
pub struct MapBuilder<'a> {
    file: SerializedFileHandle<'a>,
    transforms: HashMap<PathId, Transform>,
    transform_of: HashMap<PathId, PathId>,
    renderers: HashMap<PathId, SpriteRenderer>,
    textures: TextureCache,
    entries: Vec<Entry>,
    /// Tinted sprite images, by file name.
    sprites: BTreeMap<String, RgbaImage>,
    sprite_owners: HashMap<String, PathId>,
    skipped: Vec<String>,
}

impl<'a> MapBuilder<'a> {
    pub fn new(file: SerializedFileHandle<'a>) -> Result<Self> {
        let mut transforms = HashMap::new();
        let mut transform_of = HashMap::new();
        for handle in file.transforms() {
            let transform = handle.read()?;
            transform_of.insert(transform.m_GameObject.m_PathID, handle.path_id());
            transforms.insert(handle.path_id(), transform);
        }
        let mut renderers = HashMap::new();
        for handle in file.objects_of::<SpriteRenderer>() {
            let renderer = handle.read()?;
            renderers.insert(renderer.m_GameObject.m_PathID, renderer);
        }
        Ok(Self {
            file,
            transforms,
            transform_of,
            renderers,
            textures: TextureCache::default(),
            entries: Vec::new(),
            sprites: BTreeMap::new(),
            sprite_owners: HashMap::new(),
            skipped: Vec::new(),
        })
    }

    pub fn file(&self) -> &SerializedFileHandle<'a> {
        &self.file
    }

    /// Adds the map entry of a room. Its drawing is `full_sprite` (shown once the room is explored)
    /// if there is one, else the rough outline of its SpriteRenderer.
    pub fn add_room(
        &mut self,
        game_object: PathId,
        full_sprite: Option<TypedPPtr<Sprite>>,
    ) -> Result<()> {
        // `<map>/<area>/<room>`
        let path = self.file.object_at::<GameObject>(game_object)?.path()?;
        let parts: Vec<&str> = path.split('/').collect();
        let name = parts
            .last()
            .expect("split yields at least one item")
            .to_string();
        let area = parts.get(1).copied().unwrap_or_default().to_string();

        let transform = *self
            .transform_of
            .get(&game_object)
            .with_context(|| format!("{path} has no Transform"))?;
        let ((wx, wy), (sx, sy)) = world_transform(&self.transforms, transform)?;

        let Some(renderer) = self.renderers.get(&game_object) else {
            self.skipped.push(format!("{name}: no SpriteRenderer"));
            return Ok(());
        };
        // Tinting and the file name use the stored (rounded) color.
        let c = renderer.m_Color;
        let color = [c.r, c.g, c.b, c.a].map(|v| round(v as f64, 4));
        let c = Color {
            r: color[0] as f32,
            g: color[1] as f32,
            b: color[2] as f32,
            a: color[3] as f32,
        };

        let sprite_ptr = full_sprite
            .into_iter()
            .chain([renderer.m_Sprite])
            .find(|ptr| ptr.m_PathID != 0);
        let Some(sprite_ptr) = sprite_ptr else {
            // No map sprite at all: the room still has a map position, but no size or shape.
            self.entries.push(Entry {
                name,
                area,
                x: round(wx, 3),
                y: round(wy, 3),
                w: 0.0,
                h: 0.0,
                color,
                sprite: None,
                scale: None,
                shape: None,
            });
            return Ok(());
        };
        let sprite_handle = self
            .file
            .deref(sprite_ptr)
            .with_context(|| format!("sprite of {path}"))?;
        let sprite = sprite_handle.read()?;
        let physics_shape = sprite_handle.cast::<SpriteShape>().read()?.m_PhysicsShape;

        let ppu = sprite.m_PixelsToUnits as f64;
        let w = sprite.m_Rect.width as f64 / ppu * sx.abs();
        let h = sprite.m_Rect.height as f64 / ppu * sy.abs();
        // The SpriteRenderer puts the sprite's pivot at the transform position; physics-shape points are
        // pivot-relative in world units.
        let (pivot_x, pivot_y) = (sprite.m_Pivot.0 as f64, sprite.m_Pivot.1 as f64);
        let shape: Vec<Vec<[f64; 2]>> = physics_shape
            .iter()
            .map(|poly| {
                poly.iter()
                    .map(|&(px, py)| [round(wx + sx * px as f64, 3), round(wy + sy * py as f64, 3)])
                    .collect()
            })
            .collect();

        let file = sprite_file(&sprite.m_Name, c);
        match self.sprite_owners.insert(file.clone(), sprite_ptr.m_PathID) {
            Some(owner) if owner != sprite_ptr.m_PathID => {
                bail!(
                    "sprites {owner} and {} both map to {file}",
                    sprite_ptr.m_PathID
                )
            }
            _ => {}
        }
        if !self.sprites.contains_key(&file) {
            let atlas: Option<SpriteAtlas> = match sprite.is_atlas_packed() {
                true => Some(sprite_handle.file.deref_read(sprite.m_SpriteAtlas)?),
                false => None,
            };
            let rd = sprite
                .render_data(atlas.as_ref())
                .with_context(|| format!("sprite of {path}"))?;
            let texture = self.textures.get(&sprite_handle.file, rd.texture)?;
            let mut image =
                render_sprite(&sprite, rd, texture).with_context(|| format!("sprite of {path}"))?;
            tint(&mut image, c);
            self.sprites.insert(file.clone(), image);
        }

        self.entries.push(Entry {
            name,
            area,
            x: round(wx - pivot_x * w, 3),
            y: round(wy - pivot_y * h, 3),
            w: round(w, 3),
            h: round(h, 3),
            color,
            sprite: Some(file),
            scale: Some([round(sx, 4), round(sy, 4)]),
            shape: (!shape.is_empty()).then_some(shape),
        });
        Ok(())
    }

    /// Writes the entries, the scenes and the sprites into the game's data and public dirs.
    pub fn write(self, game: &Game, scenes: &BTreeSet<String>) -> Result<()> {
        if !self.skipped.is_empty() {
            eprintln!("skipped {} map entries:", self.skipped.len());
            for reason in &self.skipped {
                eprintln!("  {reason}");
            }
        }
        let data_dir = game.data_dir();
        let game_dir = data_dir.join("game");
        std::fs::create_dir_all(&game_dir)?;
        write_json(&game_dir.join("scenes.json"), scenes)?;
        write_json(&game_dir.join("map-entries.json"), &self.entries)?;
        write_sprites(&game.public_dir().join("map"), &self.sprites)?;
        println!(
            "{} scenes, {} map entries, {} sprites -> {}",
            scenes.len(),
            self.entries.len(),
            self.sprites.len(),
            data_dir.display()
        );
        Ok(())
    }
}

/// World position and scale of a transform, accumulated up its parent chain (no rotation on the map).
fn world_transform(
    transforms: &HashMap<PathId, Transform>,
    leaf: PathId,
) -> Result<((f64, f64), (f64, f64))> {
    let mut chain = Vec::new();
    let mut current = Some(leaf);
    while let Some(id) = current {
        let transform = transforms
            .get(&id)
            .with_context(|| format!("parent transform {id} not in map bundle"))?;
        chain.push(transform);
        current = (transform.m_Father.m_PathID != 0).then_some(transform.m_Father.m_PathID);
    }
    let (mut wx, mut wy, mut sx, mut sy) = (0.0, 0.0, 1.0, 1.0);
    for transform in chain.iter().rev() {
        wx += sx * transform.m_LocalPosition.0 as f64;
        wy += sy * transform.m_LocalPosition.1 as f64;
        sx *= transform.m_LocalScale.0 as f64;
        sy *= transform.m_LocalScale.1 as f64;
    }
    Ok(((wx, wy), (sx, sy)))
}

/// Decoded atlas textures, keyed by their stream location; the map atlases are few and large.
#[derive(Default)]
struct TextureCache {
    decoded: HashMap<String, RgbaImage>,
}

impl TextureCache {
    fn get(
        &mut self,
        file: &SerializedFileHandle<'_>,
        texture: TypedPPtr<Texture2D>,
    ) -> Result<&RgbaImage> {
        let texture: Texture2D = file.deref_read(texture)?;
        let stream = &texture.m_StreamData;
        let key = format!("{}:{}", stream.path, stream.offset);
        if !self.decoded.contains_key(&key) {
            let pixels = texture.pixels(|s| file.env.read_stream(&s.path, s.offset, s.size))?;
            let image = texture
                .decode(&pixels)
                .with_context(|| format!("decoding texture {}", texture.m_Name))?;
            self.decoded.insert(key.clone(), image);
        }
        Ok(&self.decoded[&key])
    }
}

/// Tints like the game does: texture × color.
fn tint(image: &mut RgbaImage, color: Color) {
    let tint = [color.r, color.g, color.b].map(|c| (c * 255.0).round() as u32);
    for pixel in image.pixels_mut() {
        for (channel, factor) in pixel.0.iter_mut().zip(tint) {
            *channel = (*channel as u32 * factor / 255) as u8;
        }
    }
}

fn write_sprites(dir: &Path, sprites: &BTreeMap<String, RgbaImage>) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    // Replace the directory's sprites, so ones no longer referenced don't linger.
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "webp") {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
    }
    sprites.par_iter().try_for_each(|(file, image)| {
        let path = dir.join(file);
        let out = BufWriter::new(
            std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?,
        );
        WebPEncoder::new_lossless(out)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                image::ExtendedColorType::Rgba8,
            )
            .with_context(|| format!("encoding {}", path.display()))
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    std::fs::write(path, serde_json::to_string(value)? + "\n")
        .with_context(|| format!("writing {}", path.display()))
}
