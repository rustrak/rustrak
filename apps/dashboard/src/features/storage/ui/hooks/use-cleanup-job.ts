import type { CleanupCounts, CleanupStatus } from '@rustrak/client';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { storageQueries } from '@/features/storage/api/queries';
import {
  compactedBytes,
  elapsed,
  removedParts,
} from '@/features/storage/model/retention';
import { formatBytes } from '@/shared/lib/utils';

/** How often a running cleanup is asked for its progress. */
const POLL_MS = 2000;

/** One toast per compaction, updated in place while it runs. */
const COMPACTION_TOAST = 'storage-compaction';

/**
 * The server's cleanup job, followed while it runs.
 *
 * A cleanup runs in the background on the server, so the page follows the job
 * instead of waiting on the request that started it. The status read picks up
 * a cleanup already running (started before a reload, in another tab, or
 * through the MCP) and is polled while one runs. `follow` hands it a job just
 * started. The outcome is announced once, with a toast, and everything under
 * `['storage']` read again: the start's own invalidation only saw the start.
 */
export function useCleanupJob() {
  const t = useTranslations('storage');
  const queryClient = useQueryClient();
  const { data: job } = useQuery({
    ...storageQueries.cleanupStatus(),
    refetchInterval: (query) =>
      query.state.data?.state === 'running' ? POLL_MS : false,
  });

  // Whether this page has seen the current job running: only a job seen
  // running is announced when it stops, not the last one's outcome on arrival.
  const runningRef = useRef(false);
  useEffect(() => {
    if (!job) return;
    if (job.state === 'running') {
      runningRef.current = true;
      // A compaction reports no progress, so its toast stays up with the time
      // it has run, and the outcome replaces it.
      if (job.kind === 'compaction') {
        toast.loading(
          t('toasts.compacting', {
            elapsed: elapsed(job.started_at, Date.now()),
          }),
          { id: COMPACTION_TOAST },
        );
      }
      return;
    }
    if (!runningRef.current) return;
    runningRef.current = false;
    announce(job, t);
    void queryClient.invalidateQueries({ queryKey: storageQueries.all() });
  }, [job, t, queryClient]);

  // Leaving the page stops the polling, so a compaction's loading toast,
  // which never times out, would stay up for good. Its outcome replaces it
  // in place, so only one still running is taken down.
  const compactingRef = useRef(false);
  useEffect(() => {
    compactingRef.current =
      job?.state === 'running' && job.kind === 'compaction';
  }, [job]);
  useEffect(
    () => () => {
      if (compactingRef.current) toast.dismiss(COMPACTION_TOAST);
    },
    [],
  );

  // The started job goes into the cache, so it is polled from its first
  // answer even if the status read after the start already saw it finish.
  const follow = (started: CleanupStatus) =>
    queryClient.setQueryData(storageQueries.cleanupStatus().queryKey, started);

  return { job: job ?? null, running: job?.state === 'running', follow };
}

/** The finished job's toast; a compaction's replaces the one that ran. */
function announce(
  job: CleanupStatus,
  t: (key: string, values?: Record<string, string | number>) => string,
) {
  const compacting = job.kind === 'compaction';
  const id = compacting ? COMPACTION_TOAST : undefined;
  if (job.state === 'completed') {
    // Postgres reports each table it rewrote; SQLite only that it is done.
    const freed =
      job.compacted.length > 0
        ? t('toasts.compactedFreed', {
            size: formatBytes(compactedBytes(job.compacted)),
          })
        : undefined;
    toast.success(
      compacting ? t('toasts.compacted') : summarizeRemoved(job.removed, t),
      { id, description: freed },
    );
  } else {
    toast.error(
      t(compacting ? 'toasts.compactionFailed' : 'toasts.cleanupFailed'),
      { id, description: job.error ?? undefined },
    );
  }
}

/** Success toast text after a cleanup: what it removed, zeros left out. */
function summarizeRemoved(
  counts: CleanupCounts,
  t: (key: string, values?: Record<string, string | number>) => string,
): string {
  const parts = removedParts(counts, t);
  return parts.length > 0
    ? t('toasts.removed', { parts: parts.join(', ') })
    : t('toasts.nothingRemoved');
}
