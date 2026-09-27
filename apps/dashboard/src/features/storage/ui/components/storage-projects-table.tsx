import { useQuery } from '@tanstack/react-query';
import { useFormatter, useTranslations } from 'use-intl';
import { storageQueries } from '@/features/storage/api/queries';
import { ProjectsTableSkeleton } from '@/features/storage/ui/components/storage-skeletons';
import { formatBytes } from '@/shared/lib/utils';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/ui/components/shadcn/table';

/**
 * Per-project breakdown table. Owns the heavy per-project aggregation query and
 * streams in independently of the cards above.
 */
export function StorageProjectsTable() {
  const format = useFormatter();
  const t = useTranslations('settings');
  const { data: result } = useQuery(storageQueries.projects());

  if (!result) return <ProjectsTableSkeleton />;

  if (!result.success) {
    return (
      <LoadFailure
        error={result.error}
        title={t('storage.loadBreakdownFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const projects = result.data;

  return (
    <Card className="mb-6">
      <CardHeader>
        <CardTitle>{t('storage.byProject')}</CardTitle>
        <CardDescription>{t('storage.byProjectDescription')}</CardDescription>
      </CardHeader>
      <CardContent>
        {/* Mobile: card list */}
        <div className="md:hidden space-y-3">
          {projects.length === 0 ? (
            <p className="text-center text-muted-foreground py-8 text-sm">
              {t('storage.noProjects')}
            </p>
          ) : (
            projects.map((p) => (
              <div
                key={p.project_id}
                className="rounded-lg border p-3 space-y-2"
              >
                <p className="text-sm font-medium">{p.project_name}</p>
                <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
                  <span className="text-muted-foreground">
                    {t('storage.events')}
                  </span>
                  <span className="tabular-nums text-right">
                    {format.number(p.events_count)}
                  </span>
                  <span className="text-muted-foreground">
                    {t('storage.transactions')}
                  </span>
                  <span className="tabular-nums text-right">
                    {format.number(p.transactions_count)}
                  </span>
                  <span className="text-muted-foreground">
                    {t('storage.spans')}
                  </span>
                  <span className="tabular-nums text-right">
                    {format.number(p.spans_count)}
                  </span>
                  <span className="text-muted-foreground">
                    {t('storage.logs')}
                  </span>
                  <span className="tabular-nums text-right">
                    {format.number(p.logs_count)}
                  </span>
                  <span className="text-muted-foreground">
                    {t('storage.sourceMaps')}
                  </span>
                  <span className="tabular-nums text-right">
                    {format.number(p.source_maps_count)}
                  </span>
                  <span className="text-muted-foreground">
                    {t('storage.estSize')}
                  </span>
                  <span className="tabular-nums text-right">
                    {formatBytes(p.estimated_bytes)}
                  </span>
                </div>
              </div>
            ))
          )}
        </div>

        {/* Desktop: table */}
        <Table className="hidden md:table">
          <TableHeader>
            <TableRow>
              <TableHead>{t('storage.project')}</TableHead>
              <TableHead className="text-right">
                {t('storage.events')}
              </TableHead>
              <TableHead className="text-right">
                {t('storage.transactions')}
              </TableHead>
              <TableHead className="text-right">{t('storage.spans')}</TableHead>
              <TableHead className="text-right">{t('storage.logs')}</TableHead>
              <TableHead className="text-right">
                {t('storage.sourceMaps')}
              </TableHead>
              <TableHead className="text-right">
                {t('storage.estSize')}
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {projects.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={7}
                  className="text-center text-muted-foreground py-8"
                >
                  {t('storage.noProjects')}
                </TableCell>
              </TableRow>
            ) : (
              projects.map((p) => (
                <TableRow key={p.project_id}>
                  <TableCell className="font-medium">
                    {p.project_name}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {format.number(p.events_count)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {format.number(p.transactions_count)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {format.number(p.spans_count)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {format.number(p.logs_count)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {format.number(p.source_maps_count)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatBytes(p.estimated_bytes)}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
