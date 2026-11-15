#!/usr/bin/env bash
# Windows mpv, ffmpeg, and ffprobe from pinned sources.
# shaderc is commit 2737994b552137a85545f956e303d620e6ff232c with the SPIR-V and
# glslang revisions from that commit's DEPS file. Vulkan-Headers and
# Vulkan-Loader stay pinned at vulkan-sdk-1.4.363.0 with UPDATE_DEPS OFF.
# The player links OpenGL and does not ship the loader.
# FFmpeg is the pinned 7.1.1 release. MP3 uses the pinned LAME 3.100 tarball,
# linked statically. FFmpeg does not link xvid, aom, soxr, opencore-amr,
# ffnvcodec, or libiconv. mpv links the pinned static zlib and GNU libiconv.
# HTTPS uses the pinned mbedTLS 3.6.7
# archive (Apache-2.0 OR GPL-2.0-or-later). SChannel cannot load the PEM in
# SSL_CERT_FILE, which is how Vitra supplies OS roots and custom proxy CAs.
# Usage: build-windows-mpv.sh <sidecar-output-dir>
set -euo pipefail

OUT=${1:?output directory required}
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PINS="${VITRA_MPV_SOURCES:-$SCRIPT_DIR/windows-mpv-sources.json}"

# MSYS2_PATH_TYPE=minimal hides the runner's Node. Remember it before reexec.
if [[ -z "${VITRA_NODE:-}" ]] && command -v node >/dev/null 2>&1; then
  VITRA_NODE=$(command -v node)
  if command -v cygpath >/dev/null 2>&1; then
    VITRA_NODE=$(cygpath -u "$VITRA_NODE")
  fi
  export VITRA_NODE
fi

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
  exec "$msys_bash" -lc "export MSYSTEM=MINGW64 MSYS2_PATH_TYPE=minimal TMPDIR=/tmp TEMP=/tmp TMP=/tmp VITRA_MSYS_REEXEC=1 VITRA_MPV_SOURCES='$PINS_U' VITRA_NODE='$VITRA_NODE'; mkdir -p /tmp; cd '$ROOT_U' && exec bash scripts/release/build-windows-mpv.sh '$OUT_U'"
fi
export TMPDIR=/tmp
export TEMP=/tmp
export TMP=/tmp
mkdir -p /tmp
export PATH="/mingw64/bin:/usr/bin:${PATH:-}"
hash -r
# Meson's cmake probe uses Windows Python's Path.home(). That only accepts
# USERPROFILE or HOMEDRIVE+HOMEPATH. An MSYS login shell often has neither,
# so a missing library aborts with "Could not determine home directory"
# instead of being skipped. libunwind and spirv-cross-c-shared both died
# this way. A disabled feature is not looked up; required:false still is
# (ffnvcodec, sdl2), so the variables have to be set before the first meson.
if [[ -z "${USERPROFILE:-}" ]]; then
  export USERPROFILE="$(cygpath -w /tmp)"
fi
USERPROFILE="$(cygpath -w "$USERPROFILE")"
export USERPROFILE
if [[ -z "${HOMEDRIVE:-}" || -z "${HOMEPATH:-}" ]]; then
  export HOMEDRIVE="${USERPROFILE%%:*}:"
  export HOMEPATH="${USERPROFILE#*:}"
fi
export HOME="$(cygpath -u "$USERPROFILE")"
mkdir -p "$HOME"

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
# MSYS2's pkg-config searches /mingw64 even when PKG_CONFIG_LIBDIR is set,
# which is how mpv found the distro zlib 1.3.2. This wrapper only sees $PREFIX.
pkgconf_bin=/mingw64/bin/pkgconf.exe
if [[ ! -x "$pkgconf_bin" ]]; then
  pkgconf_bin=/mingw64/bin/pkg-config.exe
