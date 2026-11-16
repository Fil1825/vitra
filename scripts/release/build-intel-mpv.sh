#!/usr/bin/env bash
# Build Vitra's macOS mpv, ffmpeg, and ffprobe from pinned sources.
# x86_64 uses deployment target 12.0. arm64 uses deployment target 14.0.
# The pins file is $VITRA_MPV_SOURCES, or intel-mpv-sources.json when unset.
# Build tools may come from Homebrew; shipped libraries must not.
# Usage: build-intel-mpv.sh <sidecar-output-dir>
set -euo pipefail

OUT=${1:?output directory required}
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PINS="${VITRA_MPV_SOURCES:-$SCRIPT_DIR/intel-mpv-sources.json}"
WORK=$(mktemp -d)
PREFIX="$WORK/prefix"
SRC="$WORK/src"
OFFER="$OUT/mpv-corresponding-source"
trap 'rm -rf "$WORK"' EXIT

if [[ -x "$OUT/ffmpeg" && -x "$OUT/ffprobe" && -x "$OUT/mpv/mpv.app/Contents/MacOS/mpv" && -f "$OUT/mpv-corresponding-source.tar.gz" ]]; then
  exit 0
fi

eval "$(python3 - "$PINS" <<'PY'
import json, pathlib, shlex, sys
pins = json.loads(pathlib.Path(sys.argv[1]).read_text())
print(f"ARCH={shlex.quote(pins['arch'])}")
print(f"DEPLOYMENT_TARGET={shlex.quote(pins['deploymentTarget'])}")
PY
)"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "$ARCH" ]]; then
  echo "macOS mpv source build requires Darwin $ARCH (got $(uname -s) $(uname -m))" >&2
  exit 1
fi

export MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"
export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
export CFLAGS="-arch $ARCH -mmacosx-version-min=$DEPLOYMENT_TARGET"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="-arch $ARCH -mmacosx-version-min=$DEPLOYMENT_TARGET"
# Shipped libraries come only from the prefix below. Drop inherited
# Homebrew search paths before any configure or Meson setup.
unset PKG_CONFIG_PATH CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH OBJC_INCLUDE_PATH LIBRARY_PATH CMAKE_PREFIX_PATH
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export PATH="$PREFIX/bin:$PATH"

if ! command -v brew >/dev/null; then
  echo "Homebrew is required for build tools (meson, nasm, cmake), not for shipped libraries" >&2
  exit 1
fi
export HOMEBREW_NO_AUTO_UPDATE=1
mkdir -p "$PREFIX/lib/pkgconfig"
brew install -q nasm meson ninja pkgconf cmake autoconf automake libtool python3
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
cp "$PINS" "$OFFER/$(basename "$PINS")"
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
unpack "$SRC/mbedtls.tar" "$WORK/mbedtls"
unpack "$SRC/lame.tar" "$WORK/lame"
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
# libplacebo 7.349.0 passes ET.parse() into ElementTree(). Python 3.14
# rejects that. The pinned patch is upstream 12509c0f (getroot()).
python3 "$SCRIPT_DIR/apply-pinned-patch.py" "$PINS" libplacebo "$WORK/libplacebo" "$SCRIPT_DIR"
mkdir -p "$OFFER/patches"
cp "$SCRIPT_DIR/libplacebo-python314.patch" "$OFFER/patches/libplacebo-python314.patch"
cp "$SCRIPT_DIR/apply-pinned-patch.py" "$OFFER/patches/apply-pinned-patch.py"

python3 - "$WORK" "$OFFER/licenses" <<'PY'
import pathlib, shutil, sys
work, dest = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
dest.mkdir(parents=True, exist_ok=True)
for tree in ("freetype", "fribidi", "harfbuzz", "libass", "ffmpeg", "mbedtls", "lame", "libplacebo", "mpv"):
    root = work / tree
    for path in root.rglob("*"):
        if path.is_file() and path.name.lower().startswith(("license", "copying")) and path.stat().st_size < 2_000_000:
            shutil.copy(path, dest / f"{tree}-{path.name}")
            break
