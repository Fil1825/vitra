//! Playlist files: M3U / M3U8, XSPF, the Music app's XML and text exports, CSV from the
//! exporters, and plain text lists. A file's kind comes from its content first, its
//! extension second, since exporters name files loosely.

use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::model::{ImportedPlaylist, ImportedTrack, parse_duration};
use crate::{table, text};

/// Why a file couldn't be read.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FileError {
    #[error("the file has no tracks Vitra can read")]
    Empty,
    #[error("the file isn't a playlist: {0}")]
    Invalid(String),
}

/// Extensions of files the import reads.
pub const EXTENSIONS: &[&str] = &["m3u", "m3u8", "xspf", "xml", "csv", "tsv", "txt"];

/// The playlists in a file (the Music app's XML can hold several). `path` gives the name
/// when the file has none, and the folder relative entries are in.
pub fn read(path: &Path, bytes: &[u8]) -> Result<Vec<ImportedPlaylist>, FileError> {
    let content = lyrics::decode_text(bytes);
    let content = content.trim_start_matches('\u{feff}');
    let folder = path.parent().unwrap_or(Path::new(""));
    let stem = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let head: String = content.chars().take(512).collect::<String>().to_lowercase();
    let mut playlists =
        if head.starts_with("#extm3u") || matches!(extension.as_str(), "m3u" | "m3u8") {
            vec![m3u(content, folder)]
        } else if head.contains("<playlist") && head.contains("xspf") || extension == "xspf" {
            vec![xspf(content)?]
        } else if head.contains("<plist") {
            music_xml(bytes)?
        } else if let Some((tracks, name)) = table::parse_named(content) {
            vec![ImportedPlaylist {
                name,
                tracks: tracks.into_iter().map(from_music_text).collect(),
                ..ImportedPlaylist::default()
            }]
        } else {
            let tracks = text::parse(content)
                .into_iter()
                .map(|item| match item {
                    text::Item::Track(track) => track,
                    text::Item::Link(url) => ImportedTrack {
                        url: Some(url),
                        ..ImportedTrack::default()
                    },
                })
                .collect();
            vec![ImportedPlaylist {
                tracks,
                ..ImportedPlaylist::default()
            }]
        };
    playlists.retain(|playlist| !playlist.tracks.is_empty());
    if playlists.is_empty() {
        return Err(FileError::Empty);
    }
    if playlists.len() == 1 && playlists[0].name.is_none() {
        playlists[0].name = stem;
    }
    Ok(playlists)
}

/// A path the Music app's text export writes old style ("Macintosh HD:Users:me:Song.mp3").
fn from_music_text(mut track: ImportedTrack) -> ImportedTrack {
    if let Some(path) = &track.path
        && !path.starts_with('/')
        && !path.contains('\\')
        && let Some((_volume, rest)) = path.split_once(':')
    {
        track.path = Some(format!("/{}", rest.replace(':', "/")));
    }
    track
}

/// A file path or a link from a playlist line, relative paths taken from `folder`.
fn location(entry: &str, folder: &Path) -> (Option<String>, Option<String>) {
    let entry = entry.trim();
    let lower = entry.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return (None, Some(entry.to_owned()));
    }
    let path = if lower.starts_with("file://") {
        url::Url::parse(entry)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .unwrap_or_else(|| PathBuf::from(&entry[7..]))
    } else {
        let path = PathBuf::from(entry.replace('\\', std::path::MAIN_SEPARATOR_STR));
        if path.is_absolute() || entry.starts_with('/') {
            path
        } else {
            folder.join(path)
        }
    };
    (Some(path.to_string_lossy().into_owned()), None)
}

/// The file name without its extension, as a title for an entry with no other.
fn file_title(entry: &str) -> String {
    let name = entry.rsplit(['/', '\\']).next().unwrap_or(entry);
    let name = percent_decode(name);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 5 => stem.to_owned(),
        _ => name,
    }
}

