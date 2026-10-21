import { describe, expect, it } from 'vitest';

import { displacementMaps } from './refraction.ts';

const decode = (uri: string) => decodeURIComponent(uri.replace('data:image/svg+xml,', ''));

describe('displacementMaps', () => {
  it('uses red for horizontal and green for vertical displacement', () => {
    const { x, y } = displacementMaps(400, 200);
    expect(decode(x)).toContain('rgb(255,0,0)');
    expect(decode(x)).toContain('x2="1" y2="0"');
    expect(decode(y)).toContain('rgb(0,255,0)');
    expect(decode(y)).toContain('x2="0" y2="1"');
  });

  it('keeps the distorted band the same width in px', () => {
    const { x, y } = displacementMaps(440, 220);
    expect(decode(x)).toContain('offset="0.05"'); // 22 / 440
    expect(decode(y)).toContain('offset="0.1"'); // 22 / 220
  });

  it('never lets the edge bands overlap on tiny elements', () => {
    expect(decode(displacementMaps(10, 10).x)).toContain('offset="0.5"');
  });
});
