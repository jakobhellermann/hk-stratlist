//! Hollow Knight's map: the `Game_Map` prefab in `resources.assets`, laid out as
//! `Game_Map/<area>/<room>`. Besides rooms, areas also hold pins and labels, so only objects named
//! after a scene count as rooms. Deeper objects named after a scene are markers (e.g. grub pins),
//! which place scenes without a drawing of their own as a point.
//!
//! Usage: hk [GAME_DIR]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;
use map_export::MapBuilder;
use rabex::objects::TypedPPtr;
use rabex::objects::pptr::PathId;
use rabex_env::unity::types::GameObject;
use rabex_texture::sprite::Sprite;
use serde::Deserialize;

const MAP_FILE: &str = "resources.assets";
const MAP_ROOT: &str = "Game_Map";
/// Children of the map root that hold markers rather than rooms.
const MARKER_GROUPS: &[&str] = &["Flame Pins"];

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct RoughMapRoom {
    m_GameObject: TypedPPtr<GameObject>,
    /// The detailed drawing shown once a room is explored; the SpriteRenderer holds the rough outline.
    fullSprite: TypedPPtr<Sprite>,
}

fn main() -> Result<()> {
    let (game, env) = map_export::open("hk")?;
    let scenes: BTreeSet<String> = env
        .build_settings()?
        .scene_names()
        .map(str::to_owned)
        .collect();
    let lowercase_scenes: BTreeSet<String> = scenes.iter().map(|s| s.to_lowercase()).collect();

    let mut map = MapBuilder::new(env.load_serialized(MAP_FILE)?)?;
    let full_sprites: HashMap<PathId, TypedPPtr<Sprite>> = map
        .file()
        .scripts::<RoughMapRoom>("RoughMapRoom")?
        .map(|handle| {
            let room = handle.read()?;
            Ok((room.m_GameObject.m_PathID, room.fullSprite))
        })
        .collect::<Result<_>>()?;

    let mut rooms = Vec::new();
    // Lowercase scene name -> first marker of it.
    let mut markers: BTreeMap<String, PathId> = BTreeMap::new();
    for handle in map.file().objects_of::<GameObject>() {
        let name = handle.read()?.m_Name;
        if !is_scene_drawing(&name, &lowercase_scenes) {
            continue;
        }
        let path = handle.path()?;
        match path.split('/').collect::<Vec<_>>()[..] {
            [MAP_ROOT, area, _room] if !MARKER_GROUPS.contains(&area) => {
                rooms.push((name.to_lowercase(), handle.path_id()))
            }
            [MAP_ROOT, ..] if lowercase_scenes.contains(&name.to_lowercase()) => {
                markers
                    .entry(name.to_lowercase())
                    .or_insert(handle.path_id());
            }
            _ => {}
        }
    }
    for &(_, room) in &rooms {
        map.add_room(room, full_sprites.get(&room).copied())?;
    }
    for (scene, marker) in markers {
        if !rooms.iter().any(|(name, _)| *name == scene) {
            map.add_point(marker)?;
        }
    }
    map.write(&game, &scenes)
}

/// Whether `name` is a scene, or a piece of one's drawing (`<scene>_<suffix>`).
fn is_scene_drawing(name: &str, lowercase_scenes: &BTreeSet<String>) -> bool {
    let name = name.to_lowercase();
    name.match_indices('_')
        .map(|(i, _)| &name[..i])
        .chain([name.as_str()])
        .any(|prefix| lowercase_scenes.contains(prefix))
}