/// `%20` and the like as the bytes they stand for.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) =
                (hex(bytes[i + 1]), bytes.get(i + 2).and_then(|b| hex(*b)))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// M3U / M3U8: `#EXTINF:seconds,Artist - Title` before each entry.
pub fn m3u(content: &str, folder: &Path) -> ImportedPlaylist {
    let mut playlist = ImportedPlaylist::default();
    let mut info: Option<(Option<f64>, String)> = None;
    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(name) = line.strip_prefix("#PLAYLIST:") {
            playlist.name = Some(name.trim().to_owned()).filter(|n| !n.is_empty());
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (seconds, title) = rest.split_once(',').unwrap_or((rest, ""));
            // "245 tvg-id=…": the length is the first word.
            let seconds = seconds.split_whitespace().next().unwrap_or_default();
            let duration = parse_duration(seconds).filter(|s| *s > 0.0);
            info = Some((duration, title.trim().to_owned()));
        } else if line.starts_with('#') {
            continue;
        } else {
            let (path, url) = location(line, folder);
            let (duration, label) = info.take().unwrap_or((None, String::new()));
            let mut track = match label.split_once(" - ") {
                Some((artist, title)) if !artist.trim().is_empty() => {
                    ImportedTrack::new(title.trim(), vec![artist.trim().to_owned()])
                }
                _ if !label.is_empty() => ImportedTrack::new(label, Vec::new()),
                _ => ImportedTrack::new(file_title(line), Vec::new()),
            };
            track.duration = duration;
            track.path = path;
            track.url = url;
            playlist.tracks.push(track);
        }
    }
    playlist
}

/// XSPF: `<track>` elements with location, title, creator, album and duration (ms).
pub fn xspf(content: &str) -> Result<ImportedPlaylist, FileError> {
    let mut reader = Reader::from_str(content);
    // Untrimmed: entities split the text, and the spaces around them count.
    reader.config_mut().trim_text(false);
    let mut playlist = ImportedPlaylist::default();
    let mut stack: Vec<String> = Vec::new();
    let mut track: Option<ImportedTrack> = None;
    let mut location_text: Option<String> = None;
    // An element's text, which entities (`&amp;`) split into several events.
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                let name = start.local_name().into_inner().to_lowercase();
                if name == "track" {
                    track = Some(ImportedTrack::default());
                    location_text = None;
                }
                stack.push(name);
                text.clear();
            }
            Ok(Event::Text(part)) => text.push_str(&part.xml10_content()),
            Ok(Event::GeneralRef(entity)) => {
                let raw = format!("&{};", entity.xml10_content());
                text.push_str(
                    &quick_xml::escape::unescape(&raw).map_or(raw.clone(), |t| t.into_owned()),
                );
            }
            Ok(Event::End(_)) => {
                let name = stack.pop().unwrap_or_default();
                let parent = stack.last().map(String::as_str);
                let value = std::mem::take(&mut text).trim().to_owned();
                match (&mut track, name.as_str()) {
                    (Some(t), "title") => t.title = value,
                    (Some(t), "creator") if !value.is_empty() => t.artists = vec![value],
                    (Some(t), "album") if !value.is_empty() => t.album = Some(value),
                    (Some(t), "duration") => {
                        t.duration = value.parse::<f64>().ok().map(|ms| ms / 1000.0);
                    }
                    (Some(_), "location") => location_text = Some(value),
                    (Some(_), "track") => {
                        if let Some(mut done) = track.take() {
                            if let Some(entry) = location_text.take() {
                                let (path, url) = location(&entry, Path::new(""));
                                if done.title.is_empty() {
                                    done.title = file_title(&entry);
                                }
                                done.path = path;
                                done.url = url;
                            }
                            if !done.title.is_empty() {
                                playlist.tracks.push(done);
                            }
                        }
                    }
                    (None, "title") if parent == Some("playlist") && !value.is_empty() => {
                        playlist.name = Some(value);
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(FileError::Invalid(err.to_string())),
            _ => {}
        }
    }
    Ok(playlist)
}

