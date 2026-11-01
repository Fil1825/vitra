import { act, fireEvent, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { INITIAL_PLAYER_STATE, type Track, type TrackMetadata } from '../../lib/player.ts';
import type { ImportEntry, ImportedTrack } from '../../lib/import.ts';
import type { LibraryTrack } from '../../lib/library.ts';
import { renderUi, settle } from '../../test/render.ts';
import { invoke } from '../../test/mocks/core.ts';
import { Menu } from '../../test/mocks/menu.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { usePlayer } from '../../stores/player.ts';
import { TrackTable, type TrackTableProps } from './TrackTable.tsx';
import type { Column } from './sort.ts';

const columns: Column[] = ['number', 'title', 'artist', 'album', 'time', 'favorite'];

const metadata: TrackMetadata = {
  title: null,
  artist: null,
  album: null,
  albumArtist: null,
  genre: null,
  year: null,
  trackNumber: null,
  discNumber: null,
  duration: null,
  format: null,
  sampleRate: null,
  bitDepth: null,
  bitrate: null,
  channels: null,
};

function track(partial: Partial<LibraryTrack> & Pick<LibraryTrack, 'id' | 'title'>): LibraryTrack {
  return {
    path: `/music/${partial.id}.flac`,
    artist: 'Nils Frahm',
    artistId: 7,
    artists: [{ id: 7, name: 'Nils Frahm' }],
    album: 'Spaces',
    albumId: 3,
    albumArtist: 'Nils Frahm',
    genre: null,
    year: 2013,
    trackNumber: partial.id,
    discNumber: 1,
    duration: 180,
    format: 'FLAC',
    sampleRate: 44100,
    bitDepth: 16,
    bitrate: null,
    favorite: false,
    playCount: 0,
    lastPlayedAt: null,
    addedAt: 1_700_000_000,
    cover: null,
    folderId: 1,
    sourceUrl: null,
    ...partial,
  };
}

function playerTrack(libraryId: number): Track {
  return {
    id: libraryId,
    path: `/music/${libraryId}.flac`,
    libraryId,
    title: 'Says',
    metadata,
    artwork: null,
  };
}

const imported: ImportedTrack = {
  title: 'Lost',
  artists: ['A'],
  album: null,
  duration: null,
  isrc: null,
  url: null,
  path: null,
  swappable: true,
};

function entry(partial: Partial<ImportEntry>): ImportEntry {
  return {
    id: 1,
    playlistId: 4,
    source: imported,
    state: 'doubtful',
    candidates: [],
    trackId: 1,
    ...partial,
  };
}

function option(index: number): HTMLElement {
  const row = screen.getAllByRole('option')[index];
  if (!row) throw new Error(`Row ${index} is not rendered`);
  return row;
}

function renderTable(props: Partial<TrackTableProps> = {}) {
  return renderUi(
    <TrackTable
      tracks={[track({ id: 1, title: 'Says' }), track({ id: 2, title: 'Familiar' })]}
      columns={columns}
      {...props}
    />,
  );
}

describe('TrackTable', () => {
  it('lists titles, artists and durations', async () => {
    await renderTable();

    expect(screen.getByRole('listbox', { name: 'Songs' })).toBeInTheDocument();
    expect(option(0)).toHaveTextContent('Says');
    expect(option(0)).toHaveTextContent('Nils Frahm');
    expect(option(0)).toHaveTextContent('Spaces');
    expect(option(0)).toHaveTextContent('3:00');
    expect(option(0)).toHaveAttribute('aria-selected', 'false');
  });

  it('shows a cover from its id', async () => {
    await renderTable({
      covers: true,
      tracks: [track({ id: 1, title: 'Says', cover: 'cafebabe' })],
    });

    expect(document.querySelector('img')?.getAttribute('src')).toContain('cafebabe');
  });

  it('renders an empty list', async () => {
    await renderTable({ tracks: [] });
    expect(screen.queryAllByRole('option')).toHaveLength(0);
    expect(screen.getByRole('listbox', { name: 'Songs' })).toBeInTheDocument();
  });

  it('selects a range with shift and toggles with ctrl', async () => {
    const user = userEvent.setup();
    await renderTable({
      tracks: [
        track({ id: 1, title: 'A' }),
        track({ id: 2, title: 'B' }),
        track({ id: 3, title: 'C' }),
      ],
    });

    await user.click(option(0));
    await user.keyboard('{Shift>}');
    await user.click(option(2));
    await user.keyboard('{/Shift}');
    expect(option(0)).toHaveAttribute('aria-selected', 'true');
    expect(option(1)).toHaveAttribute('aria-selected', 'true');
    expect(option(2)).toHaveAttribute('aria-selected', 'true');

    await user.keyboard('{Control>}');
    await user.click(option(1));
    await user.keyboard('{/Control}');
    expect(option(1)).toHaveAttribute('aria-selected', 'false');
    expect(option(0)).toHaveAttribute('aria-selected', 'true');
  });

  it('drops the selection when the list is replaced', async () => {
    const user = userEvent.setup();
    const view = await renderTable();
    await user.click(option(0));
    expect(option(0)).toHaveAttribute('aria-selected', 'true');

    view.rerender(<TrackTable tracks={[track({ id: 9, title: 'New' })]} columns={columns} />);
    await settle();
    expect(option(0)).toHaveTextContent('New');
    expect(option(0)).toHaveAttribute('aria-selected', 'false');
  });

  it('moves with the keyboard and plays the focused row on Enter', async () => {
    const user = userEvent.setup();
    await renderTable();
    option(0).focus();

    await user.keyboard('{ArrowDown}');
    expect(option(1)).toHaveAttribute('aria-selected', 'true');
    expect(option(1)).toHaveFocus();

    await user.keyboard('{Enter}');
    expect(invoke).toHaveBeenCalledWith('queue_play', { trackIds: [1, 2], start: 1 });
  });

  it('sorts by a column, then the other way, then back to the original order', async () => {
    const user = userEvent.setup();
    await renderTable({
      sortable: true,
      tracks: [track({ id: 1, title: 'Zebra' }), track({ id: 2, title: 'Alpha' })],
    });
    const title = () => screen.getByRole('button', { name: 'Title' });

    await user.click(title());
    expect(title()).toHaveAttribute('aria-sort', 'ascending');
    expect(option(0)).toHaveTextContent('Alpha');

    await user.click(title());
    expect(title()).toHaveAttribute('aria-sort', 'descending');
    expect(option(0)).toHaveTextContent('Zebra');

    await user.click(title());
    expect(title()).not.toHaveAttribute('aria-sort');
    expect(option(0)).toHaveTextContent('Zebra');
  });

  it('plays from the row button and marks a favorite', async () => {
    const user = userEvent.setup();
    await renderTable();

    // The play button is `display: none` until the row is hovered, and jsdom never
    // applies `:hover`, so it is hidden from the accessibility tree and from the
    // pointer. The click handler is the one a hover would reveal.
    const play = option(0).querySelector('button');
    if (!(play instanceof HTMLButtonElement)) throw new Error('No play button');
    fireEvent.click(play);
    expect(invoke).toHaveBeenCalledWith('queue_play', { trackIds: [1, 2], start: 0 });

    const favorite = screen.getAllByRole('button', { name: 'Add to Favorites' })[1];
    if (!favorite) throw new Error('No favorite button');
    await user.click(favorite);
    expect(invoke).toHaveBeenCalledWith('library_set_favorite', { trackId: 2, favorite: true });
  });

  it('follows the playing track from the player store', async () => {
    await renderTable();
    expect(option(0)).not.toHaveAttribute('data-playing');

    act(() => {
      usePlayer.setState({
        state: { ...INITIAL_PLAYER_STATE, status: 'playing', track: playerTrack(1) },
        positionAt: 0,
      });
    });

    expect(option(0)).toHaveAttribute('data-playing', 'true');
    expect(option(1)).not.toHaveAttribute('data-playing');
  });

  it('opens an artist from the row', async () => {
    const user = userEvent.setup();
    await renderTable();

    await user.click(screen.getAllByRole('link', { name: 'Nils Frahm' })[0]!);
    expect(useNavigation.getState().route).toEqual({ view: 'artist', id: 7 });
    expect(option(0)).toHaveAttribute('aria-selected', 'false');
  });

  it('marks a streamed track', async () => {
    await renderTable({
      tracks: [
        track({
          id: 1,
          title: 'Stream',
          path: 'https://music.example/a',
          sourceUrl: 'https://music.example/a',
        }),
      ],
    });

    expect(screen.getByRole('img', { name: 'Plays from the web' })).toBeInTheDocument();
  });

  it('asks about a doubtful match and does not play a row still waiting', async () => {
    const user = userEvent.setup();
    const onEntry = vi.fn();
    const doubtful = entry({ state: 'doubtful', trackId: 1 });
    const waiting = entry({ id: 2, state: 'pending', trackId: null });
    await renderTable({ entries: [doubtful, waiting], onEntry });

    await user.click(
      screen.getByRole('button', { name: 'Maybe not the right track: choose another' }),
    );
    expect(onEntry).toHaveBeenCalledWith(doubtful);

    expect(option(1)).toHaveAttribute('data-waiting', 'true');
    await user.dblClick(option(1));
    expect(onEntry).toHaveBeenCalledWith(waiting);
    expect(invoke).not.toHaveBeenCalledWith('queue_play', expect.anything());
  });

  it('opens a context menu for the row under the pointer', async () => {
    const user = userEvent.setup();
    await renderTable();

    await user.pointer({ keys: '[MouseRight]', target: option(1) });
    expect(Menu.new).toHaveBeenCalled();
    const items = Menu.new.mock.calls[0]?.[0]?.items ?? [];
    expect(items.map((item) => item.text)).toEqual(
      expect.arrayContaining(['Play Next', 'Go to Album', 'My Wave Like This Track']),
    );
    expect(option(1)).toHaveAttribute('aria-selected', 'true');
  });

  it('reorders the focused row with alt and the arrow keys', async () => {
    const user = userEvent.setup();
    const onMove = vi.fn();
    await renderTable({ onMove });
    option(0).focus();

    await user.keyboard('{Alt>}{ArrowDown}{/Alt}');
    expect(onMove).toHaveBeenCalledWith(0, 1);
    expect(screen.getAllByLabelText('Drag to reorder')).toHaveLength(2);
  });
});
