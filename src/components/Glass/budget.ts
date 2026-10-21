import { useEffect } from 'react';
import { create } from 'zustand';

export interface GlassSlot {
  priority: number;
  /** Registration order; breaks ties so earlier layers keep their blur. */
  order: number;
}

/** Ids of the glass layers allowed to blur: highest priority first, then the oldest. */
export function allocateSlots(slots: ReadonlyMap<string, GlassSlot>, max: number): Set<string> {
  const ranked = [...slots].sort(([, a], [, b]) => b.priority - a.priority || a.order - b.order);
  return new Set(ranked.slice(0, Math.max(0, max)).map(([id]) => id));
}

interface GlassBudgetState {
  slots: ReadonlyMap<string, GlassSlot>;
  max: number;
  nextOrder: number;
  register(id: string, priority: number): void;
  unregister(id: string): void;
  setMax(max: number): void;
}

/**
 * Limits how many surfaces use `backdrop-filter` at once: every blurred layer re-renders the
 * pixels behind it each frame, so too many of them cost the 60 fps budget.
 */
export const useGlassBudget = create<GlassBudgetState>((set) => ({
  slots: new Map(),
  max: Infinity,
  nextOrder: 0,
  register: (id, priority) =>
    set((s) => ({
      slots: new Map(s.slots).set(id, { priority, order: s.nextOrder }),
      nextOrder: s.nextOrder + 1,
    })),
  unregister: (id) =>
    set((s) => {
      const slots = new Map(s.slots);
      slots.delete(id);
      return { slots };
    }),
  setMax: (max) => set({ max }),
}));

/** Registers a glass layer and tells whether it may blur. */
export function useGlassSlot(id: string, priority: number): boolean {
  const { register, unregister } = useGlassBudget.getState();
  useEffect(() => {
    register(id, priority);
    return () => unregister(id);
  }, [id, priority, register, unregister]);

  return useGlassBudget((s) =>
    // Before registering, assume a free slot so the first paint doesn't flash solid.
    s.slots.has(id) ? allocateSlots(s.slots, s.max).has(id) : s.slots.size < s.max,
  );
}
