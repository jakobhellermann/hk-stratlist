//! Exports the Silksong world map from the game files into the data directory:
//!
//! - `game/map-entries.json`: one entry per map sprite (`GameMapScene`), with its world box, color,
//!   physics outline and sprite id
//! - `game/scenes.json`: names of all loadable scenes
//! - `public/map/<sprite id>-<rrggbb>.webp`: each sprite tinted with its room color, over its full rect
//!
//! Usage: map-export [DATA_DIR] [GAME_DIR]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use image::codecs::webp::WebPEncoder;
use image::{ImageEncoder, RgbaImage};
use rabex::objects::pptr::PathId;
use rabex::objects::{ClassId, ClassIdType, TypedPPtr};
use rabex::tpk::TpkTypeTreeBlob;
use rabex::typetree::typetree_cache::sync::TypeTreeCache;
use rabex_env::Environment;
use rabex_env::resolver::GameFiles;
use rabex_env::unity::types::{GameObject, Transform};
use rabex_texture::Texture2D;
use rabex_texture::sprite::{Sprite, SpriteAtlas, render_sprite};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

const MAP_BUNDLE: &str = "maps_assets_all.bundle";
const SCENE_BUNDLE_DIR: &str = "scenes_scenes_scenes";
const DEFAULT_GAME_DIR: &str = ".local/share/Steam/steamapps/common/Hollow Knight Silksong";

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct GameMapScene {
    m_GameObject: TypedPPtr<GameObject>,
    /// The detailed drawing shown once a room is explored; the SpriteRenderer holds the rough outline.
    fullSprite: TypedPPtr<Sprite>,
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
    sprite: Option<PathId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scale: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shape: Option<Vec<Vec<[f64; 2]>>>,
}

fn round(v: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (v * f).round() / f
}

fn sprite_file(sprite: PathId, color: Color) -> String {
    let [r, g, b] = [color.r, color.g, color.b].map(|c| (c * 255.0).round() as u8);
    format!("{sprite}-{r:02x}{g:02x}{b:02x}.webp")
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let data_dir = PathBuf::from(args.next().unwrap_or_else(|| "data".into()));
    let game_dir = match args.next() {
        Some(dir) => PathBuf::from(dir),
        None => {
            PathBuf::from(std::env::var("HOME").context("HOME not set")?).join(DEFAULT_GAME_DIR)
        }
    };

    let env = Environment::new(
        GameFiles::probe(&game_dir)
            .with_context(|| format!("game files in {}", game_dir.display()))?,
        TypeTreeCache::new(TpkTypeTreeBlob::embedded()),
    );

    let scenes = loadable_scenes(&env)?;
    let (entries, sprites) = map_entries(&env)?;

    let game_dir_out = data_dir.join("game");
    std::fs::create_dir_all(&game_dir_out)?;
    write_json(&game_dir_out.join("scenes.json"), &scenes)?;
    write_json(&game_dir_out.join("map-entries.json"), &entries)?;
    write_sprites(&data_dir.join("public/map"), &sprites)?;

    println!(
        "{} scenes, {} map entries, {} sprites -> {}",
        scenes.len(),
        entries.len(),
        sprites.len(),
        data_dir.display()
    );
    Ok(())
}

fn loadable_scenes(env: &Environment) -> Result<BTreeSet<String>> {
    let scenes: BTreeSet<String> = env
        .addressables_bundles()?
        .iter()
        .filter(|bundle| bundle.parent() == Some(Path::new(SCENE_BUNDLE_DIR)))
        .filter_map(|bundle| Some(bundle.file_stem()?.to_str()?.to_owned()))
        .collect();
    ensure!(!scenes.is_empty(), "no scene bundles in {SCENE_BUNDLE_DIR}");
    Ok(scenes)
}

