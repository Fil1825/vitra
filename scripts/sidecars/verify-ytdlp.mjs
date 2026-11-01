// Accepts yt-dlp's SHA2-256SUMS only when SHA2-256SUMS.sig is a detached
// signature from the pinned release key. The sums file is published next to the
// binary, so a checksum match alone does not prove yt-dlp signed it.
//
// `openpgp` (a devDependency) is used when `npm ci` has installed it. CI jobs
// that run this script before `npm ci` have no node_modules; they use GnuPG,
// which the GitHub runners already provide. A failed check never falls through
// to the other tool, and there is no unsigned fallback.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { YT_DLP_KEY_FINGERPRINT } from './ytdlp-release.mjs';

export { YT_DLP_KEY_FINGERPRINT };

/**
 * Verifies `signatureBytes` over the exact `data` bytes of SHA2-256SUMS.
 * Returns which implementation performed the check.
 *
 * @param {Uint8Array} data
 * @param {Uint8Array} signatureBytes
 * @param {string} keyFile absolute path of the armored public key
 * @param {{ loadOpenpgp?: () => Promise<typeof import('openpgp')> }} [options]
 * @returns {Promise<'openpgp' | 'gpg'>}
 */
export async function verifyYtDlpSums(data, signatureBytes, keyFile, options = {}) {
  const load = options.loadOpenpgp ?? (() => import('openpgp'));
  let openpgp;
  try {
    openpgp = await load();
  } catch (err) {
    if (!missingModule(err)) {
      throw err;
    }
    openpgp = null;
  }
  if (openpgp) {
    const armoredKey = fs.readFileSync(keyFile, 'utf8');
    await verifyWithOpenpgp(openpgp, data, signatureBytes, armoredKey);
    return 'openpgp';
  }
  try {
    verifyWithGpg(data, signatureBytes, keyFile);
    return 'gpg';
  } catch (err) {
    if (err?.code === 'ENOENT') {
      throw new Error(
        'cannot verify the yt-dlp checksum signature: run `npm ci` (uses the openpgp devDependency) or install GnuPG',
        { cause: err },
      );
    }
    throw err;
  }
}

/**
 * @param {typeof import('openpgp')} openpgp
 * @param {Uint8Array} data
 * @param {Uint8Array} signatureBytes binary or armored detached signature
 * @param {string} armoredKey
 * @param {string} [expectedFingerprint]
 */
export async function verifyWithOpenpgp(
  openpgp,
  data,
  signatureBytes,
  armoredKey,
  expectedFingerprint = YT_DLP_KEY_FINGERPRINT,
) {
  const expected = normalizeFingerprint(expectedFingerprint);
  let key;
  try {
    key = await openpgp.readKey({ armoredKey });
  } catch (err) {
    throw new Error(`could not read the yt-dlp release key: ${messageOf(err)}`, { cause: err });
  }
  const fingerprint = normalizeFingerprint(key.getFingerprint());
  if (fingerprint !== expected) {
    throw new Error(
      `yt-dlp release key fingerprint is ${grouped(fingerprint)}, expected ${grouped(expected)}`,
    );
  }
  try {
    const signature = await readSignature(openpgp, signatureBytes);
    const message = await openpgp.createMessage({ binary: asBytes(data) });
    const result = await openpgp.verify({
      message,
      signature,
      verificationKeys: key,
      expectSigned: true,
    });
    if (result.signatures.length === 0) {
      throw new Error('no signature');
    }
    await Promise.all(result.signatures.map((sig) => sig.verified));
  } catch (err) {
    throw new Error(`yt-dlp checksum signature is not valid: ${messageOf(err)}`, { cause: err });
  }
}

/**
 * @param {Uint8Array} data
 * @param {Uint8Array} signatureBytes
 * @param {string} keyFile
 * @param {string} [expectedFingerprint]
 */
