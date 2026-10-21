import { create } from 'zustand';

/** What the main area shows. */
export type Route =
  | { view: 'nowPlaying' }
  | { view: 'recent' }
  | { view: 'albums' }
  | { view: 'album'; id: number }
  | { view: 'artists' }
  | { view: 'artist'; id: number }
  | { view: 'songs' }
  | { view: 'favorites' }
  | { view: 'playlist'; id: number }
  | { view: 'search'; query: string };

const MAX_HISTORY = 50;

interface NavigationState {
  route: Route;
  back: Route[];
  forward: Route[];
  go(route: Route): void;
  /** Changes the route without a history entry (e.g. while typing a search). */
  replace(route: Route): void;
  goBack(): void;
  goForward(): void;
}

/** The album page of a track, if it has an album. */
export function albumRoute(albumId: number | null | undefined): Route | null {
  return albumId == null ? null : { view: 'album', id: albumId };
}

/** The artist page of a track, if it has an artist. */
export function artistRoute(artistId: number | null | undefined): Route | null {
  return artistId == null ? null : { view: 'artist', id: artistId };
}

export function sameRoute(a: Route, b: Route): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export const useNavigation = create<NavigationState>((set, get) => ({
  route: { view: 'albums' },
  back: [],
  forward: [],
  go(route) {
    const { route: current, back } = get();
    if (sameRoute(current, route)) {
      return;
    }
    set({ route, back: [...back, current].slice(-MAX_HISTORY), forward: [] });
  },
  replace(route) {
    set({ route });
  },
  goBack() {
    const { route, back, forward } = get();
    const previous = back.at(-1);
    if (previous) {
      set({ route: previous, back: back.slice(0, -1), forward: [route, ...forward] });
    }
  },
  goForward() {
    const { route, back, forward } = get();
    const [next, ...rest] = forward;
    if (next) {
      set({ route: next, back: [...back, route], forward: rest });
    }
  },
}));
