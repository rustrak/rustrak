import type {
  AgentsConversationsResponse,
  ConversationSpan,
} from '@rustrak/client';
import {
  Button,
  ChevronLeftIcon,
  ChevronRightIcon,
  cn,
  focusRingInset,
  formatSpanDuration,
  Skeleton,
  Tag,
  Text,
} from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import {
  conversationHref,
  conversationTurns,
} from '@/features/agent-trace/model/conversation';
import { Link } from '@/shared/ui/components/link';

/**
 * The conversations, the most recently active first: who took part, how many
 * turns it ran, what it cost in calls and tokens, and whether any of it
 * failed.
 */
type AgentConversationSummary = AgentsConversationsResponse['items'][number];

const SKELETON_ROWS = [0, 1, 2, 3, 4, 5, 6, 7];

export function ConversationsList({
  projectId,
  page,
  currentPage,
  loading,
  onPageChange,
}: {
  projectId: number;
  /** Absent while the first page loads: the list draws skeleton rows. */
  page?: AgentsConversationsResponse;
  currentPage: number;
  loading: boolean;
  onPageChange: (page: number) => void;
}) {
  const t = useTranslations('agents.conversations');

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-panel">
      {!page ? (
        <ul className="min-h-0 flex-1 divide-y divide-border-divider overflow-hidden">
          {SKELETON_ROWS.map((row) => (
            <li key={row} className="flex flex-col gap-1.5 px-4 py-3">
              <Skeleton className="h-4 w-48" />
              <Skeleton className="h-3.5 w-72" />
            </li>
          ))}
        </ul>
      ) : page.items.length === 0 ? (
        <Text variant="meta" tone="subtle" className="px-4 py-16 text-center">
          {t('empty')}
        </Text>
      ) : (
        <ul
          className={cn(
            'min-h-0 flex-1 divide-y divide-border-divider overflow-y-auto',
            loading && 'opacity-60',
          )}
        >
          {page.items.map((conversation) => (
            <ConversationRow
              key={conversation.conversation_id}
              projectId={projectId}
              conversation={conversation}
            />
          ))}
        </ul>
      )}
      {page && page.total_pages > 1 ? (
        <div className="flex shrink-0 items-center justify-between gap-2 border-border-divider border-t px-4 py-2">
          <Text variant="meta" tone="subtle">
            {t('pageOf', { page: currentPage, total: page.total_pages })}
          </Text>
          <div className="flex gap-1">
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronLeftIcon}
              aria-label={t('newer')}
              disabled={loading || currentPage <= 1}
              onClick={() => onPageChange(currentPage - 1)}
            />
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronRightIcon}
              aria-label={t('older')}
              disabled={loading || currentPage >= page.total_pages}
              onClick={() => onPageChange(currentPage + 1)}
            />
          </div>
        </div>
      ) : null}
    </div>
  );
}

function ConversationRow({
  projectId,
  conversation,
}: {
  projectId: number;
  conversation: AgentConversationSummary;
}) {
  const t = useTranslations('agents.conversations');
  const format = useFormatter();
  // No time when none of its spans carried one.
  const last = conversation.last_seen ? new Date(conversation.last_seen) : null;

  return (
    <li>
      <Link
        href={conversationHref(projectId, conversation.conversation_id)}
        className={cn(
          'flex flex-col gap-1 px-4 py-3 transition-colors hover:bg-surface-hover sm:flex-row sm:items-center sm:gap-4',
          focusRingInset,
        )}
      >
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="truncate font-medium text-fg text-value">
            {conversation.agent_names.join(' → ') || t('unnamed')}
          </span>
          <span className="truncate font-mono text-fg-subtle text-mono-sm">
            {conversation.conversation_id}
          </span>
        </span>
        <span className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 text-fg-secondary text-meta tabular-nums">
          <span>{t('turns', { count: conversation.turn_count })}</span>
          <span>{t('calls', { count: conversation.llm_call_count })}</span>
          <span>{t('tools', { count: conversation.tool_call_count })}</span>
          <span>
            {t('tokens', {
              count: format.number(conversation.total_tokens, 'compact'),
            })}
          </span>
          {conversation.error_count > 0 ? (
            <Tag variant="soft" tone="error">
              {t('errors', { count: conversation.error_count })}
            </Tag>
          ) : null}
          {last ? (
            <span
              className="text-fg-subtle"
              title={format.dateTime(last, 'dateTime')}
            >
              {format.relativeTime(last)}
            </span>
          ) : null}
        </span>
      </Link>
    </li>
  );
}

