import type { AlertType } from '@rustrak/client';
import { PageHeader } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { alertQueries } from '@/features/alert/api/queries';
import { alertTypes } from '@/features/alert/model/alert-types';
import { alertHref, alertsHref } from '@/features/alert/model/rule-summary';
import {
  RuleEditor,
  RuleEditorSkeleton,
} from '@/features/alert/ui/components/rule-editor/rule-editor';
import { projectQueries } from '@/features/project/api/queries';
import { startIfAllowed } from '@/shared/api/permissions';
import { searchOneOf } from '@/shared/lib/search-params';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';

const TYPES = alertTypes.map((a) => a.type);

/** A new alert; `?type=` picks the trigger it starts on. */
export const Route = createFileRoute('/_authenticated/projects/$id/alerts/new')(
  {
    validateSearch: (search: Record<string, unknown>) => ({
      type: searchOneOf(search.type, TYPES),
    }),
    loader: ({ context: { queryClient }, params }) => {
      const projectId = params.id;
      startIfAllowed(
        queryClient,
        (p) => p.can('alert-rules', 'create', projectId),
        () => void queryClient.query(alertQueries.integrations()).catch(noop),
      );
      startIfAllowed(
        queryClient,
        (p) => p.can('issues', 'read', projectId),
        () =>
          void queryClient
            .query(projectQueries.environments(projectId))
            .catch(noop),
      );
    },
    pendingComponent: NewAlertLoading,
    component: NewAlertPage,
  },
);

function NewAlertPage() {
  const projectId = Route.useParams().id;
  return usePermissions().can('alert-rules', 'create', projectId) ? (
    <NewAlert projectId={projectId} />
  ) : (
    <NotAuthorized />
  );
}

/** Separate so its reads are only made by who may create an alert. */
function NewAlert({ projectId }: { projectId: number }) {
  const t = useTranslations('alerts');
  const { type } = Route.useSearch();
  const router = useRouter();
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());
  // Only choices: without `issues:read` an environment is typed by hand.
  const canReadIssues = usePermissions().can('issues', 'read', projectId);
  const environments =
    useQuery({
      ...projectQueries.environments(projectId),
      enabled: canReadIssues,
    }).data ?? [];
  usePageTrail([{ label: t('overview.new') }]);

  return (
    <div className="flex h-full flex-col">
      <title>{t('overview.newMetaTitle')}</title>
      <PageHeader
        title={t('overview.new')}
        summary={[t('overview.newSummary')]}
      />
      <RuleEditor
        projectId={projectId}
        rule={null}
        initialType={(type ?? 'new_issue') as AlertType}
        integrations={integrations}
        environments={environments}
        onSaved={(rule) => router.push(alertHref(projectId, rule.id))}
        onCancel={() => router.push(alertsHref(projectId))}
      />
    </div>
  );
}

function NewAlertLoading() {
  const t = useTranslations('alerts');
  usePageTrail([{ label: t('overview.new') }]);
  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <title>{t('overview.newMetaTitle')}</title>
        <PageHeader
          title={t('overview.new')}
          summary={[t('overview.newSummary')]}
        />
        <RuleEditorSkeleton />
      </div>
    </PageLoading>
  );
}
