import { describe, expect, it } from 'vitest';

import { parseYear } from './year.ts';

describe('parseYear', () => {
  it('reads years and empty input', () => {
    expect(parseYear('1999')).toBe(1999);
    expect(parseYear(' 2024 ')).toBe(2024);
    expect(parseYear('')).toBeNull();
    expect(parseYear('  ')).toBeNull();
  });

  it('rejects anything else', () => {
    expect(parseYear('99a')).toBeUndefined();
    expect(parseYear('-5')).toBeUndefined();
    expect(parseYear('12.5')).toBeUndefined();
    expect(parseYear('20000')).toBeUndefined();
  });
});
