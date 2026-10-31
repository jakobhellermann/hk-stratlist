//! Groups map-data entries into rooms.
//!
//! map-data has one entry per map sprite, and a room's map drawing can consist of several sprites
//! (e.g. `Bonetown`, `Bonetown_top_left`, `Bonetown_top_right`). An entry belongs to the longest
//! `_`-separated prefix of its name that is a loadable scene. Like MapWarp, only the room's own
//! sprites are drawn; segments are reveal pieces that overlap the room or reach into others.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct MapEntry {
    pub name: String,
    area: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: [f64; 4],
}

#[derive(Serialize)]
pub struct MapRoom {
    pub name: String,
    pub area: String,
    color: [f64; 4],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// Bounding box `[x, y, w, h]` of the room's own map sprite(s).
    boxes: Vec<[f64; 4]>,
}

/// Entry name without the `:n` duplicate suffix and stray spaces (`Library_04 _bot`).
fn clean_name(name: &str) -> String {
    name.split(':')
        .next()
        .expect("split yields at least one item")
        .replace(' ', "")
}

/// `loadable` holds lowercased scene names; scene names differ in case from map entries
/// (`Abandoned_town` / `Abandoned_Town`), so matching ignores case and keeps the map's spelling.
pub fn scene_of(name: &str, loadable: &HashSet<String>) -> Option<String> {
    let parts: Vec<&str> = name.split('_').collect();
    (1..=parts.len())
        .rev()
        .map(|i| parts[..i].join("_"))
        .find(|candidate| loadable.contains(&candidate.to_lowercase()))
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Returns the grouped rooms and the names of entries that matched no loadable scene. Those are
/// kept as rooms of their own.
pub fn group(entries: Vec<MapEntry>, loadable: &HashSet<String>) -> (Vec<MapRoom>, Vec<String>) {
    let mut rooms: BTreeMap<String, MapRoom> = BTreeMap::new();
    let mut orphans = Vec::new();

    for entry in entries {
        let name = clean_name(&entry.name);
        let scene = scene_of(&name, loadable).unwrap_or_else(|| {
            orphans.push(entry.name.clone());
            name.clone()
        });
        if name != scene {
            continue;
        }
        let bbox = [entry.x, entry.y, entry.w, entry.h].map(round);

        let room = rooms.entry(scene.clone()).or_insert_with(|| MapRoom {
            name: scene.clone(),
            area: entry.area.clone(),
            color: entry.color,
            x: entry.x,
            y: entry.y,
            w: entry.w,
            h: entry.h,
            boxes: Vec::new(),
        });
        let (x1, y1) = (
            (room.x + room.w).max(entry.x + entry.w),
            (room.y + room.h).max(entry.y + entry.h),
        );
        room.x = room.x.min(entry.x);
        room.y = room.y.min(entry.y);
        room.w = x1 - room.x;
        room.h = y1 - room.y;
        room.boxes.push(bbox);
    }

    (rooms.into_values().collect(), orphans)
}

/// Lowercased name of every entry -> its room name.
pub fn lookup(
    names: impl Iterator<Item = String>,
    loadable: &HashSet<String>,
) -> BTreeMap<String, String> {
    names
        .map(|n| {
            let name = clean_name(&n);
            let scene = scene_of(&name, loadable).unwrap_or_else(|| name.clone());
            (name.to_lowercase(), scene)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_resolve_to_longest_loadable_prefix() {
        let loadable: HashSet<String> = ["bonetown", "ant_04", "ant_04_mid", "abandoned_town"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            scene_of("Bonetown_top_left", &loadable).as_deref(),
            Some("Bonetown")
        );
        assert_eq!(
            scene_of("Ant_04_mid", &loadable).as_deref(),
            Some("Ant_04_mid")
        );
        assert_eq!(
            scene_of("Ant_04_left", &loadable).as_deref(),
            Some("Ant_04")
        );
        assert_eq!(scene_of("Dust_10b", &loadable), None);
        assert_eq!(
            scene_of("Abandoned_Town", &loadable).as_deref(),
            Some("Abandoned_Town")
        );
    }

    #[test]
    fn clean_name_strips_duplicate_suffix_and_spaces() {
        assert_eq!(clean_name("Bone_East_07:1"), "Bone_East_07");
        assert_eq!(clean_name("Library_04 _bot"), "Library_04_bot");
    }
}
