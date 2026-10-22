#!/bin/sh
# Records yt-dlp `-J` output for the parser tests in crates/links/tests/fixtures.
# Usage: scripts/record-ytdlp-fixtures.sh [path/to/yt-dlp]
# Needs network access to YouTube and SoundCloud. Review the diff before committing: the
# tests assert on titles and artists, which may change on the sites.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
ytdlp=${1:-$(ls "$here"/src-tauri/sidecars/*/yt-dlp* 2>/dev/null | head -n 1)}
out="$here/crates/links/tests/fixtures"

record() { # name url [extra yt-dlp args...]
  name=$1 url=$2
  shift 2
  echo "== $name: $url"
  "$ytdlp" -J --no-warnings --ignore-config "$@" "$url" | python3 -c '
import json, sys
keep = ["_type", "id", "title", "track", "artist", "artists", "creators", "creator", "uploader",
        "channel", "album", "album_artist", "release_year", "upload_date", "duration", "thumbnail",
        "webpage_url", "original_url", "url", "ie_key", "extractor", "extractor_key", "is_live",
        "live_status", "playlist_count", "entries"]
def trim(d):
    out = {k: d[k] for k in keep if k in d}
    if out.get("entries") is not None:
        out["entries"] = [trim(e) for e in out["entries"] if e]
    return out
json.dump(trim(json.load(sys.stdin)), sys.stdout, indent=2, ensure_ascii=False)
print()' > "$out/$name.json"
}

record youtube-video "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --no-playlist
record youtube-music "https://music.youtube.com/watch?v=dX3k_QDnzHE" --no-playlist
record soundcloud-track "https://soundcloud.com/forss/flickermood"
record soundcloud-set "https://soundcloud.com/forss/sets/soulhack" --flat-playlist
echo "Done. A YouTube playlist fixture needs a public playlist URL of your choice:"
echo "  record youtube-playlist <url> --flat-playlist (edit this script)"
