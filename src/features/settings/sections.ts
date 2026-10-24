import type { ParseKeys } from 'i18next';
import type { LucideIcon } from 'lucide-react';
import {
  AudioLines,
  Info,
  Keyboard,
  LayoutDashboard,
  Library,
  Link2,
  MicVocal,
  Palette,
  PictureInPicture2,
  Settings2,
} from 'lucide-react';

import { ACTION_IDS } from '../../shortcuts/actions.ts';

/** The pages of Settings, in the order of their list. */
export const SECTION_IDS = [
  'general',
  'appearance',
  'sound',
  'library',
  'links',
  'lyrics',
  'mini',
  'layout',
  'shortcuts',
  'about',
] as const;

export type SettingsSection = (typeof SECTION_IDS)[number];

export interface SectionInfo {
  icon: LucideIcon;
  /** The page's name. */
  title: ParseKeys;
  /** What is on the page, for the search field. */
  keywords: readonly ParseKeys[];
}

export const SECTIONS: Record<SettingsSection, SectionInfo> = {
  general: {
    icon: Settings2,
    title: 'settings.sections.general',
    keywords: [
      'settings.language',
      'settings.general.trayIcon.macos',
      'settings.general.trayIcon.windows',
      'settings.general.trayIcon.linux',
      'settings.general.globalShortcuts',
      'tray.miniPlayer',
    ],
  },
  appearance: {
    icon: Palette,
    title: 'settings.appearance',
    keywords: [
      'settings.theme',
      'themes.new',
      'themes.import',
      'settings.windowMaterial',
      'settings.motion',
      'settings.transparency',
      'settings.refraction',
      'settings.vinyl',
      'settings.artworkAccent',
      'settings.backdropPulse',
      'settings.powerSaving',
    ],
  },
  sound: {
    icon: AudioLines,
    title: 'settings.sections.sound',
    keywords: [
      'sound.equalizer',
      'sound.preamp',
      'sound.preset',
      'sound.normalization',
      'sound.crossfade',
    ],
  },
  library: {
    icon: Library,
    title: 'settings.library.title',
    keywords: [
      'settings.library.folders',
      'settings.library.addFolder',
      'settings.library.rescan',
      'settings.library.addOpenedFiles',
      'settings.library.splitArtists',
      'settings.library.exceptions',
    ],
  },
  links: {
    icon: Link2,
    title: 'settings.sections.links',
    keywords: [
      'settings.downloads.folder',
      'settings.downloads.format',
      'settings.downloads.bitrate',
      'settings.downloads.template',
      'settings.downloads.albumTemplate',
      'settings.downloads.cookies',
      'settings.downloads.update',
    ],
  },
  lyrics: {
    icon: MicVocal,
    title: 'settings.lyrics.title',
    keywords: ['settings.lyrics.visible', 'settings.lyrics.online', 'settings.lyrics.translation'],
  },
  mini: {
    icon: PictureInPicture2,
    title: 'settings.mini.title',
    keywords: [
      'settings.mini.mode',
      'settings.mini.overFullScreen',
      'settings.mini.snapToEdges',
      'settings.mini.rememberPosition',
      'settings.mini.hideMainWindow',
    ],
  },
  layout: {
    icon: LayoutDashboard,
    title: 'layout.title',
    keywords: ['layout.customizeButton', 'layout.spectrum'],
  },
  shortcuts: {
    icon: Keyboard,
    title: 'shortcuts.title',
    keywords: [
      'shortcuts.everywhere',
      'settings.general.globalShortcuts',
      ...ACTION_IDS.map((id) => `shortcuts.actions.${id}` as const),
    ],
  },
  about: {
    icon: Info,
    title: 'about.title',
    keywords: ['about.version', 'about.support', 'about.license', 'settings.diagnostics'],
  },
};

/** Text as the search compares it: lower case, ё as е. */
export function fold(text: string): string {
  return text.toLocaleLowerCase().replaceAll('ё', 'е');
}

/**
 * The pages whose name or contents (`texts`, already translated) contain every word of
 * `query`, in list order; every page for an empty query.
 */
export function matchingSections(
  query: string,
  texts: Record<SettingsSection, readonly string[]>,
): SettingsSection[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...SECTION_IDS];
  return SECTION_IDS.filter((id) => {
    const haystack = texts[id].map(fold);
    return words.every((word) => haystack.some((text) => text.includes(word)));
  });
}
