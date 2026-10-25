#!/bin/sh
# Records real music service pages for the playlist import tests in crates/import/tests.
# Usage: scripts/record-import-fixtures.sh
# Needs network access to Spotify, Apple Music, Deezer and Yandex Music (the app reads the
# same pages and APIs: crates/import/src/pages.rs builds the same requests).
#
# Recordings go next to the hand-written fixtures as `<name>-recorded.<ext>`; the test
# `recorded_pages` reads every one and checks that tracks with titles and artists come out,
# whatever the list holds today. A recording is only replaced when the page came back.
# Other lists can be given with
#   VITRA_SPOTIFY_PLAYLIST=<id>  VITRA_SPOTIFY_ALBUM=<id>
#   VITRA_APPLE_PLAYLIST=https://music.apple.com/…  VITRA_APPLE_ALBUM=https://music.apple.com/…
#   VITRA_DEEZER_PLAYLIST=<id>  VITRA_DEEZER_ALBUM=<id>
#   VITRA_YANDEX_ALBUM=<id>  VITRA_YANDEX_OWNER=<login> VITRA_YANDEX_KIND=<n>  VITRA_YANDEX_TRACK=<id>
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out="$here/crates/import/tests/fixtures"
# What src-tauri/src/http.rs sends: some services strip their pages for unknown clients.
agent="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15"

failed=""
record() { # name ext url [header...]
  name=$1 ext=$2 url=$3
  shift 3
  target="$out/$name-recorded.$ext"
  temp="$target.part"
  set -- -sS -L --compressed --max-time 30 -A "$agent" \
    -H "Accept-Language: en-US,en;q=0.9,ru;q=0.8" "$@"
  # curl writes 000 itself when nothing came back.
  status=$(curl "$@" -o "$temp" -w '%{http_code}' -- "$url") || true
  if [ "$status" = 200 ] && [ -s "$temp" ]; then
    mv "$temp" "$target"
    echo "recorded $name ($(wc -c <"$target" | tr -d ' ') bytes)"
  else
    rm -f "$temp"
    echo "FAILED $name: HTTP $status from $url" >&2
    failed="$failed $name"
  fi
}

# Spotify: the embed pages (the open.spotify.com pages need a session).
record spotify-playlist html \
  "https://open.spotify.com/embed/playlist/${VITRA_SPOTIFY_PLAYLIST:-37i9dQZF1DXcBWIGoYBM5M}"
record spotify-album html \
  "https://open.spotify.com/embed/album/${VITRA_SPOTIFY_ALBUM:-2noRn2Aes5aoNVsU6iWThc}"

# Apple Music: the public web pages.
record apple-playlist html \
  "${VITRA_APPLE_PLAYLIST:-https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb}"
record apple-album html \
  "${VITRA_APPLE_ALBUM:-https://music.apple.com/us/album/discovery/697194953}"

# Deezer: its open API (first page of a playlist).
record deezer-playlist json \
  "https://api.deezer.com/playlist/${VITRA_DEEZER_PLAYLIST:-3155776842}"
record deezer-album json "https://api.deezer.com/album/${VITRA_DEEZER_ALBUM:-302127}"

# Yandex Music: its public API (the old site's handlers now answer with the new site's
# HTML). The track is the album's first unless one is given.
api=https://api.music.yandex.net
album=${VITRA_YANDEX_ALBUM:-3389008}
record yandex-album json "$api/albums/$album/with-tracks"
record yandex-playlist json \
  "$api/users/${VITRA_YANDEX_OWNER:-music-blog}/playlists/${VITRA_YANDEX_KIND:-2075}"
track=${VITRA_YANDEX_TRACK:-}
if [ -z "$track" ] && [ -f "$out/yandex-album-recorded.json" ]; then
  track=$(grep -o '"realId":"[0-9]*"' "$out/yandex-album-recorded.json" | head -n 1 | tr -dc 0-9)
fi
if [ -n "$track" ]; then
  record yandex-track json "$api/tracks/$track"
fi

echo
echo "Now: cargo test -p import --test pages recorded_pages"
if [ -n "$failed" ]; then
  echo "Not recorded:$failed" >&2
  exit 1
fi
