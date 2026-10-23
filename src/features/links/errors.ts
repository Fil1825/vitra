import type { TFunction } from 'i18next';

/** A readable message for errors with a remedy (age limits, sign-in, cookies), else `null`. */
export function knownLinkError(message: string, t: TFunction): string | null {
  if (message === 'the video is age-restricted') {
    return t('links.errors.ageRestricted');
  }
  if (message === 'the site asks to sign in') {
    return t('links.errors.signIn');
  }
  if (message.startsWith("cannot read the browser's cookies")) {
    return t('links.errors.cookies');
  }
  return null;
}

/** A readable message for a failed link command (the backend sends English text). */
export function linkError(err: unknown, t: TFunction): string {
  const message = err instanceof Error ? err.message : String(err);
  const known = knownLinkError(message, t);
  if (known !== null) {
    return known;
  }
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
