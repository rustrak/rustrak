import type { SpanDetailResponse } from '@rustrak/client';
import type { UseQueryResult } from '@tanstack/react-query';
import { useTranslations } from 'use-intl';
import { AiSpanDetail } from '@/features/agent-trace/ui/components/ai-span-detail';
import type { ApiError } from '@/shared/api/query';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { PanelPlaceholder } from '@/shared/ui/components/panel';

interface SpanDetailPaneProps {
  /** The selected span's read, or `null` when no span is selected. */
  selected: UseQueryResult<SpanDetailResponse, ApiError> | null;
  /** The URL named a span this trace does not contain. */
  requestedMissing: boolean;
}

/**
 * The right-hand pane, and the five answers it has to tell apart.
 *
 * The distinction that matters is between "gone" and "broken": reporting a
 * timeout or a 500 as "that span is no longer available" would send the reader
 * looking for a deleted span that is still there.
 */
export function SpanDetailPane({
  selected,
  requestedMissing,
}: SpanDetailPaneProps) {
  const t = useTranslations('projectPages');

  const notFound = selected?.error?.error.kind === 'not_found';

  // A stale link, not an outage -- say so in place rather than replacing the
  // whole trace view with a failure surface.
  if (requestedMissing || notFound) {
    return (
      <p className="text-fg-subtle text-meta">{t('trace.spanUnavailable')}</p>
    );
  }

  if (selected == null) {
    return <p className="text-fg-subtle text-meta">{t('trace.selectSpan')}</p>;
  }

  if (selected.data) return <AiSpanDetail span={selected.data} />;

  if (selected.isError) {
    return (
      <LoadFailure
        error={selected.error.error}
        title={t('trace.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  return <PanelPlaceholder height={256} />;
}
