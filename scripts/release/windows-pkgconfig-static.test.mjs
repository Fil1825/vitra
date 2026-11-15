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
});