/// The Music app's (and iTunes') XML export: a plist with every track and the playlists.
pub fn music_xml(bytes: &[u8]) -> Result<Vec<ImportedPlaylist>, FileError> {
    let value =
        plist::Value::from_reader_xml(bytes).map_err(|err| FileError::Invalid(err.to_string()))?;
    let root = value
        .as_dictionary()
        .ok_or_else(|| FileError::Invalid("no dictionary".into()))?;
    let tracks = root.get("Tracks").and_then(plist::Value::as_dictionary);
    let track = |id: i64| -> Option<ImportedTrack> {
        let entry = tracks?.get(&id.to_string())?.as_dictionary()?;
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(plist::Value::as_string)
                .map(str::to_owned)
        };
        let title = text("Name")?;
        let path = text("Location").and_then(|location| location_path(&location));
        Some(ImportedTrack {
            album: text("Album"),
            duration: entry
                .get("Total Time")
                .and_then(plist::Value::as_signed_integer)
                .map(|ms| ms as f64 / 1000.0),
            path,
            ..ImportedTrack::new(title, text("Artist").into_iter().collect())
        })
    };
    let mut playlists = Vec::new();
    for entry in root
        .get("Playlists")
        .and_then(plist::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(plist::Value::as_dictionary)
    {
        // The whole library and the built-in lists (Music, Movies, Podcasts…).
        let built_in = entry.get("Master").is_some()
            || entry.get("Distinguished Kind").is_some()
            || entry.get("Folder").and_then(plist::Value::as_boolean) == Some(true);
        if built_in {
            continue;
        }
        let tracks: Vec<ImportedTrack> = entry
            .get("Playlist Items")
            .and_then(plist::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_dictionary()?.get("Track ID")?.as_signed_integer())
            .filter_map(track)
            .collect();
        playlists.push(ImportedPlaylist {
            name: entry
                .get("Name")
                .and_then(plist::Value::as_string)
                .map(str::to_owned),
            tracks,
            ..ImportedPlaylist::default()
        });
    }
    Ok(playlists)
}

