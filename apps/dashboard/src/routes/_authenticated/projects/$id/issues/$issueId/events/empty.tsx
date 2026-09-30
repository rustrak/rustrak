import { createFileRoute } from '@tanstack/react-router';
import { AlertCircle, ChevronLeft } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { getIssue } from '@/features/issue/api/queries';
import { getProject } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { loadAll } from '@/shared/lib/results';
import { searchString } from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Card, CardContent } from '@/shared/ui/components/shadcn/card';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/issues/$issueId/events/empty',
)({
  validateSearch: (search: Record<string, unknown>) => ({
    environment: searchString(search.environment),
  }),
  loaderDeps: ({ search }) => ({ environment: search.environment }),
  loader: ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    return loadAll([
      getProject(projectId),
      getIssue(projectId, params.issueId, deps.environment),
    ]);
  },
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
  const { id } = Route.useParams();
  const { environment } = Route.useSearch();
  const loaded = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

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
          render={<Link href={`/projects/${projectId}`} />}
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
          <h2 className="text-xl font-bold mb-2">
            {environment
              ? t('event.noEventsInEnvironment', { environment })
              : t('event.noEventsYet')}
          </h2>
          <p className="text-muted-foreground max-w-md">
            {environment
              ? t('event.noEventsInEnvironmentDescription')
              : t('event.noEventsDescription')}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