/**
 * A conversation as it was had: turn by turn, what the user asked, the tools
 * the agents reached for -- failed ones in red, each opening its arguments
 * and result -- and what came back, with a link to the turn's trace.
 */
export function ConversationTranscript({
  projectId,
  spans,
}: {
  projectId: number;
  spans: ConversationSpan[];
}) {
  const t = useTranslations('agents.conversations');
  const format = useFormatter();

  return (
    <ol className="flex flex-col gap-6">
      {conversationTurns(spans).map((turn, index) => (
        <li key={turn.traceId} className="flex flex-col gap-2.5">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-fg-subtle text-meta">
            <span className="font-mono text-column text-fg-meta uppercase">
              {t('turn', { number: index + 1 })}
            </span>
            {turn.startedAt ? (
              <span>
                {format.dateTime(new Date(turn.startedAt), 'precise')}
              </span>
            ) : null}
            <span>{turn.agents.join(' → ')}</span>
            <span>
              {t('tokens', { count: format.number(turn.tokens, 'compact') })}
            </span>
            <Link
              href={`/projects/${projectId}/agents/${turn.traceId}`}
              className="ms-auto text-fg-secondary hover:text-fg hover:underline"
            >
              {t('openTrace')}
            </Link>
          </div>

          {turn.question ? (
            <Bubble side="user" label={t('user')}>
              {turn.question}
            </Bubble>
          ) : null}

          {turn.tools.length > 0 ? (
            <div className="flex flex-col gap-1.5 ps-4">
              {turn.tools.map((tool) => (
                <ToolStep key={tool.id} tool={tool} />
              ))}
            </div>
          ) : null}

          {turn.answer ? (
            <Bubble
              side="assistant"
              label={turn.agents.at(-1) ?? t('assistant')}
            >
              {turn.answer}
            </Bubble>
          ) : turn.errors > 0 ? (
            <p className="text-sev-error-fg text-meta">{t('noAnswer')}</p>
          ) : null}
        </li>
      ))}
    </ol>
  );
}

function Bubble({
  side,
  label,
  children,
}: {
  side: 'user' | 'assistant';
  label: string;
  children: string;
}) {
  return (
    <div
      className={cn(
        'flex max-w-[44rem] flex-col gap-1 rounded-xl border border-border px-3.5 py-2.5',
        side === 'user'
          ? 'self-end bg-surface-selected'
          : 'self-start bg-surface',
      )}
    >
      <span className="font-mono text-column text-fg-meta uppercase">
        {label}
      </span>
      <p className="whitespace-pre-wrap break-words text-body text-fg">
        {children}
      </p>
    </div>
  );
}

function ToolStep({ tool }: { tool: ConversationSpan }) {
  const failed = tool.status != null && tool.status !== 'ok';
  return (
    <details className="group rounded-lg border border-border bg-surface">
      <summary
        className={cn(
          'flex cursor-pointer list-none items-center gap-2 px-3 py-1.5 font-mono text-mono-sm',
          failed ? 'text-sev-error-fg' : 'text-fg-secondary',
        )}
      >
        <span aria-hidden="true" className="text-fg-ghost group-open:rotate-90">
          ›
        </span>
        <span className="truncate">{tool.tool_name}</span>
        {failed ? (
          <Tag variant="soft" tone="error">
            {tool.status}
          </Tag>
        ) : null}
        {tool.duration_ms != null ? (
          <span className="ms-auto text-fg-subtle tabular-nums">
            {formatSpanDuration(tool.duration_ms)}
          </span>
        ) : null}
      </summary>
      <div className="flex flex-col gap-2 border-border-divider border-t px-3 py-2">
        {tool.input ? <Pre value={tool.input} /> : null}
        {tool.output ? <Pre value={tool.output} /> : null}
      </div>
    </details>
  );
}

function Pre({ value }: { value: string }) {
  let text = value;
  try {
    text = JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    // Left as received.
  }
  return (
    <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words font-mono text-fg-secondary text-mono-sm">
      {text}
    </pre>
  );
}
