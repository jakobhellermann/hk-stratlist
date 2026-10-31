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
//!   prose is longer. If the line ends in such a long segment, the first long
//!   segment ends the tags: its first word is a tag, the rest of the line
//!   (including any later `#`) is prose. Long segments followed by short ones
//!   stay whole tags (`#mount fay first room no claw #low%`).
//! * A `,` ends a tag; the rest of its segment is prose (`#100%, notes`).
//! * Trailing `-` is a separator, not part of the tag (`#100%-#belltown_08`).
//! * A channel/thread mention (`<#id>`) ends the tags; it and the rest of the line are prose.
//! * Markdown markers and quotes around a tag are dropped (`#any% **#Trobbio**`,
//!   `#"Bell beast #taunt"`).
//!
//! Tags are only lowercased here; rooms are matched before other tags get
//! normalized, see [`crate::normalize`].

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

    let (first_line, after_mention) = match first_line.find("<#") {
        Some(i) => (&first_line[..i], first_line[i..].trim()),
        None => (first_line, ""),
    };

    // Each segment with its byte offset in `first_line`.
    let mut offset = 0;
    let segments: Vec<(usize, &str)> = first_line
        .split('#')
        .map(|seg| {
            let start = offset;
            offset += seg.len() + 1;
            (start, seg)
        })
        .filter(|(_, seg)| !seg.trim().is_empty())
        .collect();

    let is_long = |seg: &str| seg.split_whitespace().count() > MAX_TAG_WORDS;
    let ends_in_prose = segments.last().is_some_and(|(_, seg)| is_long(seg));

    let mut tags: Vec<String> = Vec::new();
    let mut inline_prose: Vec<&str> = Vec::new();

    for (start, seg) in segments {
        let seg_trimmed = seg.trim();

        if ends_in_prose && is_long(seg_trimmed) {
            let (tag, _) = seg_trimmed
                .split_once(char::is_whitespace)
                .expect("more than MAX_TAG_WORDS words");
            tags.push(trim_separators(tag).to_lowercase());
            let prose_start = start + (seg.len() - seg.trim_start().len()) + tag.len();
            inline_prose.push(first_line[prose_start..].trim());
            break;
        }

        let (tag, after_comma) = match seg_trimmed.split_once(',') {
            Some((tag, rest)) => (tag, rest.trim()),
            None => (seg_trimmed, ""),
        };
        let tag = trim_separators(tag);
        if !tag.is_empty() {
            tags.push(tag.to_lowercase());
        }
        if !after_comma.is_empty() {
            inline_prose.push(after_comma);
        }
    }

    if !after_mention.is_empty() {
        inline_prose.push(after_mention);
    }
    let inline_prose = inline_prose.join(" ");
    let prose = match (inline_prose.is_empty(), rest.is_empty()) {
        (false, true) => Some(inline_prose),
        (false, false) => Some(format!("{inline_prose}\n{rest}")),
        (true, true) => None,
        (true, false) => Some(rest.to_string()),
    };

    Some(ParsedPost { tags, prose })
}

fn trim_separators(tag: &str) -> &str {
    const WRAPPERS: [char; 7] = ['*', '~', '|', '`', '"', '“', '”'];
    tag.trim_matches(|c: char| WRAPPERS.contains(&c) || c.is_whitespace())
        .trim_end_matches(|c: char| {
            c == '-' || c == ',' || WRAPPERS.contains(&c) || c.is_whitespace()
        })
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
    fn tags_are_lowercased_but_not_normalized() {
        assert_eq!(tags("#Cog_08 #Any% #TE"), vec!["cog_08", "any%", "te"]);
    }

    #[test]
    fn prose_separated_by_newline() {
        // synthetic prose, not copied from real posts
        let parsed = parse_content("#song_07 #jubilana #true ending #te\n\nsynthetic route notes that run well past four words").unwrap();
        assert_eq!(
            parsed.tags,
            vec!["song_07", "jubilana", "true ending", "te"]
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
    fn comma_ends_tag_and_starts_prose() {
        // synthetic prose, not copied from real posts
        let parsed = parse_content("#cog_08 #100%, made up note about the route").unwrap();
        assert_eq!(parsed.tags, vec!["cog_08", "100%"]);
        assert_eq!(
            parsed.prose.as_deref(),
            Some("made up note about the route")
        );
    }

    #[test]
    fn comma_prose_combined_with_newline_prose() {
        let parsed = parse_content("#cog_08 #any%, short note\nsecond line").unwrap();
        assert_eq!(parsed.tags, vec!["cog_08", "any%"]);
        assert_eq!(parsed.prose.as_deref(), Some("short note\nsecond line"));
    }

    #[test]
    fn long_final_segment_splits_before_comma() {
        // the length heuristic sees the whole segment, so the comma stays inside the prose
        let parsed =
            parse_content("#hang_03 made up words here, and more after the comma").unwrap();
        assert_eq!(parsed.tags, vec!["hang_03"]);
        assert_eq!(
            parsed.prose.as_deref(),
            Some("made up words here, and more after the comma")
        );
    }

    #[test]
    fn trailing_comma_without_prose() {
        let parsed = parse_content("#hang_03,").unwrap();
        assert_eq!(parsed.tags, vec!["hang_03"]);
        assert_eq!(parsed.prose, None);
    }

    #[test]
    fn dash_between_tags_is_a_separator() {
        assert_eq!(
            tags("#100%-#Father of the Flame-#Belltown_08"),
            vec!["100%", "father of the flame", "belltown_08"]
        );
        // inner dashes stay
        assert_eq!(
            tags("#pre-vault #true-ending"),
            vec!["pre-vault", "true-ending"]
        );
    }

    #[test]
    fn markdown_markers_around_tags_are_dropped() {
        assert_eq!(
            tags("#cog_08 #any% **#Lace**"),
            vec!["cog_08", "any%", "lace"]
        );
        assert_eq!(tags("#~~old route~~ #`code`"), vec!["old route", "code"]);
    }

    #[test]
    fn quotes_around_tags_are_dropped() {
        assert_eq!(
            tags("#\"Some boss #cog_08 #2 cycle\""),
            vec!["some boss", "cog_08", "2 cycle"]
        );
        assert_eq!(tags("#“curly”"), vec!["curly"]);
    }

    #[test]
    fn first_long_segment_ends_tags() {
        // synthetic prose, not copied from real posts
        let parsed = parse_content(
            "#cog_08 #hang_03 made up text about the room in #cog_09. More words follow here",
        )
        .unwrap();
        assert_eq!(parsed.tags, vec!["cog_08", "hang_03"]);
        assert_eq!(
            parsed.prose.as_deref(),
            Some("made up text about the room in #cog_09. More words follow here")
        );
    }

    #[test]
    fn long_segment_followed_by_short_tags_stays_a_tag() {
        assert_eq!(
            tags("#one two three four five #short #tags"),
            vec!["one two three four five", "short", "tags"]
        );
    }

    #[test]
    fn channel_mention_ends_tags() {
        let parsed = parse_content("#cog_08 <#123> #not a tag\nmore").unwrap();
        assert_eq!(parsed.tags, vec!["cog_08"]);
        assert_eq!(parsed.prose.as_deref(), Some("<#123> #not a tag\nmore"));
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