fi
cat >"$WORK/bin/pkg-config" <<EOF
#!/bin/sh
unset PKG_CONFIG_PATH
export PKG_CONFIG_LIBDIR='$PREFIX/lib/pkgconfig'
exec '$pkgconf_bin' "\$@"
EOF
chmod +x "$WORK/bin/pkg-config"
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
unpack "$SRC/lame.tar" "$WORK/lame"
unpack "$SRC/zlib.tar" "$WORK/zlib"
unpack "$SRC/libiconv.tar" "$WORK/libiconv"
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
# libplacebo 7.349.0 passes ET.parse() into ElementTree(). Python 3.14
# rejects that. The pinned patch is upstream 12509c0f (getroot()).
"$PYTHON" "$SCRIPT_DIR/apply-pinned-patch.py" "$PINS" libplacebo "$WORK/libplacebo" "$SCRIPT_DIR"
mkdir -p "$OFFER/patches"
cp "$SCRIPT_DIR/libplacebo-python314.patch" "$OFFER/patches/libplacebo-python314.patch"
cp "$SCRIPT_DIR/apply-pinned-patch.py" "$OFFER/patches/apply-pinned-patch.py"

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
# These .pc files name shared libraries (shaderc_shared, vulkan-1). mpv does
# not link them. Leaving the files would let a later pkg-config probe do so.
rm -f "$PREFIX/lib/pkgconfig/vulkan.pc" \
  "$PREFIX/lib/pkgconfig/shaderc.pc" \
  "$PREFIX/lib/pkgconfig/shaderc_combined.pc" \
  "$PREFIX/lib/pkgconfig/shaderc_static.pc" \
  "$PREFIX/lib/pkgconfig/glslang.pc"
# Meson must not cmake-find an MSYS2 shared library after pkg-config misses.
export CMAKE_PREFIX_PATH="$PREFIX"
export CMAKE_FIND_USE_SYSTEM_ENVIRONMENT_PATH=OFF
export CMAKE_FIND_USE_CMAKE_SYSTEM_PATH=OFF
(
  cd "$WORK/zlib"
  # Static only. mpv's zlib feature otherwise finds MSYS2's zlib1.dll.
  ./configure --static --prefix="$PREFIX"
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/libiconv"
  # Static only. libass probes -liconv with the compiler and would link
  # MSYS2's libiconv-2.dll. The iconv program is not shipped.
  ./configure --prefix="$PREFIX" --disable-shared --enable-static --disable-nls
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/freetype"
  # No system zlib, png, harfbuzz, brotli, or bzip2. Those are not pinned.
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
    -Dprefer_static=true \
    -Dfreetype=enabled -Dglib=disabled -Dgobject=disabled -Dcairo=disabled \
    -Dchafa=disabled -Dicu=disabled -Dgraphite2=disabled \
    -Dtests=disabled -Ddocs=disabled -Dintrospection=disabled -Dutilities=disabled
  meson compile -C build
  meson install -C build
)
(
  cd "$WORK/libass"
  # DirectWrite on desktop loads dwrite.dll at runtime and links gdi32.
  # libunibreak, fontconfig, and libpng are not pinned, so they stay off.
  CPPFLAGS="-I$PREFIX/include" LDFLAGS="-L$PREFIX/lib" LIBS="-l:libiconv.a -l:libcharset.a" \
    ./configure --prefix="$PREFIX" --disable-shared --enable-static \
      --disable-fontconfig --disable-libunibreak
  make -j"$JOBS"
  make install
)
cp "$WORK/mbedtls/LICENSE" "$OFFER/licenses/mbedtls-LICENSE"
cp "$WORK/lame/COPYING" "$OFFER/licenses/lame-COPYING"
cp "$WORK/zlib/LICENSE" "$OFFER/licenses/zlib-LICENSE"
cp "$WORK/libiconv/COPYING.LIB" "$OFFER/licenses/libiconv-COPYING.LIB"
(
  cd "$WORK/lame"
  # Static only. The frontend does not ship. -fcommon keeps the 3.100
  # tentative definitions linkable on the mingw gcc this runner installs.
  CFLAGS="${CFLAGS:-} -O2 -fcommon" \
    ./configure --prefix="$PREFIX" --host=x86_64-w64-mingw32 \
      --disable-shared --enable-static \
      --disable-frontend --disable-gtktest --disable-nasm
  make -j"$JOBS"
  make install
)
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
# mingw cmake writes prefix=D:/... into the .pc files. pkg-config then prints
# -ID:/..., and MSYS splits that colon into a path list, so FFmpeg's probe
# cannot compile mbedtls/ssl.h. The templates also omit ws2_32 and bcrypt,
# which the static libraries reference. Point prefix at this shell path.
pc_dir="$PREFIX/lib/pkgconfig"
sed -i "s|^prefix=.*|prefix=${PREFIX}|" \
  "$pc_dir/mbedcrypto.pc" "$pc_dir/mbedtls.pc" "$pc_dir/mbedx509.pc"
