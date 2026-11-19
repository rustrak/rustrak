import { EnvironmentIcon, TimeIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import {
  PERIOD_CHOICES,
  type PerformancePeriod,
  type PeriodChoice,
} from '@/shared/lib/period';
import { scopeActions } from '@/shared/lib/scope-actions';
import { ScopeBar, ScopeSegment } from '@/shared/ui/components/scope-bar';

type AnyPeriod = PeriodChoice | PerformancePeriod;

interface PeriodProps<P extends AnyPeriod> {
  period: P;
  onChange: (period: P) => void;
  /** The windows on offer; the project pages' own by default. */
  choices?: readonly P[];
}

/** The window alone, beside the list's query bar. */
export function PeriodFilter<P extends AnyPeriod = PeriodChoice>(
  props: PeriodProps<P>,
) {
  return (
    <ScopeBar>
      <PeriodSegment {...props} />
    </ScopeBar>
  );
}

export function PeriodSegment<P extends AnyPeriod = PeriodChoice>({
  period,
  onChange,
  choices = PERIOD_CHOICES as readonly AnyPeriod[] as readonly P[],
}: PeriodProps<P>) {
  const t = useTranslations('projectPages.overview');
  return (
    <ScopeSegment
      icon={TimeIcon}
      // The short form: the menu spells it out.
      label={period === 'all' ? t('periods.allShort') : period}
      actions={choices.map((value) => ({
        id: value,
        label: t(`periods.${value as AnyPeriod}`),
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
