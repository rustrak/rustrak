import type { ProjectResponse, RateLimitsResponse } from '@rustrak/client';
import { Button, Input } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useId, useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { projectMutations } from '@/features/project/api/mutations';
import { parseRateLimit } from '@/features/project/model/rate-limit';
import { describeError } from '@/shared/lib/error-copy';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';

type LimitField = 'rate_limit_per_minute' | 'rate_limit_per_hour';

/**
 * The project's own event limits, and what the quota has dropped so far.
 *
 * A limit here can only tighten the server's `MAX_EVENTS_PER_PROJECT_*`,
 * which is what lets a team quieten one noisy project without an operator.
 */
export function RateLimitSettings({
  project,
  serverLimits,
  readOnly = false,
}: {
  project: ProjectResponse;
  serverLimits: RateLimitsResponse | null;
  /** Without `project:update`: shown, not changeable. */
  readOnly?: boolean;
}) {
  const t = useTranslations('projects.rateLimits');
  const format = useFormatter();
  // The limit the project follows while the field is empty.
  const serverLimit = (value: number | undefined) =>
    value === undefined ? '' : format.number(value);

  return (
    <SettingSection title={t('title')} description={t('description')}>
      <LimitRow
        project={project}
        readOnly={readOnly}
        field="rate_limit_per_minute"
        title={t('perMinute')}
        placeholder={serverLimit(serverLimits?.project_per_minute)}
      />
      <LimitRow
        project={project}
        readOnly={readOnly}
        field="rate_limit_per_hour"
        title={t('perHour')}
        placeholder={serverLimit(serverLimits?.project_per_hour)}
      />
      <SettingRow title={t('dropped')} description={t('droppedDescription')}>
        <p className="text-value font-medium text-fg tabular-nums">
          {format.number(project.rate_limited_event_count)}
        </p>
      </SettingRow>
    </SettingSection>
  );
}

function LimitRow({
  project,
  field,
  title,
  placeholder,
  readOnly,
}: {
  project: ProjectResponse;
  readOnly: boolean;
  field: LimitField;
  title: string;
  placeholder: string;
}) {
  const t = useTranslations('projects.rateLimits');
  const formT = useTranslations();
  const id = useId();
  const stored = project[field];
  const [raw, setRaw] = useState(stored === null ? '' : String(stored));
  const [invalid, setInvalid] = useState(false);
  const update = useMutation(projectMutations.update(project.id));
  const isPending = update.isPending;

  const parsed = parseRateLimit(raw);
  const hasChanges = !parsed.ok || parsed.value !== stored;

  const save = () => {
    if (!parsed.ok) {
      setInvalid(true);
      return;
    }
    update.mutate(
      { [field]: parsed.value },
      {
        onSuccess: () => toast.success(t('saved')),
        onError: (error) =>
          toast.error(t('saveFailed'), {
            description: describeError(error.error, formT),
          }),
      },
    );
  };

  return (
    <SettingRow title={title} description={t('limitDescription')} htmlFor={id}>
      <div className="flex items-center gap-2">
        <Input
          id={id}
          size="sm"
          boxClassName="flex-1"
          invalid={invalid}
          inputMode="numeric"
          placeholder={placeholder}
          value={raw}
          disabled={isPending || readOnly}
          onChange={(event) => {
            setRaw(event.target.value);
            setInvalid(false);
          }}
        />
        {readOnly ? null : (
          <Button
            type="button"
            onClick={save}
            loading={isPending}
            disabled={!hasChanges}
          >
            {t('save')}
          </Button>
        )}
      </div>
      {invalid && (
        <p className="mt-2 text-hint text-danger-fg">{t('invalid')}</p>
      )}
    </SettingRow>
  );
}
