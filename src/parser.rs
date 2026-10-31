//! Parsing of `#silk-strats` posts into structured tags + free-form prose.
//!
//! Convention (from the channel topic): posts are formatted with leading
//! `#roomname #category` tags, optionally followed by a free-text description.
//! Tagging is hand-written and messy, so parsing is heuristic:
//!
//! * A message only has tags if its trimmed content starts with `#`.
//! * Tags live on the first line, each introduced by `#`, and may be
//!   multi-word (e.g. `#true ending`, `#cogwork dancers`).
//! * Prose is everything that is *not* a tag. It is usually separated by a
//!   newline, but can also follow the last tag inline on the first line — in
//!   that case we use the length heuristic: a tag is short (<= 4 words),
//!   prose is longer. So a final first-line segment of more than 4 words is
//!   split into `first word = tag, rest = prose`.
//!
//! Everything is lowercased and a small synonym map collapses common
//! spellings (`te` / `true-ending` -> `true ending`, `hundo` / `100` -> `100%`, ...).

/// Maximum number of words a `#`-segment may have to still count as a single
/// tag. A longer final segment is treated as `tag + inline prose`.
const MAX_TAG_WORDS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPost {
    /// Normalized tags in the order they appeared.
    pub tags: Vec<String>,
    /// Free-form description, if any.
    pub prose: Option<String>,
}

/// Parse a single message's content. Returns `None` for messages that carry no
/// tags (pure discussion), i.e. content that does not start with `#`.
pub fn parse_content(content: &str) -> Option<ParsedPost> {
    let trimmed = content.trim();
    if !trimmed.starts_with('#') {
        return None;
    }

    // Tags live on the first line; everything after the first newline is prose.
    let (first_line, rest) = match trimmed.split_once('\n') {
        Some((first, rest)) => (first.trim_end(), rest.trim()),
        None => (trimmed, ""),
    };

    let segments: Vec<&str> = first_line
        .split('#')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let mut tags: Vec<String> = Vec::new();
    let mut inline_prose: Option<String> = None;

    for (i, seg) in segments.iter().enumerate() {
        let is_last = i + 1 == segments.len();
        let word_count = seg.split_whitespace().count();

        if is_last && word_count > MAX_TAG_WORDS {
            // Final segment is "tag + inline prose": first word is the tag.
            let mut words = seg.split_whitespace();
            let tag = words.next().expect("word_count > 0");
            tags.push(normalize_tag(tag));
            inline_prose = Some(words.collect::<Vec<_>>().join(" "));
        } else {
            tags.push(normalize_tag(seg));
        }
    }

    let prose = match (inline_prose, rest.is_empty()) {
        (Some(p), true) => Some(p),
        (Some(p), false) => Some(format!("{p}\n{rest}")),
        (None, true) => None,
        (None, false) => Some(rest.to_string()),
    };

    Some(ParsedPost { tags, prose })
}

/// Lowercase a tag and collapse known synonyms to a canonical spelling.
pub fn normalize_tag(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    match lower.as_str() {
        "te" | "true" | "true-ending" | "true ending" => "true ending".to_string(),
        "100" | "hundo" | "100%" => "100%".to_string(),
        "any" | "any%" => "any%".to_string(),
        _ => lower,
    }
}