if ! grep -q '^Libs.private:' "$pc_dir/mbedcrypto.pc"; then
  printf '\nLibs.private: -lbcrypt -lws2_32\n' >> "$pc_dir/mbedcrypto.pc"
fi
# Rewrite every -lfoo in our .pc files to -l:libfoo.a when that archive
# exists. Anything else, other than a Windows system import library, fails
# here — before ffmpeg and mpv are linked — instead of as a DLL import.
pin_pkgconfig_static() {
  local archives=()
  local archive base printed name
  shopt -s nullglob
  for archive in "$PREFIX/lib"/lib*.a; do
    base=$(basename "$archive")
    [[ "$base" == *.dll.a ]] && continue
    archives+=("$base")
  done
  shopt -u nullglob
  for name in libpthread.a libstdc++.a libgcc.a libgcc_eh.a libmingw32.a \
    libmingwex.a libmoldname.a libm.a libssp.a libatomic.a libmingwthrd.a; do
    printed=$(gcc -print-file-name="$name")
    if [[ "$printed" == */* || "$printed" == *\\* ]]; then
      archives+=("$(basename "$printed")")
    fi
  done
  if [[ -z "${VITRA_NODE:-}" || ! -x "${VITRA_NODE}" ]]; then
    echo "node is required to check pkg-config libraries" >&2
    exit 1
  fi
  shopt -s nullglob
  for archive in "$PREFIX/lib/pkgconfig"/*.pc; do
    "$VITRA_NODE" "$SCRIPT_DIR/windows-pkgconfig-static.mjs" "$archive" "${archives[@]}"
  done
  shopt -u nullglob
}
pin_pkgconfig_static
(
  cd "$WORK/ffmpeg"
  ./configure --prefix="$PREFIX" --disable-shared --enable-static \
    --arch=x86_64 --target-os=mingw32 --pkg-config-flags=--static \
    --disable-autodetect \
    --enable-gpl --enable-version3 \
    --enable-mbedtls --enable-libmp3lame \
    --extra-cflags="-I$PREFIX/include" --extra-ldflags="-L$PREFIX/lib" \
    --extra-ldexeflags="-static" \
    --disable-doc --disable-debug \
    --disable-programs --enable-ffmpeg --enable-ffprobe \
    --enable-d3d11va --enable-dxva2 \
    --disable-iconv --disable-zlib --disable-bzlib --disable-lzma --disable-sdl2 \
    --disable-libass --disable-libfreetype --disable-libfribidi --disable-libharfbuzz \
    --disable-libxml2 --disable-libzimg
  make -j"$JOBS"
  make install
)
(
  cd "$WORK/libplacebo"
  # d3d11 requires spirv-cross-c-shared, which is not pinned. OpenGL
  # (opengl32.dll, a Windows system library) is the renderer. vulkan-1.dll is
  # the Vulkan loader; the MinGW build of it imports libgcc and libwinpthread,
  # and a machine without a Vulkan driver does not have that DLL either.
  # shaderc only compiles SPIR-V for Vulkan. A disabled feature is not probed.
  meson setup build --prefix="$PREFIX" --buildtype=release --default-library=static \
    -Dprefer_static=true \
    -Ddemos=false -Dtests=false \
    -Dvulkan=disabled -Dshaderc=disabled -Dglslang=disabled \
    -Dopengl=enabled -Dd3d11=disabled -Dunwind=disabled \
    -Dlcms=disabled -Dlibdovi=disabled -Dxxhash=disabled
  meson compile -C build
  meson install -C build
)
# libplacebo's convert.cc is a C++ static archive, and its .pc file does not
# name libstdc++. mpv links with the C driver, so GCC 16 leaves
# std::to_chars/from_chars unresolved. A plain -lstdc++ selects
# libstdc++.dll.a. Name the static archives instead. libpthread.a satisfies
# the pthread symbols inside that libstdc++. Meson shell-splits c_link_args
# on spaces. A comma-joined -D value becomes one argument.
for pc in "$PREFIX/lib/pkgconfig/libplacebo.pc"; do
  if [[ -f "$pc" ]] && ! grep -q 'libstdc++\.a' "$pc"; then
    if grep -q '^Libs.private:' "$pc"; then
      sed -i 's/^Libs.private:.*/& -l:libstdc++.a -l:libpthread.a/' "$pc"
    else
      printf '\nLibs.private: -l:libstdc++.a -l:libpthread.a\n' >> "$pc"
    fi
  fi
done
pin_pkgconfig_static
(
  cd "$WORK/mpv"
  meson setup build --prefix="$PREFIX" --buildtype=release --default-library=static \
    -Dlibmpv=false -Dprefer_static=true -Dzlib=enabled \
    -Dc_link_args="-static -static-libgcc -l:libstdc++.a -l:libpthread.a" \
    -D{d3d-hwaccel,wasapi,gl,gl-win32,plain-gl}=enabled \
    -D{iconv,lua,javascript,jpeg,lcms2,cocoa,videotoolbox-gl,videotoolbox-pl,swift-build}=disabled \
    -D{wayland,x11,vaapi,dvdnav,cdda,caca,sixel,libarchive,libbluray,rubberband,uchardet,vapoursynth,zimg,manpage-build,spirv-cross,d3d11,vulkan,shaderc}=disabled
  meson compile -C build
)
mkdir -p "$OUT/mpv"
cp "$WORK/mpv/build/mpv.exe" "$OUT/mpv/mpv.exe"
cp "$PREFIX/bin/ffmpeg.exe" "$OUT/ffmpeg.exe"
cp "$PREFIX/bin/ffprobe.exe" "$OUT/ffprobe.exe"
# Before the source tarball and the installer. vulkan-1.dll, libstdc++,
# libgcc, libwinpthread, shaderc_shared, SDL2, and libass are not system DLLs.
assert_system_dlls() {
  local exe="$1"
  if [[ -z "${VITRA_NODE:-}" || ! -x "${VITRA_NODE}" ]]; then
    echo "node is required to check DLL imports of $exe" >&2
    exit 1
  fi
  objdump -p "$exe" | "$VITRA_NODE" "$SCRIPT_DIR/windows-dll-imports.mjs" "$exe"
}
assert_system_dlls "$OUT/mpv/mpv.exe"
assert_system_dlls "$OUT/ffmpeg.exe"
assert_system_dlls "$OUT/ffprobe.exe"
# Do not pipe this into grep -q. pipefail plus grep -q closes the pipe as
# soon as it sees libmp3lame, ffmpeg dies with SIGPIPE, and the check
# reports the encoder missing even though it was just compiled.
encoders=$("$OUT/ffmpeg.exe" -hide_banner -encoders) || {
  echo "sidecar ffmpeg failed to list encoders" >&2
  exit 1
}
if ! grep -q libmp3lame <<<"$encoders"; then
  echo "sidecar ffmpeg is missing the libmp3lame encoder" >&2
  exit 1
fi
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
