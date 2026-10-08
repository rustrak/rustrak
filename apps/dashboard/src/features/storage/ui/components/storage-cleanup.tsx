import type { CleanupCounts } from '@rustrak/client';
import { AlertTriangle, Loader2, Trash2 } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import {
  executeStorageCleanup,
  getStorageCleanupStatus,
  previewStorageCleanup,
} from '@/features/storage/api/mutations';
import { useCleanupJob } from '@/features/storage/ui/hooks/use-cleanup-job';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/shared/ui/components/shadcn/alert-dialog';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import { Checkbox } from '@/shared/ui/components/shadcn/checkbox';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';

interface StorageCleanupProps {
  projects: { id: number; name: string }[];
}

const PERIODS = [
  { value: '1', days: 1 },
  { value: '7', days: 7 },
  { value: '30', days: 30 },
  { value: '60', days: 60 },
  { value: '90', days: 90 },
];

const ALL_SCOPE = 'all';

const ALL_SELECTED: Record<DataType, boolean> = {
  events: true,
  transactions: true,
  logs: true,
};

/** The three selectable data categories. `events` also governs the issues that
 *  empty out; `transactions` also governs cascaded spans. */
type DataType = 'events' | 'transactions' | 'logs';

const TYPE_OPTIONS: { key: DataType }[] = [
  { key: 'events' },
  { key: 'transactions' },
  { key: 'logs' },
];

/** The human label for a retention period, or the raw value if unknown. */
const periodLabel = (
  value: string,
  t: (key: string, values?: Record<string, string | number>) => string,
) => {
  const period = PERIODS.find((p) => p.value === value);
  return period ? t('period', { count: period.days }) : value;
};

