import type { WebVitalsSummary } from '@rustrak/client';
import { useTranslations } from 'use-intl';
import {
  CORE_VITALS,
  metricHref,
  OTHER_VITALS,
} from '@/features/web-vital/model/web-vitals';
import { MetricCard } from '@/features/web-vital/ui/components/web-vitals';
import type { WebVitalsScope } from './web-vitals-filters';

/**
 * The three Core Web Vitals, then the two that lead to them, as PageSpeed
 * groups them. Each card opens its vital's own page, for the project or
 * for `page`.
 */
export function MetricCards({
  projectId,
  summary,
  scope,
  page,
}: {
  projectId: number;
  summary: WebVitalsSummary;
  scope: WebVitalsScope;
  page?: string;
}) {
  const t = useTranslations('webVitals');
  const cards = (vitals: typeof CORE_VITALS, columns: string) => (
    <div className={`grid shrink-0 gap-3 ${columns}`}>
      {vitals.map((vital) => (
        <MetricCard
          key={vital}
          vital={vital}
          stats={summary.vitals.find((s) => s.vital === vital)}
          href={metricHref(projectId, vital, { ...scope, page })}
        />
      ))}
    </div>
  );
  return (
    <>
      <section className="flex shrink-0 flex-col gap-3">
        <h2 className="text-fg text-section">{t('coreTitle')}</h2>
        {cards(CORE_VITALS, 'md:grid-cols-3')}
      </section>
      <section className="flex shrink-0 flex-col gap-3">
        <h2 className="text-fg text-section">{t('otherTitle')}</h2>
        {cards(OTHER_VITALS, 'md:grid-cols-2')}
      </section>
    </>
  );
}
