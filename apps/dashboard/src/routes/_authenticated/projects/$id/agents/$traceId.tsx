import { useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Link, notFound } from '@tanstack/react-router';
import { ArrowLeft } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { agentQueries } from '@/features/agent-trace/api/queries';
import {
  resolveSelectedSpan,
  summarizeTrace,
} from '@/features/agent-trace/model/trace-summary';
import { AgentTraceWaterfall } from '@/features/agent-trace/ui/components/agent-trace-waterfall';
import { SpanDetailPane } from '@/features/agent-trace/ui/components/span-detail-pane';
import { TraceSummaryBadges } from '@/features/agent-trace/ui/components/trace-summary-badges';
import { projectQueries } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

function _formatDuration(ms: number | null): string {
  if (ms == null) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/agents/$traceId',
)({
  validateSearch: (search: Record<string, unknown>): { span?: string } => ({
    span: searchString(search.span),
  }),
  loaderDeps: ({ search }) => ({ span: search.span }),
  loader: async ({
    params: { id, traceId },
    deps,
    context: { queryClient },
  }) => {
    const loaded = await loadAll([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(agentQueries.traceSpans(id, traceId)),
    ]);

    if (!loaded.success) return;

    const spans = loaded.data[1];
    if (spans.length === 0) throw notFound();

    // Only the selected span: attributes are the one part of a span that is
    // never trimmed server-side, so they are pulled one at a time. Choosing
    // another span fetches that span and nothing else.
    const { selectedSpanId } = resolveSelectedSpan(spans, deps.span);
    if (selectedSpanId != null) {
      await queryClient.ensureQueryData(agentQueries.span(id, selectedSpanId));
    }
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
  const { id: projectId, traceId } = Route.useParams();
  const requestedSpan = Route.useSearch({ select: (search) => search.span });
  const loaded = combine([
    useSuspenseQuery(projectQueries.detail(projectId)).data,
    useSuspenseQuery(agentQueries.traceSpans(projectId, traceId)).data,
  ]);
  const spans = loaded.success ? loaded.data[1] : [];
  const { selectedSpanId, requestedMissing } = resolveSelectedSpan(
    spans,
    requestedSpan,
  );
  const { data: selected = null } = useQuery({
    ...agentQueries.span(projectId, selectedSpanId ?? ''),
    enabled: selectedSpanId != null,
  });

  if (!loaded.success) {
    return <LoadFailure error={loaded.error} title={t('trace.loadFailed')} />;
  }

  const summary = summarizeTrace(spans);

  return (
    <div className="flex flex-col h-[calc(100vh-64px)]">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <Link
          to="/projects/$id/agents"
          params={{ id: projectId }}
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
