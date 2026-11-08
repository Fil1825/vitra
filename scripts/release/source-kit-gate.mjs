// Publish-time corresponding-source gate. The JSON `complete` flag is not an input.
// A shipped GPL/LGPL component stays open until the archives, licenses, and
// identity checks below all hold. Deno (MIT) and Linux system mpv do not block.

export const RELEASE_TARGETS = [
  'aarch64-apple-darwin',
  'x86_64-apple-darwin',
  'x86_64-pc-windows-msvc',
  'x86_64-unknown-linux-gnu',
];

export const INTEL_SOURCE_FILES = [
  'build-intel-mpv.sh',
  'intel-mpv-sources.json',
  'mach-minos.py',
  'mach-deps.py',
];

const MIT_SIDECARS = new Set(['deno']);

function platformOf(target) {
  if (target.includes('windows')) return { os: 'windows', ffmpegKey: 'win64' };
  if (target.includes('linux') && target.startsWith('aarch64')) {
    return { os: 'linux', ffmpegKey: 'linuxarm64' };
  }
  if (target.includes('linux')) return { os: 'linux', ffmpegKey: 'linux64' };
  if (target.includes('apple') && target.startsWith('x86_64')) {
    return { os: 'macos', ffmpegKey: 'amd64' };
  }
  if (target.includes('apple')) return { os: 'macos', ffmpegKey: 'arm64' };
  return { os: 'unknown', ffmpegKey: null };
}

function appliesTo(platforms, os) {
  if (!os) return true;
  return platforms.includes('all') || platforms.includes(os);
}

function depShipsOn(dep, ffmpegKey) {
  if (!ffmpegKey) return true;
  if (!Array.isArray(dep.shipsOn)) return true;
  return dep.shipsOn.includes(ffmpegKey);
}

function sourcePinned(source) {
  return Boolean(source?.sha256 && source?.licensePath && source?.url && source?.name);
}

/** Gaps for one offer. Does not read `offer.complete`. */
export function coverageGaps(offer, platform = {}) {
  const { os, ffmpegKey } = platform;
  if (!(offer.archives ?? []).some((archive) => archive.sha256 && archive.url)) {
    return [{ code: 'uncovered-shipped-component', detail: offer.id ?? offer.component }];
  }
  const gaps = [];
  for (const dep of offer.dependencies ?? []) {
    if (!depShipsOn(dep, ffmpegKey)) continue;
    if (!sourcePinned(dep.source)) {
      gaps.push({ code: 'missing-dependency-source', detail: dep.script });
    }
  }
  for (const bundled of offer.bundledSources ?? []) {
    const platforms = bundled.platforms ?? ['all'];
    if (!appliesTo(platforms, os)) continue;
    if (!sourcePinned(bundled.source)) {
      gaps.push({ code: 'missing-dependency-source', detail: bundled.name });
    }
  }
  return gaps;
}

export function sourceClosureFor(offer, platform = {}) {
  if (!offer) return 'incomplete';
  return coverageGaps(offer, platform).length === 0 ? 'complete' : 'incomplete';
}

function assetShortHash(name) {
  return /-g([0-9a-f]{7,40})-/.exec(name)?.[1] ?? null;
}

function entryHas(entries, needle) {
  return entries.some((entry) => entry === needle || entry.endsWith(`/${needle}`));
}

function push(blockers, blocker) {
  const key = `${blocker.code}\0${blocker.target ?? ''}\0${blocker.component ?? ''}\0${blocker.detail ?? ''}`;
  if (blockers.some((item) => item.key === key)) return;
  blockers.push({ ...blocker, key });
}

/**
 * @param {{
 *   offers: object[],
 *   plans: Record<string, object>,
 *   manifests?: object[],
 *   kit?: Record<string, { sha256?: string, entries?: string[] }>,
 *   intelEntries?: Record<string, string[]>,
 * }} input
 */
