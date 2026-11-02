import type {
  CleanupCounts,
  CleanupRequest,
  CleanupStatus,
  ProjectStorage,
  SourceMapGcResult,
} from '@rustrak/client';
import {
  Button,
  Checkbox,
  Dialog,
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
  Field,
  FieldLabel,
  Select,
  SelectItem,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { storageMutations } from '@/features/storage/api/mutations';
import { storageQueries } from '@/features/storage/api/queries';
import {
  estimateSize,
  impactOf,
  impactState,
  KINDS,
  type Kind,
  removedParts,
} from '@/features/storage/model/retention';
import { ApiError } from '@/shared/api/query';
import { formatBytes } from '@/shared/lib/utils';
import { ImpactLine } from './impact-line';

const OLDER_THAN = [1, 7, 14, 30, 90, 180, 365] as const;
const ALL = 'all';

/**
 * Deletes data now, without waiting for the cleanup: which kinds, older
 * than what, in every project or one. What goes is counted as it is
 * chosen, and the button says how much.
 */
export function DeleteDialog({
  open,
  onOpenChange,
  projects,
  onStarted,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** `null` until the storage figures are in. */
  projects: readonly ProjectStorage[] | null;
  onStarted: (job: CleanupStatus) => void;
}) {
  // Here rather than in the editor, so the dialog stays open while they run.
  const gc = useMutation(storageMutations.gcSourceMaps());
  const cleanup = useMutation(storageMutations.cleanup());
  const isPending = gc.isPending || cleanup.isPending;
  return (
    <Dialog
      alert
      size="lg"
      open={open}
      onOpenChange={(o) => !isPending && onOpenChange(o)}
    >
      {open && (
        <Editor
          projects={projects}
          isPending={isPending}
          gc={gc.mutateAsync}
          cleanup={cleanup.mutateAsync}
          onDone={(job) => {
            if (job) onStarted(job);
            onOpenChange(false);
          }}
        />
      )}
    </Dialog>
  );
}

function Editor({
  projects,
  isPending,
  gc,
  cleanup,
  onDone,
}: {
  /** `null` until the storage figures are in. */
  projects: readonly ProjectStorage[] | null;
  isPending: boolean;
  gc: () => Promise<unknown>;
  cleanup: (options: CleanupRequest) => Promise<CleanupStatus>;
  /** With the started job, or `null` when only source maps went. */
  onDone: (job: CleanupStatus | null) => void;
}) {
  const t = useTranslations('storage');
  const [kinds, setKinds] = useState<Kind[]>([...KINDS]);
  const [sourceMaps, setSourceMaps] = useState(true);
  const [days, setDays] = useState(90);
  const [project, setProject] = useState(ALL);

  const options = {
    older_than_days: days,
    project_id: project === ALL ? undefined : Number(project),
    include_events: kinds.includes('events'),
    include_transactions: kinds.includes('transactions'),
    include_logs: kinds.includes('logs'),
  };

  const nothingChosen = kinds.length === 0 && !sourceMaps;
  const { state, maps, rows, parts, size, retry } = useDeletion(
    options,
    { rows: kinds.length > 0, sourceMaps },
    projects,
  );

  const run = async () => {
    const done = await deleteNow(
      options,
      { maps, rows, parts },
      gc,
      cleanup,
      t,
    );
    if (done !== undefined) onDone(done);
  };

  return (
    <>
      <DialogHeader
        dismissible={false}
        title={t('delete.title')}
        description={t('delete.description')}
      />

      <DialogBody className="space-y-4">
        <fieldset className="m-0 min-w-0 space-y-1 border-0 p-0">
          <legend className="mb-1.5 text-value font-medium text-fg">
            {t('delete.what')}
          </legend>
          {KINDS.map((kind) => (
            <Choice
              key={kind}
              checked={kinds.includes(kind)}
              onChange={(checked) =>
                setKinds((ks) =>
                  checked ? [...ks, kind] : ks.filter((k) => k !== kind),
                )
              }
              title={t(`kind.${kind}`)}
              hint={t(`kindHint.${kind}`)}
              disabled={isPending}
            />
          ))}
          <Choice
            checked={sourceMaps}
            onChange={setSourceMaps}
            title={t('delete.sourceMaps')}
            hint={t('delete.sourceMapsHint')}
            disabled={isPending}
          />
        </fieldset>

        <Scope
          days={days}
          onDays={setDays}
          project={project}
          onProject={setProject}
          projects={projects}
          disabled={isPending}
        />

        <ImpactLine
          state={nothingChosen ? 'ready' : state}
          sentence={
            !nothingChosen && parts.length > 0
              ? t('delete.impact', { parts: parts.join(', '), size })
              : null
          }
          nothing={t(nothingChosen ? 'delete.pickOne' : 'delete.nothing')}
          onRetry={retry}
        />
      </DialogBody>

      <DialogFooter>
        <DialogClose
          disabled={isPending}
          render={<Button variant="secondary">{t('cancel')}</Button>}
        />
        <Button
          variant="danger-primary"
          onClick={run}
          loading={isPending}
          disabled={state !== 'ready' || (rows === 0 && maps === 0)}
        >
          {rows > 0 || maps > 0
            ? t('delete.action', { size })
            : t('delete.actionIdle')}
        </Button>
      </DialogFooter>
    </>
  );
}

/** A kind to delete: a checkbox, its name and what it covers. */
function Choice({
  checked,
  onChange,
  title,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  title: string;
  hint: string;
  disabled?: boolean;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 rounded-md px-1 py-1.5">
      <Checkbox
        className="mt-0.5"
        aria-label={title}
        checked={checked}
        onCheckedChange={(next) => onChange(next === true)}
        disabled={disabled}
      />
      <span>
        <span className="block text-value">{title}</span>
        <span className="block text-hint text-fg-subtle">{hint}</span>
      </span>
    </label>
  );
}

/**
 * Deletes what was chosen: the unused source maps first (quick, no job),
 * then the data in the background. Resolves to the started job, `null` when
 * only source maps went, or `undefined` when it failed (already said with a
 * toast).
 */
async function deleteNow(
  options: CleanupRequest,
  chosen: { maps: number; rows: number; parts: string[] },
  gc: () => Promise<unknown>,
  cleanup: (options: CleanupRequest) => Promise<CleanupStatus>,
  t: (key: string, values?: Record<string, string | number>) => string,
): Promise<CleanupStatus | null | undefined> {
  if (chosen.maps > 0) {
    try {
      await gc();
    } catch {
      toast.error(t('toasts.gcFailed'));
      return undefined;
    }
  }
  if (chosen.rows === 0) {
    toast.success(t('toasts.removed', { parts: chosen.parts.join(', ') }));
    return null;
  }
  try {
    return await cleanup(options);
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    toast.error(t('toasts.cleanupFailed'), {
      description:
        error.error.kind === 'conflict'
          ? t('dialog.alreadyRunning')
          : undefined,
    });
    return undefined;
  }
}

/** How old, and where: the two choices that narrow a deletion. */
function Scope({
  days,
  onDays,
  project,
  onProject,
  projects,
  disabled,
}: {
  days: number;
  onDays: (days: number) => void;
  project: string;
  onProject: (project: string) => void;
  /** `null` until the storage figures are in. */
  projects: readonly ProjectStorage[] | null;
  disabled?: boolean;
}) {
  const t = useTranslations('storage');
  const byName = [...(projects ?? [])].sort((a, b) =>
    a.project_name.localeCompare(b.project_name),
  );
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <Field>
        <FieldLabel nativeLabel={false}>{t('delete.olderThan')}</FieldLabel>
        <Select
          value={String(days)}
          onValueChange={(v) => onDays(Number(v))}
          items={Object.fromEntries(
            OLDER_THAN.map((d) => [String(d), t('keep.days', { count: d })]),
          )}
          disabled={disabled}
        >
          {OLDER_THAN.map((d) => (
            <SelectItem key={d} value={String(d)}>
              {t('keep.days', { count: d })}
            </SelectItem>
          ))}
        </Select>
      </Field>
      <Field>
        <FieldLabel nativeLabel={false}>{t('delete.in')}</FieldLabel>
        <Select
          value={project}
          onValueChange={onProject}
          items={{
            [ALL]: t('delete.allProjects'),
            ...Object.fromEntries(
              byName.map((p) => [String(p.project_id), p.project_name]),
            ),
          }}
          disabled={disabled}
        >
          <SelectItem value={ALL}>{t('delete.allProjects')}</SelectItem>
          {byName.map((p) => (
            <SelectItem key={p.project_id} value={String(p.project_id)}>
              {p.project_name}
            </SelectItem>
          ))}
        </Select>
      </Field>
    </div>
  );
}

