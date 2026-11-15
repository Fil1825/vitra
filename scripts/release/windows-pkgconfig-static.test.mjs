import { describe, expect, it } from 'vitest';

import { rewritePkgconfigLibs } from './windows-pkgconfig-static.mjs';

const archives = ['libz.a', 'libiconv.a', 'libcharset.a', 'libfreetype.a', 'libharfbuzz.a'];

describe('Windows pkg-config static rewrite', () => {
  it('points iconv and zlib at their static archives and keeps system import libs', () => {
    const text = `Libs: -L\${libdir} -liconv -lz -lfreetype -lharfbuzz -lshcore -lavicap32 -lgdi32
Libs.private: -lcharset -lws2_32
`;
    expect(rewritePkgconfigLibs(text, archives)).toBe(
      `Libs: -L\${libdir} -l:libiconv.a -l:libz.a -l:libfreetype.a -l:libharfbuzz.a -lshcore -lavicap32 -lgdi32
Libs.private: -l:libcharset.a -lws2_32
`,
    );
  });

  it('rejects MSYS2 libraries the recipe does not build', () => {
    const text = 'Libs.private: -lbz2 -llzma -lpng16 -lbrotlidec -lexpat -lfontconfig -lglib-2.0\n';
    expect(() => rewritePkgconfigLibs(text, archives)).toThrow(/-lbz2/);
    expect(() => rewritePkgconfigLibs('Libs: -lshaderc_shared\n', archives)).toThrow(
      /shaderc_shared/,
    );
  });

  it('rewrites the mingw libavdevice line once those crt archives are named', () => {
    const text =
      'Libs: -L${libdir}  -lavdevice -lpsapi -lole32 -lstrmiids -luuid -loleaut32 -lshlwapi -lgdi32 -lm -latomic -lvfw32\n';
    const crt = [
      'libavdevice.a',
      'libpsapi.a',
      'libole32.a',
      'libstrmiids.a',
      'libuuid.a',
      'liboleaut32.a',
      'libshlwapi.a',
      'libgdi32.a',
      'libm.a',
      'libatomic.a',
      'libvfw32.a',
    ];
    expect(rewritePkgconfigLibs(text, crt, [])).toBe(
      'Libs: -L${libdir}  -l:libavdevice.a -l:libpsapi.a -l:libole32.a -l:libstrmiids.a -l:libuuid.a -l:liboleaut32.a -l:libshlwapi.a -l:libgdi32.a -l:libm.a -l:libatomic.a -l:libvfw32.a\n',
    );
    expect(() => rewritePkgconfigLibs(text, ['libavdevice.a'], [])).toThrow(/-lpsapi/);
  });

  it('rejects the SPIRV-Tools shared pkg-config line from the pinned cmake template', () => {
    // SPIRV-Tools 71f287f cmake/SPIRV-Tools-shared.pc.in substitutes
    // SPIRV_SHARED_LIBRARIES as -lSPIRV-Tools-shared. There is no static archive.
    const shared = 'Libs: -L${libdir} -lSPIRV-Tools-shared\nCflags: -I${includedir}\n';
    expect(() =>
      rewritePkgconfigLibs(shared, [
        ...archives,
        'libSPIRV-Tools.a',
        'libSPIRV-Tools-opt.a',
        'libSPIRV-Tools-link.a',
      ]),
    ).toThrow(/-lSPIRV-Tools-shared/);
    const staticPc = 'Libs: -L${libdir} -lSPIRV-Tools-link -lSPIRV-Tools-opt -lSPIRV-Tools\n';
    expect(
      rewritePkgconfigLibs(staticPc, [
        'libSPIRV-Tools.a',
        'libSPIRV-Tools-opt.a',
        'libSPIRV-Tools-link.a',
      ]),
    ).toBe(
      'Libs: -L${libdir} -l:libSPIRV-Tools-link.a -l:libSPIRV-Tools-opt.a -l:libSPIRV-Tools.a\n',
    );
  });
});
