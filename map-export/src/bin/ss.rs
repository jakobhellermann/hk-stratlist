//! Silksong's map: every `GameMapScene` of the addressables map bundle is a room.
//!
//! Usage: ss [GAME_DIR]

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Result, ensure};
use map_export::MapBuilder;
use rabex::objects::TypedPPtr;
use rabex_env::Environment;
use rabex_env::unity::types::GameObject;
use rabex_texture::sprite::Sprite;
use serde::Deserialize;

const MAP_BUNDLE: &str = "maps_assets_all.bundle";
const SCENE_BUNDLE_DIR: &str = "scenes_scenes_scenes";

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct GameMapScene {
    m_GameObject: TypedPPtr<GameObject>,
    /// The detailed drawing shown once a room is explored; the SpriteRenderer holds the rough outline.
    fullSprite: TypedPPtr<Sprite>,
}

fn main() -> Result<()> {
    let (game, env) = map_export::open("ss")?;
    let scenes = loadable_scenes(&env)?;
    let mut map = MapBuilder::new(env.load_addressables_bundle_content(MAP_BUNDLE)?)?;
    let rooms: Vec<GameMapScene> = map
        .file()
        .scripts::<GameMapScene>("GameMapScene")?
        .map(|handle| handle.read())
        .collect::<Result<_>>()?;
    for room in rooms {
        map.add_room(room.m_GameObject.m_PathID, Some(room.fullSprite))?;
    }
    map.write(&game, &scenes)
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