/// Map entries and the tinted sprite images, by file name.
fn map_entries(env: &Environment) -> Result<(Vec<Entry>, BTreeMap<String, RgbaImage>)> {
    let maps = env.load_addressables_bundle_content(MAP_BUNDLE)?;

    let mut transforms: HashMap<PathId, Transform> = HashMap::new();
    let mut transform_of: HashMap<PathId, PathId> = HashMap::new();
    for handle in maps.transforms() {
        let transform = handle.read()?;
        transform_of.insert(transform.m_GameObject.m_PathID, handle.path_id());
        transforms.insert(handle.path_id(), transform);
    }
    let mut renderers: HashMap<PathId, SpriteRenderer> = HashMap::new();
    for handle in maps.objects_of::<SpriteRenderer>() {
        let renderer = handle.read()?;
        renderers.insert(renderer.m_GameObject.m_PathID, renderer);
    }

    let mut textures = TextureCache::default();
    let mut entries = Vec::new();
    let mut sprites: BTreeMap<String, RgbaImage> = BTreeMap::new();
    let mut skipped = Vec::new();

    for handle in maps.scripts::<GameMapScene>("GameMapScene")? {
        let scene = handle.read()?;
        let path = maps.deref(scene.m_GameObject)?.path()?;
        let parts: Vec<&str> = path.split('/').collect();
        let name = parts
            .last()
            .expect("split yields at least one item")
            .to_string();
        let area = parts.get(1).copied().unwrap_or_default().to_string();

        let go = scene.m_GameObject.m_PathID;
        let transform = *transform_of
            .get(&go)
            .with_context(|| format!("{path} has no Transform"))?;
        let ((wx, wy), (sx, sy)) = world_transform(&transforms, transform)?;

        let Some(renderer) = renderers.get(&go) else {
            skipped.push(format!("{name}: no SpriteRenderer"));
            continue;
        };
        // Rounded like the stored color, so the sprite file name can be derived from map-entries.json.
        let c = renderer.m_Color;
        let color = [c.r, c.g, c.b, c.a].map(|v| round(v as f64, 4));
        let c = Color {
            r: color[0] as f32,
            g: color[1] as f32,
            b: color[2] as f32,
            a: color[3] as f32,
        };

        let sprite_ptr = [scene.fullSprite, renderer.m_Sprite]
            .into_iter()
            .find(|ptr| ptr.m_PathID != 0);
        let Some(sprite_ptr) = sprite_ptr else {
            // No map sprite at all: the room still has a map position, but no size or shape.
            entries.push(Entry {
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
            continue;
        };
        let sprite_handle = maps
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

        let file = sprite_file(sprite_ptr.m_PathID, c);
        if !sprites.contains_key(&file) {
            let atlas: Option<SpriteAtlas> = match sprite.is_atlas_packed() {
                true => Some(sprite_handle.file.deref_read(sprite.m_SpriteAtlas)?),
                false => None,
            };
            let rd = sprite
                .render_data(atlas.as_ref())
                .with_context(|| format!("sprite of {path}"))?;
            let texture = textures.get(&sprite_handle.file, rd.texture)?;
            let mut image =
                render_sprite(&sprite, rd, texture).with_context(|| format!("sprite of {path}"))?;
            tint(&mut image, c);
            sprites.insert(file, image);
        }

        entries.push(Entry {
            name,
            area,
            x: round(wx - pivot_x * w, 3),
            y: round(wy - pivot_y * h, 3),
            w: round(w, 3),
            h: round(h, 3),
            color,
            sprite: Some(sprite_ptr.m_PathID),
            scale: Some([round(sx, 4), round(sy, 4)]),
            shape: (!shape.is_empty()).then_some(shape),
        });
    }

    if !skipped.is_empty() {
        eprintln!("skipped {} map entries:", skipped.len());
        for reason in &skipped {
            eprintln!("  {reason}");
        }
    }
    Ok((entries, sprites))
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
        file: &rabex_env::handle::SerializedFileHandle<'_>,
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
