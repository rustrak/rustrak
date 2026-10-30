import { Button, DeleteIcon, EditIcon, Skeleton, Switch } from '@rustrak/ui';
import {
  noop,
  useMutation,
  useQuery,
  useSuspenseQuery,
} from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { type ReactNode, useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { alertMutations } from '@/features/alert/api/mutations';
import { alertQueries } from '@/features/alert/api/queries';
import {
  alertHref,
  alertsHref,
  ruleShape,
} from '@/features/alert/model/rule-summary';
import {
  HistoryChart,
  HistoryTable,
} from '@/features/alert/ui/components/alert-history';
import { AlertTypeIcon } from '@/features/alert/ui/components/alert-type-icon';
import { ConfirmDeleteDialog } from '@/features/alert/ui/components/confirm-delete-dialog';
import { ProviderIcon } from '@/features/alert/ui/components/provider-icon';
import { useFrequency } from '@/features/alert/ui/components/rule-list';
import { RuleSentence } from '@/features/alert/ui/components/rule-sentence';
import { Link } from '@/shared/ui/components/link';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { Panel, RailBlock } from '@/shared/ui/components/panel';
import { QueryState } from '@/shared/ui/components/query-state';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { useRouter } from '@/shared/ui/hooks/use-router';

/**
 * One alert, laid out as the endpoint and monitor pages are: what it is and
 * what can be done to it in the header, what it has sent in the reading
 * column -- per day, then delivery by delivery with the reason for every
 * failure and skip -- and how it is set up in the rail.
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/alerts/$ruleId/',
)({
  // The rule and its channels are the page; what it has sent arrives after.
  loader: ({ context: { queryClient }, params }) => {
    const reads = ruleReads(params);
    void queryClient.query(reads.rule).catch(noop);
    void queryClient.query(alertQueries.integrations()).catch(noop);
    void queryClient.query(reads.history).catch(noop);
    void queryClient.query(reads.days).catch(noop);
  },
  pendingComponent: AlertLoading,
  component: AlertPage,
});

/** The page's reads, built once for the loader and the component. */
function ruleReads(params: { id: string; ruleId: string }) {
  const projectId = Number.parseInt(params.id, 10);
  const ruleId = Number.parseInt(params.ruleId, 10);
  return {
    rule: alertQueries.rule(projectId, ruleId),
    history: alertQueries.history(projectId, { ruleId, limit: 100 }),
    // Counted on the server: the table below is only the newest page.
    days: alertQueries.historyByDay(projectId, { ruleId, days: 14 }),
  };
}

function AlertPage() {
  const t = useTranslations('alerts');
  const format = useFormatter();
  const frequency = useFrequency();
  const params = Route.useParams();
  const router = useRouter();
  const [confirming, setConfirming] = useState(false);
  const projectId = Number.parseInt(params.id, 10);
  const reads = ruleReads(params);
  const { data: rule } = useSuspenseQuery(reads.rule);
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());
  const history = useQuery(reads.history);
  const days = useQuery(reads.days);
  const update = useMutation(alertMutations.updateRule(projectId));
  const remove = useMutation(alertMutations.deleteRule(projectId, rule.id));
  const isPending = update.isPending || remove.isPending;
  usePageTrail([{ label: rule.name }]);

  const shape = ruleShape(rule, integrations);
  const linked = new Set(rule.integration_ids);
  const channels = integrations.filter((i) => linked.has(i.id));
  // A dash until the days arrive, never a zero that is not one.
  const count = (status: 'sent' | 'skipped' | 'failed') =>
    days.data?.reduce((sum, day) => sum + day[status], 0);
  const figure = (value: number | undefined) =>
    value === undefined ? '—' : format.number(value);

  const setEnabled = (enabled: boolean) =>
    update.mutate(
      { ruleId: rule.id, body: { is_enabled: enabled } },
      {
        onError: (error) =>
          toast.error(t('ruleDialog.saveFailed'), {
            description: error.error.message,
          }),
        onSuccess: () =>
          toast.success(t(enabled ? 'list.resumed' : 'list.pausedToast')),
      },
    );

  const confirmDelete = () =>
    remove.mutate(undefined, {
      onError: (error) =>
        toast.error(t('overview.deleteFailed'), {
          description: error.error.message,
        }),
      onSuccess: () => {
        toast.success(t('overview.deleted'));
        setConfirming(false);
        router.push(alertsHref(projectId));
      },
    });

  const figures: [string, ReactNode][] = [
    [
      t('history.lastTriggered'),
      rule.last_triggered_at
        ? format.relativeTime(new Date(rule.last_triggered_at))
        : '—',
    ],
    [t('history.status.sent'), figure(count('sent'))],
    [t('history.status.skipped'), figure(count('skipped'))],
    [
      t('history.status.failed'),
      <span
        key="failed"
        className={count('failed') ? 'text-sev-error-fg' : undefined}
      >
        {figure(count('failed'))}
      </span>,
    ],
  ];

  const facts: [string, ReactNode][] = [
    [
      t('table.when'),
      <span key="when" className="inline-flex items-center gap-1.5">
        <AlertTypeIcon
          type={rule.alert_type}
          className="size-3.5 text-fg-subtle"
        />
        {t(`types.${rule.alert_type}.name`)}
      </span>,
    ],
    [
      t('editor.environments'),
      shape.environments.length
        ? shape.environments.join(', ')
        : t('editor.allEnvironments'),
    ],
    ...(rule.alert_type === 'new_feedback'
      ? []
      : ([
          [
            t('editor.level'),
            shape.minLevel === 'any'
              ? t('editor.anyLevel')
              : t('editor.levelOrHigher', { level: shape.minLevel }),
          ],
        ] as [string, ReactNode][])),
    ...(rule.alert_type === 'new_issue'
      ? ([[t('editor.minEvents'), format.number(shape.minEvents)]] as [
          string,
          ReactNode,
        ][])
      : []),
    [t('table.frequency'), frequency(rule.cooldown_minutes)],
    [
      t('detail.created'),
      <span
        key="created"
        title={format.dateTime(new Date(rule.created_at), 'dateTime')}
      >
        {format.relativeTime(new Date(rule.created_at))}
      </span>,
    ],
  ];

  return (
    <div className="flex h-full flex-col">
      <title>{t('overview.ruleMetaTitle', { name: rule.name })}</title>
      <header className="@container shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
        <div className="flex flex-col gap-4 @4xl:flex-row @4xl:items-start @4xl:justify-between @4xl:gap-8">
          <div className="flex min-w-0 flex-col gap-1.5">
            <h1 className="text-fg text-page-title">{rule.name}</h1>
            <p className="max-w-3xl text-body text-fg-secondary leading-relaxed">
              <RuleSentence rule={shape} />
            </p>
          </div>
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            <label className="me-2 flex items-center gap-2 text-fg-secondary text-meta">
              <Switch
                checked={rule.is_enabled}
                disabled={isPending}
                onCheckedChange={setEnabled}
              />
              {t(rule.is_enabled ? 'overview.active' : 'list.paused')}
            </label>
            <Button
              icon={EditIcon}
              render={<Link href={`${alertHref(projectId, rule.id)}/edit`} />}
            >
              {t('overview.edit')}
            </Button>
            <Button
              variant="ghost"
              icon={DeleteIcon}
              disabled={isPending}
              onClick={() => setConfirming(true)}
            >
              {t('overview.delete')}
            </Button>
          </div>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto">
        <main className="min-w-0 flex-1 lg:overflow-y-auto">
          <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
            <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle xl:grid-cols-4">
              {figures.map(([label, value]) => (
                <div
                  key={label}
                  className="flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5"
                >
                  <dt className="truncate text-fg-meta text-meta">{label}</dt>
                  <dd className="text-fg text-title tabular-nums">{value}</dd>
                </div>
              ))}
            </dl>
            <Panel title={t('history.last14')} hint={t('history.chartHint')}>
              <QueryState
                query={days}
                placeholder={<div className="h-[180px]" />}
                title={t('overview.loadFailed')}
              >
                {(loaded) => <HistoryChart days={loaded} />}
              </QueryState>
            </Panel>
            <Panel
              flush
              title={t('history.title')}
              hint={t('history.ruleHint')}
            >
              <QueryState
                query={history}
                placeholder={null}
                title={t('overview.loadFailed')}
              >
                {(deliveries) => (
                  <HistoryTable projectId={projectId} history={deliveries} />
                )}
              </QueryState>
            </Panel>
          </div>
        </main>
        <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
          <RailBlock title={t('detail.setup')}>
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2.5 text-meta">
              {facts.map(([label, value]) => (
                <div key={label} className="contents">
                  <dt className="text-fg-subtle">{label}</dt>
                  <dd className="min-w-0 text-right text-fg-secondary">
                    {value}
                  </dd>
                </div>
              ))}
            </dl>
          </RailBlock>
          <RailBlock title={t('table.notifies')}>
            <ul className="flex flex-col gap-2.5 text-meta">
              {channels.map((channel) => (
                <li key={channel.id} className="flex items-center gap-2">
                  <ProviderIcon
                    type={channel.provider_type}
                    className="size-3.5 shrink-0 text-fg-subtle"
                  />
                  <span className="min-w-0 flex-1 truncate text-fg-secondary">
                    {channel.name}
                  </span>
                  {channel.last_failure_at &&
                  (!channel.last_success_at ||
                    channel.last_failure_at > channel.last_success_at) ? (
                    <span
                      className="shrink-0 text-sev-error-fg"
                      title={channel.last_failure_message ?? undefined}
                    >
                      {t('detail.channelFailing')}
                    </span>
                  ) : null}
                </li>
              ))}
            </ul>
          </RailBlock>
        </aside>
      </div>

      <ConfirmDeleteDialog
        open={confirming}
        title={t('overview.deleteTitle', { name: rule.name })}
        description={t('overview.deleteDescription')}
        isPending={isPending}
        onConfirm={confirmDelete}
        onClose={() => setConfirming(false)}
      />
    </div>
  );
}

