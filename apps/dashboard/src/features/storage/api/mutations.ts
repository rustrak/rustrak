import type { CleanupRequest, RetentionSettings } from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { storageQueries } from './queries';

/**
 * Anything that deletes data changes every figure on the page (summary, per
 * project, ages, compaction, the dry runs, the status and the last cleanup in
 * retention), so those invalidate the whole root.
 *
 * Cleanup, retention runs and compaction return once the server has started
 * them; what they free shows once `storageQueries.cleanupStatus` says they are
 * done.
 */
export const storageMutations = {
  /** Permanently delete old data and emptied issues. */
  cleanup: () =>
    mutationOptions({
      mutationFn: (body: CleanupRequest) =>
        orThrow(api().storage.executeCleanup({ body })),
      meta: { invalidates: [storageQueries.all()] },
    }),
  /** Enforce the saved retention now. */
  runRetention: () =>
    mutationOptions({
      mutationFn: () => orThrow(api().storage.runRetention()),
      meta: { invalidates: [storageQueries.all()] },
    }),
  /** Compact a SQLite file created before incremental mode, in the background. */
  compact: () =>
    mutationOptions({
      mutationFn: () => orThrow(api().storage.compact()),
      meta: { invalidates: [storageQueries.all()] },
    }),
  /** Garbage-collect orphaned source maps (DB rows + disk files). */
  gcSourceMaps: () =>
    mutationOptions({
      mutationFn: () => orThrow(api().storage.gcSourceMaps()),
      meta: { invalidates: [storageQueries.all()] },
    }),
  /** Saves the retention settings; deletes nothing until a run. */
  updateRetention: () =>
    mutationOptions({
      mutationFn: (body: RetentionSettings) =>
        orThrow(api().storage.updateRetention({ body })),
      meta: {
        invalidates: [
          storageQueries.retention().queryKey,
          // What expires in each plugin follows its window.
          storageQueries.plugins().queryKey,
        ],
      },
    }),
};
