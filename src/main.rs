use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

mod map;
mod parser;
use parser::{is_roomlike, parse_content};

#[derive(Deserialize)]
struct Export {
    guild: Id,
    channel: Id,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    id: String,
    timestamp: String,
    timestamp_edited: Option<String>,
    author: User,
    content: String,
    attachments: Vec<Attachment>,
    mentions: Vec<User>,
}

#[derive(Deserialize)]
struct Attachment {
    url: String,
}

#[derive(Deserialize)]
struct User {
    id: String,
    nickname: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Post<'a> {
    id: &'a str,
    /// Permalink to the message in Discord.
    link: String,
    author: &'a str,
    date: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    edited: Option<&'a str>,
    /// Room (scene) names.
    rooms: Vec<&'a str>,
    /// Tags that are not known rooms.
    tags: Vec<String>,
    prose: Option<String>,
    attachments: Vec<&'a str>,
}

#[derive(Serialize)]
struct Strats<'a> {
    /// Room name -> area.
    areas: BTreeMap<&'a str, &'a str>,
    posts: Vec<Post<'a>>,
}

const STRATS_PATH: &str = "export/strats.json";
const MAP_DATA_PATH: &str = "/home/jakob/dev/hkss/other/maptools/map-data.json";
const SCENE_SIZES_PATH: &str = "/home/jakob/dev/hkss/mods/MapWarp/data/mapwarp_scene_sizes.json";
const OUT_DIR: &str = "data";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let strats_path = args.next().unwrap_or_else(|| STRATS_PATH.to_string());
    let map_path = args.next().unwrap_or_else(|| MAP_DATA_PATH.to_string());
    let scene_sizes_path = args.next().unwrap_or_else(|| SCENE_SIZES_PATH.to_string());
    let out_dir = args.next().unwrap_or_else(|| OUT_DIR.to_string());

    let export: Export = serde_json::from_str(&std::fs::read_to_string(&strats_path)?)?;
    let messages = &export.messages;
    let entries: Vec<map::MapEntry> = serde_json::from_str(&std::fs::read_to_string(&map_path)?)?;
    let loadable: HashSet<String> = serde_json::from_str::<HashMap<String, serde_json::Value>>(
        &std::fs::read_to_string(&scene_sizes_path)?,
    )?
    .into_keys()
    .map(|scene| scene.to_lowercase())
    .collect();
    let canonical = map::lookup(entries.iter().map(|e| e.name.clone()), &loadable);
    let (rooms, orphans) = map::group(entries, &loadable);

    let tagged: Vec<(&Message, parser::ParsedPost)> = messages
        .iter()
        .filter_map(|m| {
            Some((
                m,
                parse_content(&replace_custom_emoji(&resolve_mentions(m)))?,
            ))
        })
        .collect();

    let total = messages.len();
    let mut tag_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut posts = Vec::new();

    for (msg, parsed) in tagged {
        let mut post_rooms = Vec::new();
        let mut post_tags = Vec::new();
        for tag in parsed.tags {
            *tag_counts.entry(tag.clone()).or_default() += 1;
            match canonical.get(&tag).map(String::as_str) {
                Some(room) if !post_rooms.contains(&room) => post_rooms.push(room),
                Some(_) => {}
                None if !post_tags.contains(&tag) => post_tags.push(tag),
                None => {}
            }
        }
        posts.push(Post {
            id: &msg.id,
            link: format!(
                "https://discord.com/channels/{}/{}/{}",
                export.guild.id, export.channel.id, msg.id
            ),
            author: &msg.author.nickname,
            date: &msg.timestamp,
            edited: msg.timestamp_edited.as_deref(),
            rooms: post_rooms,
            tags: post_tags,
            prose: parsed.prose,
            attachments: msg.attachments.iter().map(|a| a.url.as_str()).collect(),
        });
    }

    let roomlike: Vec<(&String, &usize)> =
        tag_counts.iter().filter(|(t, _)| is_roomlike(t)).collect();
    let unmatched: Vec<(&String, &usize)> = roomlike
        .iter()
        .filter(|(t, _)| !canonical.contains_key(t.as_str()))
        .copied()
        .collect();

    println!("messages total:        {total}");
    println!("messages with tags:    {}", posts.len());
    println!(
        "  without known room:  {}",
        posts.iter().filter(|p| p.rooms.is_empty()).count()
    );
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

    println!("\nmap entries without a loadable scene (kept as own rooms):");
    for name in &orphans {
        println!("  {name}");
    }

    let strats = Strats {
        areas: rooms
            .iter()
            .map(|r| (r.name.as_str(), r.area.as_str()))
            .collect(),
        posts,
    };
    let out_dir = Path::new(&out_dir);
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(
        out_dir.join("strats.json"),
        serde_json::to_string_pretty(&strats)?,
    )?;
    std::fs::write(out_dir.join("map.json"), serde_json::to_string(&rooms)?)?;
    println!("\nwrote {}/{{strats,map}}.json", out_dir.display());

    Ok(())
}

/// Content with `<@id>` / `<@!id>` user mentions replaced by `@nickname`.
fn resolve_mentions(msg: &Message) -> String {
    msg.mentions
        .iter()
        .fold(msg.content.clone(), |content, user| {
            let name = format!("@{}", user.nickname);
            content
                .replace(&format!("<@{}>", user.id), &name)
                .replace(&format!("<@!{}>", user.id), &name)
        })
}

/// Replaces custom emoji markup `<:name:id>` / `<a:name:id>` with `:name:`.
fn replace_custom_emoji(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        match custom_emoji(tail) {
            Some((name, len)) => {
                out.push_str(&format!(":{name}:"));
                rest = &tail[len..];
            }
            None => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Emoji name and markup length if `s` starts with custom emoji markup.
fn custom_emoji(s: &str) -> Option<(&str, usize)> {
    let end = s.find('>')?;
    let inner = &s[1..end];
    let inner = inner
        .strip_prefix("a:")
        .or_else(|| inner.strip_prefix(':'))?;
    let (name, id) = inner.split_once(':')?;
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !id.is_empty()
        && id.chars().all(|c| c.is_ascii_digit());
    valid.then_some((name, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_emoji_become_shortcodes() {
        assert_eq!(
            replace_custom_emoji("nice <:grubnote:123> and <a:spin:456>!"),
            "nice :grubnote: and :spin:!"
        );
    }

    #[test]
    fn other_angle_brackets_are_kept() {
        assert_eq!(
            replace_custom_emoji("a < b, <@123>, <:bad name:1>"),
            "a < b, <@123>, <:bad name:1>"
        );
    }
}
