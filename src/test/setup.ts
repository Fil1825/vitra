import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';

import { initI18n } from '../i18n/index.ts';
import { resetUi } from './reset.ts';

// English is what the assertions read. Each file gets its own module graph.
await initI18n('en');

afterEach(async () => {
  cleanup();
  await resetUi();
});
