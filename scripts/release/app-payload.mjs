#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { loadReuseDocument } from './reused-artifacts.mjs';

const ORCHESTRATION = [/^\.github\/workflows\/release\.yml$/, /^scripts\/release\//, /^docs\//];

export function isOrchestrationPath(filePath) {
  return ORCHESTRATION.some((pattern) => pattern.test(filePath));
}

export function parseLsTree(text) {
  const entries = new Map();
  for (const line of text.split('\n')) {
    if (!line) continue;
    const tab = line.indexOf('\t');
    if (tab < 0) throw new Error(`bad git ls-tree line: ${line}`);
    const meta = line.slice(0, tab).split(' ');
    entries.set(line.slice(tab + 1), { mode: meta[0], sha: meta[2] });
  }
  return entries;
}

/** Paths whose blob or mode differs, ignoring the release orchestration. */
export function payloadDiff(baseText, headText) {
  const base = parseLsTree(baseText);
  const head = parseLsTree(headText);
  const paths = new Set([...base.keys(), ...head.keys()]);
  const diffs = [];
  for (const filePath of [...paths].sort()) {
    if (isOrchestrationPath(filePath)) continue;
    const before = base.get(filePath);
    const after = head.get(filePath);
    if (!before) diffs.push({ path: filePath, reason: 'added' });
    else if (!after) diffs.push({ path: filePath, reason: 'removed' });
    else if (before.sha !== after.sha || before.mode !== after.mode) {
      diffs.push({ path: filePath, reason: 'changed' });
    }
  }
  return diffs;
}

function lsTree(rev) {
  const result = spawnSync('git', ['ls-tree', '-r', rev], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(result.stderr || `git ls-tree ${rev} failed`);
  }
  return result.stdout;
}

function main() {
  const baseFlag = process.argv.indexOf('--base');
  const base = baseFlag === -1 ? loadReuseDocument().appPayloadCommit : process.argv[baseFlag + 1];
  if (!base) throw new Error('missing app payload commit');
  const diffs = payloadDiff(lsTree(base), lsTree('HEAD'));
  if (diffs.length) {
    for (const diff of diffs) console.error(`${diff.reason}\t${diff.path}`);
    console.error(`application payload differs from ${base}`);
    process.exit(1);
  }
  console.log(`application payload matches ${base}`);
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) main();
