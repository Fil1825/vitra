import { describe, expect, it } from 'vitest';

import { startupFailure } from './startup.ts';

describe('startup failure before i18n is ready', () => {
  it('uses the system language and preserves diagnostic details as text', async () => {
    expect(await startupFailure(new Error('test detail'), ['ru-RU'])).toContain(
      'Не удалось запустить Vitra',
    );
    expect(await startupFailure(new Error('test detail'), ['en-US'])).toContain('test detail');
    expect(await startupFailure('offline', ['ja'])).toBe('Vitra could not start: offline');
  });
});
