//! The games the site covers, as configured in `games.json`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

const CONFIG_PATH: &str = "games.json";
const DATA_DIR: &str = "data";

#[derive(Deserialize)]
struct Config {
    games: Vec<Game>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub slug: String,
    /// Install directory name under `steamapps/common`.
    pub steam_dir: String,
    #[serde(default)]
    pub tags: TagRules,
}

/// How the community's spellings of tags map to canonical ones.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TagRules {
    /// Canonical tag -> variants that mean the same thing (lowercase, `_` and `-` already turned into spaces).
    #[serde(default)]
    pub synonyms: BTreeMap<String, Vec<String>>,
    /// Misspelled or area-named room-code prefix -> scene prefix.
    #[serde(default)]
    pub room_typos: BTreeMap<String, String>,
    /// Room-code prefix of numbered rooms -> the community's short prefix (`dust_maze_02` -> `m2`).
    #[serde(default)]
    pub room_nicknames: BTreeMap<String, String>,
}

impl Game {
    pub fn load(slug: &str) -> Result<Game> {
        let config: Config = serde_json::from_str(
            &std::fs::read_to_string(CONFIG_PATH)
                .with_context(|| format!("reading {CONFIG_PATH}"))?,
        )
        .with_context(|| format!("parsing {CONFIG_PATH}"))?;
        config
            .games
            .into_iter()
            .find(|game| game.slug == slug)
            .with_context(|| format!("no game {slug:?} in {CONFIG_PATH}"))
    }

    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(DATA_DIR).join(&self.slug)
    }

    /// Files served by the site as-is.
    pub fn public_dir(&self) -> PathBuf {
        PathBuf::from(DATA_DIR).join("public").join(&self.slug)
    }
}
