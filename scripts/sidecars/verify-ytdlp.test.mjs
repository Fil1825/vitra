import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import * as openpgp from 'openpgp';
import { describe, expect, it } from 'vitest';

import { YT_DLP_KEY_FINGERPRINT } from './ytdlp-release.mjs';
import { verifyWithGpg, verifyWithOpenpgp, verifyYtDlpSums } from './verify-ytdlp.mjs';

const dir = path.dirname(fileURLToPath(import.meta.url));
const keyFile = path.join(dir, 'yt-dlp-public.key');
const armoredKey = fs.readFileSync(keyFile, 'utf8');
const sums = fs.readFileSync(path.join(dir, 'fixtures', 'ytdlp-SHA2-256SUMS'));
const signature = fs.readFileSync(path.join(dir, 'fixtures', 'ytdlp-SHA2-256SUMS.sig'));

const hasGpg = spawnSync('gpg', ['--version'], { encoding: 'utf8' }).status === 0;

function flipped(bytes, index) {
  const copy = Uint8Array.from(bytes);
  copy[index] ^= 0xff;
  return copy;
}

describe('verifyWithOpenpgp', () => {
  it('accepts the yt-dlp 2026.10.26 checksum signature', async () => {
    await expect(verifyWithOpenpgp(openpgp, sums, signature, armoredKey)).resolves.toBeUndefined();
  });

  it('reads the pinned fingerprint from the vendored key', async () => {
    const key = await openpgp.readKey({ armoredKey });
    expect(key.getFingerprint().toUpperCase()).toBe(YT_DLP_KEY_FINGERPRINT);
  });

  it('rejects a modified checksum file', async () => {
    await expect(
      verifyWithOpenpgp(openpgp, flipped(sums, 0), signature, armoredKey),
    ).rejects.toThrow(/not valid/);
  });

  it('rejects a modified signature', async () => {
    await expect(
      verifyWithOpenpgp(openpgp, sums, flipped(signature, signature.length - 5), armoredKey),
    ).rejects.toThrow(/not valid/);
  });

  it('rejects an empty signature', async () => {
    await expect(verifyWithOpenpgp(openpgp, sums, new Uint8Array(), armoredKey)).rejects.toThrow(
      /not valid/,
    );
  });

  it('rejects a key whose fingerprint is not the pin', async () => {
    await expect(
      verifyWithOpenpgp(openpgp, sums, signature, armoredKey, 'A'.repeat(40)),
    ).rejects.toThrow(/fingerprint/);
  });
});

describe('verifyYtDlpSums', () => {
  it('uses openpgp when the package is installed', async () => {
    await expect(verifyYtDlpSums(sums, signature, keyFile)).resolves.toBe('openpgp');
  });

  it('does not fall back to gpg when openpgp rejects the signature', async () => {
    await expect(verifyYtDlpSums(flipped(sums, 20), signature, keyFile)).rejects.toThrow(
      /not valid/,
    );
  });

  it('does not treat an openpgp failure as a missing install', async () => {
    await expect(
      verifyYtDlpSums(sums, signature, keyFile, {
        loadOpenpgp: async () => {
          throw new Error('openpgp broke');
        },
      }),
    ).rejects.toThrow(/openpgp broke/);
  });

  (hasGpg ? it : it.skip)('uses gpg when openpgp is not installed', async () => {
    const missing = new Error("Cannot find package 'openpgp'");
    missing.code = 'ERR_MODULE_NOT_FOUND';
    await expect(
      verifyYtDlpSums(sums, signature, keyFile, {
        loadOpenpgp: async () => {
          throw missing;
        },
      }),
    ).resolves.toBe('gpg');
  });

  (hasGpg ? it : it.skip)('gpg rejects a modified checksum file', async () => {
    expect(() => verifyWithGpg(flipped(sums, 30), signature, keyFile)).toThrow(/not valid/);
  });
});
