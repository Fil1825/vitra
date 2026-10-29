import ts from 'typescript';
import { describe, expect, it } from 'vitest';

const components = import.meta.glob('../**/*.tsx', {
  query: '?raw',
  import: 'default',
  eager: true,
});
const brands = new Set(['Vitra', 'yt-dlp']);
const attributes = new Set(['title', 'aria-label', 'placeholder', 'alt']);

describe('translated interface labels', () => {
  it('keeps plain JSX text and accessibility labels in the locale catalog', () => {
    const untranslated: string[] = [];
    for (const [file, text] of Object.entries(components)) {
      if (file.includes('.test.')) continue;
      const source = ts.createSourceFile(
        file,
        text as string,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.TSX,
      );
      const check = (text: string, node: ts.Node) => {
        const label = text.trim();
        if (/\p{L}{2}/u.test(label) && !brands.has(label)) {
          untranslated.push(
            `${file}:${source.getLineAndCharacterOfPosition(node.pos).line + 1}: ${label}`,
          );
        }
      };
      const visit = (node: ts.Node) => {
        if (ts.isJsxText(node)) check(node.text, node);
        if (
          ts.isJsxAttribute(node) &&
          attributes.has(node.name.getText(source)) &&
          node.initializer &&
          ts.isStringLiteral(node.initializer)
        )
          check(node.initializer.text, node);
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
    expect(untranslated).toEqual([]);
  });
});
