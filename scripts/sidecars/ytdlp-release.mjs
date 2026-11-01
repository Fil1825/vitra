// Official yt-dlp release key, the upstream `public.key`
// (https://github.com/yt-dlp/yt-dlp/blob/master/public.key), copied here so
// `fetch-sidecars` does not depend on the in-app updater's copy. The fingerprint
// is pinned in code: replacing the key file must not bless a checksum.
// Checked with `gpg --show-keys` on 2026-11-01:
//   AC0C BBE6 848D 6A87 3464 AF4E 57CF 6593 3B5A 7581
//   Simon Sawicki (yt-dlp signing key) <contact@grub4k.xyz>

/** Repo-relative path of the armored public key. */
export const YT_DLP_RELEASE_KEY = 'scripts/sidecars/yt-dlp-public.key';

/** Primary fingerprint, uppercase hex, no spaces. */
export const YT_DLP_KEY_FINGERPRINT = 'AC0CBBE6848D6A873464AF4E57CF65933B5A7581';