PY

JOBS="$(sysctl -n hw.ncpu)"
(
  cd "$WORK/freetype"
  # Internal zlib only: do not link a host or Homebrew zlib.
  ./configure --prefix="$PREFIX" --enable-shared --disable-static \
    --with-zlib=no --with-png=no --with-harfbuzz=no --with-brotli=no --with-bzip2=no
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/fribidi"
  ./configure --prefix="$PREFIX" --enable-shared --disable-static
  make -j"$JOBS"
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
  make -j"$JOBS"
  make install
)
(
  # Static mbedTLS. FFmpeg 7.1.1's Secure Transport reports every
  # SecTrustEvaluate failure as -9807, including a CA that signed the server
  # certificate, so -ca_file cannot authenticate the HTTPS test. mbedTLS reads
  # that PEM. SChannel is not used.
  cmake -S "$WORK/mbedtls" -B "$WORK/mbedtls/build" -G "Unix Makefiles" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$PREFIX" \
    -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET" \
    -DUSE_SHARED_MBEDTLS_LIBRARY=OFF \
    -DUSE_STATIC_MBEDTLS_LIBRARY=ON \
    -DENABLE_PROGRAMS=OFF \
    -DENABLE_TESTING=OFF \
    -DMBEDTLS_FATAL_WARNINGS=OFF
  cmake --build "$WORK/mbedtls/build" -j"$JOBS"
  cmake --install "$WORK/mbedtls/build"
)
(
  cd "$WORK/lame"
  # config.sub in LAME 3.100 accepts aarch64, not Apple's arm64 uname.
  lame_cpu="$ARCH"
  [[ "$lame_cpu" == arm64 ]] && lame_cpu=aarch64
  # Static only. The frontend does not ship. -fcommon keeps the 3.100
  # tentative definitions linkable. Headers land in the prefix FFmpeg searches.
  CFLAGS="$CFLAGS -O2 -fcommon" \
    ./configure --prefix="$PREFIX" --host="${lame_cpu}-apple-darwin" \
      --disable-shared --enable-static \
      --disable-frontend --disable-gtktest --disable-nasm
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/ffmpeg"
  # 7.1.1 with --disable-autodetect enables libc_iconv and only probes iconv in
  # libc (configure: "disabled iconv || enable libc_iconv"). That probe links on
  # the macOS 15 Intel runner, but libavcodec's shared link does not: library.mak
  # puts LDFLAGS before the objects and FFEXTRALIBS after them, and iconv_extralibs
  # stays empty. --extra-libs=-liconv is what reaches that link. Do not disable iconv.
  # ffmpeg and ffprobe are the sidecar programs. LAME 3.100 is the MP3 encoder,
  # linked from the static archive above. --disable-autodetect drops TLS.
  # mbedTLS is the pinned archive above.
  ./configure --prefix="$PREFIX" --enable-shared --disable-static \
    --arch="$ARCH" --target-os=darwin \
    --extra-cflags="$CFLAGS -I$PREFIX/include" --extra-ldflags="$LDFLAGS -L$PREFIX/lib" \
    --disable-autodetect \
    --enable-gpl --enable-version3 \
    --enable-mbedtls --enable-libmp3lame --pkg-config-flags=--static \
    --disable-doc --disable-debug \
    --disable-programs --enable-ffmpeg --enable-ffprobe \
    --enable-videotoolbox --enable-audiotoolbox \
    --enable-zlib --enable-iconv \
    --extra-libs=-liconv \
    --disable-bzlib --disable-lzma --disable-sdl2
  make -j"$JOBS"
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
  # mpv 0.41 meson.build:1509 requires features['vulkan'] and CoreVideo together
  # before videotoolbox-pl can be enabled. This recipe has no pinned Vulkan loader
  # (the libplacebo Vulkan-Headers tree is not a loader, and MoltenVK is not built).
  # videotoolbox-gl still builds hwdec_vt.c and hwdec_mac_gl.c, so VideoToolbox
  # hardware decode stays on the OpenGL path. FFmpeg still uses --enable-videotoolbox.
  # The same meson file adds -L/usr/local/lib. Remove it so ld cannot take Homebrew.
  python3 - "$WORK/mpv/meson.build" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
old = ", '-L/usr/local/lib'"
if old not in text:
    raise SystemExit("mpv meson.build no longer has the Homebrew link path the recipe removes")
path.write_text(text.replace(old, "", 1))
PY
  meson setup build --prefix="$PREFIX" --buildtype=release \
    -Dlibmpv=false \
    -D{gl,iconv,plain-gl}=enabled \
    -D{cocoa,coreaudio,gl-cocoa,videotoolbox-gl}=enabled \
    -D{swift-build,macos-cocoa-cb,macos-media-player}=enabled \
    -D{videotoolbox-pl,vulkan,macos-touchbar,lua,jpeg,lcms2}=disabled \
    -D{cdda,dvdnav,javascript,libarchive,libbluray,rubberband,uchardet,vapoursynth,zimg,shaderc,wayland,x11,manpage-build}=disabled \
    -Dswift-flags="-target ${ARCH}-apple-macos${DEPLOYMENT_TARGET}" \
    -Dc_args="$CFLAGS" -Dobjc_args="$CFLAGS -Wno-error=deprecated -Wno-error=deprecated-declarations" \
    -Dc_link_args="$LDFLAGS -liconv -framework CoreVideo -framework IOSurface"
  meson compile -C build
  meson compile -C build macos-bundle
)
rm -rf "$OUT/mpv"
mkdir -p "$OUT/mpv"
cp -a "$WORK/mpv/build/mpv.app" "$OUT/mpv/mpv.app"
cp "$PREFIX/bin/ffmpeg" "$PREFIX/bin/ffprobe" "$OUT/"
cp -a "$PREFIX/lib/"*.dylib "$OUT/"
python3 - "$OUT" "$PREFIX/lib" <<'PY'
import pathlib, subprocess, sys
out = pathlib.Path(sys.argv[1])
prefix = sys.argv[2]
machos = [out / "ffmpeg", out / "ffprobe"]
machos += [path for path in out.glob("*.dylib") if path.is_file() and not path.is_symlink()]
names = {path.name for path in machos}

