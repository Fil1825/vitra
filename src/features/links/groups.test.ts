import { describe, expect, it } from 'vitest';

import type { DownloadGroup, DownloadJob, DownloadStatus } from '../../lib/links.ts';
import { groupDownloads } from './groups.ts';

const album: DownloadGroup = {
  id: 100,
  kind: 'album',
  url: 'https://y/list',
  title: 'Polar Lights',
  artist: 'Aurora Collective',
  cover: null,
};

function job(id: number, status: DownloadStatus, group: DownloadGroup | null = album): DownloadJob {
  return {
    id,
    trackId: id,
    url: `https://y/${id}`,
    title: `Track ${id}`,
    artist: null,
    album: null,
    status,
    group,
  };
}

describe('groupDownloads', () => {
  it('gathers an album where its first track was', () => {
    const items = groupDownloads([
      job(1, { state: 'done', path: '/a' }, null),
      job(2, { state: 'done', path: '/b' }),
      job(3, { state: 'downloading', progress: 0.5 }),
      job(4, { state: 'failed', error: 'x' }),
      job(5, { state: 'queued' }, null),
      job(6, { state: 'queued' }),
    ]);
    expect(items.map((item) => (item.kind === 'job' ? item.job.id : item.group.id))).toEqual([
      1, 100, 5,
    ]);
    const group = items[1];
    if (group?.kind !== 'group') throw new Error('a group');
    expect(group.jobs.map((j) => j.id)).toEqual([2, 3, 4, 6]);
    expect(group.done).toBe(1);
    expect(group.failed).toBe(1);
    expect(group.active).toBe(true);
    expect(group.progress).toBeCloseTo((1 + 0.475) / 4);
  });

  it('finishes when every track has', () => {
    const [group] = groupDownloads([
      job(1, { state: 'done', path: '/a' }),
      job(2, { state: 'cancelled' }),
    ]);
    if (group?.kind !== 'group') throw new Error('a group');
    expect(group.active).toBe(false);
    expect(group.failed).toBe(1);
  });
});
