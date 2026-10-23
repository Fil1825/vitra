#!/bin/sh
# Records yt-dlp `-J` output for the parser tests in crates/links/tests/fixtures.
# Usage: scripts/record-ytdlp-fixtures.sh [path/to/yt-dlp]
# Needs network access to YouTube and SoundCloud.
#
# Recordings go next to the hand-written fixtures as `<name>-recorded.json`: the hand-written
# ones are checked for exact values, recordings for what doesn't depend on the sites' data,
# whatever track or album was recorded. A recording is only replaced when yt-dlp succeeds.
# Other links can be given with
#   VITRA_MUSIC_URL=https://music.youtube.com/watch?v=…   (a track of an "Artist - Topic" channel)
#   VITRA_ALBUM_URL=https://music.youtube.com/playlist?list=OLAK5uy_…
#   VITRA_ALBUM_PAGE_URL=https://music.youtube.com/browse/MPREb_…
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out="$here/crates/links/tests/fixtures"

# The fetched sidecars: on macOS yt-dlp is a folder (yt-dlp/yt-dlp_macos plus _internal/).
find_sidecar() { # name...
  for dir in "$here"/src-tauri/sidecars/*/; do
    for name in "$@"; do
      if [ -f "$dir$name" ] && [ -x "$dir$name" ]; then
        echo "$dir$name"
        return
      fi
    done
  done
}

ytdlp=${1:-$(find_sidecar yt-dlp/yt-dlp_macos yt-dlp yt-dlp.exe)}
ytdlp=${ytdlp:-$(command -v yt-dlp || true)}
if [ -z "$ytdlp" ]; then
  echo "No yt-dlp found: run npm run fetch-sidecars, or pass its path." >&2
  exit 1
fi
echo "yt-dlp: $ytdlp ($("$ytdlp" --version))"
# YouTube needs a JavaScript runtime; the app passes its Deno the same way.
deno=$(find_sidecar deno deno.exe)
run_ytdlp() {
  if [ -n "$deno" ]; then
    "$ytdlp" --no-warnings --ignore-config --js-runtimes "deno:$deno" "$@"
  else
    "$ytdlp" --no-warnings --ignore-config "$@"
  fi
}

failed=""
record() { # name url [extra yt-dlp args...]
  name=$1 url=$2
  shift 2
  target="$out/$name-recorded.json"
  echo "== $name: $url"
  raw=$(mktemp)
  # In the fixtures folder, so the final rename can't be seen half-written.
  trimmed=$(mktemp "$out/.$name.XXXXXX")
  if run_ytdlp -J "$@" -- "$url" > "$raw" &&
    python3 -c '
import json, sys
keep = ["_type", "id", "title", "track", "artist", "artists", "creators", "creator", "uploader",
        "channel", "album", "album_artist", "album_artists", "track_number", "playlist_index",
        "release_year", "upload_date", "duration", "thumbnail", "webpage_url", "original_url",
        "url", "ie_key", "extractor", "extractor_key", "is_live", "live_status",
        "playlist_count", "entries"]
def trim(d):
    out = {k: d[k] for k in keep if k in d}
    if out.get("entries") is not None:
        out["entries"] = [trim(e) for e in out["entries"] if e]
    return out
info = json.load(open(sys.argv[1], encoding="utf-8"))
json.dump(trim(info), sys.stdout, indent=2, ensure_ascii=False)
print()' "$raw" > "$trimmed"; then
    mv "$trimmed" "$target"
    echo "   saved $target"
  else
    rm -f "$trimmed"
    echo "   FAILED: $target left as it was" >&2
    failed="$failed $name"
  fi
  rm -f "$raw"
}

record youtube-video "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --no-playlist
# A track of an auto-generated "Artist - Topic" channel (from yt-dlp's own tests): it has
# music tags (track, artists, album). Official videos, like M83's, don't.
record youtube-music "${VITRA_MUSIC_URL:-https://music.youtube.com/watch?v=MgNrAu2pzNs}" --no-playlist
record soundcloud-track "https://soundcloud.com/forss/flickermood"
record soundcloud-set "https://soundcloud.com/forss/sets/soulhack" --flat-playlist

# By default an album from yt-dlp's own tests (an "OLAK5uy_" playlist and its album page).
album=${VITRA_ALBUM_URL:-https://music.youtube.com/playlist?list=OLAK5uy_l1m0thk3g31NmIIz_vMIbWtyv7eZixlH0}
page=${VITRA_ALBUM_PAGE_URL:-https://music.youtube.com/browse/MPREb_gTAcphH99wE}
record youtube-album "$album" --flat-playlist
record youtube-album-page "$page" --flat-playlist
# One of its tracks in full, as the app reads it while filling in an album.
if [ -f "$out/youtube-album-recorded.json" ]; then
  entry=$(python3 -c '
import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["entries"][0]["url"])' \
    "$out/youtube-album-recorded.json")
  record youtube-album-entry "$entry" --no-playlist
fi

if [ -n "$failed" ]; then
  echo "Not recorded:$failed" >&2
  exit 1
fi
echo "Done: run cargo test -p links, then commit the *-recorded.json files."
