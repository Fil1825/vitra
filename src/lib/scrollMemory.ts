import type { Route } from '../stores/navigation.ts';

/**
 * Where each screen of the main area was scrolled, for this session only: going back to a
 * playlist (from the sidebar, Back / Forward, after Now Playing) shows the same rows. The
 * main area is remounted per route, so the position is put back before its first paint.
 */

/** The part of a scroll container the memory reads and writes (a DOM element in the app). */
export interface Scroller {
  scrollTop: number;
  readonly scrollHeight: number;
  readonly clientHeight: number;
  readonly dataset: DOMStringMap;
}

const positions = new Map<string, number>();
/** Scrollers whose saved position was already put back (or that the user scrolled). */
const settled = new WeakSet<object>();

/** One key per screen: the view plus its playlist, album, artist or search words. */
export function routeKey(route: Route): string {
  switch (route.view) {
    case 'album':
    case 'artist':
    case 'playlist':
      return `${route.view}:${route.id}`;
    case 'search':
      return `search:${route.query}`;
    default:
      return route.view;
  }
}

export function rememberScroll(key: string, top: number): void {
  if (top > 0) {
    positions.set(key, top);
  } else {
    positions.delete(key);
  }
}

export function savedScroll(key: string): number {
  return positions.get(key) ?? 0;
}

/** Forgets every position (tests). */
export function forgetScrolls(): void {
  positions.clear();
}

/** `top` within what a list of this height can scroll to. */
export function clampScroll(top: number, scrollHeight: number, clientHeight: number): number {
  return Math.max(0, Math.min(top, scrollHeight - clientHeight));
}

/**
 * Puts the saved position of the screen in `scroller.dataset.scrollKey` back, once. Called
 * by the main area and by virtual lists before they measure (React runs a child's layout
 * effects before its parent's, so whichever comes first does it). Returns whether the
 * whole position fit: a list still loading may need another call when it has grown.
 */
export function restoreScroll(scroller: Scroller): boolean {
  const key = scroller.dataset.scrollKey;
  if (key === undefined || settled.has(scroller)) {
    return true;
  }
  const wanted = savedScroll(key);
  const top = clampScroll(wanted, scroller.scrollHeight, scroller.clientHeight);
  if (scroller.scrollTop !== top) {
    scroller.scrollTop = top;
  }
  const fits = top >= wanted;
  if (fits) {
    settled.add(scroller);
  }
  return fits;
}

/** The user scrolled (or the screen is done loading): nothing more to put back. */
export function settleScroll(scroller: Scroller): void {
  settled.add(scroller);
}
