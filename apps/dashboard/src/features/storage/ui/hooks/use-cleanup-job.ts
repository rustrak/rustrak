import type { CleanupCounts, CleanupStatus } from '@rustrak/client';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { getStorageCleanupStatus } from '@/features/storage/api/mutations';
import { useRouter } from '@/shared/ui/hooks/use-router';

/** How often a running cleanup is asked for its progress. */
const POLL_MS = 2000;

/**
 * The server's cleanup job, followed while it runs.
 *
 * A cleanup runs in the background on the server, so the page follows the job
 * instead of waiting on the request that started it. On mount it picks up a
 * cleanup already running (started before a reload, in another tab, or through
 * the MCP). `follow` hands it a job just started. The outcome is announced
 * once, with a toast, and the page data refreshed.
 */
export function useCleanupJob() {
  const t = useTranslations('storage');
  const router = useRouter();
  const [job, setJob] = useState<CleanupStatus | null>(null);

  useEffect(() => {
    let cancelled = false;
    getStorageCleanupStatus().then((status) => {
      if (!cancelled && status.success && status.data.state === 'running') {
        setJob(status.data);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Each answer is a new `job`, which schedules the next ask; a failed ask is
  // retried on the next tick. A chain of timeouts rather than an interval, so
  // a slow answer never overlaps the next one and the outcome is announced
  // once.
  useEffect(() => {
    if (job?.state !== 'running') return;
    let cancelled = false;
    const timer = setTimeout(async () => {
      const status = await getStorageCleanupStatus();
      if (cancelled) return;
      if (!status.success) {
        setJob((current) => current && { ...current });
        return;
      }
      setJob(status.data);
      if (status.data.state === 'completed') {
        toast.success(summarizeRemoved(status.data.removed, t));
        router.refresh();
      } else if (status.data.state === 'failed') {
        toast.error(t('toasts.cleanupFailed'), {
          description: status.data.error ?? undefined,
        });
        router.refresh();
      }
    }, POLL_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [job, router, t]);

  return { job, running: job?.state === 'running', follow: setJob };
}

/** Success toast text after a cleanup — categories the server reports as zero
 *  (spared or empty) simply don't show up. */
function summarizeRemoved(
  counts: CleanupCounts,
  t: (key: string, values?: Record<string, string | number>) => string,
): string {
  const parts: string[] = [];
  if (counts.events) parts.push(t('unit.errors', { count: counts.events }));
  if (counts.transactions)
    parts.push(t('unit.transactions', { count: counts.transactions }));
  if (counts.spans) parts.push(t('unit.spans', { count: counts.spans }));
  if (counts.logs) parts.push(t('unit.logs', { count: counts.logs }));
  if (counts.issues_removed)
    parts.push(t('unit.emptyIssues', { count: counts.issues_removed }));
  return parts.length > 0
    ? t('toasts.removed', { parts: parts.join(', ') })
    : t('toasts.nothingRemoved');
}
