import mono from '../../../assets/brand/mono.svg?raw';

// The one-color logo, drawn in `currentColor` so CSS gives it the theme's color. Its title
// would show as a tooltip; the placeholder is decoration.
const markup = mono.replace(/<title>.*?<\/title>/s, '').replaceAll('#FFFFFF', 'currentColor');

/** Vitra's symbol where a track or album has no cover (`--vitra-artwork-placeholder`). */
export function ArtworkPlaceholder({ className }: { className?: string }) {
  return (
    <span
      className={className}
      aria-hidden="true"
      // Our own asset, read at build time: no user content gets in.
      dangerouslySetInnerHTML={{ __html: markup }}
    />
  );
}