export function StorageCleanup({ projects }: StorageCleanupProps) {
  const format = useFormatter();
  const t = useTranslations('storage');
  const [isPending, startTransition] = useTransition();
  const [period, setPeriod] = useState('90');
  const [scope, setScope] = useState(ALL_SCOPE);
  const [selected, setSelected] = useState<Record<DataType, boolean>>({
    events: true,
    transactions: true,
    logs: true,
  });
  const [preview, setPreview] = useState<CleanupCounts | null>(null);
  const { job, running, follow } = useCleanupJob();
  const busy = isPending || running;

  const olderThanDays = Number(period);
  const projectId = scope === ALL_SCOPE ? undefined : Number(scope);

  // Any change to period/scope/selection invalidates a stale preview — the
  // breakdown only ever reflects the exact options it was run with.
  const resetPreview = () => setPreview(null);

  const noneSelected =
    !selected.events && !selected.transactions && !selected.logs;

  const toggle = (key: DataType) => {
    setSelected((prev) => ({ ...prev, [key]: !prev[key] }));
    resetPreview();
  };

  const handlePreview = () => {
    startTransition(async () => {
      const counts = await previewStorageCleanup({
        older_than_days: olderThanDays,
        project_id: projectId,
        include_events: selected.events,
        include_transactions: selected.transactions,
        include_logs: selected.logs,
      });

      if (!counts.success) {
        toast.error(t('toasts.previewFailed'), {
          description: counts.error.message,
        });
        return;
      }

      setPreview(counts.data);
    });
  };

  const handleExecute = () => {
    startTransition(async () => {
      const started = await executeStorageCleanup({
        older_than_days: olderThanDays,
        project_id: projectId,
        include_events: selected.events,
        include_transactions: selected.transactions,
        include_logs: selected.logs,
      });

      if (!started.success) {
        toast.error(t('toasts.cleanupFailed'), {
          description: started.error.message,
        });
        // Another cleanup is running: follow that one instead.
        if (started.error.kind === 'conflict') {
          const status = await getStorageCleanupStatus();
          if (status.success) follow(status.data);
        }
        return;
      }

      setPreview(null);
      follow(started.data);
    });
  };

  const scopeLabel = (value: string) =>
    value === ALL_SCOPE
      ? t('allProjects')
      : (projects.find((p) => String(p.id) === value)?.name ?? value);

  // Breakdown rows for the categories the preview was run with — only the
  // selected ones, so it never claims to touch data the selection spared.
  const previewLines = preview ? buildLines(preview, selected, t) : [];
  const nothingToDelete =
    preview !== null && previewLines.every((l) => l.count === 0);

  return (
    <Card className="border-destructive/30">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Trash2 className="size-4" />
          {t('cleanup.title')}
        </CardTitle>
        <CardDescription>{t('cleanup.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-col sm:flex-row gap-3">
          <Select
            value={period}
            onValueChange={(value) => {
              if (value) {
                setPeriod(value);
                resetPreview();
              }
            }}
            disabled={busy}
          >
            <SelectTrigger
              className="sm:w-56"
              aria-label={t('retentionPeriodLabel')}
            >
              <SelectValue>{(value) => periodLabel(value, t)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {PERIODS.map((p) => (
                <SelectItem key={p.value} value={p.value}>
                  {t('period', { count: p.days })}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={scope}
            onValueChange={(value) => {
              if (value) {
                setScope(value);
                resetPreview();
              }
            }}
            disabled={busy}
          >
            <SelectTrigger
              className="sm:w-56"
              aria-label={t('projectScopeLabel')}
            >
              <SelectValue>{(value) => scopeLabel(value)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL_SCOPE}>{t('allProjects')}</SelectItem>
              {projects.map((p) => (
                <SelectItem key={p.id} value={String(p.id)}>
                  {p.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {/* Sober inline selection — pick the categories, no numbers until the
            dry-run actually runs. */}
        <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
          <span className="text-sm font-medium text-muted-foreground">
            {t('dataLabel')}
          </span>
          {TYPE_OPTIONS.map((item) => (
            <label
              key={item.key}
              className="flex items-center gap-2 text-sm cursor-pointer select-none"
            >
              <Checkbox
                checked={selected[item.key]}
                onCheckedChange={() => toggle(item.key)}
                disabled={busy}
              />
              {t(`type.${item.key}`)}
            </label>
          ))}
        </div>

        <Button
          type="button"
          variant="outline"
          onClick={handlePreview}
          disabled={busy || noneSelected}
        >
          {isPending ? t('working') : t('preview')}
        </Button>

        {running && job && <CleanupProgress removed={job.removed} />}

        {!running &&
          preview !== null &&
          (nothingToDelete ? (
            <div className="border-t pt-4">
              <p className="text-sm text-muted-foreground">
                {t('nothingMatches', {
                  period: periodLabel(period, t).toLowerCase(),
                  scope: scopeLabel(scope),
                })}
              </p>
            </div>
          ) : (
            <div className="border-t pt-4 space-y-3">
              <p className="text-sm font-medium">{t('wouldRemove')}</p>
              <dl className="text-sm">
                {previewLines.map((line) => (
                  <div
                    key={line.key}
                    className="flex items-center justify-between border-b py-1.5 last:border-b-0"
                  >
                    <dt className="text-muted-foreground">{line.label}</dt>
                    <dd className="tabular-nums font-medium">
                      {line.key === 'spans' && '≈ '}
                      {format.number(line.count)}
                    </dd>
                  </div>
                ))}
              </dl>

              <AlertDialog>
                <AlertDialogTrigger
                  render={<Button variant="destructive" disabled={isPending} />}
                >
                  <Trash2 className="mr-2 size-4" />
                  {t('deleteTrigger')}
                </AlertDialogTrigger>
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle className="flex items-center gap-2">
                      <AlertTriangle className="size-5 text-destructive" />
                      {t('deleteRowsTitle', { count: totalRows(previewLines) })}
                    </AlertDialogTitle>
                    <AlertDialogDescription>
                      {t('deleteRowsDescription', {
                        period: periodLabel(period, t).toLowerCase(),
                        scope: scopeLabel(scope),
                      })}
                    </AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel disabled={isPending}>
                      {t('cancel')}
                    </AlertDialogCancel>
                    <AlertDialogAction
                      onClick={handleExecute}
                      disabled={isPending}
                      className="bg-destructive text-white hover:bg-destructive/90"
                    >
                      {isPending ? t('deleting') : t('deleteAction')}
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          ))}
      </CardContent>
    </Card>
  );
}

interface PreviewLine {
  key: string;
  label: string;
  count: number;
}

/** Breakdown rows for the categories the selection includes. `spans` and
 *  `empty issues` ride along with their governing category (transactions and
 *  events respectively). */
function buildLines(
  counts: CleanupCounts,
  selected: Record<DataType, boolean>,
  t: (key: string, values?: Record<string, string | number>) => string,
): PreviewLine[] {
  const lines: PreviewLine[] = [];
  if (selected.events) {
    lines.push({
      key: 'events',
      label: t('type.events'),
      count: counts.events,
    });
    lines.push({
      key: 'emptyIssues',
      label: t('line.emptyIssues'),
      count: counts.issues_removed,
    });
  }
  if (selected.transactions) {
    lines.push({
      key: 'transactions',
      label: t('type.transactions'),
      count: counts.transactions,
    });
    lines.push({
      key: 'spans',
      label: t('line.spans'),
      count: counts.spans,
    });
  }
  if (selected.logs) {
    lines.push({ key: 'logs', label: t('type.logs'), count: counts.logs });
  }
  return lines;
}

function totalRows(lines: PreviewLine[]): number {
  return lines.reduce((sum, l) => sum + l.count, 0);
}

/** The running cleanup: what it has removed so far, across every category. */
function CleanupProgress({ removed }: { removed: CleanupCounts }) {
  const format = useFormatter();
  const t = useTranslations('storage');

  return (
    <div className="border-t pt-4 space-y-3" aria-live="polite">
      <p className="flex items-center gap-2 text-sm font-medium">
        <Loader2 className="size-4 animate-spin" />
        {t('running.title')}
      </p>
      <p className="text-sm text-muted-foreground">
        {t('running.description')}
      </p>
      <dl className="text-sm">
        {buildLines(removed, ALL_SELECTED, t).map((line) => (
          <div
            key={line.key}
            className="flex items-center justify-between border-b py-1.5 last:border-b-0"
          >
            <dt className="text-muted-foreground">{line.label}</dt>
            <dd className="tabular-nums font-medium">
              {format.number(line.count)}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