/// Whether a (lowercased) tag looks like a room code: alphabetic segment(s)
/// joined by `_`, ending in a number with an optional single trailing letter,
/// e.g. `cog_08`, `bone_east_12`, `tut_01b`.
pub fn is_roomlike(tag: &str) -> bool {
    let Some((prefix, last)) = tag.rsplit_once('_') else {
        return false;
    };
    // prefix segments must all be non-empty and alphabetic
    if prefix.is_empty()
        || prefix
            .split('_')
            .any(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_alphabetic()))
    {
        return false;
    }
    // last segment: digits, optionally followed by exactly one letter
    let mut chars = last.chars();
    let mut seen_digit = false;
    let mut seen_trailing_letter = false;
    for c in chars.by_ref() {
        if c.is_ascii_digit() {
            if seen_trailing_letter {
                return false; // letter must be last
            }
            seen_digit = true;
        } else if c.is_ascii_alphabetic() {
            if seen_trailing_letter {
                return false; // at most one trailing letter
            }
            seen_trailing_letter = true;
        } else {
            return false;
        }
    }
    seen_digit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(content: &str) -> Vec<String> {
        parse_content(content).expect("expected tags").tags
    }

    #[test]
    fn no_hash_means_no_tags() {
        // pure discussion message (synthetic prose) -> not a strat post
        assert_eq!(
            parse_content("we should move routing talk into threads"),
            None
        );
        assert_eq!(parse_content(""), None);
        // a stray hash mid-sentence does not start a tag region
        assert_eq!(parse_content("see the pins in channel for details"), None);
    }

    #[test]
    fn single_tag_no_prose() {
        assert_eq!(
            parse_content("#hang_03"),
            Some(ParsedPost {
                tags: vec!["hang_03".into()],
                prose: None,
            })
        );
    }

    #[test]
    fn trailing_whitespace_segment() {
        assert_eq!(tags("#Bone_East_01 "), vec!["bone_east_01"]);
    }

    #[test]
    fn multiple_tags_lowercased() {
        assert_eq!(
            tags("#Father of the Flame #100% All Bosses #Belltown_08"),
            vec!["father of the flame", "100% all bosses", "belltown_08"]
        );
    }

    #[test]
    fn multiword_tag_kept_whole() {
        assert_eq!(
            tags("#cogwork dancers #any%"),
            vec!["cogwork dancers", "any%"]
        );
    }

    #[test]
    fn synonyms_are_normalized() {
        assert_eq!(
            tags("#tut_02 #any #te #100"),
            vec!["tut_02", "any%", "true ending", "100%"]
        );
        assert_eq!(
            tags("#Cog_08 #Any% #TE"),
            vec!["cog_08", "any%", "true ending"]
        );
        assert_eq!(tags("#true-ending #hundo"), vec!["true ending", "100%"]);
    }

    #[test]
    fn prose_separated_by_newline() {
        // synthetic prose, not copied from real posts
        let parsed = parse_content("#song_07 #jubilana #true ending #te\n\nsynthetic route notes that run well past four words").unwrap();
        assert_eq!(
            parsed.tags,
            vec!["song_07", "jubilana", "true ending", "true ending"]
        );
        assert_eq!(
            parsed.prose.as_deref(),
            Some("synthetic route notes that run well past four words")
        );
    }

    #[test]
    fn inline_prose_after_last_tag() {
        // final segment > 4 words -> first word is the tag, the rest is prose
        let parsed =
            parse_content("#cog_08 fictional upper route description spanning many words").unwrap();
        assert_eq!(parsed.tags, vec!["cog_08"]);
        assert_eq!(
            parsed.prose.as_deref(),
            Some("fictional upper route description spanning many words")
        );
    }

    #[test]
    fn inline_prose_combined_with_newline_prose() {
        let parsed = parse_content(
            "#shellwood_01 inline synthetic prose with several words\nsecond line of fake prose",
        )
        .unwrap();
        assert_eq!(parsed.tags, vec!["shellwood_01"]);
        assert_eq!(
            parsed.prose.as_deref(),
            Some("inline synthetic prose with several words\nsecond line of fake prose")
        );
    }

    #[test]
    fn short_final_segment_stays_a_tag() {
        // exactly 4 words -> still one tag, no prose split
        assert_eq!(tags("#a b c d"), vec!["a b c d"]);
        // 5 words -> split into tag + prose
        let parsed = parse_content("#a b c d e").unwrap();
        assert_eq!(parsed.tags, vec!["a"]);
        assert_eq!(parsed.prose.as_deref(), Some("b c d e"));
    }

    #[test]
    fn roomlike_detection() {
        assert!(is_roomlike("cog_08"));
        assert!(is_roomlike("bone_east_12"));
        assert!(is_roomlike("tut_01b"));
        assert!(is_roomlike("steps_0x"));

        assert!(!is_roomlike("any%"));
        assert!(!is_roomlike("true ending"));
        assert!(!is_roomlike("cogwork dancers"));
        assert!(!is_roomlike("cog")); // no number
        assert!(!is_roomlike("cog_08b2")); // letter not last
        assert!(!is_roomlike("_08")); // empty prefix
    }
}
