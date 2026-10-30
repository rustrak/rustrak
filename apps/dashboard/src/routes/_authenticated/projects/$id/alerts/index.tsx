import type {
  AlertHistory,
  AlertIntegration,
  AlertRuleResponse,
  ProjectResponse,
} from '@rustrak/client';
import {
  Button,
  IntegrationsIcon,
  NewIcon,
  Page,
  PageHeader,
  Skeleton,
  Tab,
  TabList,
  Tabs,
  Text,
} from '@rustrak/ui';
import {
  noop,
  type UseQueryResult,
  useMutation,
  useQuery,
  useSuspenseQuery,
} from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { alertMutations } from '@/features/alert/api/mutations';
import { alertQueries } from '@/features/alert/api/queries';
import { alertHref, newAlertHref } from '@/features/alert/model/rule-summary';
import { HistoryTable } from '@/features/alert/ui/components/alert-history';
import {
  AlertStarters,
  RulesTable,
} from '@/features/alert/ui/components/rule-list';
import { projectQueries } from '@/features/project/api/queries';
import type { ApiError } from '@/shared/api/query';
import { searchOneOf } from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { QueryState } from '@/shared/ui/components/query-state';

const VIEWS = ['rules', 'deliveries'] as const;
const HISTORY = { limit: 100 };

/**
 * Alerts: every rule of the project as a table, and every delivery they
 * made as another, one tab each. Rules are independent: new issues can go by
 * email with one set of filters and to Slack with another.
 */
export const Route = createFileRoute('/_authenticated/projects/$id/alerts/')({
  validateSearch: (search: Record<string, unknown>) => ({
    view: searchOneOf(search.view, VIEWS),
  }),
  // The deliveries are read on both tabs: their own, and the failures counted
  // in the header of the rules tab.
  loader: ({ context: { queryClient }, params }) => {
    const projectId = Number.parseInt(params.id, 10);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    void queryClient.query(alertQueries.ruleList(projectId)).catch(noop);
    void queryClient.query(alertQueries.integrations()).catch(noop);
    void queryClient
      .query(alertQueries.history(projectId, HISTORY))
      .catch(noop);
  },
  pendingComponent: AlertsLoading,
  component: AlertsPage,
});

/**
 * The lists used to fall back to `[]`, which drew "no alert rules yet" over
 * an outage and invited the admin to recreate rules that already exist, so
 * the page suspends on them. The deliveries arrive on their own.
 */
function AlertsPage() {
  const { id } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: rules } = useSuspenseQuery(alertQueries.ruleList(projectId));
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());

  return (
    <AlertsScreen project={project} rules={rules} integrations={integrations} />
  );
}

/** The same screen, with the project's rules and channels still to come. */
function AlertsLoading() {
  return (
    <PageLoading>
      <AlertsScreen />
    </PageLoading>
  );
}

function AlertsScreen({
  project,
  rules,
  integrations,
}: {
  project?: ProjectResponse;
  rules?: AlertRuleResponse[];
  integrations?: AlertIntegration[];
}) {
  const t = useTranslations('alerts');
  const { id } = Route.useParams();
  const { view = 'rules' } = Route.useSearch();
  const navigate = Route.useNavigate();
  const projectId = Number.parseInt(id, 10);
  const history = useQuery(alertQueries.history(projectId, HISTORY));
  const update = useMutation(alertMutations.updateRule(projectId));

  const failed = (history.data ?? []).filter(
    (h) => h.status === 'failed',
  ).length;
  const channelsHref = `/settings/integrations?projectId=${projectId}`;

  const toggle = (rule: AlertRuleResponse, enabled: boolean) =>
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

  return (
    <div className="flex h-full flex-col">
      <title>{t('overview.metaTitle')}</title>
      <PageHeader
        title={t('overview.title')}
        summary={
          project && rules
            ? [
                project.name,
                t('overview.activeCount', {
                  count: rules.filter((r) => r.is_enabled).length,
                }),
                failed > 0 ? (
                  <span className="text-sev-error-fg">
                    {t('overview.failedCount', { count: failed })}
                  </span>
                ) : null,
              ]
            : [<Skeleton key="summary" className="h-4 w-56" />]
        }
        actions={
          <div className="flex items-center gap-2">
            <Button
              icon={IntegrationsIcon}
              render={<Link href={channelsHref} />}
            >
              {integrations ? (
                t('overview.channels', { count: integrations.length })
              ) : (
                <Skeleton className="h-3 w-16" />
              )}
            </Button>
            <Button
              variant="primary"
              icon={NewIcon}
              render={<Link href={newAlertHref(projectId)} />}
            >
              {t('overview.new')}
            </Button>
          </div>
        }
        tabs={
          <Tabs
            value={view}
            onValueChange={(next) =>
              navigate({
                search: {
                  view: next === 'rules' ? undefined : (next as 'deliveries'),
                },
              })
            }
          >
            <TabList>
              <Tab value="rules">
                {t('overview.tabRules', { count: rules?.length ?? 0 })}
              </Tab>
              <Tab value="deliveries">{t('overview.tabDeliveries')}</Tab>
            </TabList>
          </Tabs>
        }
      />
      <Page scroll={view === 'rules' && rules?.length === 0}>
        {integrations?.length === 0 ? <NoChannels href={channelsHref} /> : null}
        {view === 'deliveries' ? (
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-panel">
            <Deliveries projectId={projectId} rules={rules} history={history} />
          </div>
        ) : rules?.length === 0 ? (
          <AlertStarters projectId={projectId} />
        ) : (
          <RulesTable
            projectId={projectId}
            rules={rules}
            integrations={integrations ?? []}
            busy={update.isPending}
            onToggle={toggle}
          />
        )}
      </Page>
    </div>
  );
}

/** What the alerts can be sent through is not set up yet. */
function NoChannels({ href }: { href: string }) {
  const t = useTranslations('alerts');
  return (
    <div className="flex shrink-0 flex-col gap-1 rounded-lg border border-border-brand bg-success-surface px-4 py-3">
      <Text variant="body">{t('overview.noChannelsTitle')}</Text>
      <Text variant="meta" tone="subtle">
        {t.rich('overview.noChannelsDescription', {
          link: (chunks) => (
            <Link href={href} className="text-fg underline">
              {chunks}
            </Link>
          ),
        })}
      </Text>
    </div>
  );
}

/**
 * Every delivery, each under the name of the alert that sent it. Skeleton
 * rows until both the deliveries and the rules that name them are in.
 */
function Deliveries({
  projectId,
  rules,
  history,
}: {
  projectId: number;
  rules?: AlertRuleResponse[];
  history: UseQueryResult<AlertHistory[], ApiError>;
}) {
  const t = useTranslations('alerts');
  const placeholder = (
    <div className="flex flex-col">
      {Array.from({ length: 6 }, (_, row) => (
        <div
          // Placeholder rows have no identity beyond their position.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          key={row}
          className="flex items-center gap-4 border-b border-border-divider px-4 py-3 last:border-b-0"
        >
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-3 flex-1" />
          <Skeleton className="h-3 w-20" />
        </div>
      ))}
    </div>
  );
  if (!rules) return placeholder;
  return (
    <QueryState
      query={history}
      placeholder={placeholder}
      title={t('overview.loadFailed')}
    >
      {(deliveries) => (
        <HistoryTable
          projectId={projectId}
          history={deliveries}
          ruleNames={new Map(rules.map((r) => [r.id, r.name]))}
          ruleHref={(ruleId) => alertHref(projectId, ruleId)}
        />
      )}
    </QueryState>
  );
}