export function verifyWithGpg(
  data,
  signatureBytes,
  keyFile,
  expectedFingerprint = YT_DLP_KEY_FINGERPRINT,
) {
  const expected = normalizeFingerprint(expectedFingerprint);
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-gpg-'));
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-gpg-data-'));
  try {
    const shown = runGpg(home, ['--with-colons', '--show-keys', gpgPath(keyFile)]);
    if (shown.status !== 0) {
      throw new Error(`could not read the yt-dlp release key: ${shown.stderr.trim()}`);
    }
    const fingerprints = shown.stdout
      .split(/\r?\n/)
      .filter((line) => line.startsWith('fpr:'))
      .map((line) => normalizeFingerprint(line.split(':')[9] ?? ''));
    if (fingerprints.length === 0 || fingerprints.some((fp) => fp !== expected)) {
      const found = fingerprints[0] ? grouped(fingerprints[0]) : 'none';
      throw new Error(`yt-dlp release key fingerprint is ${found}, expected ${grouped(expected)}`);
    }

    const sums = path.join(dir, 'SHA2-256SUMS');
    const sig = path.join(dir, 'SHA2-256SUMS.sig');
    fs.writeFileSync(sums, asBytes(data));
    fs.writeFileSync(sig, asBytes(signatureBytes));
    const imported = runGpg(home, ['--quiet', '--import', gpgPath(keyFile)]);
    if (imported.status !== 0) {
      throw new Error(`could not import the yt-dlp release key: ${imported.stderr.trim()}`);
    }
    const verified = runGpg(home, ['--status-fd', '1', '--verify', gpgPath(sig), gpgPath(sums)]);
    const valid = statusFields(verified.stdout, 'VALIDSIG');
    // Field 0 is the signing key; the last field is the primary key. Both must
    // be the pinned key, so a signature from any other key is refused.
    if (verified.status !== 0 || valid?.[0] !== expected || valid.at(-1) !== expected) {
      const detail = [verified.stderr, verified.stdout].filter(Boolean).join('\n').trim();
      throw new Error(`yt-dlp checksum signature is not valid: ${detail || 'no VALIDSIG'}`);
    }
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

async function readSignature(openpgp, signatureBytes) {
  const raw = asBytes(signatureBytes);
  const head = new TextDecoder().decode(raw.subarray(0, 40));
  if (head.startsWith('-----BEGIN PGP')) {
    return openpgp.readSignature({ armoredSignature: new TextDecoder().decode(raw) });
  }
  return openpgp.readSignature({ binarySignature: raw });
}

function runGpg(home, args) {
  const homedir = gpgPath(home);
  const result = spawnSync('gpg', ['--homedir', homedir, '--batch', ...args], {
    encoding: 'utf8',
    timeout: 30_000,
  });
  if (result.error) {
    const error = new Error(`gpg failed: ${result.error.message}`);
    error.code = result.error.code;
    throw error;
  }
  return result;
}

function statusFields(stdout, name) {
  const prefix = `[GNUPG:] ${name} `;
  const line = stdout.split(/\r?\n/).find((entry) => entry.startsWith(prefix));
  return line ? line.slice(prefix.length).split(' ') : null;
}

/** Absolute paths GnuPG accepts on Windows (MSYS builds garble mixed separators). */
function gpgPath(filePath) {
  let resolved = path.resolve(filePath);
  try {
    resolved = fs.realpathSync.native(resolved);
  } catch {
    // Temp dirs and not-yet-written files still get a resolved path.
  }
  if (process.platform !== 'win32') {
    return resolved;
  }
  // Git-Bash GnuPG treats `C:/…` as relative to the shell cwd and prefixes `/d/a/...`.
  const drive = /^([A-Za-z]):[\\/](.*)$/.exec(resolved);
  if (drive) {
    return `/${drive[1].toLowerCase()}/${drive[2].replace(/\\/g, '/')}`;
  }
  return resolved.replace(/\\/g, '/');
}

function missingModule(err) {
  return err?.code === 'ERR_MODULE_NOT_FOUND' || err?.cause?.code === 'ERR_MODULE_NOT_FOUND';
}

function asBytes(value) {
  return value instanceof Uint8Array ? value : new Uint8Array(value);
}

function normalizeFingerprint(value) {
  return String(value)
    .replace(/[^0-9a-fA-F]/g, '')
    .toUpperCase();
}

function grouped(hex) {
  return hex.replace(/(.{4})(?=.)/g, '$1 ');
}

function messageOf(err) {
  return err instanceof Error ? err.message : String(err);
}
