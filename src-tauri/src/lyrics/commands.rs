//! Lyrics commands: the playing track's lyrics and offset, and the editor's reading, parsing
//! and saving.

use std::path::{Path, PathBuf};

use lyrics::Lyrics;
use serde::Serialize;
use tauri::State;

use super::sources::{self, Found, Source};
use super::{CurrentLyrics, LyricsService, Target, is_local};
use crate::error::AppError;
use crate::library::LibraryService;

#[tauri::command]
pub fn lyrics_current(lyrics: State<'_, LyricsService>) -> CurrentLyrics {
    lyrics.current()
}

#[tauri::command]
pub async fn lyrics_set_offset(
    lyrics: State<'_, LyricsService>,
    offset_ms: i64,
) -> Result<(), AppError> {
    lyrics.set_offset(offset_ms).await
}

#[tauri::command]
pub fn lyrics_search_online(lyrics: State<'_, LyricsService>) {
    lyrics.search_online();
}

/// Where the editor can save a track's lyrics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Targets {
    /// The file's format has tags lofty can write.
    pub tags: bool,
    /// A local file, so a `.lrc` can go next to it.
    pub file: bool,
    /// The track is in the library.
    pub library: bool,
}

/// What the editor opens for a track.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorLyrics {
    /// The lyrics shown for the track, as LRC or plain text (empty when there are none).
    pub text: String,
    pub source: Option<Source>,
    pub targets: Targets,
}

#[tauri::command]
pub async fn lyrics_get(
    library: State<'_, LibraryService>,
    path: String,
    library_id: Option<i64>,
) -> Result<EditorLyrics, AppError> {
    let library = library.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut found = Found::default();
        if let Some(id) = library_id {
            found.saved = library.with(|db| db.saved_lyrics(id))?;
        }
        let local = is_local(&path);
        let mut tags = false;
        if local {
            sources::read_local(Path::new(&path), &mut found);
            tags = media::read_lyrics(Path::new(&path)).is_ok();
        }
        let chosen = sources::choose(&found);
        Ok(EditorLyrics {
            text: chosen.as_ref().map(|c| c.text.clone()).unwrap_or_default(),
            source: chosen.map(|c| c.source),
            targets: Targets {
                tags,
                file: local,
                library: library_id.is_some(),
            },
        })
    })
    .await?
}

/// Parses lyrics for the editor, with any `[offset]` folded into the timestamps.
#[tauri::command]
pub fn lyrics_parse(text: String) -> Lyrics {
    lyrics::parse(&text).normalized()
}

/// Writes the editor's lyrics as LRC (plain text when nothing is timed).
#[tauri::command]
pub fn lyrics_serialize(lyrics: Lyrics) -> String {
    lyrics.to_lrc()
}

#[tauri::command]
pub async fn lyrics_save(
    lyrics: State<'_, LyricsService>,
    path: String,
    library_id: Option<i64>,
    text: String,
    target: Target,
) -> Result<(), AppError> {
    lyrics.save(path, library_id, text, target).await
}

/// Only lyrics files are read or written through the import/export commands.
fn lyrics_file(path: &Path) -> Result<(), AppError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("lrc" | "txt") => Ok(()),
        _ => Err(AppError::Lyrics(format!(
            "{} is not a .lrc or .txt file",
            path.display()
        ))),
    }
}

/// Reads a `.lrc` or `.txt` picked for import.
#[tauri::command]
pub async fn lyrics_import(path: PathBuf) -> Result<String, AppError> {
    lyrics_file(&path)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(path)).await??;
    Ok(lyrics::decode_text(&bytes))
}

/// Writes the editor's lyrics to a `.lrc` or `.txt` picked for export.
#[tauri::command]
pub async fn lyrics_export(path: PathBuf, text: String) -> Result<(), AppError> {
    lyrics_file(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::write(path, format!("{}\n", text.trim()))
    })
    .await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lyrics_files_are_imported_or_exported() {
        assert!(lyrics_file(Path::new("/x/Song.lrc")).is_ok());
        assert!(lyrics_file(Path::new("/x/Song.TXT")).is_ok());
        assert!(lyrics_file(Path::new("/x/Song.flac")).is_err());
        assert!(lyrics_file(Path::new("/x/.ssh/config")).is_err());
    }
}
