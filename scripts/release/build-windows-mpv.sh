#!/usr/bin/env bash
# Windows mpv, ffmpeg, and ffprobe from pinned sources.
# shaderc is commit 2737994b552137a85545f956e303d620e6ff232c with the SPIR-V and
# glslang revisions from that commit's DEPS file. Vulkan-Headers and
# Vulkan-Loader are the tag vulkan-sdk-1.4.363.0. UPDATE_DEPS stays OFF.
# FFmpeg is the pinned 7.1.1 release and does not link lame, xvid, aom, soxr,
# opencore-amr, ffnvcodec, or libiconv. HTTPS uses the pinned mbedTLS 3.6.7
# archive (Apache-2.0 OR GPL-2.0-or-later). SChannel cannot load the PEM in
# SSL_CERT_FILE, which is how Vitra supplies OS roots and custom proxy CAs.
# Usage: build-windows-mpv.sh <sidecar-output-dir>
set -euo pipefail

OUT=${1:?output directory required}
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PINS="${VITRA_MPV_SOURCES:-$SCRIPT_DIR/windows-mpv-sources.json}"

# setup-msys2's default release install lives under RUNNER_TEMP/msys64, not in
# the image copy at C:/msys64. That image has no mingw cmake, so bash reports
# "cmake: command not found" (exit 127). Enter the MSYS2 whose mingw64 bin
# contains cmake. Keep temp files under /tmp: Git Bash's libtool otherwise
# writes C:/Users/RUNNER~1/... and mingw ar cannot open objs/.libs.
# Do not pass PATH=a:/b on the bash.exe command line. MSYS2 rewrites that
# colon list into semicolons and the rest of the command is dropped.
if [[ ! -x /mingw64/bin/cmake.exe ]]; then
  if [[ "${VITRA_MSYS_REEXEC:-}" == 1 ]]; then
    echo "mingw cmake is not in this MSYS2 ($(cygpath -m / 2>/dev/null || echo unknown))" >&2
    exit 1
  fi
  msys_bash=""
  if [[ -n "${MSYS2_LOCATION:-}" && -x "${MSYS2_LOCATION}/usr/bin/bash.exe" ]]; then
    msys_bash=$(cygpath -u "${MSYS2_LOCATION}/usr/bin/bash.exe")
  fi
  if [[ -z "$msys_bash" ]]; then
    IFS=':' read -r -a path_entries <<< "${PATH:-}"
    for entry in "${path_entries[@]}"; do
      launcher="${entry}/msys2.cmd"
      [[ -f "$launcher" ]] || continue
      win_bash=$(awk 'match($0, /[A-Za-z]:\\[^ ]*usr\\bin\\bash.exe/) { print substr($0, RSTART, RLENGTH); exit }' "$launcher" | tr -d '\r')
      if [[ -n "$win_bash" ]]; then
        win_bash=${win_bash//\\//}
        msys_bash=$(cygpath -u "$win_bash")
        break
      fi
    done
  fi
  if [[ -z "$msys_bash" && -x /c/msys64/usr/bin/bash.exe ]]; then
    msys_bash=/c/msys64/usr/bin/bash.exe
  fi
  if [[ -z "$msys_bash" || ! -x "$msys_bash" ]]; then
    echo "MSYS2 mingw-w64 gcc is required to build Windows mpv" >&2
    exit 1
  fi
  OUT_U=$(cygpath -u "$OUT")
  ROOT_U=$(cygpath -u "$PWD")
  PINS_U=$(cygpath -u "$PINS")
  exec "$msys_bash" -lc "export MSYSTEM=MINGW64 MSYS2_PATH_TYPE=minimal TMPDIR=/tmp TEMP=/tmp TMP=/tmp VITRA_MSYS_REEXEC=1 VITRA_MPV_SOURCES='$PINS_U'; mkdir -p /tmp; cd '$ROOT_U' && exec bash scripts/release/build-windows-mpv.sh '$OUT_U'"
fi
export TMPDIR=/tmp
export TEMP=/tmp
export TMP=/tmp
mkdir -p /tmp
export PATH="/mingw64/bin:/usr/bin:${PATH:-}"
hash -r

if [[ -x "$OUT/ffmpeg.exe" && -x "$OUT/ffprobe.exe" && -x "$OUT/mpv/mpv.exe" && -f "$OUT/mpv-corresponding-source.tar.gz" ]]; then
  exit 0
fi

WORK=$(mktemp -d)
PREFIX="$WORK/prefix"
SRC="$WORK/src"
OFFER="$OUT/mpv-corresponding-source"
trap 'rm -rf "$WORK"' EXIT
export PATH="/mingw64/bin:$PREFIX/bin:$PATH"
unset PKG_CONFIG_PATH
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export CMAKE_PREFIX_PATH="$PREFIX"
mkdir -p "$PREFIX/lib/pkgconfig" "$SRC" "$OFFER/tarballs" "$OFFER/licenses"
cp "$PINS" "$OFFER/$(basename "$PINS")"
cp "$SCRIPT_DIR/build-windows-mpv.sh" "$OFFER/build-windows-mpv.sh"

# Refuse a later step that tries to clone shaderc or Vulkan dependencies.
mkdir -p "$WORK/bin"
cat >"$WORK/bin/git" <<'EOF'
#!/bin/sh
case "${1:-}" in
  clone|fetch|submodule|pull)
    echo "refusing unpinned git $*" >&2
    exit 1
    ;;
