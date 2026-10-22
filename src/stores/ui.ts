import { create } from 'zustand';

const SIDE_PANEL_KEY = 'vitra.sidePanel';
/** Where stage 3 kept whether the queue was open. */
const OLD_QUEUE_KEY = 'vitra.queueOpen';

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

interface UiState {
  sidePanel: SidePanel | null;
  /** File whose tags are being edited. */
  tagEditorPath: string | null;
  lyricsEditor: LyricsEditorTarget | null;
  /** Playlist whose name is being edited (e.g. right after creating it). */
  renamingPlaylist: number | null;
  /** The link dialog is open, with this text in its field. */
  linkInput: string | null;
  /** Shows a panel, or closes it if it is showing. */
  togglePanel(panel: SidePanel): void;
  editTags(path: string | null): void;
  editLyrics(target: LyricsEditorTarget | null): void;
  renamePlaylist(id: number | null): void;
  /** Opens the link dialog (with a pasted or dropped link), or closes it with `null`. */
  openLink(input: string | null): void;
}

/** Window-local UI state (panels, dialogs). */
export const useUi = create<UiState>((set, get) => ({
  sidePanel: storedPanel(),
  tagEditorPath: null,
  lyricsEditor: null,
  renamingPlaylist: null,
  linkInput: null,
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
  renamePlaylist(id) {
    set({ renamingPlaylist: id });
  },
  openLink(input) {
    set({ linkInput: input });
  },
}));
