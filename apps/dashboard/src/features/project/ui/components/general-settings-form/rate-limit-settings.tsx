import type { Project, RateLimits } from '@rustrak/client';
import { Loader2 } from 'lucide-react';
import { useId, useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { updateProject } from '@/features/project/api/mutations';
import { parseRateLimit } from '@/features/project/model/rate-limit';
import { describeError } from '@/shared/lib/error-copy';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Input } from '@/shared/ui/components/shadcn/input';
import { useRouter } from '@/shared/ui/hooks/use-router';

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
}: {
  project: Project;
  serverLimits: RateLimits | null;
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
        field="rate_limit_per_minute"
        title={t('perMinute')}
        placeholder={serverLimit(serverLimits?.project_per_minute)}
      />
      <LimitRow
        project={project}
        field="rate_limit_per_hour"
        title={t('perHour')}
        placeholder={serverLimit(serverLimits?.project_per_hour)}
      />
      <SettingRow title={t('dropped')} description={t('droppedDescription')}>
        <p className="text-sm font-medium tabular-nums">
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
}: {
  project: Project;
  field: LimitField;
  title: string;
  placeholder: string;
}) {
  const t = useTranslations('projects.rateLimits');
  const formT = useTranslations();
  const router = useRouter();
  const id = useId();
  const stored = project[field];
  const [raw, setRaw] = useState(stored === null ? '' : String(stored));
  const [invalid, setInvalid] = useState(false);
  const [isPending, startTransition] = useTransition();

  const parsed = parseRateLimit(raw);
  const hasChanges = !parsed.ok || parsed.value !== stored;

  const save = () => {
    if (!parsed.ok) {
      setInvalid(true);
      return;
    }
    startTransition(async () => {
      const result = await updateProject(project.id, { [field]: parsed.value });
      if (!result.success) {
        toast.error(t('saveFailed'), {
          description: describeError(result.error, formT),
        });
        return;
      }
      toast.success(t('saved'));
      router.refresh();
    });
  };

  return (
    <SettingRow title={title} description={t('limitDescription')} htmlFor={id}>
      <div className="flex items-center gap-2">
        <Input
          id={id}
          inputMode="numeric"
          placeholder={placeholder}
          value={raw}
          disabled={isPending}
          aria-invalid={invalid}
          onChange={(event) => {
            setRaw(event.target.value);
            setInvalid(false);
          }}
        />
        <Button
          type="button"
          size="sm"
          onClick={save}
          disabled={isPending || !hasChanges}
        >
          {isPending ? <Loader2 className="size-4 animate-spin" /> : t('save')}
        </Button>
      </div>
      {invalid && (
        <p className="mt-2 text-xs text-destructive">{t('invalid')}</p>
      )}
    </SettingRow>
  );
}