esac
exec /usr/bin/git "$@"
EOF
chmod +x "$WORK/bin/git"
export PATH="$WORK/bin:$PATH"

# mingw-w64-x86_64-python ships python.exe and removes python3.exe.
# Prefer that interpreter. Git Bash has neither name until this MSYS2 PATH
# is in place. The py launcher covers a runner that has not entered MSYS2 yet.
if [[ -x /mingw64/bin/python.exe ]]; then
  PYTHON=/mingw64/bin/python.exe
elif command -v python3 >/dev/null 2>&1; then
  PYTHON=python3
elif command -v python >/dev/null 2>&1; then
  PYTHON=python
elif command -v py >/dev/null 2>&1; then
  PYTHON=py
else
  echo "python3 is required to build Windows mpv" >&2
  exit 1
fi

"$PYTHON" - "$PINS" "$SRC" <<'PY'
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

unpack() {
  local archive="$1" dest="$2"
  rm -rf "$dest"
  mkdir -p "$dest"
  tar -xf "$archive" -C "$dest" --strip-components=1
}

unpack "$SRC/mpv.tar.gz" "$WORK/mpv"
unpack "$SRC/ffmpeg.tar" "$WORK/ffmpeg"
unpack "$SRC/mbedtls.tar" "$WORK/mbedtls"
unpack "$SRC/libplacebo.tar" "$WORK/libplacebo"
unpack "$SRC/freetype.tar" "$WORK/freetype"
unpack "$SRC/fribidi.tar" "$WORK/fribidi"
unpack "$SRC/harfbuzz.tar" "$WORK/harfbuzz"
unpack "$SRC/libass.tar" "$WORK/libass"
unpack "$SRC/shaderc.tar" "$WORK/shaderc"
unpack "$SRC/spirv-tools.tar" "$WORK/shaderc/third_party/spirv-tools"
unpack "$SRC/spirv-headers.tar" "$WORK/shaderc/third_party/spirv-headers"
unpack "$SRC/glslang.tar" "$WORK/shaderc/third_party/glslang"
unpack "$SRC/vulkan-headers.tar" "$WORK/vulkan-headers"
unpack "$SRC/vulkan-loader.tar" "$WORK/vulkan-loader"
"$PYTHON" - "$PINS" "$SRC" "$WORK/libplacebo" <<'PY'
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

