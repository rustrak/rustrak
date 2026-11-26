/**
 * Reads for the storage page. Instance-wide, so under their own root.
 *
 * The dry runs are POST on the server, because a preview takes a body it has
 * to compute against, but they write nothing: they are reads, cached by the
 * request they answer.
 */
import type { CleanupRequest, RetentionSettings } from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

export const storageQueries = {
  all: () => ['storage'] as const,
  /** Instance-wide storage summary (counts + DB size + source-map weight). */
  summary: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'summary'],
      queryFn: ({ signal }) => orThrow(api().storage.getSummary({ signal })),
    }),
  /** Per-project storage breakdown (one row per project). */
  projects: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'projects'],
      queryFn: ({ signal }) => orThrow(api().storage.getProjects({ signal })),
    }),
  /** How long ago each project's data arrived, per kind. */
  age: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'age'],
      queryFn: ({ signal }) => orThrow(api().storage.getAge({ signal })),
    }),
  /** Whether the SQLite file gives freed space back, and what compacting takes. */
  compaction: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'compaction'],
      queryFn: ({ signal }) => orThrow(api().storage.getCompaction({ signal })),
    }),
  /** Each plugin's file, quota and history against its retention. */
  plugins: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'plugins'],
      queryFn: ({ signal }) => orThrow(api().storage.getPlugins({ signal })),
    }),
  /** Retention: settings, next cleanup and last cleanup. */
  retention: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'retention'],
      queryFn: ({ signal }) => orThrow(api().storage.getRetention({ signal })),
    }),
  /** Dry run: what enforcing these settings now would delete. */
  retentionPreview: (settings: RetentionSettings) =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'retention', 'preview', settings],
      queryFn: ({ signal }) =>
        orThrow(api().storage.previewRetention({ body: settings, signal })),
    }),
  /** Dry run: count what a cleanup would remove. */
  cleanupPreview: (request: CleanupRequest) =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'cleanup', 'preview', request],
      queryFn: ({ signal }) =>
        orThrow(api().storage.previewCleanup({ body: request, signal })),
    }),
  /**
   * The running cleanup's progress, or the outcome of the last one. Cleanup,
   * retention runs and compaction all report here; poll it with
   * `refetchInterval` while one runs.
   */
  cleanupStatus: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'cleanup', 'status'],
      queryFn: ({ signal }) =>
        orThrow(api().storage.getCleanupStatus({ signal })),
    }),
  /** Dry run: count the orphaned source maps a GC would remove. */
  sourceMapGcPreview: () =>
    queryOptions({
      queryKey: [...storageQueries.all(), 'source-maps', 'gc-preview'],
      queryFn: ({ signal }) =>
        orThrow(api().storage.previewSourceMapGc({ signal })),
    }),
};
