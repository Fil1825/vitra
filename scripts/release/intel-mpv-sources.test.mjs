import fs from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { planFor } from '../sidecars/plan.mjs';

const pins = JSON.parse(
  fs.readFileSync(path.join(import.meta.dirname, 'intel-mpv-sources.json'), 'utf8'),
);
const recipe = fs.readFileSync(path.join(import.meta.dirname, 'build-intel-mpv.sh'), 'utf8');

describe('Intel mpv source pins', () => {
  it('targets Monterey and pins every archive', () => {
    expect(pins.deploymentTarget).toBe('12.0');
    expect(pins.arch).toBe('x86_64');
    expect(pins.mpv.sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(pins.mpv.url).not.toMatch(/stolendata/);
    for (const dep of pins.dependencies) {
      expect(dep.sha256).toMatch(/^[0-9a-f]{64}$/);
      expect(dep.url).not.toMatch(/brew|stolendata/);
      for (const sub of dep.submodules ?? []) {
        expect(sub.commit).toMatch(/^[0-9a-f]{40}$/);
        expect(sub.sha256).toMatch(/^[0-9a-f]{64}$/);
      }
    }
  });

  it('keeps the recipe fail-closed on checksums and min OS', () => {
    expect(recipe).toContain('MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"');
    expect(recipe).toContain('checksum mismatch');
    expect(recipe).toContain('--max "$DEPLOYMENT_TARGET"');
    expect(recipe).not.toContain('stolendata');
    expect(planFor('x86_64-apple-darwin').mpv.sources).toBe(
      'scripts/release/intel-mpv-sources.json',
    );
    const arm = JSON.parse(
      fs.readFileSync(path.join(import.meta.dirname, 'macos-arm64-mpv-sources.json'), 'utf8'),
    );
    expect(arm.deploymentTarget).toBe('14.0');
    expect(arm.arch).toBe('arm64');
    expect(arm.dependencies.find((dep) => dep.name === 'ffmpeg').sha256).toBe(
      pins.dependencies.find((dep) => dep.name === 'ffmpeg').sha256,
    );
    expect(arm.dependencies.find((dep) => dep.name === 'mbedtls').sha256).toBe(
      pins.dependencies.find((dep) => dep.name === 'mbedtls').sha256,
    );
  });

  it('builds HarfBuzz 10 with Meson and does not search Homebrew pkg-config', () => {
    expect(recipe).not.toMatch(/cd "\$WORK\/harfbuzz"[\s\S]{0,80}\.\/configure/);
    expect(recipe).toContain('-Dfreetype=enabled -Dglib=disabled');
    expect(recipe).toContain('unset PKG_CONFIG_PATH');
    expect(recipe).toContain('PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"');
    expect(recipe).toContain('pkg-config sees libraries outside the source prefix');
    expect(recipe).toContain('--disable-autodetect');
    expect(recipe).toContain('--with-zlib=no');
    expect(recipe).toContain('--disable-fontconfig');
    expect(recipe).toContain('-Dlcms=disabled');
    expect(recipe).toContain('rubberband');
  });

  it('keeps VideoToolbox on the OpenGL path and does not ask for Vulkan', () => {
    const enabled = [...recipe.matchAll(/-D\{([^}]+)\}=enabled/g)].flatMap((match) =>
      match[1].split(','),
    );
    const disabled = [...recipe.matchAll(/-D\{([^}]+)\}=disabled/g)].flatMap((match) =>
      match[1].split(','),
    );
    expect(enabled).toEqual(expect.arrayContaining(['gl-cocoa', 'videotoolbox-gl', 'plain-gl']));
    expect(enabled).not.toContain('videotoolbox-pl');
    expect(enabled).not.toContain('vulkan');
    expect(disabled).toEqual(expect.arrayContaining(['videotoolbox-pl', 'vulkan']));
    expect(recipe).toContain('--enable-videotoolbox');
    expect(recipe).toContain('--enable-audiotoolbox');
    expect(recipe).toContain('--enable-mbedtls');
    expect(recipe).not.toContain('--enable-securetransport');
    expect(recipe).toContain(
      '-Dc_link_args="$LDFLAGS -liconv -framework CoreVideo -framework IOSurface"',
    );
    expect(recipe).toContain(", '-L/usr/local/lib'");
    expect(recipe).toContain('Homebrew link path the recipe removes');
  });

  it('links libiconv into the shared FFmpeg build without dropping iconv', () => {
    expect(recipe).toContain('--enable-iconv');
    expect(recipe).not.toContain('--disable-iconv');
    expect(recipe).toContain('--extra-libs=-liconv');
    expect(recipe).toContain('-liconv');
    expect(recipe).toContain('-framework CoreVideo');
    expect(recipe).toContain('-framework IOSurface');
    expect(recipe).toContain('MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"');
    expect(recipe).toContain('-D{gl,iconv,plain-gl}=enabled');
  });

  it('pins Windows shaderc and Vulkan and leaves the seven external codecs out', () => {
    const windows = JSON.parse(
      fs.readFileSync(path.join(import.meta.dirname, 'windows-mpv-sources.json'), 'utf8'),
    );
    const script = fs.readFileSync(path.join(import.meta.dirname, 'build-windows-mpv.sh'), 'utf8');
    const gpl = fs.readFileSync(path.join(import.meta.dirname, 'build-gpl-ffmpeg.sh'), 'utf8');
    expect(windows.shadercCommit).toBe('2737994b552137a85545f956e303d620e6ff232c');
    expect(windows.vulkanTag).toBe('vulkan-sdk-1.4.363.0');
    for (const name of [
      'shaderc',
      'spirv-tools',
      'spirv-headers',
      'glslang',
      'vulkan-headers',
      'vulkan-loader',
    ]) {
      const dep = windows.dependencies.find((item) => item.name === name);
      expect(dep.sha256).toMatch(/^[0-9a-f]{64}$/);
      expect(dep.url).toMatch(/^https:\/\/github.com\/(google|KhronosGroup)\//);
    }
    expect(script).toContain('-DUPDATE_DEPS=OFF');
    expect(script).not.toContain('git clone');
    expect(script).toContain('--disable-iconv');
    expect(script).toContain('TMPDIR=/tmp');
    expect(script).toContain('C:/msys64');
    expect(script).toContain('msys2.cmd');
    expect(script).toContain('/mingw64/bin/cmake.exe');
    expect(script).toContain('VITRA_MSYS_REEXEC');
    expect(script).toContain('command -v python3');
    expect(script).toContain('command -v python');
    expect(script).toContain('"$PYTHON"');
    expect(gpl).toContain('--disable-iconv');
    expect(gpl).not.toContain('--disable-x86asm');
    expect(gpl).not.toContain('BtbN');
    const gplPins = JSON.parse(
      fs.readFileSync(path.join(import.meta.dirname, 'gpl-ffmpeg-sources.json'), 'utf8'),
    );
    expect(gplPins.mbedtls.version).toBe('3.6.7');
    expect(gplPins.mbedtls.sha256).toBe(
      'a7e8bcbec0e6f761b4af24f25677626b35f762f68eef79c08677a363212d11f6',
    );
    expect(gplPins.mbedtls.url).toBe(
      'https://github.com/Mbed-TLS/mbedtls/releases/download/mbedtls-3.6.7/mbedtls-3.6.7.tar.bz2',
    );
    expect(windows.dependencies.find((item) => item.name === 'mbedtls').sha256).toBe(
      gplPins.mbedtls.sha256,
    );
    expect(script).toContain('--enable-mbedtls');
    expect(script).toContain('Libs.private: -lbcrypt -lws2_32');
    expect(script).toContain('prefix=${PREFIX}');
    expect(gpl).toContain('--enable-mbedtls');
    expect(pins.dependencies.find((item) => item.name === 'mbedtls').sha256).toBe(
      gplPins.mbedtls.sha256,
    );
  });

  it('copies the recipe, pins, and Mach-O checkers into the source offer', () => {
    expect(recipe).toContain('mach-minos.py');
    expect(recipe).toContain('mach-deps.py');
    expect(recipe).toContain('"$SCRIPT_DIR/mach-minos.py"');
    expect(recipe).toContain('cp "$SCRIPT_DIR/mach-deps.py"');
  });
});
