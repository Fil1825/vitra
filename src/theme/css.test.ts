import { describe, expect, it } from 'vitest';

import { MAX_CSS_LENGTH, sanitizeCss } from './css.ts';

const assets = new Set(['assets/bg.png', 'assets/Inter.woff2']);
const assetUrl = (path: string) =>
  assets.has(path) ? `theme://localhost/sunset/${path.slice('assets/'.length)}` : null;
const clean = (css: string) => sanitizeCss(css, assetUrl);

describe('sanitizeCss', () => {
  it('keeps ordinary CSS as it is', () => {
    const css = `.sidebar { color: var(--vitra-color-text); }
[data-part="controls"]:hover { content: "a;b}c"; border-radius: 4px }
@media (prefers-color-scheme: dark) { :root { --x: 1px; } }`;
    expect(clean(css)).toEqual({ css, removed: [] });
  });

  it("points the theme's own assets at the theme scheme", () => {
    const { css, removed } = clean(
      `@font-face { font-family: Inter; src: url(assets/Inter.woff2) format("woff2"); }
body { background: url( './assets/bg.png' ) }`,
    );
    expect(css).toContain('src: url("theme://localhost/sunset/Inter.woff2") format("woff2")');
    expect(css).toContain('background: url("theme://localhost/sunset/bg.png")');
    expect(removed).toEqual([]);
  });

  it('loads nothing from anywhere else', () => {
    const { css, removed } = clean(
      `a { background: url(https://tracker.example/p.gif) }
b { background-image: URL("assets/missing.png"); }
c { background: url(data:image/png;base64,AAAA) }
d { background: url(  "assets/../theme.json" ) }`,
    );
    expect(css).not.toMatch(/tracker|missing|data:|theme\.json/);
    expect(css).toContain('a { background: none }');
    expect(removed).toHaveLength(4);
  });

  it('takes out imports and script hooks', () => {
    const { css, removed } = clean(
      `@import url("https://evil.example/x.css");
@import 'other.css';
@charset "utf-8";
a { color: red; width: expression(alert(1)); height: 1px }
b { behavior: url(x.htc); -moz-binding: url(x.xml#y); color: blue }
c { background-image: image-set("https://evil.example/a.png" 1x); }
d { background: url(javascript:alert(1)) }`,
    );
    expect(css).not.toMatch(/import|evil|expression|behavior|binding|image-set|javascript/i);
    expect(css).toContain('a { color: red; height: 1px }');
    expect(css).toContain('b { color: blue }');
    expect(removed.length).toBeGreaterThanOrEqual(7);
  });

  it('sees through escapes and comments', () => {
    const { css } = clean(
      `a { background: u\\72l(https://evil.example/x.png) }
b { background: ur/**/l(https://evil.example/y.png) }
c { width: exp/* */ression(alert(1)) }
e { content: "\\201C quoted \\201D" }`,
    );
    expect(css).not.toMatch(/url\(https|expression\(/);
    // Escapes inside strings are fine: they can't call anything.
    expect(css).toContain('content: "\\201C quoted \\201D"');
  });

  it('keeps url-like text inside strings as text', () => {
    const css = 'a::after { content: "url(https://example.com)" }';
    expect(clean(css)).toEqual({ css, removed: [] });
  });

  it('caps the length', () => {
    const { css, removed } = clean(`a{}${' '.repeat(MAX_CSS_LENGTH)}b{color:red}`);
    expect(css).toBe('a{}');
    expect(removed).toHaveLength(1);
  });
});
