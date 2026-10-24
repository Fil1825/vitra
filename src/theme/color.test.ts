import { describe, expect, it } from 'vitest';

import { joinColor, mixColors, splitColor } from './color.ts';

describe('theme colors', () => {
  it('splits every hex form into a color and an opacity', () => {
    expect(splitColor('#3FA9F5')).toEqual({ rgb: '#3fa9f5', alpha: 1 });
    expect(splitColor('#EAF6FFA3')).toEqual({ rgb: '#eaf6ff', alpha: 0.64 });
    expect(splitColor('#fff')).toEqual({ rgb: '#ffffff', alpha: 1 });
    expect(splitColor('#0008')).toEqual({ rgb: '#000000', alpha: 0.53 });
    expect(splitColor('red')).toBeNull();
    expect(splitColor('#12345')).toBeNull();
  });

  it('joins them back, dropping full opacity', () => {
    expect(joinColor('#3fa9f5', 1)).toBe('#3FA9F5');
    expect(joinColor('#eaf6ff', 0.64)).toBe('#EAF6FFA3');
    expect(joinColor('#000000', 0)).toBe('#00000000');
    expect(joinColor('#fff', 2)).toBe('#FFFFFF');
    expect(joinColor('nope', 0.5)).toBe('#00000080');
  });

  it('mixes two colors', () => {
    expect(mixColors('#000000', '#FFFFFF', 0.5)).toBe('#808080');
    expect(mixColors('#ff0000', '#0000ff80', 0.25)).toBe('#bf0040');
    expect(mixColors('#123', '#123456', 0)).toBe('#112233');
    expect(mixColors('red', '#fff', 0.5)).toBe('red');
  });
});
