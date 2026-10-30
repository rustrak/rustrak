import type { AlertType } from '@rustrak/client';
import { PageHeader } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
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
import { searchOneOf } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { useRouter } from '@/shared/ui/hooks/use-router';

const TYPES = alertTypes.map((a) => a.type);

/** A new alert; `?type=` picks the trigger it starts on. */
export const Route = createFileRoute('/_authenticated/projects/$id/alerts/new')(
  {
    validateSearch: (search: Record<string, unknown>) => ({
      type: searchOneOf(search.type, TYPES),
    }),
    loader: ({ context: { queryClient }, params }) => {
      const projectId = Number.parseInt(params.id, 10);
      void queryClient.query(alertQueries.integrations()).catch(noop);
      void queryClient
        .query(projectQueries.environments(projectId))
        .catch(noop);
    },
    pendingComponent: NewAlertLoading,
    component: NewAlertPage,
  },
);

function NewAlertPage() {
  const t = useTranslations('alerts');
  const { id } = Route.useParams();
  const { type } = Route.useSearch();
  const router = useRouter();
  const projectId = Number.parseInt(id, 10);
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());
  const { data: environments } = useSuspenseQuery(
    projectQueries.environments(projectId),
  );
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
