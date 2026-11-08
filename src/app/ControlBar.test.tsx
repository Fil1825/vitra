import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { i18n } from '../i18n/index.ts';
import type { LibraryTrack } from '../lib/library.ts';
import {
  INITIAL_PLAYER_STATE,
  type PlayerState,
  type Track,
  type TrackMetadata,
} from '../lib/player.ts';
import { EMPTY_QUEUE, type QueueItem } from '../lib/queue.ts';
import { onCommand, invoke } from '../test/mocks/core.ts';
import { renderUi } from '../test/render.ts';
import { useNavigation } from '../stores/navigation.ts';
import { usePlayer } from '../stores/player.ts';
import { useQueue } from '../stores/queue.ts';
import { useSettings } from '../stores/settings.ts';
import { useUi } from '../stores/ui.ts';
import { ControlBar } from './ControlBar.tsx';

const metadata: TrackMetadata = {
  title: 'Says',
  artist: 'Nils Frahm',
  album: 'Spaces',
  albumArtist: 'Nils Frahm',
  genre: null,
  year: 2013,
  trackNumber: 1,
  discNumber: 1,
  duration: 180,
  format: 'FLAC',
  sampleRate: 44100,
  bitDepth: 16,
  bitrate: null,
  channels: 2,
};

const playing: Track = {
  id: 1,
  path: '/music/says.flac',
  libraryId: null,
  title: 'Says',
  metadata,
  artwork: null,
};

function showTrack(patch: Partial<PlayerState> = {}) {
  usePlayer.setState({
    state: {
      ...INITIAL_PLAYER_STATE,
      status: 'paused',
      position: 65,
      duration: 180,
      volume: 80,
      track: playing,
      ...patch,
    },
    // While playing, the bar moves the time on from this clock. A clock of 0
    // would run it all the way to the end before the assertion.
    positionAt: performance.now(),
  });
}

function queueItem(uid: number): QueueItem {
  return {
    uid,
    path: `/music/${uid}.flac`,
    trackId: uid,
    title: `Track ${uid}`,
    artist: null,
    album: null,
    duration: 180,
    cover: null,
  };
}

const libraryTrack: LibraryTrack = {
  id: 5,
  path: '/music/says.flac',
  title: 'Says',
  artist: 'Nils Frahm',
  artistId: 7,
  artists: [{ id: 7, name: 'Nils Frahm' }],
  album: 'Spaces',
  albumId: 3,
  albumArtist: 'Nils Frahm',
  genre: null,
  year: 2013,
  trackNumber: 1,
  discNumber: 1,
  duration: 180,
  format: 'FLAC',
  sampleRate: 44100,
  bitDepth: 16,
  bitrate: null,
  favorite: false,
  playCount: 2,
  lastPlayedAt: null,
  addedAt: 1_700_000_000,
  cover: null,
  folderId: 1,
  sourceUrl: null,
};

