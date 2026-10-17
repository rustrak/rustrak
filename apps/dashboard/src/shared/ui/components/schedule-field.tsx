import type { IntervalUnit, Schedule } from '@rustrak/client';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Input } from '@/shared/ui/components/shadcn/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';

const UNITS: readonly IntervalUnit[] = [
  'minute',
  'hour',
  'day',
  'week',
  'month',
  'year',
];

const LABEL =
  'text-xs font-bold uppercase tracking-widest text-muted-foreground';

/**
 * Picks a schedule: a crontab, or "every N units". Controlled, so any form
 * can hold it; it remembers what was typed in the mode not shown, so
 * switching back and forth loses nothing.
 */
export function ScheduleField({
  id,
  value,
  onChange,
  disabled,
  error,
}: {
  id?: string;
  value: Schedule;
  onChange: (schedule: Schedule) => void;
  disabled?: boolean;
  /** Shown under the crontab or interval input. */
  error?: string;
}) {
  const t = useTranslations('schedule');
  const [crontab, setCrontab] = useState(
    value.type === 'crontab' ? value.value : '0 * * * *',
  );
  const [interval, setInterval] = useState<{
    value: number;
    unit: IntervalUnit;
  }>(
    value.type === 'interval'
      ? { value: value.value, unit: value.unit }
      : { value: 1, unit: 'hour' },
  );
  const count = Number.isNaN(interval.value) ? 2 : interval.value;

  const switchTo = (type: Schedule['type']) => {
    if (type === value.type) return;
    onChange(
      type === 'crontab'
        ? { type: 'crontab', value: crontab }
        : { type: 'interval', ...interval },
    );
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <span className={`block ${LABEL}`}>{t('type')}</span>
        <div className="flex w-fit items-center gap-1 rounded-lg border bg-muted/30 p-1">
          {(['crontab', 'interval'] as const).map((option) => (
            <Button
              key={option}
              type="button"
              size="sm"
              variant={value.type === option ? 'secondary' : 'ghost'}
              className="h-7 px-3"
              aria-pressed={value.type === option}
              disabled={disabled}
              onClick={() => switchTo(option)}
            >
              {t(option)}
            </Button>
          ))}
        </div>
      </div>

      {value.type === 'crontab' ? (
        <div className="space-y-2">
          <label htmlFor={id} className={`block ${LABEL}`}>
            {t('crontab')}
          </label>
          <Input
            id={id}
            className="max-w-xs font-mono"
            placeholder="0 * * * *"
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            aria-invalid={error ? true : undefined}
            value={crontab}
            onChange={(event) => {
              setCrontab(event.target.value);
              onChange({ type: 'crontab', value: event.target.value });
            }}
          />
          <p className="text-sm text-muted-foreground">{t('crontabHint')}</p>
        </div>
      ) : (
        <div className="space-y-2">
          <span className={`block ${LABEL}`}>{t('every')}</span>
          <div className="flex items-start gap-2">
            <Input
              id={id}
              type="number"
              inputMode="numeric"
              min={1}
              className="w-24 tabular-nums"
              disabled={disabled}
              aria-invalid={error ? true : undefined}
              value={Number.isNaN(interval.value) ? '' : interval.value}
              onChange={(event) => {
                const next = { ...interval, value: event.target.valueAsNumber };
                setInterval(next);
                onChange({ type: 'interval', ...next });
              }}
            />
            <Select
              value={interval.unit}
              onValueChange={(unit) => {
                if (!unit) return;
                const next = { ...interval, unit: unit as IntervalUnit };
                setInterval(next);
                onChange({ type: 'interval', ...next });
              }}
              disabled={disabled}
            >
              <SelectTrigger className="w-36" aria-label={t('unit')}>
                <SelectValue>
                  {(unit: string) => t(`units.${unit}`, { count })}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                {UNITS.map((unit) => (
                  <SelectItem key={unit} value={unit}>
                    {t(`units.${unit}`, { count })}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <p className="text-sm text-muted-foreground">{t('intervalHint')}</p>
        </div>
      )}
      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  );
}
