/**
 * A theme's `custom.css` is someone else's text running in the app's page, so it is cleaned
 * before it is used: it may restyle anything, but it may not load anything from outside the
 * theme. `url()` may only name the theme's own assets (rewritten to `theme://` addresses);
 * `@import`, `image-set()` (whose strings are addresses too), and old script hooks
 * (`expression()`, `behavior`, `-moz-binding`, `javascript:`) are taken out. The CSP stops
 * scripts anyway; this keeps a theme from calling home with a picture.
 */

/** The longest custom.css kept (the theme package allows a little more raw text). */
export const MAX_CSS_LENGTH = 256 * 1024;

const ASSET = /^(?:\.\/)?(assets\/[A-Za-z0-9][A-Za-z0-9._-]{0,63}\.[a-z0-9]+)$/;

/** Declarations or rules containing these are dropped whole. */
const FORBIDDEN = /expression\s*\(|image-set\s*\(|-moz-binding|\bbehavior\s*:|javascript\s*:/i;

export interface CleanCss {
  css: string;
  /** What was taken out, for the importer or editor to show. */
  removed: string[];
}

/**
 * Cleans `input`. `assetUrl` turns an asset path from the theme (`assets/bg.png`) into the
 * address the page loads it from, or returns null for an asset the theme doesn't have.
 */
export function sanitizeCss(input: string, assetUrl: (path: string) => string | null): CleanCss {
  const removed: string[] = [];
  let source = input;
  if (source.length > MAX_CSS_LENGTH) {
    source = source.slice(0, MAX_CSS_LENGTH);
    removed.push(`everything after ${MAX_CSS_LENGTH} characters`);
  }

  // Strings are set aside as placeholders (between private-use marks), so nothing below mistakes their text for CSS
  // (and they come back as they were). Comments go, url() is resolved right away.
  const strings: string[] = [];
  const hold = (text: string) => `\uE000${strings.push(text) - 1}\uE000`;
  let text = '';
  let i = 0;
  while (i < source.length) {
    const char = source[i] as string;
    if (char === '/' && source[i + 1] === '*') {
      const end = source.indexOf('*/', i + 2);
      i = end === -1 ? source.length : end + 2;
      text += ' ';
    } else if (char === '"' || char === "'") {
      const [literal, next] = readString(source, i);
      text += hold(literal);
      i = next;
    } else if (/^url\s*\(/i.test(source.slice(i, i + 8)) && !/[\w-]/.test(text.at(-1) ?? '')) {
      const [argument, next] = readUrl(source, i);
      const path = ASSET.exec(argument)?.[1];
      const resolved = path ? assetUrl(path) : null;
      if (resolved) {
        text += `url(${hold(JSON.stringify(resolved))})`;
      } else {
        text += 'none';
        removed.push(`url(${argument})`);
      }
      i = next;
    } else if (char === '\\') {
      // An escape outside a string could spell a function name (`u\72l(`): drop it.
      removed.push('an escaped character');
      i += 2;
    } else if (char === '\uE000') {
      i += 1;
    } else {
      text += char;
      i += 1;
    }
  }

  text = text.replace(/@(?:import|charset|namespace)\b[^;{}]*(?:;|$)/gi, (rule) => {
    removed.push(rule.trim());
    return '';
  });
  // A declaration ends at `;` or at the `}` of its block. One pass, not a regex: a regex here
  // backtracks over long runs without delimiters and takes minutes on a hostile file.
  let kept = '';
  let from = 0;
  for (let j = 0; j <= text.length; j += 1) {
    const char = text[j];
    if (j < text.length && char !== ';' && char !== '{' && char !== '}') continue;
    const segment = text.slice(from, j);
    const delimiter = char ?? '';
    // Selectors (before `{`) load nothing, so only declarations are checked.
    if (delimiter !== '{' && FORBIDDEN.test(restore(segment, strings))) {
      removed.push(restore(segment, strings).trim());
      kept += delimiter === ';' ? '' : delimiter;
    } else {
      kept += segment + delimiter;
    }
    from = j + 1;
  }
  text = kept;

  return { css: restore(text, strings).trim(), removed };
}

/** A quoted string from `start` (its opening quote): the literal as written, and the index after it. */
function readString(source: string, start: number): [string, number] {
  const quote = source[start];
  let i = start + 1;
  while (i < source.length) {
    const char = source[i];
    if (char === '\\') {
      i += 2;
    } else if (char === quote || char === '\n') {
      return [source.slice(start, i + 1), i + 1];
    } else {
      i += 1;
    }
  }
  // An unclosed string: CSS ends it at the end of the text.
  return [`${source.slice(start)}${quote}`, source.length];
}

/** The argument of `url(` at `start` (quotes taken off, escapes left in), and the index after `)`. */
function readUrl(source: string, start: number): [string, number] {
  let i = source.indexOf('(', start) + 1;
  while (/\s/.test(source[i] ?? '')) i += 1;
  let argument = '';
  if (source[i] === '"' || source[i] === "'") {
    const [literal, next] = readString(source, i);
    argument = literal.slice(1, -1);
    i = next;
  }
  const close = source.indexOf(')', i);
  const end = close === -1 ? source.length : close;
  argument = (argument || source.slice(i, end)).trim();
  return [argument, end + 1];
}

function restore(text: string, strings: readonly string[]): string {
  return text.replace(/\uE000(\d+)\uE000/g, (_, index: string) => strings[Number(index)] ?? '');
}
