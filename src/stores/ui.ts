import { create } from 'zustand';

import type { ArtworkSource } from '../lib/library.ts';

/** A track whose lyrics are being edited. */
export interface LyricsEditorTarget {
  path: string;
  libraryId: number | null;
  title: string;
  artist: string | null;
}

/** Artwork shown full screen: where its original comes from, and an image to show meanwhile. */
export interface ArtworkView {
  source: ArtworkSource;
  preview: string | null;
}

interface UiState {
  /** The search field (which only the full sidebar shows) should take the focus. */
  searchRequested: boolean;
  /** File whose tags are being edited. */
  tagEditorPath: string | null;
  lyricsEditor: LyricsEditorTarget | null;
  artworkView: ArtworkView | null;
  /** The cover an image file is dragged over (its `data-cover-drop` key). */
  coverDrop: string | null;
  /** Playlist whose name is being edited (e.g. right after creating it). */
  renamingPlaylist: number | null;
  /** The link dialog is open, with this text in its field. */
  linkInput: string | null;
  /** The search field took the focus. */
  searchFocused(): void;
  editTags(path: string | null): void;
  editLyrics(target: LyricsEditorTarget | null): void;
  /** Shows artwork full screen, or closes the viewer with `null`. */
  viewArtwork(view: ArtworkView | null): void;
  renamePlaylist(id: number | null): void;
  /** Opens the link dialog (with a pasted or dropped link), or closes it with `null`. */
  openLink(input: string | null): void;
}

/**
 * Window-local UI state (dialogs, the artwork viewer). Which panels show is the layout's
 * (`layout/state.ts`), kept in the settings.
 */
export const useUi = create<UiState>((set) => ({
  searchRequested: false,
  tagEditorPath: null,
  lyricsEditor: null,
  artworkView: null,
  coverDrop: null,
  renamingPlaylist: null,
  linkInput: null,
  searchFocused() {
    set({ searchRequested: false });
  },
  editTags(path) {
    set({ tagEditorPath: path });
  },
  editLyrics(target) {
    set({ lyricsEditor: target });
  },
  viewArtwork(view) {
    set({ artworkView: view });
  },
  renamePlaylist(id) {
    set({ renamingPlaylist: id });
  },
  openLink(input) {
    set({ linkInput: input });
  },
}));
