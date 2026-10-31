use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

mod parser;
use parser::{is_roomlike, parse_content};

#[derive(Deserialize)]
struct Export {
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

#[derive(Deserialize)]
struct Room {
    name: String,
}

const STRATS_PATH: &str = "export/strats.json";
const MAP_DATA_PATH: &str = "/home/jakob/dev/hkss/other/maptools/map-data.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let strats_path = args.next().unwrap_or_else(|| STRATS_PATH.to_string());
    let map_path = args.next().unwrap_or_else(|| MAP_DATA_PATH.to_string());

    let export: Export = serde_json::from_str(&std::fs::read_to_string(&strats_path)?)?;
    let rooms: Vec<Room> = serde_json::from_str(&std::fs::read_to_string(&map_path)?)?;
    let canonical: HashSet<String> = rooms.iter().map(|r| r.name.to_lowercase()).collect();

    let total = export.messages.len();
    let mut tagged = 0usize;
    let mut tag_counts: BTreeMap<String, usize> = BTreeMap::new();

    for msg in &export.messages {
        let Some(parsed) = parse_content(&msg.content) else {
            continue;
        };
        tagged += 1;
        for tag in parsed.tags {
            *tag_counts.entry(tag).or_default() += 1;
        }
    }

    let roomlike: Vec<(&String, &usize)> =
        tag_counts.iter().filter(|(t, _)| is_roomlike(t)).collect();
    let unmatched: Vec<(&String, &usize)> = roomlike
        .iter()
        .filter(|(t, _)| !canonical.contains(t.as_str()))
        .copied()
        .collect();

    println!("messages total:        {total}");
    println!("messages with tags:    {tagged}");
    println!("distinct tags:         {}", tag_counts.len());
    println!("distinct roomlike:     {}", roomlike.len());
    println!(
        "  matched in map-data: {}",
        roomlike.len() - unmatched.len()
    );
    println!("  NOT in map-data:     {}", unmatched.len());

    println!("\nunmatched roomlike tags (raw, for manual fixing):");
    let mut unmatched_sorted = unmatched.clone();
    unmatched_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (tag, count) in &unmatched_sorted {
        println!("  {tag:24} {count}");
    }

    println!("\ntop 25 tags:");
    let mut by_count: Vec<(&String, &usize)> = tag_counts.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (tag, count) in by_count.iter().take(25) {
        println!("  {tag:24} {count}");
    }

    Ok(())
}
