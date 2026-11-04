'use client';

import { Button, ChevronDownIcon } from '@rustrak/ui';
import type { ArchiveStatus } from './use-release-archive';

/** The end of what is loaded: either a way to fetch more, or the first release. */
export function ArchiveFooter({
  loaded,
  total,
  nextBatch,
  status,
  hasMore,
  onLoadMore,
}: {
  loaded: number;
  total: number;
  nextBatch: number;
  status: ArchiveStatus;
  hasMore: boolean;
  onLoadMore: () => void;
}) {
  return (
    <div
      className="flex flex-col items-center gap-4 border-border-subtle border-t pt-16 pb-28 text-center"
      aria-live="polite"
    >
      <p className="text-fg-meta text-meta tabular-nums">
        {status === 'error'
          ? 'Could not reach the archive'
          : `${loaded} of ${total} releases`}
      </p>
      {hasMore ? (
        <Button
          variant="secondary"
          size="lg"
          icon={ChevronDownIcon}
          onClick={onLoadMore}
          disabled={status === 'loading'}
        >
          {status === 'loading'
            ? 'Loading…'
            : status === 'error'
              ? 'Try again'
              : `Load ${nextBatch} earlier release${nextBatch === 1 ? '' : 's'}`}
        </Button>
      ) : (
        <p className="text-fg-subtle text-section">
          That was the first release.
        </p>
      )}
      <noscript>
        <p className="text-fg-subtle text-value">
          The remaining {total - loaded} releases load on demand and need
          JavaScript. They are also on{' '}
          <a
            className="text-fg-brand hover:underline"
            href="https://github.com/rustrak/rustrak/releases"
          >
            GitHub Releases
          </a>
          .
        </p>
      </noscript>
    </div>
  );
}
