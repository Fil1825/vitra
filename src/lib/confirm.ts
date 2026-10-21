import { ask } from '@tauri-apps/plugin-dialog';

import { i18n } from '../i18n/index.ts';

/** A native yes/no warning with translated buttons (the system's default is English). */
export function confirm(message: string, action: 'remove' | 'delete'): Promise<boolean> {
  return ask(message, {
    kind: 'warning',
    okLabel: i18n.t(`dialog.${action}`),
    cancelLabel: i18n.t('dialog.cancel'),
  });
}