describe('ControlBar', () => {
  it('starts idle, with nothing to play', async () => {
    await renderUi(<ControlBar />);

    expect(screen.getByRole('contentinfo', { name: 'Playback controls' })).toBeInTheDocument();
    expect(screen.getByText('Not playing')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Previous' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
    expect(screen.getByRole('slider', { name: 'Playback position' })).toBeDisabled();
  });

  it('follows the player and queue stores', async () => {
    await renderUi(<ControlBar />);
    expect(screen.getByRole('button', { name: 'Shuffle' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );

    act(() => {
      showTrack({ status: 'playing' });
      useQueue.setState({
        queue: { ...EMPTY_QUEUE, items: [queueItem(1), queueItem(2)], current: 0, shuffle: true },
      });
    });

    expect(screen.getByText('Says')).toBeInTheDocument();
    expect(screen.getByText('Nils Frahm')).toBeInTheDocument();
    expect(screen.getByText('Spaces')).toBeInTheDocument();
    expect(screen.getByText('1:05')).toBeInTheDocument();
    expect(screen.getByText('-1:55')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pause' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Shuffle' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('sends transport, shuffle and repeat commands', async () => {
    const user = userEvent.setup();
    showTrack({ status: 'playing' });
    useQueue.setState({
      queue: { ...EMPTY_QUEUE, items: [queueItem(1), queueItem(2)], current: 0 },
    });
    await renderUi(<ControlBar />);

    await user.click(screen.getByRole('button', { name: 'Pause' }));
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await user.click(screen.getByRole('button', { name: 'Previous' }));
    await user.click(screen.getByRole('button', { name: 'Shuffle' }));
    await user.click(screen.getByRole('button', { name: 'Repeat: off' }));

    expect(invoke).toHaveBeenCalledWith('player_toggle', undefined);
    expect(invoke).toHaveBeenCalledWith('player_next', undefined);
    expect(invoke).toHaveBeenCalledWith('player_previous', undefined);
    expect(invoke).toHaveBeenCalledWith('queue_set_shuffle', { shuffle: true });
    expect(invoke).toHaveBeenCalledWith('queue_set_repeat', { repeat: 'all' });
  });

  it('seeks when the progress bar is released and sets the volume as it moves', async () => {
    showTrack();
    await renderUi(<ControlBar />);

    fireEvent.change(screen.getByRole('slider', { name: 'Playback position' }), {
      target: { value: '90' },
    });
    expect(screen.getByText('1:30')).toBeInTheDocument();
    fireEvent.pointerUp(screen.getByRole('slider', { name: 'Playback position' }));
    expect(invoke).toHaveBeenCalledWith('player_seek', { position: 90 });

    fireEvent.change(screen.getByRole('slider', { name: 'Volume' }), { target: { value: '40' } });
    expect(invoke).toHaveBeenCalledWith('player_set_volume', { volume: 40 });
  });

  it('mutes, changes speed and opens the mini player', async () => {
    const user = userEvent.setup();
    showTrack({ muted: true });
    await renderUi(<ControlBar />);

    await user.click(screen.getByRole('button', { name: 'Unmute' }));
    expect(invoke).toHaveBeenCalledWith('player_set_muted', { muted: false });

    await user.click(screen.getByRole('button', { name: '1×' }));
    await user.click(screen.getByRole('menuitemradio', { name: '1.5×' }));
    expect(invoke).toHaveBeenCalledWith('player_set_speed', { speed: 1.5 });
    expect(screen.queryByRole('menuitemradio')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /^Mini Player/ }));
    expect(invoke).toHaveBeenCalledWith('mini_show');
  });

  it('shows and hides the queue and the lyrics from the layout', async () => {
    const user = userEvent.setup();
    await renderUi(<ControlBar />);

    await user.click(screen.getByRole('button', { name: 'Show queue' }));
    expect(useSettings.getState().settings.layout.hidden).not.toContain('queue');
    expect(screen.getByRole('button', { name: 'Hide queue' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    await user.click(screen.getByRole('button', { name: 'Show lyrics panel' }));
    expect(useSettings.getState().settings.layout.hidden).not.toContain('lyrics');
    expect(screen.getByRole('button', { name: 'Hide lyrics panel' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  it('explains a playback error and opens Now Playing from it', async () => {
    const user = userEvent.setup();
    showTrack({ error: { kind: 'mpvMissing' } });
    await renderUi(<ControlBar />);

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent("wasn't found");
    await user.click(alert);
    expect(useNavigation.getState().route).toEqual({ view: 'nowPlaying' });
  });

  it('opens the artwork viewer, or Now Playing when there is no picture', async () => {
    const user = userEvent.setup();
    await renderUi(<ControlBar />);
    await user.click(screen.getByRole('button', { name: 'Now Playing' }));
    expect(useNavigation.getState().route).toEqual({ view: 'nowPlaying' });

    act(() => {
      showTrack({ track: { ...playing, artwork: { id: 'abc', palette: null } } });
    });
    await user.click(screen.getByRole('button', { name: 'Show artwork' }));
    expect(useUi.getState().artworkView).toMatchObject({
      source: { kind: 'track', path: '/music/says.flac' },
    });
    expect(useUi.getState().artworkView?.preview).toContain('abc');
  });

  it('favorites the library track and, in a wave, skips one', async () => {
    const user = userEvent.setup();
    onCommand('library_track', () => libraryTrack);
    showTrack({ track: { ...playing, libraryId: 5 } });
    useQueue.setState({ queue: { ...EMPTY_QUEUE, wave: { kind: 'taste' } } });
    await renderUi(<ControlBar />);

    await waitFor(() => expect(screen.getByRole('button', { name: 'Like' })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: i18n.t('wave.dislike') }));
    expect(invoke).toHaveBeenCalledWith('wave_dislike', { trackId: 5 });

    await user.click(screen.getByRole('button', { name: 'Like' }));
    expect(invoke).toHaveBeenCalledWith('library_set_favorite', { trackId: 5, favorite: true });

    await user.click(screen.getByRole('button', { name: 'Nils Frahm' }));
    expect(useNavigation.getState().route).toEqual({ view: 'artist', id: 7 });
    await user.click(screen.getByRole('button', { name: 'Says' }));
    expect(useNavigation.getState().route).toEqual({ view: 'album', id: 3 });
  });

  it('does not seek while a link is still opening', async () => {
    showTrack({ status: 'loading' });
    await renderUi(<ControlBar />);

    expect(screen.getByRole('button', { name: i18n.t('player.loading') })).toHaveAttribute(
      'aria-busy',
      'true',
    );
    expect(screen.getByRole('slider', { name: 'Playback position' })).toBeDisabled();
  });
});
