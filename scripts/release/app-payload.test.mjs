import { spawnSync } from 'node:child_process';

import { describe, expect, it } from 'vitest';

import { payloadDiff } from './app-payload.mjs';
import { loadReuseDocument } from './reused-artifacts.mjs';

function tree(lines) {
  return lines.map(([sha, filePath]) => `100644 blob ${sha}\t${filePath}`).join('\n');
}

describe('application payload', () => {
  it('ignores the release orchestration and reports application changes', () => {
    const base = tree([
      ['aaa', 'src/App.tsx'],
      ['bbb', 'scripts/sidecars/plan.mjs'],
      ['ccc', 'scripts/release/build-intel-mpv.sh'],
    ]);
    expect(
      payloadDiff(
        base,
        tree([
          ['aaa', 'src/App.tsx'],
          ['bbb', 'scripts/sidecars/plan.mjs'],
          ['ddd', 'scripts/release/build-intel-mpv.sh'],
          ['eee', 'docs/dev/MACOS-INTEL-MONTEREY.md'],
        ]),
      ),
    ).toEqual([]);
    expect(
      payloadDiff(
        base,
        tree([
          ['fff', 'src/App.tsx'],
          ['bbb', 'scripts/sidecars/plan.mjs'],
        ]),
      ),
    ).toEqual([{ path: 'src/App.tsx', reason: 'changed' }]);
    expect(
      payloadDiff(
        base,
        tree([
          ['aaa', 'src/App.tsx'],
          ['ggg', 'scripts/sidecars/plan.mjs'],
          ['ccc', 'scripts/release/build-intel-mpv.sh'],
        ]),
      ),
    ).toEqual([{ path: 'scripts/sidecars/plan.mjs', reason: 'changed' }]);
  });

  it('matches the commit that built the three reused installers', () => {
    const base = loadReuseDocument().appPayloadCommit;
    const listed = spawnSync('git', ['ls-tree', '-r', base], { encoding: 'utf8' });
    const head = spawnSync('git', ['ls-tree', '-r', 'HEAD'], { encoding: 'utf8' });
    expect(listed.status).toBe(0);
    expect(head.status).toBe(0);
    expect(payloadDiff(listed.stdout, head.stdout)).toEqual([]);
  });
});
