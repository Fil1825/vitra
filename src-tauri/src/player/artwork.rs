//! Artwork of recent tracks, kept in memory and served to the WebView through the `artwork://`
//! URI scheme, so the WebView needs no file system access.

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex};

use media::PreparedArtwork;
use tauri::http::{Response, StatusCode, header};

pub const SCHEME: &str = "artwork";

/// Enough for the current track plus some history; each entry is at most ~1 MB.
const CAPACITY: usize = 16;
const THUMBNAIL_SUFFIX: &str = "-thumb";

#[derive(Default)]
pub struct ArtworkStore(Mutex<VecDeque<(String, Arc<PreparedArtwork>)>>);

impl ArtworkStore {
    /// Stores artwork and returns its id. Ids derive from the image bytes, so a whole album
    /// shares one entry and the WebView can cache it forever.
    pub fn insert(&self, artwork: PreparedArtwork) -> String {
        let mut hasher = DefaultHasher::new();
        artwork.image.bytes.hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish());

        let mut entries = self.lock();
        if let Some(index) = entries.iter().position(|(key, _)| *key == id) {
            // Most recently used goes to the back.
            if let Some(entry) = entries.remove(index) {
                entries.push_back(entry);
            }
        } else {
            entries.push_back((id.clone(), Arc::new(artwork)));
            while entries.len() > CAPACITY {
                entries.pop_front();
            }
        }
        id
    }

    /// Stores an image as it is, without the downscaled copy and the backdrop thumbnail (for
    /// the full-screen viewer); returns its id.
    pub fn insert_original(&self, image: media::Artwork) -> String {
        let mime = image
            .mime
            .clone()
            .or_else(|| media::image_mime(&image.bytes));
        self.insert(PreparedArtwork {
            image: media::Artwork {
                bytes: image.bytes,
                mime,
            },
            thumbnail: Vec::new(),
            palette: None,
        })
    }

    pub fn get(&self, id: &str) -> Option<Arc<PreparedArtwork>> {
        self.lock()
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, artwork)| Arc::clone(artwork))
    }

    /// Answers `artwork://<id>` (display image) and `artwork://<id>-thumb` (backdrop thumbnail).
    pub fn respond(&self, path: &str) -> Response<Vec<u8>> {
        let name = path.trim_start_matches('/');
        let (id, thumbnail) = match name.strip_suffix(THUMBNAIL_SUFFIX) {
            Some(id) => (id, true),
            None => (name, false),
        };
        let Some(artwork) = self.get(id) else {
            return status(StatusCode::NOT_FOUND);
        };
        let (bytes, mime) = if thumbnail {
            (artwork.thumbnail.clone(), "image/jpeg")
        } else {
            (
                artwork.image.bytes.clone(),
                artwork
                    .image
                    .mime
                    .as_deref()
                    .unwrap_or("application/octet-stream"),
            )
        };
        Response::builder()
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<(String, Arc<PreparedArtwork>)>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Path prefix of library album covers (`artwork://localhost/cover/<id>`), cached on disk.
pub const COVER_PREFIX: &str = "/cover/";

/// Serves a cached album cover from `dir`. Ids are 16 hex digits (see `library::cover_id`),
/// which also keeps the request from naming any other file.
pub fn respond_cover(dir: &Path, id: &str) -> Response<Vec<u8>> {
    if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return status(StatusCode::BAD_REQUEST);
    }
    match std::fs::read(dir.join(format!("{id}.jpg"))) {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

pub fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = code;
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use media::Artwork;

    fn artwork(byte: u8) -> PreparedArtwork {
        PreparedArtwork {
            image: Artwork {
                bytes: vec![byte; 4],
                mime: Some("image/png".into()),
            },
            thumbnail: vec![0xFF, 0xD8, byte],
            palette: None,
        }
    }

    #[test]
    fn serves_image_and_thumbnail() {
        let store = ArtworkStore::default();
        let id = store.insert(artwork(1));

        let full = store.respond(&format!("/{id}"));
        assert_eq!(full.status(), StatusCode::OK);
        assert_eq!(full.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(full.body(), &vec![1; 4]);

        let thumb = store.respond(&format!("/{id}-thumb"));
        assert_eq!(thumb.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(thumb.body(), &vec![0xFF, 0xD8, 1]);
    }

    #[test]
    fn same_image_same_id() {
        let store = ArtworkStore::default();
        assert_eq!(store.insert(artwork(7)), store.insert(artwork(7)));
        assert_ne!(store.insert(artwork(7)), store.insert(artwork(8)));
    }

    #[test]
    fn unknown_ids_are_not_found() {
        let store = ArtworkStore::default();
        assert_eq!(store.respond("/nope").status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn serves_cached_covers_by_hex_id_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("00112233aabbccdd.jpg"), [0xFF, 0xD8]).unwrap();
        let ok = respond_cover(dir.path(), "00112233aabbccdd");
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.body(), &vec![0xFF, 0xD8]);
        assert_eq!(
            respond_cover(dir.path(), "ffffffffffffffff").status(),
            StatusCode::NOT_FOUND
        );
        for bad in [
            "../../etc/passwd",
            "00112233aabbccd",
            "00112233aabbccdz",
            "",
        ] {
            assert_eq!(
                respond_cover(dir.path(), bad).status(),
                StatusCode::BAD_REQUEST
            );
        }
    }

    #[test]
    fn keeps_only_recent_entries() {
        let store = ArtworkStore::default();
        let first = store.insert(artwork(0));
        for byte in 1..=CAPACITY as u8 {
            store.insert(artwork(byte));
        }
        assert_eq!(
            store.respond(&format!("/{first}")).status(),
            StatusCode::NOT_FOUND
        );
    }
}
