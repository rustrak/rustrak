import type { SpanResponse } from '@rustrak/client';
import { ArrowRightIcon, cn, Text } from '@rustrak/ui';
import { Fragment } from 'react';
import { useTranslations } from 'use-intl';
import {
  agentGraph,
  type GraphAgent,
} from '@/features/agent-trace/model/conversation';

/**
 * The trace as its agents: a card each, in the order they first ran, with
 * the models it called and the tools it used -- failures in red -- and the
 * handoffs between them as arrows. Left to right on a wide screen, top to
 * bottom on a narrow one.
 */
export function AgentGraphView({ spans }: { spans: SpanResponse[] }) {
  const t = useTranslations('agents.graph');
  const { agents, handoffs } = agentGraph(spans);

  if (agents.length === 0) {
    return (
      <Text variant="meta" tone="subtle" className="px-5 pb-5">
        {t('empty')}
      </Text>
    );
  }

  // A handoff back to an earlier agent, or to one further along, has no
  // arrow between neighbouring cards: those are listed under them.
  const unseen = handoffs.filter(
    (h) =>
      !agents.some(
        (agent, index) =>
          agents[index - 1]?.name === h.from && agent.name === h.to,
      ),
  );

  return (
    <div className="flex flex-col gap-3 px-5 pb-5">
      <div className="flex flex-col items-stretch gap-3 overflow-x-auto lg:flex-row lg:items-start">
        <AgentCards agents={agents} handoffs={handoffs} />
      </div>
      {unseen.length > 0 ? (
        <ul className="flex flex-col gap-1 text-fg-subtle text-hint">
          {unseen.map((h) => (
            <li key={`${h.from}→${h.to}`} className="flex items-center gap-1.5">
              <span className="text-fg-secondary">{h.from}</span>
              <ArrowRightIcon size="sm" aria-hidden="true" />
              <span className="text-fg-secondary">{h.to}</span>
              <span>{t('handoff', { count: h.count })}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

/** The cards in the order the agents first ran, an arrow between each pair. */
function AgentCards({
  agents,
  handoffs,
}: {
  agents: GraphAgent[];
  handoffs: { from: string; to: string; count: number }[];
}) {
  const t = useTranslations('agents.graph');
  return (
    <>
      {agents.map((agent, index) => {
        const previous = agents[index - 1];
        const handoff =
          previous &&
          handoffs.find((h) => h.from === previous.name && h.to === agent.name);
        return (
          <Fragment key={agent.name}>
            {previous ? (
              <div className="flex shrink-0 items-center justify-center gap-1.5 self-center text-fg-subtle text-hint">
                <ArrowRightIcon
                  size="sm"
                  className="max-lg:rotate-90"
                  aria-hidden="true"
                />
                {handoff ? t('handoff', { count: handoff.count }) : t('then')}
              </div>
            ) : null}
            <AgentCard agent={agent} />
          </Fragment>
        );
      })}
    </>
  );
}

function AgentCard({ agent }: { agent: GraphAgent }) {
  const t = useTranslations('agents.graph');
  return (
    <section
      className={cn(
        'flex min-w-56 flex-col gap-3 rounded-xl border bg-surface p-3.5 lg:w-64 lg:shrink-0',
        agent.errors > 0 ? 'border-sev-error' : 'border-border',
      )}
    >
      <header className="flex items-center gap-2">
        <span
          aria-hidden="true"
          className="size-2 shrink-0 rounded-pill bg-chart-1"
        />
        <h3 className="truncate font-medium text-fg text-value">
          {agent.name}
        </h3>
        <span className="ms-auto shrink-0 text-fg-subtle text-meta">
          {t('runs', { count: agent.runs })}
        </span>
      </header>
      <Uses title={t('models')} uses={agent.models} swatch="bg-chart-4" />
      <Uses title={t('tools')} uses={agent.tools} swatch="bg-chart-5" />
    </section>
  );
}

function Uses({
  title,
  uses,
  swatch,
}: {
  title: string;
  uses: GraphAgent['tools'];
  swatch: string;
}) {
  if (uses.length === 0) return null;
  return (
    <div className="flex flex-col gap-1">
      <span className="text-column text-fg-meta">{title}</span>
      <ul className="flex flex-col gap-1">
        {uses.map((use) => (
          <li key={use.name} className="flex items-center gap-2 text-meta">
            <span
              aria-hidden="true"
              className={cn('size-1.5 shrink-0 rounded-pill', swatch)}
            />
            <span className="min-w-0 flex-1 truncate font-mono text-fg-secondary text-mono-sm">
              {use.name}
            </span>
            <span className="shrink-0 text-fg-subtle tabular-nums">
              ×{use.calls}
            </span>
            {use.errors > 0 ? (
              <span className="shrink-0 text-sev-error-fg tabular-nums">
                {use.errors}✕
              </span>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  );
}
