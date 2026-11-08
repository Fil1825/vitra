import document from './reused-artifacts.json' with { type: 'json' };

export const REUSE_REPO = 'Fil1825/vitra';
export const REUSE_VERSION = '1.0.0-rc.1';
export const REQUIRED_TARGETS = [
  'x86_64-unknown-linux-gnu',
  'x86_64-pc-windows-msvc',
  'aarch64-apple-darwin',
  'x86_64-apple-darwin',
];

export function loadReuseDocument() {
  return document;
}

export function packageNameFromZipListing(names) {
  const packages = names.filter((name) => /\.(?:dmg|exe|deb)$/.test(name));
  if (packages.length !== 1) return null;
  return packages[0].split('/').pop();
}

export function archFromPackage(packageName) {
  if (packageName.endsWith('_amd64.deb')) return 'amd64';
  if (packageName.endsWith('_x64-setup.exe')) return 'x64';
  if (packageName.endsWith('_aarch64.dmg')) return 'aarch64';
  if (packageName.endsWith('_x64.dmg')) return 'x64';
  return null;
}

function versionToken(version) {
  return `_${version}_`;
}

/**
 * Compare a pin document with observations of the downloaded artifact zips.
 * Observations are produced by the publisher; tests pass them in directly.
 * An observation is `{ repo, runId, headSha, artifactId, artifactName, size, expired, sha256, packageName }`.
 */
export function verifyReuseSet(doc, observations) {
  const errors = [];
  if (doc?.repo !== REUSE_REPO) {
    errors.push({ code: 'wrong-repo', detail: doc?.repo ?? '' });
  }
  if (doc?.version !== REUSE_VERSION) {
    errors.push({ code: 'wrong-version', detail: doc?.version ?? '' });
  }
  for (const target of REQUIRED_TARGETS) {
    const pin = doc?.targets?.[target];
    if (!pin || pin.status !== 'pinned') {
      errors.push({ code: 'missing-target', target });
      continue;
    }
    if (!String(pin.packageName ?? '').includes(versionToken(REUSE_VERSION))) {
      errors.push({ code: 'wrong-version', target, detail: pin.packageName ?? '' });
    }
    const obs = observations?.[target];
    if (!obs) {
      errors.push({ code: 'missing-observation', target });
      continue;
    }
    if (obs.repo !== REUSE_REPO)
      errors.push({ code: 'wrong-repo', target, detail: obs.repo ?? '' });
    if (String(obs.runId) !== String(pin.runId)) {
      errors.push({ code: 'wrong-run', target, detail: String(obs.runId) });
    }
    if (obs.headSha !== pin.headSha)
      errors.push({ code: 'wrong-head', target, detail: obs.headSha ?? '' });
    if (
      String(obs.artifactId) !== String(pin.artifactId) ||
      obs.artifactName !== pin.artifactName
    ) {
      errors.push({ code: 'wrong-artifact', target, detail: obs.artifactName ?? '' });
    }
    if (obs.expired === true) errors.push({ code: 'expired', target });
    if (Number(obs.size) !== pin.size) errors.push({ code: 'substituted', target, detail: 'size' });
    const digest = String(obs.sha256 ?? '').replace(/^sha256:/, '');
    if (digest !== pin.sha256) errors.push({ code: 'wrong-sha', target, detail: digest });
    if (obs.packageName !== pin.packageName) {
      errors.push({ code: 'wrong-package', target, detail: obs.packageName ?? '' });
    }
    const arch = archFromPackage(obs.packageName ?? '');
    if (pin.arch && arch !== pin.arch)
      errors.push({ code: 'wrong-arch', target, detail: arch ?? '' });
  }
  return errors;
}

export function publicationDecision(reuseErrors, blockers) {
  return {
    ok: reuseErrors.length === 0 && blockers.length === 0,
    reuseErrors,
    blockers,
  };
}

export function provenanceNotes(doc) {
  const lines = [
    'These installer bytes were not all built from one commit.',
    'The release commit records the pins below. It is not the build commit of every file.',
    '',
  ];
  for (const target of REQUIRED_TARGETS) {
    const pin = doc?.targets?.[target];
    if (!pin || pin.status !== 'pinned') {
      lines.push(`- ${target}: not pinned`);
      continue;
    }
    lines.push(
      `- ${target}: run ${pin.runId}, commit ${pin.headSha}, artifact ${pin.artifactId} ${pin.artifactName}, zip sha256 ${pin.sha256}, package ${pin.packageName}`,
    );
  }
  return `${lines.join('\n')}\n`;
}
