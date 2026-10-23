import { create } from 'zustand';

import type { ArtworkSource } from '../lib/library.ts';

const SIDE_PANEL_KEY = 'vitra.sidePanel';
/** Where stage 3 kept whether the queue was open. */
const OLD_QUEUE_KEY = 'vitra.queueOpen';

const SIDEBAR_KEY = 'vitra.sidebar';

/** The sidebar: full, a narrow strip of icons, or gone. */
export type SidebarMode = 'expanded' | 'rail' | 'hidden';

const SIDEBAR_ORDER: readonly SidebarMode[] = ['expanded', 'rail', 'hidden'];

/** The state the sidebar button and shortcut switch to from `mode`. */
export function nextSidebarMode(mode: SidebarMode): SidebarMode {
  return SIDEBAR_ORDER[(SIDEBAR_ORDER.indexOf(mode) + 1) % SIDEBAR_ORDER.length] ?? 'expanded';
}

function storedSidebar(): SidebarMode {
  try {
    const stored = localStorage.getItem(SIDEBAR_KEY);
    return SIDEBAR_ORDER.find((mode) => mode === stored) ?? 'expanded';
  } catch {
    return 'expanded';
  }
}

/** What the panel next to the main area shows. */
export type SidePanel = 'queue' | 'lyrics';

function storedPanel(): SidePanel | null {
  try {
    const stored = localStorage.getItem(SIDE_PANEL_KEY);
    if (stored !== null) {
      return stored === 'queue' || stored === 'lyrics' ? stored : null;
    }
    return localStorage.getItem(OLD_QUEUE_KEY) === '1' ? 'queue' : null;
  } catch {
    return null;
  }
}

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
  sidebar: SidebarMode;
  /** The search field (which only the full sidebar shows) should take the focus. */
  searchRequested: boolean;
  sidePanel: SidePanel | null;
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
  setSidebar(mode: SidebarMode): void;
  /** Shows the full sidebar if needed and focuses its search field. */
  focusSearch(): void;
  /** The search field took the focus. */
  searchFocused(): void;
  /** Shows a panel, or closes it if it is showing. */
  togglePanel(panel: SidePanel): void;
  editTags(path: string | null): void;
  editLyrics(target: LyricsEditorTarget | null): void;
  /** Shows artwork full screen, or closes the viewer with `null`. */
  viewArtwork(view: ArtworkView | null): void;
  renamePlaylist(id: number | null): void;
  /** Opens the link dialog (with a pasted or dropped link), or closes it with `null`. */
  openLink(input: string | null): void;
}

/** Window-local UI state (panels, dialogs). */
export const useUi = create<UiState>((set, get) => ({
  sidebar: storedSidebar(),
  searchRequested: false,
  sidePanel: storedPanel(),
  tagEditorPath: null,
  lyricsEditor: null,
  artworkView: null,
  coverDrop: null,
  renamingPlaylist: null,
  linkInput: null,
  setSidebar(sidebar) {
    set({ sidebar });
    try {
      localStorage.setItem(SIDEBAR_KEY, sidebar);
    } catch {
      // Storage may be unavailable; the sidebar just won't be remembered.
    }
  },
  focusSearch() {
    if (get().sidebar !== 'expanded') {
      get().setSidebar('expanded');
    }
    set({ searchRequested: true });
  },
  searchFocused() {
    set({ searchRequested: false });
  },
  togglePanel(panel) {
    const sidePanel = get().sidePanel === panel ? null : panel;
    set({ sidePanel });
    try {
      localStorage.setItem(SIDE_PANEL_KEY, sidePanel ?? '');
    } catch {
      // Storage may be unavailable; the panel just won't be remembered.
    }
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
