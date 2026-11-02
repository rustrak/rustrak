import type { SpanDetailResponse } from '@rustrak/client';
import { cn, formatSpanDuration, Tag, type TagTone } from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import {
  aiInput,
  aiOutput,
  availableTools,
  chatMessages,
  hasTokenMismatch,
  type SpanAttributes,
  tokenBreakdown,
} from '@/features/agent-trace/model/gen-ai';

/** The four kinds of AI span, in the waterfall's colours. */
const OPERATION_TONE: Record<string, TagTone> = {
  agent: 'brand',
  ai_client: 'info',
  tool: 'warning',
  handoff: 'neutral',
};

// The spec types `attributes` as `unknown` (a `serde_json::Value` on the
// server); it is always the flattened `{key: value}` object.
function attributesOf(span: SpanDetailResponse): SpanAttributes {
  return span.attributes as SpanAttributes;
}

/**
 * One AI span in full, in the order it is read: what it was and how it
 * ended, who and which model, the tokens it cost, then what went in and what
 * came out -- a conversation drawn as one, tool arguments and results as
 * JSON -- and every attribute as sent, last, as the escape hatch.
 */
export function AiSpanDetail({ span }: { span: SpanDetailResponse }) {
  const t = useTranslations('agents.spanDetail');
  const attributes = attributesOf(span);
  const tokens = tokenBreakdown(attributes);
  const input = aiInput(attributes);
  const output = aiOutput(attributes);
  const tools = availableTools(attributes);

  return (
    <div className="flex flex-col gap-5">
      <SpanHeader span={span} />
      <Highlights span={span} />
      {tokens ? (
        <Section title={t('tokens')}>
          {hasTokenMismatch(attributes) ? (
            // Said out loud rather than hidden: a reader comparing token
            // counts across spans would otherwise trust a number nothing in
            // the span supports.
            <p className="rounded-md bg-sev-warning-surface px-2.5 py-1.5 text-fg text-meta">
              {t('tokenMismatch')}
            </p>
          ) : null}
          <Tokens tokens={tokens} />
        </Section>
      ) : null}
      {tools ? (
        <Section title={t('availableTools')}>
          <div className="flex flex-wrap gap-1.5">
            {tools.map((tool) => (
              <Tag key={tool} variant="soft" tone="neutral">
                {tool}
              </Tag>
            ))}
          </div>
        </Section>
      ) : null}
      {input ? (
        <Section title={t('input')}>
          {input.systemInstructions ? (
            <Message speaker="system" content={input.systemInstructions} />
          ) : null}
          <Conversation raw={input.messages} />
        </Section>
      ) : null}
      {output ? (
        <Section title={t('output')}>
          {output.text ? (
            <Conversation raw={output.text} speaker="assistant" />
          ) : null}
          {output.object ? (
            <Labelled label={t('responseObject')}>
              <Payload value={output.object} />
            </Labelled>
          ) : null}
          {output.toolCalls ? (
            <Labelled label={t('requestedToolCalls')}>
              <Payload value={output.toolCalls} />
            </Labelled>
          ) : null}
        </Section>
      ) : null}
      <Section title={t('allAttributes')}>
        <dl className="divide-y divide-border-divider rounded-lg border border-border">
          {Object.entries(attributes)
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([key, value]) => (
              <div
                key={key}
                className="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-3 px-3 py-1.5 font-mono text-mono-sm"
              >
                <dt className="break-all text-fg-subtle">{key}</dt>
                <dd className="max-h-40 overflow-y-auto break-all text-fg">
                  {typeof value === 'string' ? value : JSON.stringify(value)}
                </dd>
              </div>
            ))}
        </dl>
      </Section>
    </div>
  );
}

function SpanHeader({ span }: { span: SpanDetailResponse }) {
  const failed = span.status != null && span.status !== 'ok';
  return (
    <header className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        {span.gen_ai_operation_type ? (
          <Tag
            variant="soft"
            tone={OPERATION_TONE[span.gen_ai_operation_type] ?? 'neutral'}
          >
            {span.gen_ai_operation_type}
          </Tag>
        ) : null}
        {span.status ? (
          <Tag variant="soft" tone={failed ? 'error' : 'neutral'}>
            {span.status}
          </Tag>
        ) : null}
        {span.duration_ms != null ? (
          <span className="font-mono text-fg-subtle text-mono-sm tabular-nums">
            {formatSpanDuration(span.duration_ms)}
          </span>
        ) : null}
      </div>
      <p className="break-all font-mono text-fg text-mono-sm">
        {span.description ?? span.op ?? span.span_id}
      </p>
    </header>
  );
}

