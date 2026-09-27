import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Link } from '@tanstack/react-router';
import { AlertCircle, ChevronLeft } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';
import { projectQueries } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Card, CardContent } from '@/shared/ui/components/shadcn/card';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/issues/$issueId/events/empty',
)({
  loader: ({ params: { id, issueId }, context: { queryClient } }) =>
    loadAll([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(issueQueries.detail(id, issueId)),
    ]),
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.success) {
      return { meta: [{ title: t('event.meta.issueNotFound') }] };
    }

    const [, issue] = loaderData.data;
    return {
      meta: [{ title: t('event.meta.noEvents', { issue: issue.title }) }],
    };
  },
  component: EmptyEventsPage,
});

function EmptyEventsPage() {
  const t = useTranslations('projectPages');
  const { id: projectId, issueId } = Route.useParams();
  const loaded = combine([
    useSuspenseQuery(projectQueries.detail(projectId)).data,
    useSuspenseQuery(issueQueries.detail(projectId, issueId)).data,
  ]);

  if (!loaded.success) {
    return <LoadFailure error={loaded.error} title={t('loadIssueFailed')} />;
  }

  const [project, issue] = loaded.data;

  return (
    <div className="w-full px-8 py-10">
      {/* Breadcrumb */}
      <div className="mb-6">
        <Button
          variant="ghost"
          size="sm"
          nativeButton={false}
          render={<Link to="/projects/$id" params={{ id: projectId }} />}
        >
          <ChevronLeft className="mr-1 size-4" />
          {project.name}
        </Button>
      </div>

      {/* Issue Title */}
      <div className="mb-8">
        <h1 className="text-3xl font-extrabold tracking-tighter">
          {issue.title}
        </h1>
      </div>

      {/* Empty State */}
      <Card className="border-dashed">
        <CardContent className="flex flex-col items-center justify-center py-16 text-center">
          <AlertCircle className="size-12 text-muted-foreground mb-4" />
          <h2 className="text-xl font-bold mb-2">{t('event.noEventsYet')}</h2>
          <p className="text-muted-foreground max-w-md">
            {t('event.noEventsDescription')}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
