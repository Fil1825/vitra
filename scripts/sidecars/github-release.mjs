/**
 * GitHub REST release assets (documented public API). Resolves asset IDs per tag;
 * downloads with Accept: application/octet-stream (may 302). Do not persist redirect URLs.
 */

import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const API = 'https://api.github.com';
const API_VERSION = '2022-11-28';
export const GITHUB_RELEASE_USER_AGENT = 'vitra-sidecar-fetch';

export function releaseByTagUrl(owner, repo, tag) {
  return `${API}/repos/${owner}/${repo}/releases/tags/${encodeURIComponent(tag)}`;
}

export function releaseAssetUrl(owner, repo, assetId) {
  return `${API}/repos/${owner}/${repo}/releases/assets/${assetId}`;
}

function githubHeaders(accept) {
  return {
    Accept: accept,
    'X-GitHub-Api-Version': API_VERSION,
    'User-Agent': GITHUB_RELEASE_USER_AGENT,
  };
}

/** @returns {{ assetId: number, name: string }} */
export async function resolveReleaseAsset({ owner, repo, tag, asset }, fetchFn = fetch) {
  const res = await fetchFn(releaseByTagUrl(owner, repo, tag), {
    headers: githubHeaders('application/vnd.github+json'),
  });
  if (!res.ok) {
    throw new Error(`GitHub release ${owner}/${repo}@${tag}: HTTP ${res.status}`);
  }
  const release = await res.json();
  const found = release.assets?.find((entry) => entry.name === asset);
  if (!found) {
    throw new Error(`no release asset ${asset} on ${owner}/${repo}@${tag}`);
  }
  return { assetId: found.id, name: found.name };
}

/**
 * Streams a release asset to `destFile`, hashing while writing. Returns hex sha256.
 * Records `api:owner/repo@tag/asset` as provenance, not any expiring redirect target.
 */
export async function downloadReleaseAsset(
  { owner, repo, tag, asset, assetId },
  destFile,
  fetchFn = fetch,
) {
  const id = assetId ?? (await resolveReleaseAsset({ owner, repo, tag, asset }, fetchFn)).assetId;
  const res = await fetchFn(releaseAssetUrl(owner, repo, id), {
    headers: githubHeaders('application/octet-stream'),
    redirect: 'follow',
  });
  if (!res.ok) {
    throw new Error(`GitHub asset ${owner}/${repo}#${id}: HTTP ${res.status}`);
  }
  if (!res.body) {
    throw new Error(`GitHub asset ${owner}/${repo}#${id}: empty body`);
  }
  const hash = createHash('sha256');
  const body = Readable.fromWeb(res.body);
  body.on('data', (chunk) => hash.update(chunk));
  await pipeline(body, createWriteStream(destFile));
  return {
    sha256: hash.digest('hex'),
    provenance: `github-api:${owner}/${repo}@${tag}/${asset}`,
  };
}
