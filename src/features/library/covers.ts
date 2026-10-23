import { message, open } from '@tauri-apps/plugin-dialog';

import { i18n } from '../../i18n/index.ts';
import { covers, type CoverTarget } from '../../lib/library.ts';
import type { MenuEntry } from '../../lib/menu.ts';

/** Image files a cover can be made of. */
export const COVER_EXTENSIONS = ['jpg', 'jpeg', 'png', 'webp'] as const;

export function isCoverImage(path: string): boolean {
  const extension = path.split('.').pop()?.toLowerCase() ?? '';
  return (COVER_EXTENSIONS as readonly string[]).includes(extension);
}

/** How a cover marks itself as a drop target (`data-cover-drop`). */
export function coverKey(target: CoverTarget): string {
  return `${target.kind}:${target.id}`;
}

export function parseCoverKey(key: string): CoverTarget | null {
  const [kind, id] = key.split(':');
  const number = Number(id);
  if (!Number.isInteger(number)) {
    return null;
  }
  switch (kind) {
    case 'album':
    case 'artist':
    case 'playlist':
      return { kind, id: number };
    default:
      return null;
  }
}

async function warn(err: unknown): Promise<void> {
  console.error('Changing a cover failed', err);
  await message(i18n.t('library.coverFailed', { error: String(err) }), { kind: 'warning' });
}

/** Makes the image file the cover of `target`. */
export function setCover(target: CoverTarget, path: string): void {
  covers.set(target, path).catch(warn);
}

/** Asks for an image file and makes it the cover. */
export async function chooseCover(target: CoverTarget): Promise<void> {
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: i18n.t('library.coverImages'), extensions: [...COVER_EXTENSIONS] }],
  });
  if (typeof path === 'string') {
    setCover(target, path);
  }
}

/** Goes back to the automatic cover. */
export function resetCover(target: CoverTarget): void {
  covers.reset(target).catch(warn);
}

/** Menu entries for changing a cover; `custom` says whether there is one to reset. */
export function coverMenu(target: CoverTarget, custom: boolean): MenuEntry[] {
  return [
    { label: i18n.t('library.changeCover'), action: () => void chooseCover(target).catch(warn) },
    { label: i18n.t('library.resetCover'), enabled: custom, action: () => resetCover(target) },
  ];
}
