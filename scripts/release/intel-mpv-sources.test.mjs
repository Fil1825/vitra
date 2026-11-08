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
    expect(recipe).toContain('MACOSX_DEPLOYMENT_TARGET=12.0');
    expect(recipe).toContain('checksum mismatch');
    expect(recipe).toContain('--max 12.0');
    expect(recipe).not.toContain('stolendata');
    expect(planFor('x86_64-apple-darwin').mpv.sources).toBe(
      'scripts/release/intel-mpv-sources.json',
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
    expect(recipe).toContain('MACOSX_DEPLOYMENT_TARGET=12.0');
    expect(recipe).toContain('-D{gl,iconv,plain-gl}=enabled');
  });

  it('copies the recipe, pins, and Mach-O checkers into the source offer', () => {
    expect(recipe).toContain('mach-minos.py');
    expect(recipe).toContain('mach-deps.py');
    expect(recipe).toContain('"$SCRIPT_DIR/mach-minos.py"');
    expect(recipe).toContain('cp "$SCRIPT_DIR/mach-deps.py"');
  });
});
