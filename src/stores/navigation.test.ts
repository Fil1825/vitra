import { describe, expect, it } from 'vitest';

import { albumRoute, artistRoute, sameRoute, useNavigation } from './navigation.ts';

describe('track routes', () => {
  it('link only what the library knows', () => {
    expect(albumRoute(3)).toEqual({ view: 'album', id: 3 });
    expect(artistRoute(0)).toEqual({ view: 'artist', id: 0 });
    expect(albumRoute(null)).toBeNull();
    expect(artistRoute(undefined)).toBeNull();
  });
});

describe('navigation history', () => {
  it('goes back and forward', () => {
    const nav = useNavigation.getState();
    nav.go({ view: 'songs' });
    nav.go({ view: 'album', id: 1 });
    useNavigation.getState().goBack();
    expect(useNavigation.getState().route).toEqual({ view: 'songs' });
    useNavigation.getState().goForward();
    expect(sameRoute(useNavigation.getState().route, { view: 'album', id: 1 })).toBe(true);
  });
});