def load_commands(path):
    text = subprocess.check_output(["otool", "-L", str(path)], text=True)
    return [line.strip().split(" ", 1)[0] for line in text.splitlines()[1:]]

for path in machos:
    if path.suffix == ".dylib":
        subprocess.check_call(["install_name_tool", "-id", f"@executable_path/{path.name}", str(path)])
    for dep in load_commands(path):
        base = pathlib.Path(dep).name
        new = f"@executable_path/{base}"
        if dep == new:
            continue
        if base in names or dep.startswith(prefix):
            subprocess.check_call(["install_name_tool", "-change", dep, new, str(path)])
PY
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
python3 "$SCRIPT_DIR/mach-minos.py" "$OUT/mpv/mpv.app" "$OUT/ffmpeg" "$OUT/ffprobe" "$OUT"/*.dylib --max "$DEPLOYMENT_TARGET"
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
    "programs": ["mpv", "ffmpeg", "ffprobe"],
    "clang": show(["clang", "--version"]),
    "xcode": show(["xcodebuild", "-version"]),
    "sdk": show(["xcrun", "--sdk", "macosx", "--show-sdk-path"]),
    "sources": pins,
}
pathlib.Path(sys.argv[2]).write_text(json.dumps(record, indent=2) + "\n")
PY
tar -C "$OUT" -czf "$OUT/mpv-corresponding-source.tar.gz" mpv-corresponding-source
rm -rf "$OFFER"
echo "macOS source build installed at $OUT (mpv, ffmpeg, ffprobe)"
