import type {
  OffsetPaginatedResponse,
  Result,
  RustrakError,
  Span,
} from '@rustrak/client';
import { Ok } from '@rustrak/client';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { ArrowLeft } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { getSpan, listSpans } from '@/features/agent-trace/api/queries';
import {
  resolveSelectedSpan,
  summarizeTrace,
} from '@/features/agent-trace/model/trace-summary';
import { AgentTraceWaterfall } from '@/features/agent-trace/ui/components/agent-trace-waterfall';
import { SpanDetailPane } from '@/features/agent-trace/ui/components/span-detail-pane';
import { TraceSummaryBadges } from '@/features/agent-trace/ui/components/trace-summary-badges';
import { getProject } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { loadAll } from '@/shared/lib/results';
import { searchString } from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';

const PER_PAGE = 100;

/**
 * A trace can hold more spans than one page: the totals and the waterfall are
 * only correct over the whole trace, so pull the remaining pages too.
 */
async function collectAllSpans(
  projectId: number,
  traceId: string,
  firstPage: OffsetPaginatedResponse<Span>,
): Promise<Result<Span[], RustrakError>> {
  if (firstPage.total_pages <= 1) {
    return Ok(firstPage.items);
  }

  const rest = await Promise.all(
    Array.from({ length: firstPage.total_pages - 1 }, (_, i) =>
      listSpans(projectId, {
        trace_id: traceId,
        per_page: PER_PAGE,
        page: i + 2,
      }),
    ),
  );

  const spans = [...firstPage.items];

  for (const page of rest) {
    // A missing page is not an empty page: the waterfall's timings are only
    // correct over the whole trace, so a partial set would draw a plausible
    // and wrong picture.
    if (!page.success) {
      return page;
    }
    spans.push(...page.data.items);
  }

  return Ok(spans);
}

function _formatDuration(ms: number | null): string {
  if (ms == null) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/agents/$traceId',
)({
  validateSearch: (search: Record<string, unknown>) => ({
    span: searchString(search.span),
  }),
  loaderDeps: ({ search }) => ({ span: search.span }),
  loader: async ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const { traceId } = params;

    const loaded = await loadAll([
      getProject(projectId),
      listSpans(projectId, { trace_id: traceId, per_page: PER_PAGE }),
    ]);

    if (!loaded.success) return { failure: loaded.error, trace: null };

    const collected = await collectAllSpans(projectId, traceId, loaded.data[1]);

    if (!collected.success) return { failure: collected.error, trace: null };

    const spans = collected.data;

    if (spans.length === 0) throw notFound();

    const { selectedSpanId, requestedMissing } = resolveSelectedSpan(
      spans,
      deps.span,
    );

    // Only the selected span: attributes are the one part of a span that is
    // never trimmed server-side, so they are pulled one at a time.
    const selected =
      selectedSpanId != null ? await getSpan(projectId, selectedSpanId) : null;

    return {
      failure: null,
      trace: {
        spans,
        summary: summarizeTrace(spans),
        selectedSpanId,
        requestedMissing,
        selected,
      },
    };
  },
  head: ({ params }) => {
    const t = translator('projectPages');
    return {
      meta: [{ title: t('trace.meta.title', { traceId: params.traceId }) }],
    };
  },
  component: AgentTraceDetailPage,
});

function AgentTraceDetailPage() {
  const t = useTranslations('projectPages');
  const { id, traceId } = Route.useParams();
  const { failure, trace } = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (failure !== null || trace === null) {
    return (
      <LoadFailure
        error={
          failure ?? {
            kind: 'unknown',
            message: t('trace.loadFailed'),
          }
        }
        title={t('trace.loadFailed')}
      />
    );
  }

  const { spans, summary, selectedSpanId, requestedMissing, selected } = trace;

  return (
    <div className="flex flex-col h-full">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <Link
          href={`/projects/${projectId}/agents`}
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="size-4" />
          {t('trace.backLink')}
        </Link>
        <h1 className="font-mono text-lg font-semibold break-all">
          {summary.agentName || t('trace.unnamed')}
        </h1>
        <TraceSummaryBadges summary={summary} />

        <p className="mt-1 text-xs text-muted-foreground font-mono truncate">
          {traceId}
        </p>
      </div>

      {/* Two panes on a wide screen, stacked on a narrow one. The details
          panel scrolls independently: a long prompt must not push the
          waterfall out of view, since reading them side by side is the point. */}
      <div className="flex-1 min-h-0 w-full px-4 md:px-8 py-4 md:py-6 flex flex-col lg:flex-row gap-4 overflow-auto lg:overflow-hidden">
        {/* min-w-0 lets this pane shrink below its content's intrinsic width —
    without it a long span label makes the pane grow and push the details
    panel out of the clipped container instead of truncating. */}
        <section className="rounded-lg border flex flex-col min-h-0 min-w-0 lg:flex-1">
          <div className="border-b px-4 py-2.5 flex items-center justify-between shrink-0">
            <h2 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
              {t('trace.spans')}
            </h2>
            <span className="text-xs text-muted-foreground">
              {t('spanCount', { count: spans.length })}
            </span>
          </div>
          <div className="p-3 lg:overflow-auto">
            <AgentTraceWaterfall
              spans={spans}
              projectId={projectId}
              traceId={traceId}
              selectedSpanId={selectedSpanId}
            />
          </div>
        </section>

        <section className="rounded-lg border flex flex-col min-h-0 lg:w-[480px] lg:shrink-0">
          <div className="border-b px-4 py-2.5 shrink-0">
            <h2 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
              {t('trace.spanDetail')}
            </h2>
          </div>
          <div className="p-4 lg:overflow-auto">
            <SpanDetailPane
              selected={selected}
              requestedMissing={requestedMissing}
            />
          </div>
        </section>
      </div>
    </div>
  );
}
