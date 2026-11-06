import { PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { alertQueries } from '@/features/alert/api/queries';
import { alertHref } from '@/features/alert/model/rule-summary';
import {
  RuleEditor,
  RuleEditorSkeleton,
} from '@/features/alert/ui/components/rule-editor/rule-editor';
import { projectQueries } from '@/features/project/api/queries';
import { startIfAllowed } from '@/shared/api/permissions';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';

/** An alert's form, filled in; saving goes back to the alert. */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/alerts/$ruleId/edit',
)({
  loader: ({ context: { queryClient }, params }) => {
    const projectId = Number.parseInt(params.id, 10);
    void queryClient
      .query(alertQueries.rule(projectId, Number.parseInt(params.ruleId, 10)))
      .catch(noop);
    startIfAllowed(
      queryClient,
      (p) => p.can('alert-rules', 'update', projectId),
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
  pendingComponent: EditAlertLoading,
  component: EditAlertPage,
});

function EditAlertPage() {
  const projectId = Number.parseInt(Route.useParams().id, 10);
  return usePermissions().can('alert-rules', 'update', projectId) ? (
    <EditAlert projectId={projectId} />
  ) : (
    <NotAuthorized />
  );
}

/** Separate so its reads are only made by who may change the alert. */
function EditAlert({ projectId }: { projectId: number }) {
  const t = useTranslations('alerts');
  const { ruleId } = Route.useParams();
  const router = useRouter();
  const { data: rule } = useSuspenseQuery(
    alertQueries.rule(projectId, Number.parseInt(ruleId, 10)),
  );
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());
  // Only choices: without `issues:read` an environment is typed by hand.
  const canReadIssues = usePermissions().can('issues', 'read', projectId);
  const environments =
    useQuery({
      ...projectQueries.environments(projectId),
      enabled: canReadIssues,
    }).data ?? [];
  usePageTrail([
    { label: rule.name, href: alertHref(projectId, rule.id) },
    { label: t('overview.edit') },
  ]);

  return (
    <div className="flex h-full flex-col">
      <title>{t('overview.editMetaTitle', { name: rule.name })}</title>
      <PageHeader title={t('overview.editTitle', { name: rule.name })} />
      <RuleEditor
        projectId={projectId}
        rule={rule}
        initialType={rule.alert_type}
        integrations={integrations}
        environments={environments}
        onSaved={() => router.push(alertHref(projectId, rule.id))}
        onCancel={() => router.push(alertHref(projectId, rule.id))}
      />
    </div>
  );
}

function EditAlertLoading() {
  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <PageHeader title={<Skeleton className="h-7 w-64" />} />
        <RuleEditorSkeleton />
      </div>
    </PageLoading>
  );
}
