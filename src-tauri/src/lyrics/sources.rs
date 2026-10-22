//! Where a track's lyrics come from, best first:
//!
//! 1. lyrics the user saved in the app (the editor's "library only"): an explicit choice made
//!    after seeing what the file has, so nothing in the file overrides it;
//! 2. synced lyrics in the tags (ID3 `SYLT`, or LRC text in `SYNCEDLYRICS`/`LYRICS`/`USLT`/
//!    `©lyr`);
//! 3. a `.lrc` file next to the track, with the same name;
//! 4. plain lyrics in the tags;
//! 5. lyrics found online earlier (cached in the library).

use std::path::Path;

use library::{SOURCE_USER, SavedLyrics};
use lyrics::Lyrics;
use media::TagLyrics;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Tags,
    /// The `.lrc` file next to the track.
    File,
    /// Saved in the app by the user.
    Library,
    /// Found on LRCLIB.
    Online,
}

/// Everything found for a track, before choosing.
#[derive(Debug, Default)]
pub struct Found {
    pub saved: Option<SavedLyrics>,
    pub tags: TagLyrics,
    pub file: Option<String>,
}

/// Lyrics with the text they were read from (what the editor opens).
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    pub lyrics: Lyrics,
    pub text: String,
    pub source: Source,
}

fn chosen(text: &str, source: Source) -> Option<Chosen> {
    let lyrics = lyrics::parse(text);
    (!lyrics.is_empty()).then(|| Chosen {
        lyrics,
        text: text.to_owned(),
        source,
    })
}

/// Picks the lyrics to show.
pub fn choose(found: &Found) -> Option<Chosen> {
    let (user, online) = match &found.saved {
        Some(saved) if saved.source == SOURCE_USER => (Some(saved.text.as_str()), None),
        Some(saved) => (None, Some(saved.text.as_str())),
        None => (None, None),
    };
    if let Some(chosen) = user.and_then(|text| chosen(text, Source::Library)) {
        return Some(chosen);
    }
    let sylt = lyrics::from_sylt(&found.tags.sylt);
    if !sylt.is_empty() {
        return Some(Chosen {
            text: sylt.to_lrc(),
            lyrics: sylt,
            source: Source::Tags,
        });
    }
    let tag_texts: Vec<Chosen> = found
        .tags
        .texts
        .iter()
        .filter_map(|text| chosen(text, Source::Tags))
        .collect();
    if let Some(synced) = tag_texts.iter().find(|c| c.lyrics.synced) {
        return Some(synced.clone());
    }
    found
        .file
        .as_deref()
        .and_then(|text| chosen(text, Source::File))
        .or_else(|| tag_texts.into_iter().next())
        .or_else(|| online.and_then(|text| chosen(text, Source::Online)))
}

/// Reads what a local file has: its tags and its `.lrc`.
pub fn read_local(track: &Path, found: &mut Found) {
    match media::read_lyrics(track) {
        Ok(tags) => found.tags = tags,
        Err(err) => log::info!("lyrics in the tags of {}: {err}", track.display()),
    }
    found.file = lyrics::read_lrc(track);
}

#[cfg(test)]
mod tests {
    use super::*;
    use library::SOURCE_ONLINE;

    fn saved(text: &str, source: &str) -> Option<SavedLyrics> {
        Some(SavedLyrics {
            text: text.into(),
            synced: false,
            source: source.into(),
            updated_at: 0,
        })
    }

    fn source_of(found: &Found) -> Option<(Source, String)> {
        choose(found).map(|c| (c.source, c.lyrics.lines[0].text.clone()))
    }

    #[test]
    fn follows_the_priority_order() {
        let mut found = Found {
            saved: saved("[00:01.00]online", SOURCE_ONLINE),
            ..Found::default()
        };
        assert_eq!(source_of(&found), Some((Source::Online, "online".into())));

        found.tags.texts = vec!["plain tag".into()];
        assert_eq!(source_of(&found), Some((Source::Tags, "plain tag".into())));

        found.file = Some("[00:01.00]from file".into());
        assert_eq!(source_of(&found), Some((Source::File, "from file".into())));

        found.tags.texts.push("[00:02.00]synced tag".into());
        assert_eq!(source_of(&found), Some((Source::Tags, "synced tag".into())));

        found.tags.sylt = vec![(500, "sylt".into())];
        let chosen = choose(&found).unwrap();
        assert_eq!(chosen.source, Source::Tags);
        assert_eq!(chosen.text.trim_end(), "[00:00.50]sylt");

        found.saved = saved("mine", SOURCE_USER);
        assert_eq!(source_of(&found), Some((Source::Library, "mine".into())));
    }

    #[test]
    fn empty_texts_are_skipped() {
        let found = Found {
            saved: saved("  \n", SOURCE_USER),
            tags: TagLyrics {
                sylt: Vec::new(),
                texts: vec!["[ar:Only a tag]".into()],
            },
            file: Some(String::new()),
        };
        assert_eq!(choose(&found), None);
    }
}
