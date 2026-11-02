import { Select, SelectItem } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { presetsWith } from '@/features/storage/model/retention';

const FOREVER = 'forever';

/** How long a kind of data is kept: a few presets, or forever. */
export function KeepForSelect({
  id,
  value,
  onChange,
  disabled,
  label,
}: {
  id?: string;
  value: number | null;
  onChange: (days: number | null) => void;
  disabled?: boolean;
  /** Names the select when no visible label points at it. */
  label?: string;
}) {
  const t = useTranslations('storage');
  const name = (option: string) =>
    option === FOREVER
      ? t('keep.forever')
      : t('keep.days', { count: Number(option) });

  const options = presetsWith(value).map((days) =>
    days === null ? FOREVER : String(days),
  );

  return (
    <Select
      id={id}
      aria-label={label}
      value={value === null ? FOREVER : String(value)}
      onValueChange={(option) =>
        onChange(option === FOREVER ? null : Number(option))
      }
      items={Object.fromEntries(options.map((o) => [o, name(o)]))}
      disabled={disabled}
    >
      {options.map((option) => (
        <SelectItem key={option} value={option}>
          {name(option)}
        </SelectItem>
      ))}
    </Select>
  );
}
