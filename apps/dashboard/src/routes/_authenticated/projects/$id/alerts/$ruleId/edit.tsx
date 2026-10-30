import { PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { alertQueries } from '@/features/alert/api/queries';
import { alertHref } from '@/features/alert/model/rule-summary';
import {
  RuleEditor,
  RuleEditorSkeleton,
} from '@/features/alert/ui/components/rule-editor/rule-editor';
import { projectQueries } from '@/features/project/api/queries';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
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
    void queryClient.query(alertQueries.integrations()).catch(noop);
    void queryClient.query(projectQueries.environments(projectId)).catch(noop);
  },
  pendingComponent: EditAlertLoading,
  component: EditAlertPage,
});

function EditAlertPage() {
  const t = useTranslations('alerts');
  const { id, ruleId } = Route.useParams();
  const router = useRouter();
  const projectId = Number.parseInt(id, 10);
  const { data: rule } = useSuspenseQuery(
    alertQueries.rule(projectId, Number.parseInt(ruleId, 10)),
  );
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());
  const { data: environments } = useSuspenseQuery(
    projectQueries.environments(projectId),
  );
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
