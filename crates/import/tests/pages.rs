//! The page readers against pages written by hand in each service's shape (the cloud that
//! wrote them can't reach the services), and against real recordings when there are any
//! (`scripts/record-import-fixtures.sh` writes them as `*-recorded.*`).

use std::path::Path;

use import::links;
use import::pages::{self, PageError};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

#[test]
fn spotify_playlist() {
    let playlist = pages::spotify(&fixture("spotify-playlist.html")).unwrap();
    assert_eq!(playlist.name.as_deref(), Some("Today's Top Hits"));
    assert_eq!(playlist.tracks.len(), 3);
    let stay = &playlist.tracks[2];
    assert_eq!(stay.title, "STAY (with Justin Bieber)");
    assert_eq!(stay.artists, ["The Kid LAROI", "Justin Bieber"]);
    assert!((stay.duration.unwrap() - 141.805).abs() < 1e-9);
    assert_eq!(
        stay.url.as_deref(),
        Some("https://open.spotify.com/track/5PjdY0CKGZdEuoNab3yDmX")
    );
    assert!(playlist.cover_url.is_some());
    assert!(!playlist.partial);
}

#[test]
fn spotify_track() {
    let playlist = pages::spotify(&fixture("spotify-track.html")).unwrap();
    assert_eq!(playlist.tracks.len(), 1);
    assert_eq!(playlist.tracks[0].artists, ["Rick Astley"]);
}

#[test]
fn apple_music() {
    let playlist = pages::apple(&fixture("apple-playlist.html")).unwrap();
    assert_eq!(playlist.name.as_deref(), Some("Русский рок & классика"));
    assert_eq!(playlist.tracks.len(), 2);
    assert_eq!(playlist.tracks[1].title, "Выхода нет");
    assert_eq!(playlist.tracks[1].artists, ["Сплин"]);
    assert_eq!(playlist.tracks[1].duration, Some(237.0));

    // Without the app data, schema.org's list (with the album's artist).
    let album = pages::apple(&fixture("apple-album-schema.html")).unwrap();
    assert_eq!(album.name.as_deref(), Some("Discovery by Daft Punk"));
    assert_eq!(album.tracks.len(), 2);
    assert_eq!(album.tracks[0].artists, ["Daft Punk"]);
    assert_eq!(album.tracks[0].duration, Some(320.0));
}

#[test]
fn deezer() {
    let page = pages::deezer(&fixture("deezer-playlist.json")).unwrap();
    assert_eq!(page.playlist.name.as_deref(), Some("Rap Français"));
    assert_eq!(page.playlist.tracks.len(), 2);
    assert_eq!(page.playlist.tracks[1].album.as_deref(), Some("Discovery"));
    assert_eq!(page.playlist.tracks[0].duration, Some(326.0));
    assert!(page.next.is_some());
    assert_eq!(
        pages::deezer(&fixture("deezer-error.json")),
        Err(PageError::NotFound)
    );
}

#[test]
fn yandex() {
    let album = pages::yandex(&fixture("yandex-album.json")).unwrap();
    assert_eq!(album.name.as_deref(), Some("Группа крови"));
    assert_eq!(album.tracks.len(), 2);
    assert_eq!(album.tracks[1].title, "Закрой за мной дверь, я ухожу");
    assert_eq!(album.tracks[0].album.as_deref(), Some("Группа крови"));

    // The album's own title isn't a track (no length), its volumes' tracks are.
    assert_eq!(album.tracks[0].artists, ["Кино"]);
    assert!((album.tracks[0].duration.unwrap() - 285.72).abs() < 1e-9);
    assert!(!album.partial);

    let liked = pages::yandex(&fixture("yandex-playlist.json")).unwrap();
    assert_eq!(liked.name.as_deref(), Some("Мне нравится"));
    assert_eq!(liked.tracks.len(), 2);
    assert_eq!(liked.tracks[1].title, "Кукушка (Live)");
    assert_eq!(liked.tracks[1].album.as_deref(), Some("Последний концерт"));
    // Three tracks counted, two given.
    assert!(liked.partial);

    let track = pages::yandex(&fixture("yandex-track.json")).unwrap();
    assert_eq!(track.tracks.len(), 1);
    assert_eq!(track.tracks[0].title, "Закрой за мной дверь, я ухожу");

    // A list that isn't there (or isn't shown to visitors) answers with an error.
    assert_eq!(
        pages::yandex(&fixture("yandex-error.json")),
        Err(PageError::NotFound)
    );
    // The handlers of the old site now answer with the new site's page.
    assert!(matches!(
        pages::yandex("<!DOCTYPE html><html><body>music.yandex.ru</body></html>"),
        Err(PageError::Unreadable(_))
    ));
}

#[test]
fn read_goes_by_service() {
    let link = links::parse("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC").unwrap();
    assert_eq!(
        pages::read(&link, &fixture("spotify-track.html"))
            .unwrap()
            .tracks
            .len(),
        1
    );
    assert!(matches!(
        pages::read(&link, "<html>nothing</html>"),
        Err(PageError::Unreadable(_))
    ));
}

/// Real pages recorded on a machine that can reach the services: whatever their data, a
/// reader must find named tracks in them.
#[test]
fn recorded_pages() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.contains("-recorded.") {
            continue;
        }
        let body = std::fs::read_to_string(entry.path()).unwrap();
        let playlist = if name.starts_with("spotify") {
            pages::spotify(&body)
        } else if name.starts_with("apple") {
            pages::apple(&body)
        } else if name.starts_with("deezer") {
            pages::deezer(&body).map(|page| page.playlist)
        } else if name.starts_with("yandex") {
            pages::yandex(&body)
        } else {
            continue;
        }
        .unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(!playlist.tracks.is_empty(), "{name}: no tracks");
        for track in &playlist.tracks {
            assert!(!track.title.is_empty(), "{name}: a track without a title");
            assert!(
                !track.artists.is_empty(),
                "{name}: {} has no artist",
                track.title
            );
        }
    }
}