JOBS="$(nproc)"
(
  cd "$WORK/vulkan-headers"
  cmake -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$PREFIX" -DBUILD_TESTS=OFF
  ninja -j"$JOBS"
  ninja install
)
(
  cd "$WORK/vulkan-loader"
  cmake -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$PREFIX" \
    -DUPDATE_DEPS=OFF -DBUILD_TESTS=OFF
  ninja -j"$JOBS"
  ninja install
)
(
  cd "$WORK/shaderc"
  cmake -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$PREFIX" \
    -DBUILD_SHARED_LIBS=OFF \
    -DSHADERC_SKIP_TESTS=ON -DSHADERC_SKIP_EXAMPLES=ON -DSHADERC_SKIP_EXECUTABLES=ON \
    -DSHADERC_SKIP_COPYRIGHT_CHECK=ON -DSPIRV_SKIP_TESTS=ON
  ninja -j"$JOBS"
  ninja install
)
(
  cd "$WORK/freetype"
  ./configure --prefix="$PREFIX" --disable-shared --enable-static \
    --with-zlib=no --with-png=no --with-harfbuzz=no --with-brotli=no --with-bzip2=no
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/fribidi"
  ./configure --prefix="$PREFIX" --disable-shared --enable-static
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/harfbuzz"
  meson setup build --prefix="$PREFIX" --buildtype=release --default-library=static \
    -Dfreetype=enabled -Dglib=disabled -Dgobject=disabled -Dcairo=disabled \
    -Dchafa=disabled -Dicu=disabled -Dgraphite2=disabled \
    -Dtests=disabled -Ddocs=disabled -Dintrospection=disabled -Dutilities=disabled
  meson compile -C build
  meson install -C build
)
(
  cd "$WORK/libass"
  ./configure --prefix="$PREFIX" --disable-shared --enable-static --disable-fontconfig
  make -j"$JOBS"
  make install
)
cp "$WORK/mbedtls/LICENSE" "$OFFER/licenses/mbedtls-LICENSE"
(
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
)
(
  cd "$WORK/ffmpeg"
  ./configure --prefix="$PREFIX" --disable-shared --enable-static \
    --arch=x86_64 --target-os=mingw32 --pkg-config-flags=--static \
    --disable-autodetect \
    --enable-gpl --enable-version3 \
    --enable-mbedtls \
    --extra-cflags="-I$PREFIX/include" --extra-ldflags="-L$PREFIX/lib" \
    --disable-doc --disable-debug \
    --disable-programs --enable-ffmpeg --enable-ffprobe \
    --enable-d3d11va --enable-dxva2 \
    --disable-iconv --disable-zlib --disable-bzlib --disable-lzma --disable-sdl2
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/libplacebo"
  meson setup build --prefix="$PREFIX" --buildtype=release --default-library=static \
    -Ddemos=false -Dtests=false \
    -Dvulkan=enabled -Dshaderc=enabled -Dglslang=disabled \
    -Dopengl=disabled -Dd3d11=enabled \
    -Dlcms=disabled -Dlibdovi=disabled -Dxxhash=disabled
  meson compile -C build
  meson install -C build
)
(
  cd "$WORK/mpv"
  meson setup build --prefix="$PREFIX" --buildtype=release --default-library=static \
    -Dlibmpv=false \
    -D{d3d11,d3d-hwaccel,wasapi,vulkan,shaderc,gl,gl-win32,plain-gl}=enabled \
    -D{iconv,lua,javascript,jpeg,lcms2,cocoa,videotoolbox-gl,videotoolbox-pl,swift-build}=disabled \
    -D{wayland,x11,vaapi,dvdnav,cdda,libarchive,libbluray,rubberband,uchardet,vapoursynth,zimg,manpage-build,spirv-cross}=disabled
  meson compile -C build
)
mkdir -p "$OUT/mpv"
cp "$WORK/mpv/build/mpv.exe" "$OUT/mpv/mpv.exe"
cp "$PREFIX/bin/ffmpeg.exe" "$OUT/ffmpeg.exe"
cp "$PREFIX/bin/ffprobe.exe" "$OUT/ffprobe.exe"
vulkan_dll=$(find "$PREFIX" -name 'vulkan-1.dll' -o -name 'libvulkan-1.dll' | head -n 1)
if [[ -z "$vulkan_dll" ]]; then
  echo "Vulkan-Loader did not install vulkan-1.dll" >&2
  exit 1
fi
cp "$vulkan_dll" "$OUT/mpv/vulkan-1.dll"
"$PYTHON" - "$PINS" "$OFFER/build-record.json" <<'PY'
import json, pathlib, sys
pins = json.loads(pathlib.Path(sys.argv[1]).read_text())
pathlib.Path(sys.argv[2]).write_text(json.dumps({
    "recipe": "scripts/release/build-windows-mpv.sh",
    "shadercCommit": pins["shadercCommit"],
    "vulkanTag": pins["vulkanTag"],
    "updateDeps": False,
    "programs": ["mpv", "ffmpeg", "ffprobe"],
    "sources": pins,
}, indent=2) + "\n")
PY
tar -C "$OUT" -czf "$OUT/mpv-corresponding-source.tar.gz" mpv-corresponding-source
rm -rf "$OFFER"
echo "Windows source build installed at $OUT"
