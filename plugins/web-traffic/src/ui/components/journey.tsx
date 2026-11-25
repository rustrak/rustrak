import {
  PluginLink,
  useFormatter,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { FactList } from '@rustrak/ui';
import { duration, type Profile, type Step } from '../model';
import { useRegionName } from './region';

/** One page read or event done, at its time. */
function StepRow({ step }: { step: Step }) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const event = step.kind === 'event';
  return (
    <li className="flex items-baseline gap-4 px-5 py-1.5 text-meta">
      <span className="w-16 shrink-0 tabular-nums text-fg-subtle">
        {format.dateTime(new Date(step.at * 1000), {
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
        })}
      </span>
      <span
        className={
          event
            ? 'shrink-0 rounded bg-surface-brand px-1.5 text-fg-on-brand'
            : 'shrink-0 rounded bg-surface-chip px-1.5 text-fg-secondary'
        }
      >
        {t(`journey.${step.kind}`)}
      </span>
      <span className="min-w-0 flex-1">
        <PluginLink
          to={event ? `events/${encodeURIComponent(step.label ?? '')}` : ''}
          search={event ? undefined : { path: step.path }}
          className="text-fg hover:text-fg-brand"
        >
          {event ? step.label : step.path}
        </PluginLink>
        <span className="ml-2 truncate text-fg-subtle">
          {event ? step.path : step.label}
        </span>
        {step.data ? (
          <span className="mt-0.5 block truncate font-mono text-fg-subtle">
            {step.data}
          </span>
        ) : null}
      </span>
    </li>
  );
}

/** One visit of the day: when it began, how long, and its steps. */
export function VisitSection({ visit, n }: { visit: Step[]; n: number }) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const first = visit[0]!;
  const last = visit.at(-1)!;
  return (
    <section className="border-b border-border-divider py-2 last:border-b-0">
      <h3 className="px-5 py-1 text-meta font-medium text-fg-subtle">
        {t('visitor.visit', { n })} ·{' '}
        {format.dateTime(new Date(first.at * 1000), 'dateTime')} ·{' '}
        {duration(last.at - first.at)}
      </h3>
      <ol>
        {visit.map((step, i) => (
          // Steps have no id; their order is the identity.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          <StepRow key={i} step={step} />
        ))}
      </ol>
    </section>
  );
}

/** Everything known of a visitor, as facts. */
export function ProfileFacts({ profile: p }: { profile: Profile }) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const regionName = useRegionName();
  const when = (at: number) => format.dateTime(new Date(at * 1000), 'dateTime');
  const or = (value: string | null) => value ?? '–';
  return (
    <FactList
      className="gap-y-2"
      valueClassName="text-fg-secondary"
      facts={[
        [t('dimension.country'), p.country ? regionName(p.country) : '–'],
        [t('dimension.language'), or(p.language)],
        [t('dimension.browser'), or(p.browser)],
        [t('dimension.os'), or(p.os)],
        [t('dimension.device'), or(p.device)],
        [t('dimension.referrer'), or(p.referrer)],
        [t('dimension.source'), or(p.source)],
        [t('dimension.campaign'), or(p.campaign)],
        [t('ranking.entry'), p.entry],
        [t('ranking.exit'), p.exit],
        [t('visitor.pages'), format.number(p.pages)],
        [t('visitor.first'), when(p.first)],
        [t('visitor.last'), when(p.last)],
      ]}
    />
  );
}
