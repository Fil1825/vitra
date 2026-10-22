import { describe, expect, it } from 'vitest';

import { firstUri } from './useLinkInput.ts';

describe('firstUri', () => {
  it('skips comments and blank lines', () => {
    expect(firstUri('# title\r\n\r\nhttps://youtu.be/x\r\nhttps://other')).toBe(
      'https://youtu.be/x',
    );
    expect(firstUri('')).toBeNull();
    expect(firstUri('# only a comment')).toBeNull();
  });
});
