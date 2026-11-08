#!/usr/bin/env bash
# Build Vitra's Intel macOS mpv from pinned sources (deployment target 12.0).
# Build tools may come from Homebrew; shipped libraries must not.
# Usage: build-intel-mpv.sh <sidecar-output-dir>
set -euo pipefail

OUT=${1:?output directory required}
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PINS="$SCRIPT_DIR/intel-mpv-sources.json"
WORK=$(mktemp -d)
PREFIX="$WORK/prefix"
SRC="$WORK/src"
OFFER="$OUT/mpv-corresponding-source"
trap 'rm -rf "$WORK"' EXIT

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "x86_64" ]]; then
  echo "Intel mpv source build requires macOS x86_64 (got $(uname -s) $(uname -m))" >&2
  exit 1
fi

export MACOSX_DEPLOYMENT_TARGET=12.0
export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
export CFLAGS="-arch x86_64 -mmacosx-version-min=12.0"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="-arch x86_64 -mmacosx-version-min=12.0"
# Shipped libraries come only from the prefix below. Drop inherited
# Homebrew search paths before any configure or Meson setup.
unset PKG_CONFIG_PATH CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH OBJC_INCLUDE_PATH LIBRARY_PATH CMAKE_PREFIX_PATH
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export PATH="$PREFIX/bin:$PATH"

if ! command -v brew >/dev/null; then
  echo "Homebrew is required for build tools (meson, nasm), not for shipped libraries" >&2
  exit 1
fi
export HOMEBREW_NO_AUTO_UPDATE=1
mkdir -p "$PREFIX/lib/pkgconfig"
brew install -q nasm meson ninja pkgconf autoconf automake libtool python3
# Fail before compiling if pkg-config can still see host libraries.
if pkg-config --exists fontconfig \
  || pkg-config --exists libpng \
  || pkg-config --exists harfbuzz \
  || pkg-config --exists freetype2 \
  || pkg-config --exists libass; then
  echo "pkg-config sees libraries outside the source prefix; refusing to build" >&2
  exit 1
fi

mkdir -p "$SRC" "$PREFIX" "$OFFER/tarballs" "$OFFER/licenses"
cp "$PINS" "$OFFER/intel-mpv-sources.json"
cp "$SCRIPT_DIR/build-intel-mpv.sh" "$OFFER/build-intel-mpv.sh"
cp "$SCRIPT_DIR/mach-minos.py" "$OFFER/mach-minos.py"
cp "$SCRIPT_DIR/mach-deps.py" "$OFFER/mach-deps.py"

unpack() {
  local archive="$1" dest="$2"
  rm -rf "$dest"
  mkdir -p "$dest"
  tar -xf "$archive" -C "$dest" --strip-components=1
}

python3 - "$PINS" "$SRC" <<'PY'
import hashlib, json, pathlib, subprocess, sys
pins = json.loads(pathlib.Path(sys.argv[1]).read_text())
src = pathlib.Path(sys.argv[2])
items = [("mpv.tar.gz", pins["mpv"]["url"], pins["mpv"]["sha256"])]
for dep in pins["dependencies"]:
    items.append((f"{dep['name']}.tar", dep["url"], dep["sha256"]))
    for sub in dep.get("submodules", []):
        items.append((f"{dep['name']}-{sub['name']}.tar.gz", sub["url"], sub["sha256"]))
for name, url, sha in items:
    dest = src / name
    print(f"fetch {name}", flush=True)
    subprocess.check_call(["curl", "-fsSL", "--retry", "5", "--retry-all-errors", "-o", str(dest), url])
    digest = hashlib.sha256(dest.read_bytes()).hexdigest()
    if digest != sha:
        raise SystemExit(f"checksum mismatch for {name}: expected {sha}, got {digest}")
PY
cp -a "$SRC/." "$OFFER/tarballs/"

unpack "$SRC/mpv.tar.gz" "$WORK/mpv"
unpack "$SRC/ffmpeg.tar" "$WORK/ffmpeg"
unpack "$SRC/libplacebo.tar" "$WORK/libplacebo"
unpack "$SRC/freetype.tar" "$WORK/freetype"
unpack "$SRC/fribidi.tar" "$WORK/fribidi"
unpack "$SRC/harfbuzz.tar" "$WORK/harfbuzz"
unpack "$SRC/libass.tar" "$WORK/libass"
python3 - "$PINS" "$SRC" "$WORK/libplacebo" <<'PY'
import json, pathlib, subprocess, sys
pins = json.loads(pathlib.Path(sys.argv[1]).read_text())
src, tree = pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
for dep in pins["dependencies"]:
    if dep["name"] != "libplacebo":
        continue
    for sub in dep.get("submodules", []):
        dest = tree / sub["path"]
        archive = src / f"libplacebo-{sub['name']}.tar.gz"
        dest.mkdir(parents=True, exist_ok=True)
        subprocess.check_call(["tar", "-xf", str(archive), "-C", str(dest), "--strip-components=1"])
PY

python3 - "$WORK" "$OFFER/licenses" <<'PY'
import pathlib, shutil, sys
work, dest = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
dest.mkdir(parents=True, exist_ok=True)
for tree in ("freetype", "fribidi", "harfbuzz", "libass", "ffmpeg", "libplacebo", "mpv"):
    root = work / tree
    for path in root.rglob("*"):
        if path.is_file() and path.name.lower().startswith(("license", "copying")) and path.stat().st_size < 2_000_000:
            shutil.copy(path, dest / f"{tree}-{path.name}")
            break
PY

