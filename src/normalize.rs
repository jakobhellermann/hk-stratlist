//! Collapses spelling variants of a non-room tag into one canonical form.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};

use crate::parser::is_roomlike;

/// Canonical tag -> variants that mean the same thing (lowercase, `_` already turned into spaces).
pub const SYNONYMS: &[(&str, &[&str])] = &[
    ("100% all bosses", &["100ab"]),
    ("100%", &["100", "hundo", "100-percent"]),
    ("2 spool fragments", &["2sf"]),
    ("4 mask shards", &["4ms"]),
    ("all bosses", &["allbosses"]),
    ("any%", &["any"]),
    ("awoo%", &["awoo"]),
    (
        "bell route",
        &["bells route", "bells", "bell", "bell route any%"],
    ),
    ("fourth chorus", &["4c", "4cs", "fourth", "fourthchorus"]),
    ("hha", &["hhg"]),
    ("lace 1", &["lace_1"]),
    ("lace 2", &["lace_2"]),
    ("mount fay", &["fay", "mtfay"]),
    ("true ending", &["te", "true", "true-ending"]),
];

/// Lowercase a tag, use spaces instead of `_` (except in room-code words) and map known variants
/// to their canonical spelling.
pub fn normalize_tag(raw: &str) -> String {
    let spaced = raw
        .to_lowercase()
        .split_whitespace()
        .map(|word| {
            if let Some(maze) = dust_maze_short(word) {
                maze
            } else if is_roomlike(word) {
                word.to_string()
            } else {
                word.replace('_', " ")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    SYNONYMS
        .iter()
        .find(|(_, variants)| variants.iter().any(|v| *v == spaced))
        .map_or(spaced, |(canonical, _)| canonical.to_string())
}

/// Misspelled or area-named room-code prefixes -> scene prefix.
const ROOM_TYPOS: &[(&str, &str)] = &[
    ("bone_ease_", "bone_east_"),
    ("shadows_", "shadow_"),
    ("cogworks_", "cog_"),
    ("whiteward_", "ward_"),
    ("sinners_road_", "dust_"),
    ("high_", "hang_"),
    ("bone_east_05b", "bone_east_05"),
];

/// Fixes common misspellings of a (lowercase) room code: known prefix typos and a missing
/// leading zero (`cog_4` -> `cog_04`).
pub fn room_spelling(tag: &str) -> String {
    let fixed = ROOM_TYPOS
        .iter()
        .find_map(|(typo, fix)| tag.strip_prefix(typo).map(|rest| format!("{fix}{rest}")))
        .unwrap_or_else(|| tag.to_string());
    match fixed.rsplit_once('_') {
        Some((prefix, n)) if n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()) => {
            format!("{prefix}_0{n}")
        }
        _ => fixed,
    }
}

/// `dust_maze_02` -> `m2`, the community's name for the maze rooms.
fn dust_maze_short(word: &str) -> Option<String> {
    let n: u32 = word.strip_prefix("dust_maze_")?.parse().ok()?;
    Some(format!("m{n}"))
}

/// Variant -> canonical tag for tags that only differ by spaces (`crawskip` / `craw skip`).
/// The variant with the most spaces wins, ties go to the more frequent one.
pub fn space_variants<'a>(tags: impl IntoIterator<Item = &'a str>) -> BTreeMap<String, String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for tag in tags {
        *counts.entry(tag).or_default() += 1;
    }
    let mut groups: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for &tag in counts.keys() {
        groups.entry(tag.replace(' ', "")).or_default().push(tag);
    }

    let mut merged = BTreeMap::new();
    for variants in groups.into_values().filter(|v| v.len() > 1) {
        let canonical = *variants
            .iter()
            .max_by_key(|t| (t.matches(' ').count(), counts[*t], Reverse(**t)))
            .expect("group is non-empty");
        for variant in variants.into_iter().filter(|v| *v != canonical) {
            merged.insert(variant.to_string(), canonical.to_string());
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_map_to_canonical() {
        assert_eq!(normalize_tag("TE"), "true ending");
        assert_eq!(normalize_tag("true-ending"), "true ending");
        assert_eq!(normalize_tag("hundo"), "100%");
        assert_eq!(normalize_tag("Any"), "any%");
    }

    #[test]
    fn underscores_become_spaces_except_in_room_codes() {
        assert_eq!(normalize_tag("true_ending"), "true ending");
        assert_eq!(normalize_tag("bell_beast"), "bell beast");
        assert_eq!(normalize_tag("clover_10"), "clover_10");
        assert_eq!(normalize_tag("4_ms"), "4 ms");
        assert_eq!(normalize_tag("2_spool_fragments"), "2 spool fragments");
        assert_eq!(normalize_tag("cog_08 sprint_jump"), "cog_08 sprint jump");
    }

    #[test]
    fn space_variants_prefer_more_spaces_then_frequency() {
        let merged = space_variants([
            "crawskip",
            "crawskip",
            "craw skip",
            "keyroute",
            "lace1",
            "lace 1",
            "solo",
        ]);
        assert_eq!(
            merged,
            BTreeMap::from([
                ("crawskip".to_string(), "craw skip".to_string()),
                ("lace1".to_string(), "lace 1".to_string()),
            ])
        );
    }

    #[test]
    fn dust_maze_rooms_use_short_names() {
        assert_eq!(normalize_tag("dust_maze_02"), "m2");
        assert_eq!(normalize_tag("Dust_Maze_08 cycle"), "m8 cycle");
    }

    #[test]
    fn room_spelling_fixes_typos_and_padding() {
        assert_eq!(room_spelling("cog_4"), "cog_04");
        assert_eq!(room_spelling("bone_ease_10"), "bone_east_10");
        assert_eq!(room_spelling("shadows_10"), "shadow_10");
        assert_eq!(room_spelling("sinners_road_09"), "dust_09");
        assert_eq!(room_spelling("tut_01b"), "tut_01b");
    }

    #[test]
    fn canonical_and_unknown_tags_are_only_lowercased() {
        assert_eq!(normalize_tag("True Ending"), "true ending");
        assert_eq!(normalize_tag(" Cogwork Dancers "), "cogwork dancers");
    }
}
