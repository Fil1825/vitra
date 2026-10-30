import fs from 'node:fs';
import path from 'node:path';

function identical(left, right) {
  if (fs.realpathSync(left) === fs.realpathSync(right)) return true;
  const a = fs.statSync(left),
    b = fs.statSync(right);
  if (a.isFile() && b.isFile()) return fs.readFileSync(left).equals(fs.readFileSync(right));
  if (!a.isDirectory() || !b.isDirectory()) return false;
  const names = fs.readdirSync(left).sort();
  return (
    JSON.stringify(names) === JSON.stringify(fs.readdirSync(right).sort()) &&
    names.every((name) => identical(path.join(left, name), path.join(right, name)))
  );
}

/** yt-dlp's ZIP can duplicate framework aliases as files/directories, confusing codesign. */
export function normalizePythonFramework(sidecars) {
  const root = path.join(sidecars, 'yt-dlp', '_internal', 'Python.framework');
  if (!fs.existsSync(root)) return;
  const versions = path.join(root, 'Versions');
  const names = fs.readdirSync(versions).filter((name) => name !== 'Current');
  if (names.length !== 1) throw new Error('Expected one bundled Python framework version');
  const version = names[0];
  const actual = path.join(versions, version);
  const aliases = [
    [path.join(versions, 'Current'), actual, version],
    [path.join(root, 'Python'), path.join(actual, 'Python'), 'Versions/Current/Python'],
    [path.join(root, 'Resources'), path.join(actual, 'Resources'), 'Versions/Current/Resources'],
  ];
  // Verify every duplicate before replacing any entry; never discard differing payloads.
  for (const [alias, original] of aliases) {
    if (!fs.existsSync(original) || (fs.existsSync(alias) && !identical(alias, original)))
      throw new Error(`Python framework alias differs from its versioned payload: ${alias}`);
  }
  for (const [alias, , target] of aliases) {
    if (fs.lstatSync(alias, { throwIfNoEntry: false })?.isSymbolicLink()) continue;
    fs.rmSync(alias, { recursive: true, force: true });
    fs.symlinkSync(target, alias);
  }
}
