import { create } from 'zustand';

const QUEUE_OPEN_KEY = 'vitra.queueOpen';

function storedFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === '1';
  } catch {
    return false;
  }
}

interface UiState {
  /** The queue panel next to the main area. */
  queueOpen: boolean;
  /** File whose tags are being edited. */
  tagEditorPath: string | null;
  /** Playlist whose name is being edited (e.g. right after creating it). */
  renamingPlaylist: number | null;
  /** The link dialog is open, with this text in its field. */
  linkInput: string | null;
  toggleQueue(): void;
  editTags(path: string | null): void;
  renamePlaylist(id: number | null): void;
  /** Opens the link dialog (with a pasted or dropped link), or closes it with `null`. */
  openLink(input: string | null): void;
}

/** Window-local UI state (panels, dialogs). */
export const useUi = create<UiState>((set, get) => ({
  queueOpen: storedFlag(QUEUE_OPEN_KEY),
  tagEditorPath: null,
  renamingPlaylist: null,
  linkInput: null,
  toggleQueue() {
    const queueOpen = !get().queueOpen;
    set({ queueOpen });
    try {
      localStorage.setItem(QUEUE_OPEN_KEY, queueOpen ? '1' : '0');
    } catch {
      // Storage may be unavailable; the panel just won't be remembered.
    }
  },
  editTags(path) {
    set({ tagEditorPath: path });
  },
  renamePlaylist(id) {
    set({ renamingPlaylist: id });
  },
  openLink(input) {
    set({ linkInput: input });
  },
}));
