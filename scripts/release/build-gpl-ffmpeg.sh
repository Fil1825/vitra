#!/usr/bin/env bash
# GPL ffmpeg and ffprobe from the pinned FFmpeg 7.1.1 release tarball.
# HTTPS uses the pinned mbedTLS 3.6.7 release (Apache-2.0 OR GPL-2.0-or-later).
# MP3 uses the pinned LAME 3.100 release tarball (LGPL-2.0-or-later), linked
# statically. libiconv, aom, ffnvcodec, opencore-amr, soxr, and xvid are not
# configured: those archives are not stable. An earlier GPL build used lame
# svn r7113, which is not this release tarball.
# x86 assembly stays enabled. The CI and release jobs install nasm.
# Usage: build-gpl-ffmpeg.sh <sidecar-output-dir>
set -euo pipefail

OUT=${1:?output directory required}
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PINS="${VITRA_MPV_SOURCES:-$SCRIPT_DIR/gpl-ffmpeg-sources.json}"
WORK=$(mktemp -d)
PREFIX="$WORK/prefix"
OFFER="$OUT/mpv-corresponding-source"
trap 'rm -rf "$WORK"' EXIT

if [[ -x "$OUT/ffmpeg" && -x "$OUT/ffprobe" && -f "$OUT/mpv-corresponding-source.tar.gz" ]]; then
  exit 0
fi
if [[ "$(uname -s)" != "Linux" ]]; then
  echo "GPL ffmpeg source build requires Linux (got $(uname -s))" >&2
  exit 1
fi

mkdir -p "$PREFIX" "$OFFER/tarballs" "$OFFER/licenses"
cp "$PINS" "$OFFER/$(basename "$PINS")"
cp "$SCRIPT_DIR/build-gpl-ffmpeg.sh" "$OFFER/build-gpl-ffmpeg.sh"

python3 - "$PINS" "$WORK" <<'PY'
import hashlib, json, pathlib, subprocess, sys
pins = json.loads(pathlib.Path(sys.argv[1]).read_text())
work = pathlib.Path(sys.argv[2])
for name, filename in (("ffmpeg", "ffmpeg.tar.xz"), ("mbedtls", "mbedtls.tar.bz2"), ("lame", "lame.tar.gz")):
    spec = pins[name]
    dest = work / filename
    print(f"fetch {name}", flush=True)
    subprocess.check_call(["curl", "-fsSL", "--retry", "5", "--retry-all-errors", "-o", str(dest), spec["url"]])
    digest = hashlib.sha256(dest.read_bytes()).hexdigest()
    if digest != spec["sha256"]:
        raise SystemExit(f"checksum mismatch for {name}: expected {spec['sha256']}, got {digest}")
PY
cp "$WORK/ffmpeg.tar.xz" "$OFFER/tarballs/ffmpeg.tar.xz"
cp "$WORK/mbedtls.tar.bz2" "$OFFER/tarballs/mbedtls.tar.bz2"
cp "$WORK/lame.tar.gz" "$OFFER/tarballs/lame.tar.gz"
mkdir -p "$WORK/ffmpeg" "$WORK/mbedtls" "$WORK/lame"
tar -xf "$WORK/ffmpeg.tar.xz" -C "$WORK/ffmpeg" --strip-components=1
tar -xf "$WORK/mbedtls.tar.bz2" -C "$WORK/mbedtls" --strip-components=1
tar -xf "$WORK/lame.tar.gz" -C "$WORK/lame" --strip-components=1
license=$(find "$WORK/ffmpeg" -maxdepth 2 -iname 'COPYING*' -o -iname 'LICENSE*' | head -n 1)
if [[ -n "$license" ]]; then
  cp "$license" "$OFFER/licenses/ffmpeg-$(basename "$license")"
fi
cp "$WORK/mbedtls/LICENSE" "$OFFER/licenses/mbedtls-LICENSE"
cp "$WORK/lame/COPYING" "$OFFER/licenses/lame-COPYING"

JOBS="$(nproc)"
cmake -S "$WORK/mbedtls" -B "$WORK/mbedtls/build" -G "Unix Makefiles" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DUSE_SHARED_MBEDTLS_LIBRARY=OFF \
  -DUSE_STATIC_MBEDTLS_LIBRARY=ON \
  -DENABLE_PROGRAMS=OFF \
  -DENABLE_TESTING=OFF \
  -DMBEDTLS_FATAL_WARNINGS=OFF
cmake --build "$WORK/mbedtls/build" -j"$JOBS"
cmake --install "$WORK/mbedtls/build"
(
  cd "$WORK/lame"
  # Static only. The frontend does not ship, and nasm objects are not needed
  # for the encoder FFmpeg links. -fcommon keeps gcc 10+ from rejecting the
  # 3.100 tentative definitions.
  CFLAGS="${CFLAGS:-} -O2 -fcommon" \
    ./configure --prefix="$PREFIX" --disable-shared --enable-static \
      --disable-frontend --disable-gtktest --disable-nasm
  make -j"$JOBS"
  make install
)
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"

(
  cd "$WORK/ffmpeg"
  ./configure --prefix="$PREFIX" --disable-shared --enable-static \
    --disable-autodetect \
    --enable-gpl --enable-version3 \
    --enable-mbedtls --enable-libmp3lame --pkg-config-flags=--static \
    --extra-cflags="-I$PREFIX/include" --extra-ldflags="-L$PREFIX/lib" \
    --disable-doc --disable-debug \
    --disable-programs --enable-ffmpeg --enable-ffprobe \
    --disable-iconv --disable-zlib --disable-bzlib --disable-lzma --disable-sdl2
  make -j"$JOBS"
  make install
)
cp "$PREFIX/bin/ffmpeg" "$PREFIX/bin/ffprobe" "$OUT/"
# Do not pipe this into grep -q. pipefail plus grep -q closes the pipe as
# soon as it sees libmp3lame, ffmpeg dies with SIGPIPE, and the check
# reports the encoder missing even though it was just compiled.
encoders=$("$OUT/ffmpeg" -hide_banner -encoders) || {
  echo "sidecar ffmpeg failed to list encoders" >&2
  exit 1
}
if ! grep -q libmp3lame <<<"$encoders"; then
  echo "sidecar ffmpeg is missing the libmp3lame encoder" >&2
  exit 1
fi
python3 - "$PINS" "$OFFER/build-record.json" <<'PY'
import json, pathlib, sys
pins = json.loads(pathlib.Path(sys.argv[1]).read_text())
pathlib.Path(sys.argv[2]).write_text(json.dumps({
    "recipe": "scripts/release/build-gpl-ffmpeg.sh",
    "programs": ["ffmpeg", "ffprobe"],
    "omittedLibraries": pins.get("omittedLibraries", []),
    "sources": pins,
}, indent=2) + "\n")
PY
tar -C "$OUT" -czf "$OUT/mpv-corresponding-source.tar.gz" mpv-corresponding-source
rm -rf "$OFFER"
echo "GPL ffmpeg source build installed at $OUT"
