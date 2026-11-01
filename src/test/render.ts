import { type ReactElement } from 'react';
import { act, render, type RenderResult } from '@testing-library/react';

/**
 * Renders `ui` and lets effects finish their first `invoke` (library lists, the
 * playing track). A macrotask runs after those promises resolve, and `act` then
 * flushes the resulting render.
 */
export async function settle(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

export async function renderUi(ui: ReactElement): Promise<RenderResult> {
  const view = render(ui);
  await settle();
  return view;
}
