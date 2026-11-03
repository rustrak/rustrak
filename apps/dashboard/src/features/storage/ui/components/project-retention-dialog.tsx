import type {
  CleanupStatus,
  ProjectStorage,
  RetentionResponse,
  RetentionWindows,
} from '@rustrak/client';
import { Dialog, DialogBody, DialogHeader } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import {
  KINDS,
  WINDOW_FIELD,
  windowsFor,
} from '@/features/storage/model/retention';
import { cn } from '@/shared/lib/utils';
import { useRetentionDraft } from '../hooks/use-retention-draft';
import { KeepForSelect } from './keep-for-select';
import { RetentionFooter } from './retention-footer';

/**
 * One project's retention: the general one, or its own periods.
 */
export function ProjectRetentionDialog({
  project,
  onClose,
  retention,
  projects,
  onDone,
}: {
  /** The project to edit; `null` closes the dialog. */
  project: { id: number; name: string } | null;
  onClose: () => void;
  retention: RetentionResponse;
  /** `null` until the storage figures are in. */
  projects: readonly ProjectStorage[] | null;
  onDone: (job: CleanupStatus | null) => void;
}) {
  return (
    <Dialog
      size="lg"
      open={project !== null}
      onOpenChange={(open) => !open && onClose()}
    >
      {project && (
        <Editor
          key={project.id}
          project={project}
          retention={retention}
          projects={projects}
          onDone={(job) => {
            onDone(job);
            onClose();
          }}
          onCancel={onClose}
        />
      )}
    </Dialog>
  );
}

function Editor({
  project,
  retention,
  projects,
  onDone,
  onCancel,
}: {
  project: { id: number; name: string };
  retention: RetentionResponse;
  /** `null` until the storage figures are in. */
  projects: readonly ProjectStorage[] | null;
  onDone: (job: CleanupStatus | null) => void;
  onCancel: () => void;
}) {
  const t = useTranslations('storage');
  const { draft, setDraft, impact, retry, isPending, save } = useRetentionDraft(
    retention.settings,
    onDone,
  );
  const own = draft.overrides?.find((o) => o.project_id === project.id);
  const keep = (days: number | null) =>
    days === null ? t('keep.forever') : t('keep.days', { count: days });
  const general = KINDS.map(
    (k) => `${t(`kind.${k}`)} ${keep(draft.defaults[WINDOW_FIELD[k]])}`,
  ).join(' · ');

  const setOwn = (windows: RetentionWindows | null) =>
    setDraft((d) => ({
      ...d,
      overrides: [
        ...d.overrides.filter((o) => o.project_id !== project.id),
        ...(windows ? [{ project_id: project.id, ...windows }] : []),
      ],
    }));

  return (
    <>
      <DialogHeader
        title={t('projectDialog.title', { project: project.name })}
        description={t('projectDialog.description')}
      />

      <DialogBody>
        <fieldset className="m-0 min-w-0 space-y-2 border-0 p-0">
          <legend className="sr-only">
            {t('projectDialog.title', { project: project.name })}
          </legend>
          <Choice
            checked={!own}
            onSelect={() => setOwn(null)}
            title={t('projectDialog.general')}
            detail={general}
            disabled={isPending}
          />
          <Choice
            checked={!!own}
            onSelect={() => !own && setOwn(windowsFor(draft, project.id))}
            title={t('projectDialog.own')}
            detail={t('projectDialog.ownHint')}
            disabled={isPending}
          >
            {own && (
              <div className="mt-3 grid gap-2 sm:grid-cols-3">
                {KINDS.map((kind) => (
                  <div key={kind} className="space-y-1">
                    <span className="text-hint text-fg-subtle">
                      {t(`kind.${kind}`)}
                    </span>
                    <KeepForSelect
                      value={own[WINDOW_FIELD[kind]]}
                      onChange={(days) =>
                        setOwn({
                          events_days: own.events_days,
                          transactions_days: own.transactions_days,
                          logs_days: own.logs_days,
                          [WINDOW_FIELD[kind]]: days,
                        })
                      }
                      label={t(`kind.${kind}`)}
                      disabled={isPending}
                    />
                  </div>
                ))}
              </div>
            )}
          </Choice>
        </fieldset>
      </DialogBody>

      <RetentionFooter
        impact={impact}
        onRetry={retry}
        projects={projects}
        isPending={isPending}
        onCancel={onCancel}
        onSave={save}
      />
    </>
  );
}

/** A radio option as a card: a title, a line of detail, and what it opens. */
function Choice({
  checked,
  onSelect,
  title,
  detail,
  disabled,
  children,
}: {
  checked: boolean;
  onSelect: () => void;
  title: string;
  detail: string;
  disabled?: boolean;
  children?: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        'rounded-md border border-border-subtle p-3',
        checked && 'border-border-brand bg-surface-selected',
      )}
    >
      <label className="flex cursor-pointer items-start gap-3">
        <input
          type="radio"
          name="project-retention"
          checked={checked}
          onChange={onSelect}
          disabled={disabled}
          className="mt-1 accent-fg-brand"
        />
        <span className="min-w-0">
          <span className="block text-value font-medium text-fg">{title}</span>
          <span className="block text-hint text-fg-subtle">{detail}</span>
        </span>
      </label>
      {children}
    </div>
  );
}