export function publicationBlockers(input) {
  const offers = input.offers ?? [];
  const blockers = [];
  const offerById = new Map(offers.map((offer) => [offer.id, offer]));

  for (const [target, plan] of Object.entries(input.plans ?? {})) {
    const platform = platformOf(target);
    for (const [name, sidecar] of Object.entries(plan)) {
      if (MIT_SIDECARS.has(name) || sidecar.type === 'system') continue;
      if (sidecar.type === 'source-build') {
        if (!input.manifests) continue;
        const published = input.manifests.some((manifest) => manifest.target === target);
        if (!published) continue;
        const entries = input.intelEntries?.[target] ?? [];
        const missing = INTEL_SOURCE_FILES.filter((file) => !entryHas(entries, file));
        if (missing.length) {
          push(blockers, {
            code: 'missing-intel-source',
            target,
            component: name,
            detail: missing.join(','),
          });
        }
        continue;
      }
      for (const download of sidecar.downloads ?? []) {
        const offer = offerById.get(download.sourceOfferId);
        if (!offer) {
          push(blockers, {
            code: 'uncovered-shipped-component',
            target,
            component: name,
            detail: download.name,
          });
          continue;
        }
        if (offer.ffmpegCommit) {
          const short = assetShortHash(download.name);
          const token = (offer.targets ?? []).find((item) => download.name.includes(`-${item}-`));
          if (
            !short ||
            !offer.ffmpegCommit.startsWith(short) ||
            !download.name.includes('-gpl-') ||
            !token
          ) {
            push(blockers, {
              code: 'ffmpeg-commit-mismatch',
              target,
              component: name,
              detail: download.name,
            });
          }
        }
        for (const gap of coverageGaps(offer, platform)) {
          push(blockers, { ...gap, target, component: name });
        }
        if (input.kit && offer.ffmpegCommit) {
          const ffmpegArchive = (offer.archives ?? []).find((archive) =>
            archive.name.includes(offer.ffmpegCommit),
          );
          const listed = ffmpegArchive ? input.kit[ffmpegArchive.name] : null;
          if (
            listed?.entries &&
            !listed.entries.some((entry) => entry.includes(offer.ffmpegCommit))
          ) {
            push(blockers, {
              code: 'ffmpeg-commit-mismatch',
              target,
              component: name,
              detail: ffmpegArchive?.name ?? offer.ffmpegCommit,
            });
          }
        }
        if (!input.kit) continue;
        for (const archive of offer.archives ?? []) {
          if (!archive?.sha256 || !archive?.name) continue;
          const got = input.kit[archive.name];
          if (!got || got.sha256 !== archive.sha256) {
            push(blockers, {
              code: 'missing-dependency-source',
              target,
              component: name,
              detail: archive.name,
            });
          }
        }
        const pinned = [
          ...(offer.dependencies ?? [])
            .filter((dep) => depShipsOn(dep, platform.ffmpegKey))
            .map((dep) => dep.source),
          ...(offer.bundledSources ?? [])
            .filter((bundled) => appliesTo(bundled.platforms ?? ['all'], platform.os))
            .map((bundled) => bundled.source),
        ].filter(sourcePinned);
        for (const source of pinned) {
          const got = input.kit[source.name];
          if (!got || got.sha256 !== source.sha256) {
            push(blockers, {
              code: 'missing-dependency-source',
              target,
              component: name,
              detail: source.name,
            });
            continue;
          }
          if (got.entries && !got.entries.includes(source.licensePath)) {
            push(blockers, {
              code: 'missing-dependency-source',
              target,
              component: name,
              detail: source.licensePath,
            });
          }
        }
      }
    }
  }

  for (const manifest of input.manifests ?? []) {
    const plan = input.plans?.[manifest.target];
    if (!plan) {
      push(blockers, {
        code: 'unknown-shipped-binary',
        target: manifest.target,
        detail: manifest.target,
      });
      continue;
    }
    const known = new Set();
    for (const sidecar of Object.values(plan)) {
      for (const download of sidecar.downloads ?? []) known.add(download.name);
      if (sidecar.type === 'source-build') known.add('intel-mpv-source-build');
    }
    for (const sidecar of Object.values(manifest.sidecars ?? {})) {
      for (const download of sidecar.downloads ?? []) {
        if (!known.has(download.name)) {
          push(blockers, {
            code: 'unknown-shipped-binary',
            target: manifest.target,
            detail: download.name,
          });
        }
      }
    }
  }

  return blockers.map((blocker) => {
    delete blocker.key;
    return blocker;
  });
}
