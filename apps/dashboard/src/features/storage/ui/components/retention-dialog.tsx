import type {
  CleanupStatus,
  ProjectStorage,
  RetentionResponse,
  RetentionSettings,
} from '@rustrak/client';
import {
  Dialog,
  DialogBody,
  DialogHeader,
  Select,
  SelectItem,
} from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { KINDS, WINDOW_FIELD } from '@/features/storage/model/retention';
import { useRetentionDraft } from '../hooks/use-retention-draft';
import { KeepForSelect } from './keep-for-select';
import { RetentionFooter } from './retention-footer';

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const WEEKDAYS = Array.from({ length: 7 }, (_, d) => d);

/**
 * Retention for every project: how long each kind is kept, and when the
 * cleanup runs. A project's own retention is changed from its row.
 */
export function RetentionDialog({
  open,
  onOpenChange,
  retention,
  projects,
  onDone,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  retention: RetentionResponse;
  /** `null` until the storage figures are in. */
  projects: readonly ProjectStorage[] | null;
  onDone: (job: CleanupStatus | null) => void;
}) {
  return (
    <Dialog size="lg" open={open} onOpenChange={onOpenChange}>
      {open && (
        <Editor
          retention={retention}
          projects={projects}
          onDone={(job) => {
            onDone(job);
            onOpenChange(false);
          }}
          onCancel={() => onOpenChange(false)}
        />
      )}
    </Dialog>
  );
}

function Editor({
  retention,
  projects,
  onDone,
  onCancel,
}: {
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
  const custom = draft.overrides.length;

  return (
    <>
      <DialogHeader
        title={t('dialog.title')}
        description={t('dialog.description')}
      />

      <DialogBody className="space-y-4">
        <div className="divide-y divide-border-divider rounded-md border border-border-subtle">
          <div className="flex items-center justify-between gap-4 bg-surface-hover px-3 py-2 text-label text-fg-meta">
            <span>{t('dialog.kindColumn')}</span>
            <span className="w-44">{t('dialog.keepColumn')}</span>
          </div>
          {KINDS.map((kind) => (
            <div
              key={kind}
              className="flex items-center justify-between gap-4 px-3 py-2.5"
            >
              <label htmlFor={`keep-${kind}`} className="min-w-0">
                <span className="block text-value font-medium text-fg">
                  {t(`kind.${kind}`)}
                </span>
                <span className="block text-hint text-fg-subtle">
                  {t(`kindHint.${kind}`)}
                </span>
              </label>
              <div className="w-44 shrink-0">
                <KeepForSelect
                  id={`keep-${kind}`}
                  value={draft.defaults[WINDOW_FIELD[kind]]}
                  onChange={(days) =>
                    setDraft((d) => ({
                      ...d,
                      defaults: { ...d.defaults, [WINDOW_FIELD[kind]]: days },
                    }))
                  }
                  disabled={isPending}
                />
              </div>
            </div>
          ))}
        </div>

        <p className="-mt-2 text-hint text-fg-subtle">
          {t('dialog.foreverHint')}
        </p>

        <div className="space-y-1.5">
          <p className="text-value font-medium text-fg">{t('dialog.runs')}</p>
          <Schedule
            settings={draft}
            onChange={(patch) => setDraft((d) => ({ ...d, ...patch }))}
            disabled={isPending}
          />
          {custom > 0 && (
            <p className="text-hint text-fg-subtle">
              {t('dialog.customProjects', { count: custom })}
            </p>
          )}
        </div>
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

/** "Every day / Every week on …, at hh:00 UTC" as one row of selects. */
function Schedule({
  settings,
  onChange,
  disabled,
}: {
  settings: RetentionSettings;
  onChange: (patch: Partial<RetentionSettings>) => void;
  disabled?: boolean;
}) {
  const t = useTranslations('storage');
  const format = useFormatter();
  // 1 January 2023 was a Sunday: day `d` of that week names weekday `d`.
  const weekday = (d: number) =>
    format.dateTime(new Date(Date.UTC(2023, 0, 1 + d)), {
      weekday: 'long',
      timeZone: 'UTC',
    });
  const hour = (h: number) => `${String(h).padStart(2, '0')}:00 UTC`;

  return (
    <div className="flex flex-wrap items-center gap-2 text-value">
      <Select
        className="w-36"
        aria-label={t('dialog.frequency')}
        value={settings.frequency}
        onValueChange={(v) => onChange({ frequency: v })}
        items={{ daily: t('schedule.daily'), weekly: t('schedule.weekly') }}
        disabled={disabled}
      >
        <SelectItem value="daily">{t('schedule.daily')}</SelectItem>
        <SelectItem value="weekly">{t('schedule.weekly')}</SelectItem>
      </Select>
      {settings.frequency === 'weekly' && (
        <Select
          className="w-36"
          aria-label={t('dialog.weekday')}
          value={String(settings.weekday)}
          onValueChange={(v) => onChange({ weekday: Number(v) })}
          items={Object.fromEntries(WEEKDAYS.map((d) => [d, weekday(d)]))}
          disabled={disabled}
        >
          {WEEKDAYS.map((d) => (
            <SelectItem key={d} value={String(d)}>
              {weekday(d)}
            </SelectItem>
          ))}
        </Select>
      )}
      <span className="text-fg-subtle">{t('dialog.at')}</span>
      <Select
        className="w-32 tabular-nums"
        aria-label={t('dialog.hour')}
        value={String(settings.hour)}
        onValueChange={(v) => onChange({ hour: Number(v) })}
        items={Object.fromEntries(HOURS.map((h) => [h, hour(h)]))}
        disabled={disabled}
      >
        {HOURS.map((h) => (
          <SelectItem key={h} value={String(h)}>
            {hour(h)}
          </SelectItem>
        ))}
      </Select>
    </div>
  );
}
