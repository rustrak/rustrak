'use client';

import type { Release, ReleasePulse } from '@/lib/release';
import { ArchiveFooter } from './archive-footer';
import { ReleaseArticle } from './release-article';
import { ReleaseStrip } from './release-strip';
import { useReleaseArchive } from './use-release-archive';

type Props = {
  initial: Release[];
  chunkCount: number;
  total: number;
  perChunk: number;
  pulse: ReleasePulse[];
  chunkByAnchor: Record<string, number>;
};

/**
 * The strip, then every loaded release, then the control that fetches the
 * next part of the archive. Only the first chunk is in the HTML; the rest
 * arrives on demand (see `use-release-archive.ts`).
 */
export function ChangelogFeed({
  initial,
  chunkCount,
  total,
  perChunk,
  pulse,
  chunkByAnchor,
}: Props) {
  const { releases, loadedChunks, status, loadThrough, jumpTo } =
    useReleaseArchive({ initial, chunkByAnchor });

  return (
    <div className="mx-auto w-full max-w-6xl px-6">
      <div className="pt-16 pb-20">
        <ReleaseStrip pulse={pulse} onSelect={jumpTo} />
      </div>

      {releases.map((release) => (
        <ReleaseArticle key={release.anchor} release={release} />
      ))}

      <ArchiveFooter
        loaded={releases.length}
        total={total}
        nextBatch={Math.min(perChunk, total - releases.length)}
        status={status}
        hasMore={loadedChunks < chunkCount}
        onLoadMore={() => void loadThrough(loadedChunks + 1)}
      />
    </div>
  );
}
