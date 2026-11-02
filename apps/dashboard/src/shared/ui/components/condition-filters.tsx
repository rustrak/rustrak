import {
  Field,
  FieldHint,
  FieldLabel,
  Input,
  Select,
  SelectItem,
} from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { type FilterFields, levels } from '@/shared/lib/rule-conditions';

/**
 * What keeps an automatic action quiet: the quietest level that counts, the
 * environments that count, and how many events make a new issue worth it.
 * Alert rules and GitHub's automatic issues both use it.
 */
export function ConditionFilters({
  value,
  onChange,
  showMinEvents,
  disabled,
}: {
  value: FilterFields;
  onChange: (value: FilterFields) => void;
  showMinEvents: boolean;
  disabled: boolean;
}) {
  const t = useTranslations('conditions');
  const [eventsDraft, setEventsDraft] = useState<string | null>(null);
  const set = (change: Partial<FilterFields>) =>
    onChange({ ...value, ...change });
  const levelLabels: Record<string, string> = {
    any: t('anyLevel'),
    ...Object.fromEntries(
      levels.map((level) => [level, t('levelOrAbove', { level })]),
    ),
  };

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-3">
        <Field>
          <FieldLabel>{t('minLevel')}</FieldLabel>
          <Select
            id="condition-level"
            value={value.min_level}
            items={levelLabels}
            onValueChange={(min_level) =>
              set({ min_level: min_level as FilterFields['min_level'] })
            }
            disabled={disabled}
          >
            {Object.entries(levelLabels).map(([level, label]) => (
              <SelectItem key={level} value={level}>
                {label}
              </SelectItem>
            ))}
          </Select>
        </Field>
        {showMinEvents && (
          <Field>
            <FieldLabel>{t('minEvents')}</FieldLabel>
            <Input
              id="condition-events"
              type="number"
              min={1}
              value={eventsDraft ?? value.min_events}
              disabled={disabled}
              onChange={(e) => {
                // Hold what is typed, even an empty field, until it is a count.
                setEventsDraft(e.target.value);
                // `Number`, not `parseInt`: the field accepts `1e2` as 100.
                const count = Number(e.target.value);
                if (Number.isSafeInteger(count) && count >= 1)
                  set({ min_events: count });
              }}
              onBlur={() => setEventsDraft(null)}
            />
          </Field>
        )}
      </div>
      <Field>
        <FieldLabel>{t('environments')}</FieldLabel>
        <Input
          id="condition-environments"
          placeholder="production"
          value={value.environments}
          disabled={disabled}
          onChange={(e) => set({ environments: e.target.value })}
        />
        <FieldHint>{t('description')}</FieldHint>
      </Field>
    </div>
  );
}
