import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

const scriptPath = path.join(import.meta.dirname, 'build-windows-mpv.sh');
const script = fs.readFileSync(scriptPath, 'utf8');

function extractFunction(name) {
  const start = script.indexOf(`${name}() {`);
  if (start < 0) throw new Error(`missing ${name}`);
  let depth = 0;
  for (let i = script.indexOf('{', start); i < script.length; i += 1) {
    if (script[i] === '{') depth += 1;
    else if (script[i] === '}') {
      depth -= 1;
      if (depth === 0) return script.slice(start, i + 1);
    }
  }
  throw new Error(`unclosed ${name}`);
}

const helpers = `${extractFunction('report_failed_step')}\n${extractFunction('run_step')}\n`;

function runBash(body) {
  return spawnSync('bash', ['-c', `${helpers}\n${body}`], { encoding: 'utf8' });
}

describe('Windows source-build failure report', () => {
  it('reprints the failed step and the configure logs', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-fail-'));
    const src = path.join(root, 'component');
    fs.mkdirSync(path.join(src, 'build', 'meson-logs'), { recursive: true });
    const config = [
      'config-start',
      ...Array.from({ length: 100 }, (_, i) => `config-line-${i}`),
      'config-tail',
    ].join('\n');
    const meson = [
      'meson-start',
      ...Array.from({ length: 100 }, (_, i) => `meson-line-${i}`),
      'meson-tail',
    ].join('\n');
    fs.writeFileSync(path.join(src, 'config.log'), `${config}\n`);
    fs.writeFileSync(path.join(src, 'build', 'meson-logs', 'meson-log.txt'), `${meson}\n`);
    const result = runBash(`
      set -euo pipefail
      export OUT=${JSON.stringify(root)}
      boom() {
        echo step-start
        echo step-tail
        exit 7
      }
      run_step "mbedtls" ${JSON.stringify(src)} boom
    `);
    const report = fs.readFileSync(path.join(root, 'source-build-failure.txt'), 'utf8');
    expect(result.status).toBe(7);
    expect(report).toContain('===== FAILED STEP: mbedtls (exit 7) =====');
    expect(report).toContain('step-tail');
    expect(report).toContain('config-tail');
    expect(report).toContain('meson-tail');
    expect(report).not.toContain('config-start');
    expect(report).not.toContain('meson-start');
    expect(report).toContain('===== END FAILED STEP: mbedtls =====');
    expect(result.stderr).toContain('===== FAILED STEP: mbedtls (exit 7) =====');
    expect(result.stdout).toContain('step-tail');
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('leaves no report when the step succeeds', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-ok-'));
    const result = runBash(`
      set -euo pipefail
      export OUT=${JSON.stringify(root)}
      ok() { echo built; }
      run_step "zlib" ${JSON.stringify(root)} ok
    `);
    expect(result.status).toBe(0);
    expect(fs.existsSync(path.join(root, 'source-build-failure.txt'))).toBe(false);
    expect(result.stdout).toContain('built');
    fs.rmSync(root, { recursive: true, force: true });
  });
});
