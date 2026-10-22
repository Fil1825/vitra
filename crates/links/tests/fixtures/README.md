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