(
  cd "$WORK/freetype"
  # Internal zlib only: do not link a host or Homebrew zlib.
  ./configure --prefix="$PREFIX" --enable-shared --disable-static \
    --with-zlib=no --with-png=no --with-harfbuzz=no --with-brotli=no --with-bzip2=no
  make -j"$(sysctl -n hw.ncpu)"
  make install
)
(
  cd "$WORK/fribidi"
  ./configure --prefix="$PREFIX" --enable-shared --disable-static
  make -j"$(sysctl -n hw.ncpu)"
  make install
)
(
  cd "$WORK/harfbuzz"
  # 10.1.0 ships Meson, not Autotools.
  meson setup build --prefix="$PREFIX" --buildtype=release --default-library=shared \
    -Dfreetype=enabled -Dglib=disabled -Dgobject=disabled -Dcairo=disabled \
    -Dchafa=disabled -Dicu=disabled -Dgraphite2=disabled \
    -Dtests=disabled -Ddocs=disabled -Dintrospection=disabled -Dutilities=disabled \
    -Dc_args="$CFLAGS" -Dcpp_args="$CXXFLAGS" -Dc_link_args="$LDFLAGS"
  meson compile -C build
  meson install -C build
)
(
  cd "$WORK/libass"
  ./configure --prefix="$PREFIX" --enable-shared --disable-static \
    --disable-fontconfig --enable-coretext
  make -j"$(sysctl -n hw.ncpu)"
  make install
)
(
  cd "$WORK/ffmpeg"
  # 7.1.1 with --disable-autodetect enables libc_iconv and only probes iconv in
  # libc (configure: "disabled iconv || enable libc_iconv"). That probe links on
  # the macOS 15 Intel runner, but libavcodec's shared link does not: library.mak
  # puts LDFLAGS before the objects and FFEXTRALIBS after them, and iconv_extralibs
  # stays empty. --extra-libs=-liconv is what reaches that link. Do not disable iconv.
  ./configure --prefix="$PREFIX" --enable-shared --disable-static \
    --arch=x86_64 --target-os=darwin \
    --extra-cflags="$CFLAGS" --extra-ldflags="$LDFLAGS" \
    --disable-autodetect \
    --enable-gpl --enable-version3 \
    --disable-doc --disable-debug --disable-programs \
    --enable-videotoolbox --enable-audiotoolbox \
    --enable-zlib --enable-iconv \
    --extra-libs=-liconv \
    --disable-bzlib --disable-lzma --disable-sdl2
  make -j"$(sysctl -n hw.ncpu)"
  make install
)
(
  cd "$WORK/libplacebo"
  meson setup build --prefix="$PREFIX" --buildtype=release \
    -Ddemos=false -Dtests=false \
    -Dvulkan=disabled -Dshaderc=disabled -Dglslang=disabled -Dd3d11=disabled \
    -Dopengl=enabled -Dlcms=disabled -Dlibdovi=disabled -Dxxhash=disabled \
    -Dc_args="$CFLAGS" -Dcpp_args="$CXXFLAGS" -Dc_link_args="$LDFLAGS"
  meson compile -C build
  meson install -C build
)
(
  cd "$WORK/mpv"
  # dependency('iconv') uses the same libc probe as FFmpeg. -liconv stays on the link.
  meson setup build --prefix="$PREFIX" --buildtype=release \
    -Dlibmpv=false \
    -D{gl,iconv,plain-gl}=enabled \
    -D{cocoa,coreaudio,gl-cocoa,videotoolbox-gl,videotoolbox-pl}=enabled \
    -D{swift-build,macos-cocoa-cb,macos-media-player}=enabled \
    -D{vulkan,macos-touchbar,lua,jpeg,lcms2}=disabled \
    -D{cdda,dvdnav,javascript,libarchive,libbluray,rubberband,uchardet,vapoursynth,zimg,shaderc,wayland,x11,manpage-build}=disabled \
    -Dswift-flags="-target x86_64-apple-macos12.0" \
    -Dc_args="$CFLAGS" -Dobjc_args="$CFLAGS -Wno-error=deprecated -Wno-error=deprecated-declarations" \
    -Dc_link_args="$LDFLAGS -liconv"
  meson compile -C build
  meson compile -C build macos-bundle
)
rm -rf "$OUT/mpv"
mkdir -p "$OUT/mpv"
cp -a "$WORK/mpv/build/mpv.app" "$OUT/mpv/mpv.app"
python3 "$SCRIPT_DIR/mach-minos.py" "$OUT/mpv/mpv.app" --max 12.0
python3 "$SCRIPT_DIR/mach-deps.py" "$OUT/mpv/mpv.app"

python3 - "$PINS" "$OFFER/build-record.json" <<'PY'
import json, pathlib, subprocess, sys
pins = json.loads(pathlib.Path(sys.argv[1]).read_text())
def show(cmd):
    try:
        return subprocess.check_output(cmd, text=True, stderr=subprocess.STDOUT).strip()
    except Exception as err:
        return f"unavailable: {err}"
record = {
    "deploymentTarget": pins["deploymentTarget"],
    "arch": pins["arch"],
    "recipe": "scripts/release/build-intel-mpv.sh",
    "clang": show(["clang", "--version"]),
    "xcode": show(["xcodebuild", "-version"]),
    "sdk": show(["xcrun", "--sdk", "macosx", "--show-sdk-path"]),
    "sources": pins,
}
pathlib.Path(sys.argv[2]).write_text(json.dumps(record, indent=2) + "\n")
PY
tar -C "$OUT" -czf "$OUT/mpv-corresponding-source.tar.gz" mpv-corresponding-source
echo "Intel mpv source build installed at $OUT/mpv/mpv.app"
