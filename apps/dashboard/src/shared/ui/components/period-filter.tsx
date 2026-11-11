import { EnvironmentIcon, TimeIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { PERIOD_CHOICES, type PeriodChoice } from '@/shared/lib/period';
import { scopeActions } from '@/shared/lib/scope-actions';
import { ScopeBar, ScopeSegment } from '@/shared/ui/components/scope-bar';

/** The window alone, beside the list's query bar. */
export function PeriodFilter(props: {
  period: PeriodChoice;
  onChange: (period: PeriodChoice) => void;
}) {
  return (
    <ScopeBar>
      <PeriodSegment {...props} />
    </ScopeBar>
  );
}

export function PeriodSegment({
  period,
  onChange,
}: {
  period: PeriodChoice;
  onChange: (period: PeriodChoice) => void;
}) {
  const t = useTranslations('projectPages.overview');
  return (
    <ScopeSegment
      icon={TimeIcon}
      // The short form: the menu spells it out.
      label={period === 'all' ? t('periods.allShort') : period}
      actions={PERIOD_CHOICES.map((value) => ({
        id: value,
        label: t(`periods.${value}`),
        selected: value === period,
        // All time stands apart: it is the one with nothing to compare to.
        separated: value === 'all',
        onSelect: () => onChange(value),
      }))}
    />
  );
}

/** One environment or all; no choices while they load or when they fail. */
export function EnvironmentSegment({
  environments,
  environment,
  onChange,
}: {
  environments: string[];
  environment?: string;
  onChange: (environment: string | undefined) => void;
}) {
  const t = useTranslations('projectPages.overview');
  return (
    <ScopeSegment
      icon={EnvironmentIcon}
      label={environment ?? t('environment')}
      active={environment !== undefined}
      actions={scopeActions(
        environments,
        environment,
        t('allEnvironments'),
        t('noEnvironments'),
        onChange,
      )}
    />
  );
}
