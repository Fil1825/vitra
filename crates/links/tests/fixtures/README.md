# yt-dlp fixtures

`generic-*.json` were recorded from yt-dlp's generic extractor against a local test page
(`tests/real_ytdlp.rs` builds the same pages).

`youtube-*.json` and `soundcloud-*.json` were written by hand in yt-dlp's `-J` format, because
the cloud environment that wrote them cannot reach those sites. Replace them with real
recordings when you can:

```sh
scripts/record-ytdlp-fixtures.sh
```

The script keeps only the fields the parser reads, so recordings stay small and stable.

Albums are recorded next to the hand-written ones, as `youtube-album-recorded.json` (an
`OLAK5uy_` playlist), `youtube-album-page-recorded.json` (its `music.youtube.com/browse/MPREb…`
page) and `youtube-album-entry-recorded.json` (one of its tracks in full): the hand-written
album has a private video and a second artist, which a real album may not, and its tests check
exact values. The parser tests check every recording they find, whatever album it is
(`VITRA_ALBUM_URL` and `VITRA_ALBUM_PAGE_URL` pick another one).
