import { invoke, isTauri } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { describe, expect, it } from 'vitest';

import { invoke as invokeMock } from './mocks/core.ts';
import { ask as askMock } from './mocks/dialog.ts';
import { emit as emitMock } from './mocks/event.ts';

describe('Tauri mocks', () => {
  it('are the modules the UI imports', () => {
    expect(isTauri()).toBe(true);
    expect(invoke).toBe(invokeMock);
    expect(emit).toBe(emitMock);
    expect(ask).toBe(askMock);
  });

  it('records commands', async () => {
    await invoke('player_toggle');
    expect(invokeMock).toHaveBeenCalledWith('player_toggle');
  });

  it('delivers an event until the listener unsubscribes', async () => {
    const seen: unknown[] = [];
    const unlisten = await listen<{ status: string }>('player://state', (event) => {
      seen.push(event.payload);
    });
    await emit('player://state', { status: 'paused' });
    unlisten();
    await emit('player://state', { status: 'playing' });
    expect(seen).toEqual([{ status: 'paused' }]);
  });

  it('declines a dialog until a test says yes', async () => {
    await expect(ask('Save changes?')).resolves.toBe(false);
  });
});