function Highlights({ span }: { span: SpanDetailResponse }) {
  const t = useTranslations('agents.spanDetail');
  const effort = attributesOf(span)['gen_ai.request.reasoning_effort'];
  const rows: [string, string | null | undefined][] = [
    [t('agentName'), span.gen_ai_agent_name],
    [t('model'), span.gen_ai_response_model ?? span.gen_ai_request_model],
    [t('toolName'), span.gen_ai_tool_name],
    [t('reasoningEffort'), typeof effort === 'string' ? effort : null],
    [t('conversationId'), span.gen_ai_conversation_id],
  ];
  const shown = rows.filter(([, value]) => value);
  if (shown.length === 0) return null;

  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 text-meta">
      {shown.map(([label, value]) => (
        <div key={label} className="contents">
          <dt className="text-fg-subtle">{label}</dt>
          <dd className="break-all font-mono text-fg-secondary text-mono-sm">
            {value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function Tokens({
  tokens,
}: {
  tokens: NonNullable<ReturnType<typeof tokenBreakdown>>;
}) {
  const t = useTranslations('agents.spanDetail');
  const format = useFormatter();
  const rows: [string, number][] = [
    [t('inputTokens'), tokens.netNewInput],
    ...(tokens.cached > 0
      ? [[t('cachedTokens'), tokens.cached] as [string, number]]
      : []),
    [t('outputTokens'), tokens.output],
  ];
  return (
    <dl className="grid grid-cols-[repeat(auto-fit,minmax(6rem,1fr))] gap-px overflow-hidden rounded-lg border border-border bg-border">
      {[...rows, [t('totalTokens'), tokens.total] as [string, number]].map(
        ([label, value]) => (
          <div
            key={label}
            className="flex flex-col gap-0.5 bg-surface px-3 py-2"
          >
            <dt className="truncate text-fg-subtle text-hint">{label}</dt>
            <dd className="font-mono text-fg text-mono-sm tabular-nums">
              {format.number(value)}
            </dd>
          </div>
        ),
      )}
    </dl>
  );
}

/** A payload that is a conversation is drawn as one; anything else as JSON. */
function Conversation({ raw, speaker }: { raw: string; speaker?: string }) {
  const messages = chatMessages(raw);
  if (messages) {
    return (
      <div className="flex flex-col gap-2">
        {messages.map((message, index) => (
          // Messages are positional by nature: index is their identity.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          <Message
            key={index}
            speaker={message.role}
            content={message.content}
          />
        ))}
      </div>
    );
  }
  return speaker && !looksLikeJson(raw) ? (
    <Message speaker={speaker} content={raw} />
  ) : (
    <Payload value={raw} />
  );
}

const ROLE_STYLE: Record<string, string> = {
  user: 'bg-surface-selected',
  assistant: 'bg-surface',
  system: 'bg-transparent border-dashed',
  tool: 'bg-transparent',
};

function Message({ speaker, content }: { speaker: string; content: string }) {
  return (
    <div
      className={cn(
        'flex flex-col gap-1 rounded-lg border border-border px-3 py-2',
        ROLE_STYLE[speaker] ?? 'bg-surface',
      )}
    >
      <span className="text-column text-fg-meta">{speaker}</span>
      {looksLikeJson(content) ? (
        <Payload value={content} bare />
      ) : (
        <p className="whitespace-pre-wrap break-words text-body text-fg">
          {content}
        </p>
      )}
    </div>
  );
}

function Payload({ value, bare }: { value: string; bare?: boolean }) {
  let text = value;
  try {
    text = JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    // Left as received.
  }
  return (
    <pre
      className={cn(
        'max-h-96 overflow-auto whitespace-pre-wrap break-words font-mono text-fg-secondary text-mono-sm',
        !bare && 'rounded-lg border border-border bg-surface px-3 py-2',
      )}
    >
      {text}
    </pre>
  );
}

function looksLikeJson(value: string): boolean {
  const start = value.trimStart()[0];
  if (start !== '{' && start !== '[') return false;
  try {
    JSON.parse(value);
    return true;
  } catch {
    return false;
  }
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-column text-fg-meta">{title}</h3>
      {children}
    </section>
  );
}

function Labelled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-fg-subtle text-hint">{label}</span>
      {children}
    </div>
  );
}