/** What a choice deletes: rows, unused source maps, as words and a size. */
function whatGoes(
  counts: CleanupCounts | null | 'failed',
  unused: SourceMapGcResult | null,
  projects: readonly ProjectStorage[] | null,
  t: (key: string, values?: Record<string, string | number>) => string,
) {
  const known = counts === null || counts === 'failed' ? null : counts;
  const maps = unused?.files_removed ?? 0;
  const rows = known ? KINDS.reduce((sum, k) => sum + known[k], 0) : 0;
  const parts = known ? removedParts({ ...known, source_maps: maps }, t) : [];
  const freed = unused?.bytes_freed ?? 0;
  const size = known
    ? estimateSize(known, projects, freed)
    : formatBytes(freed);
  return { maps, rows, parts, size };
}

/** Two counts as one: failed if either failed, else counting if either is. */
function joined(
  ...states: ('counting' | 'failed' | 'ready')[]
): 'counting' | 'failed' | 'ready' {
  if (states.includes('failed')) return 'failed';
  return states.includes('counting') ? 'counting' : 'ready';
}

/**
 * Everything a deletion would remove, counted as its options change: the
 * rows of the chosen kinds and the unused source maps. Ready only once
 * every count asked for is in, since deleting on one still pending would
 * skip what it had not counted.
 */
function useDeletion(
  options: CleanupRequest,
  chosen: { rows: boolean; sourceMaps: boolean },
  projects: readonly ProjectStorage[] | null,
) {
  const t = useTranslations('storage');
  // What the chosen rows would be, and the source maps no upload uses (one
  // count, with no age or project).
  const countRead = useQuery(storageQueries.cleanupPreview(options));
  const unusedRead = useQuery(storageQueries.sourceMapGcPreview());
  const counts = impactOf(countRead);
  const unused = impactOf(unusedRead);
  const goes = whatGoes(
    counts,
    chosen.sourceMaps && unused !== 'failed' ? unused : null,
    projects,
    t,
  );
  const state = joined(
    chosen.rows ? impactState(counts) : 'ready',
    chosen.sourceMaps ? impactState(unused) : 'ready',
  );
  return {
    ...goes,
    state,
    retry: () => {
      void countRead.refetch();
      void unusedRead.refetch();
    },
  };
}