/// A `file://` URL as a path.
fn location_path(location: &str) -> Option<String> {
    let url = url::Url::parse(location).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    Some(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3u_with_extinf_relative_paths_and_links() {
        let content = "#EXTM3U\n#PLAYLIST:Road Trip\n\
                       #EXTINF:329,Massive Attack - Teardrop\n\
                       Music/Massive Attack/Teardrop.flac\n\
                       /abs/Song%20Two.mp3\n\
                       #EXTINF:-1,Radio\n\
                       https://example.com/stream.mp3\n";
        let folder = Path::new(if cfg!(windows) {
            r"C:\Users\me"
        } else {
            "/home/me"
        });
        let playlist = m3u(content, folder);
        assert_eq!(playlist.name.as_deref(), Some("Road Trip"));
        assert_eq!(playlist.tracks.len(), 3);
        let first = &playlist.tracks[0];
        assert_eq!(first.title, "Teardrop");
        assert_eq!(first.artists, ["Massive Attack"]);
        assert_eq!(first.duration, Some(329.0));
        assert_eq!(
            first.path.as_ref().map(PathBuf::from),
            Some(folder.join("Music/Massive Attack/Teardrop.flac"))
        );
        assert_eq!(playlist.tracks[1].title, "Song Two");
        assert_eq!(playlist.tracks[2].duration, None);
        assert_eq!(
            playlist.tracks[2].url.as_deref(),
            Some("https://example.com/stream.mp3")
        );
    }

    #[test]
    fn xspf_tracks() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<playlist version="1" xmlns="http://xspf.org/ns/0/">
  <title>Evening &amp; Night</title>
  <trackList>
    <track>
      <location>file:///music/Teardrop.flac</location>
      <title>Teardrop</title>
      <creator>Massive Attack</creator>
      <album>Mezzanine</album>
      <duration>329000</duration>
    </track>
    <track><location>file:///music/Unknown%20Song.mp3</location></track>
  </trackList>
</playlist>"#;
        let playlist = xspf(content).unwrap();
        assert_eq!(playlist.name.as_deref(), Some("Evening & Night"));
        assert_eq!(playlist.tracks.len(), 2);
        assert_eq!(playlist.tracks[0].artists, ["Massive Attack"]);
        assert_eq!(playlist.tracks[0].duration, Some(329.0));
        assert_eq!(
            playlist.tracks[0].path.as_deref(),
            Some("/music/Teardrop.flac")
        );
        assert_eq!(playlist.tracks[1].title, "Unknown Song");
    }

    #[test]
    fn music_app_xml() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Tracks</key><dict>
    <key>101</key><dict>
      <key>Track ID</key><integer>101</integer>
      <key>Name</key><string>Группа крови</string>
      <key>Artist</key><string>Кино</string>
      <key>Album</key><string>Группа крови</string>
      <key>Total Time</key><integer>285000</integer>
      <key>Location</key><string>file:///Users/me/Music/Kino/01%20Gruppa.mp3</string>
    </dict>
  </dict>
  <key>Playlists</key><array>
    <dict><key>Name</key><string>Library</string><key>Master</key><true/>
      <key>Playlist Items</key><array><dict><key>Track ID</key><integer>101</integer></dict></array></dict>
    <dict><key>Name</key><string>Русский рок</string>
      <key>Playlist Items</key><array><dict><key>Track ID</key><integer>101</integer></dict></array></dict>
  </array>
</dict></plist>"#;
        let expected_path = if cfg!(windows) {
            r"C:\Users\me\Music\Kino\01 Gruppa.mp3"
        } else {
            "/Users/me/Music/Kino/01 Gruppa.mp3"
        };
        let location = url::Url::from_file_path(expected_path).unwrap();
        let content = content.replace(
            "file:///Users/me/Music/Kino/01%20Gruppa.mp3",
            location.as_str(),
        );
        let playlists = read(Path::new("Library.xml"), content.as_bytes()).unwrap();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name.as_deref(), Some("Русский рок"));
        let track = &playlists[0].tracks[0];
        assert_eq!(track.title, "Группа крови");
        assert_eq!(track.duration, Some(285.0));
        assert_eq!(track.path.as_deref(), Some(expected_path));
    }

    #[test]
    fn music_app_text_export_in_utf16() {
        let text = "Name\tArtist\tComposer\tAlbum\tTime\tLocation\r\n\
                    Teardrop\tMassive Attack\t\tMezzanine\t329\tMacintosh HD:Users:me:Music:Teardrop.m4a\r\n";
        let mut bytes = vec![0xff, 0xfe];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let playlists = read(Path::new("/tmp/Evening.txt"), &bytes).unwrap();
        assert_eq!(playlists[0].name.as_deref(), Some("Evening"));
        let track = &playlists[0].tracks[0];
        assert_eq!(track.artists, ["Massive Attack"]);
        assert_eq!(track.duration, Some(329.0));
        assert_eq!(track.path.as_deref(), Some("/Users/me/Music/Teardrop.m4a"));
    }

    #[test]
    fn plain_text_files_and_empty_ones() {
        let playlists = read(
            Path::new("list.txt"),
            "Daft Punk - One More Time\n".as_bytes(),
        )
        .unwrap();
        assert_eq!(playlists[0].name.as_deref(), Some("list"));
        assert_eq!(playlists[0].tracks[0].title, "One More Time");
        assert_eq!(
            read(Path::new("x.m3u"), b"#EXTM3U\n"),
            Err(FileError::Empty)
        );
    }
}
