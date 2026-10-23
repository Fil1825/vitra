import { isActive, type DownloadGroup, type DownloadJob } from '../../lib/links.ts';

/** A download as the list shows it: a track on its own, or an album or playlist. */
export type DownloadItem =
  | { kind: 'job'; job: DownloadJob }
  | {
      kind: 'group';
      group: DownloadGroup;
      jobs: DownloadJob[];
      /** 0–1 over all its tracks. */
      progress: number;
      done: number;
      failed: number;
      active: boolean;
    };

/** How far one download is, 0–1 (processing counts as nearly done). */
function share(job: DownloadJob): number {
  switch (job.status.state) {
    case 'done':
      return 1;
    case 'processing':
      return 0.95;
    case 'downloading':
      return (job.status.progress ?? 0) * 0.95;
    default:
      return 0;
  }
}

/** Gathers the downloads of each album or playlist into one item, where its first track was. */
export function groupDownloads(jobs: readonly DownloadJob[]): DownloadItem[] {
  const items: DownloadItem[] = [];
  const groups = new Map<number, Extract<DownloadItem, { kind: 'group' }>>();
  for (const job of jobs) {
    if (job.group === null) {
      items.push({ kind: 'job', job });
      continue;
    }
    let item = groups.get(job.group.id);
    if (!item) {
      item = {
        kind: 'group',
        group: job.group,
        jobs: [],
        progress: 0,
        done: 0,
        failed: 0,
        active: false,
      };
      groups.set(job.group.id, item);
      items.push(item);
    }
    item.jobs.push(job);
  }
  for (const item of groups.values()) {
    const { jobs: members } = item;
    item.progress = members.reduce((sum, job) => sum + share(job), 0) / members.length;
    item.done = members.filter((job) => job.status.state === 'done').length;
    item.failed = members.filter(
      (job) => job.status.state === 'failed' || job.status.state === 'cancelled',
    ).length;
    item.active = members.some((job) => isActive(job.status));
  }
  return items;
}
