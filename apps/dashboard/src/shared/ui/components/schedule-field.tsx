import type { IntervalUnit, Schedule } from '@rustrak/client';
import {
  Button,
  Field,
  FieldHint,
  FieldLabel,
  Input,
  Select,
  SelectItem,
} from '@rustrak/ui';
import { useId, useState } from 'react';
import { useTranslations } from 'use-intl';

const UNITS: readonly IntervalUnit[] = [
  'minute',
  'hour',
  'day',
  'week',
  'month',
  'year',
];

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
  const fallbackId = useId();
  const inputId = id ?? fallbackId;
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
  const unitLabels = Object.fromEntries(
    UNITS.map((unit) => [unit, t(`units.${unit}`, { count })]),
  );

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
      <div className="flex flex-col gap-1.5">
        <span className="text-fg-secondary text-label">{t('type')}</span>
        <div className="flex w-fit items-center gap-1 rounded-lg border border-border-subtle bg-surface p-1">
          {(['crontab', 'interval'] as const).map((option) => (
            <Button
              key={option}
              size="sm"
              variant="ghost"
              selected={value.type === option}
              disabled={disabled}
              onClick={() => switchTo(option)}
            >
              {t(option)}
            </Button>
          ))}
        </div>
      </div>

      {value.type === 'crontab' ? (
        <Field>
          <FieldLabel>{t('crontab')}</FieldLabel>
          <Input
            id={inputId}
            boxClassName="max-w-xs"
            className="font-mono"
            placeholder="0 * * * *"
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            invalid={Boolean(error)}
            value={crontab}
            onChange={(event) => {
              setCrontab(event.target.value);
              onChange({ type: 'crontab', value: event.target.value });
            }}
          />
          <FieldHint>{t('crontabHint')}</FieldHint>
        </Field>
      ) : (
        <Field>
          <FieldLabel>{t('every')}</FieldLabel>
          <div className="flex items-start gap-2">
            <Input
              id={inputId}
              type="number"
              inputMode="numeric"
              min={1}
              numeric
              boxClassName="w-24"
              disabled={disabled}
              invalid={Boolean(error)}
              value={Number.isNaN(interval.value) ? '' : interval.value}
              onChange={(event) => {
                const next = { ...interval, value: event.target.valueAsNumber };
                setInterval(next);
                onChange({ type: 'interval', ...next });
              }}
            />
            <Select
              value={interval.unit}
              items={unitLabels}
              onValueChange={(unit) => {
                const next = { ...interval, unit };
                setInterval(next);
                onChange({ type: 'interval', ...next });
              }}
              disabled={disabled}
              className="w-36"
              aria-label={t('unit')}
            >
              {UNITS.map((unit) => (
                <SelectItem key={unit} value={unit}>
                  {unitLabels[unit]}
                </SelectItem>
              ))}
            </Select>
          </div>
          <FieldHint>{t('intervalHint')}</FieldHint>
        </Field>
      )}
      {error && <p className="text-danger-fg text-hint">{error}</p>}
    </div>
  );
}
