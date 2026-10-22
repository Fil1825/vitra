import type { TFunction } from 'i18next';

/** A readable message for a failed link command (the backend sends English text). */
export function linkError(err: unknown, t: TFunction): string {
  const message = err instanceof Error ? err.message : String(err);
  if (message.startsWith('not a web link')) {
    return t('links.errors.invalid');
  }
  if (message === 'yt-dlp was not found') {
    return t('links.errors.ytDlpMissing');
  }
  if (message === 'yt-dlp did not answer in time') {
    return t('links.errors.timeout');
  }
  if (message.startsWith('Unsupported URL')) {
    return t('links.errors.unsupported');
  }
  return t('links.errors.failed', { message });
}
