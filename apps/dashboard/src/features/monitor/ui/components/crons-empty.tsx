import type { Schedule } from '@rustrak/client';
import { Button, CronsIcon, Text } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { CronSetupGuide } from '@/features/monitor/ui/components/cron-setup-guide';
import { Link } from '@/shared/ui/components/link';

/** A sample slug and schedule for the snippets shown before any monitor exists. */
const SAMPLE_SLUG = 'my-job';
const SAMPLE_SCHEDULE: Schedule = { type: 'crontab', value: '0 * * * *' };

const STEPS = ['checkIn', 'run', 'alert'] as const;

/**
 * The crons page before any monitor exists: the three steps to get one, and
 * the snippet that takes them, side by side.
 */
export function CronsEmpty({
  dsn,
  newHref,
}: {
  /** Absent without `client-keys:read`: there is nothing to fill in. */
  dsn: string | null;
  /** Absent when the reader may not create monitors by hand. */
  newHref?: string;
}) {
  const t = useTranslations('crons');

  return (
    <div className="grid w-full items-start gap-10 py-2 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] lg:gap-14">
      <div className="flex flex-col gap-8">
        <div className="flex flex-col gap-3">
          <span className="flex size-10 items-center justify-center rounded-lg border border-border bg-surface text-fg-secondary">
            <CronsIcon size="md" />
          </span>
          <Text variant="title">{t('empty.title')}</Text>
          <Text variant="body" tone="secondary" className="max-w-md">
            {t('empty.description')}
          </Text>
        </div>

        <ol className="flex flex-col">
          {STEPS.map((step, i) => (
            <li key={step} className="relative flex gap-3 pb-6 last:pb-0">
              {i < STEPS.length - 1 ? (
                <span className="absolute start-3 top-7 bottom-1 w-px bg-border" />
              ) : null}
              <span className="flex size-6 shrink-0 items-center justify-center rounded-full border border-border bg-surface font-mono text-fg-secondary text-mono-sm">
                {i + 1}
              </span>
              <div className="flex flex-col gap-0.5 pt-0.5">
                <Text variant="label">{t(`empty.steps.${step}.title`)}</Text>
                <Text variant="meta" tone="subtle">
                  {t(`empty.steps.${step}.description`)}
                </Text>
              </div>
            </li>
          ))}
        </ol>
      </div>

      <div className="flex min-w-0 flex-col gap-3">
        {dsn ? (
          <CronSetupGuide
            dsn={dsn}
            slug={SAMPLE_SLUG}
            schedule={SAMPLE_SCHEDULE}
          />
        ) : null}
        <div className="flex items-center gap-2">
          <span className="size-2 shrink-0 animate-pulse rounded-full bg-fg-ghost" />
          <Text variant="label" tone="secondary">
            {t('health.waiting')}
          </Text>
        </div>
        <Text variant="meta" tone="subtle">
          {t('empty.upsertNote')}
        </Text>
        {newHref ? (
          <div className="mt-3 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border border-dashed px-4 py-3">
            <Text variant="meta" tone="subtle">
              {t('empty.orManual')}
            </Text>
            <Button size="sm" render={<Link href={newHref} />}>
              {t('empty.manual')}
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
