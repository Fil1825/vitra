/** Width of the distorted band along each edge, in px. */
const EDGE_PX = 22;

/** Maximum displacement in px at refraction strength 1. */
export const MAX_DISPLACEMENT_PX = 36;

function svgUri(svg: string): string {
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/**
 * One displacement channel as an SVG gradient: full value at the start edge, neutral (128) in
 * the middle, zero at the end edge. Near the edges the backdrop is sampled from further inside,
 * which bends it like the rim of a lens.
 */
function channelMap(channel: 'r' | 'g', edge: number): string {
  const color = (v: number) => (channel === 'r' ? `rgb(${v},0,0)` : `rgb(0,${v},0)`);
  const [x2, y2] = channel === 'r' ? [1, 0] : [0, 1];
  return svgUri(
    `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" preserveAspectRatio="none">` +
      `<linearGradient id="g" x2="${x2}" y2="${y2}">` +
      `<stop offset="0" stop-color="${color(255)}"/>` +
      `<stop offset="${edge}" stop-color="${color(128)}"/>` +
      `<stop offset="${1 - edge}" stop-color="${color(128)}"/>` +
      `<stop offset="1" stop-color="${color(0)}"/>` +
      `</linearGradient><rect width="100" height="100" fill="url(#g)"/></svg>`,
  );
}

/** Horizontal (red) and vertical (green) displacement maps for an element of the given size. */
export function displacementMaps(width: number, height: number): { x: string; y: string } {
  const edge = (size: number) => Number(Math.min(0.5, EDGE_PX / Math.max(size, 1)).toFixed(4));
  return { x: channelMap('r', edge(width)), y: channelMap('g', edge(height)) };
}