/** The alert's layout, with the rule and what it has sent still to come. */
function AlertLoading() {
  const t = useTranslations('alerts');
  const figures = [
    t('history.lastTriggered'),
    t('history.status.sent'),
    t('history.status.skipped'),
    t('history.status.failed'),
  ];

  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <header className="@container shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
          <div className="flex flex-col gap-1.5">
            <Skeleton className="h-7 w-64 max-w-full" />
            <Skeleton className="my-1 h-4 w-96 max-w-full" />
          </div>
        </header>

        <div className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto">
          <main className="min-w-0 flex-1 lg:overflow-y-auto">
            <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
              <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle xl:grid-cols-4">
                {figures.map((label) => (
                  <div
                    key={label}
                    className="flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5"
                  >
                    <dt className="truncate text-fg-meta text-meta">{label}</dt>
                    <dd>
                      <Skeleton className="my-1 h-5 w-12" />
                    </dd>
                  </div>
                ))}
              </dl>
              <Panel title={t('history.last14')} hint={t('history.chartHint')}>
                <Skeleton className="h-[180px] w-full" />
              </Panel>
              <Panel title={t('history.title')} hint={t('history.ruleHint')}>
                <div className="flex flex-col gap-3">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-2/3" />
                </div>
              </Panel>
            </div>
          </main>
          <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
            <RailBlock title={t('detail.setup')}>
              <div className="flex flex-col gap-2.5">
                <Skeleton className="h-3 w-full" />
                <Skeleton className="h-3 w-full" />
                <Skeleton className="h-3 w-2/3" />
              </div>
            </RailBlock>
            <RailBlock title={t('table.notifies')}>
              <Skeleton className="h-3 w-32" />
            </RailBlock>
          </aside>
        </div>
      </div>
    </PageLoading>
  );
}
