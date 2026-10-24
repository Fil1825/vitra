import { describe, expect, it } from 'vitest';

import { SECTION_IDS, matchingSections, type SettingsSection } from './sections.ts';

const texts = Object.fromEntries(SECTION_IDS.map((id) => [id, [id]])) as Record<
  SettingsSection,
  string[]
>;
texts.sound = ['Звук', 'Эквалайзер', 'Кроссфейд'];
texts.appearance = ['Appearance', 'Theme', 'Accent from artwork'];
texts.lyrics = ['Lyrics', 'Search lyrics online'];

describe('the settings search', () => {
  it('lists every page without a query', () => {
    expect(matchingSections('  ', texts)).toEqual([...SECTION_IDS]);
  });

  it('finds pages by what is on them, in list order', () => {
    expect(matchingSections('эквалайзер', texts)).toEqual(['sound']);
    expect(matchingSections('THEME', texts)).toEqual(['appearance']);
    expect(matchingSections('ar', texts)).toEqual(['appearance', 'library', 'lyrics']);
  });

  it('wants every word somewhere on the page', () => {
    expect(matchingSections('accent artwork', texts)).toEqual(['appearance']);
    expect(matchingSections('accent lyrics', texts)).toEqual([]);
  });

  it('treats ё as е', () => {
    texts.about = ['Ещё'];
    expect(matchingSections('еще', texts)).toEqual(['about']);
  });
});
