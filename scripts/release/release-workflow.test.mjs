import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const workflowPath = path.join(root, '.github/workflows/release.yml');

const ACTIONLINT = {
  version: '1.7.12',
  assets: {
    'linux-x64': {
      file: 'actionlint_1.7.12_linux_amd64.tar.gz',
      sha256: '8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8',
    },
    'linux-arm64': {
      file: 'actionlint_1.7.12_linux_arm64.tar.gz',
      sha256: '325e971b6ba9bfa504672e29be93c24981eeb1c07576d730e9f7c8805afff0c6',
    },
    'darwin-x64': {
      file: 'actionlint_1.7.12_darwin_amd64.tar.gz',
      sha256: '5b44c3bc2255115c9b69e30efc0fecdf498fdb63c5d58e17084fd5f16324c644',
    },
    'darwin-arm64': {
      file: 'actionlint_1.7.12_darwin_arm64.tar.gz',
      sha256: 'aba9ced2dee8d27fecca3dc7feb1a7f9a52caefa1eb46f3271ea66b6e0e6953f',
    },
    'win32-x64': {
      file: 'actionlint_1.7.12_windows_amd64.zip',
      sha256: '6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9',
    },
  },
};

async function actionlintBinary() {
  const asset = ACTIONLINT.assets[`${process.platform}-${process.arch}`];
  if (!asset) throw new Error(`no actionlint binary for ${process.platform}-${process.arch}`);
  const dir = path.join(
    os.tmpdir(),
    `vitra-actionlint-${ACTIONLINT.version}-${process.platform}-${process.arch}`,
  );
  const bin = path.join(dir, process.platform === 'win32' ? 'actionlint.exe' : 'actionlint');
  if (fs.existsSync(bin)) return bin;
  fs.mkdirSync(dir, { recursive: true });
  const response = await fetch(
    `https://github.com/rhysd/actionlint/releases/download/v${ACTIONLINT.version}/${asset.file}`,
  );
  if (!response.ok) throw new Error(`actionlint download failed: ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const digest = createHash('sha256').update(bytes).digest('hex');
  if (digest !== asset.sha256) throw new Error(`actionlint checksum mismatch: ${digest}`);
  const archive = path.join(dir, asset.file);
  fs.writeFileSync(archive, bytes);
  const extracted = spawnSync('tar', ['-xf', archive, '-C', dir], { encoding: 'utf8' });
  if (extracted.status !== 0) throw new Error(extracted.stderr || 'cannot extract actionlint');
  return bin;
}

function lint(bin, file) {
  return spawnSync(bin, ['-shellcheck=', file], { encoding: 'utf8' });
}

describe('release workflow validation', () => {
  it('accepts the workflow and rejects matrix inside the build job if', async () => {
    const bin = await actionlintBinary();
    const workflow = fs.readFileSync(workflowPath, 'utf8');
    const valid = lint(bin, workflowPath);
    expect(valid.stdout + valid.stderr, valid.stdout + valid.stderr).toBe('');
    expect(valid.status).toBe(0);

    const broken = workflow.replace(
      "github.event_name == 'pull_request' ||",
      "matrix.name == 'macOS-Intel' ||",
    );
    expect(broken).not.toBe(workflow);
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-workflow-'));
    const file = path.join(dir, 'release.yml');
    fs.writeFileSync(file, broken);
    const invalid = lint(bin, file);
    expect(invalid.status).not.toBe(0);
    expect(invalid.stdout + invalid.stderr).toMatch(/context "matrix" is not allowed/);
  }, 60_000);
});
