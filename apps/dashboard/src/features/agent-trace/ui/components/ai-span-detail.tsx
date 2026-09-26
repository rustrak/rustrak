import type { SpanDetail } from '@rustrak/client';
import { useFormatter, useTranslations } from 'use-intl';
import {
  aiInput,
  aiOutput,
  availableTools,
  hasTokenMismatch,
  tokenBreakdown,
} from '@/features/agent-trace/model/gen-ai';
import { Badge } from '@/shared/ui/components/shadcn/badge';

interface AiSpanDetailProps {
  span: SpanDetail;
}

/**
 * Renders a value that arrived as an SDK-serialized JSON string.
 *
 * Pretty-printing is best-effort: these strings are frequently truncated by
 * the SDK's own size limits, and a half-written JSON array is still the most
 * useful thing available to a reader. So a parse failure falls back to the
 * raw text rather than hiding it.
 */
function Payload({ value }: { value: string }) {
  let text = value;
  try {
    text = JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    // Left as received.
  }

  return (
    <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted/40 px-3 py-2 font-mono text-[11px] leading-relaxed">
      {text}
    </pre>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-1.5">
      <h3 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
        {title}
      </h3>
      {children}
    </section>
  );
}

/**
 * The details panel for one selected span.
 *
 * Ordering follows Sentry's own: identity first (agent, model), then the
 * numbers a reader compares across spans (tokens, duration), then the payload
 * that explains them. The raw attribute table sits last as the escape hatch
 * for anything this panel does not model.
 */
export function AiSpanDetail({ span }: AiSpanDetailProps) {
  const t = useTranslations('agents.spanDetail');
  const attributes = span.attributes;
  const tokens = tokenBreakdown(attributes);
  const input = aiInput(attributes);
  const output = aiOutput(attributes);
  const tools = availableTools(attributes);

  return (
    <div className="space-y-4">
      <SpanHeader span={span} />
      <Highlights span={span} />
      {tokens && (
        <TokensSection
          tokens={tokens}
          looksWrong={hasTokenMismatch(attributes)}
        />
      )}
      {tools && <ToolsSection tools={tools} />}
      {input && <InputSection input={input} />}
      {output && <OutputSection output={output} />}
      <Section title={t('allAttributes')}>
        <AttributesTable attributes={attributes} />
      </Section>
    </div>
  );
}

/** What the span is, how it ended and how long it took. */
function SpanHeader({ span }: { span: SpanDetail }) {
  const format = useFormatter();
  const failed = span.status != null && span.status !== 'ok';

  return (
    <header className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        {span.gen_ai_operation_type && (
          <Badge variant="secondary">{span.gen_ai_operation_type}</Badge>
        )}
        {span.status && (
          <Badge variant={failed ? 'destructive' : 'outline'}>
            {span.status}
          </Badge>
        )}
        {span.duration_ms != null && (
          <span className="font-mono text-xs text-muted-foreground">
            {format.number(Math.round(span.duration_ms))} ms
          </span>
        )}
      </div>
      <p className="break-all font-mono text-sm">
        {span.description ?? span.op ?? span.span_id}
      </p>
    </header>
  );
}

/** The identity rows a reader compares across spans, whichever are present. */
function Highlights({ span }: { span: SpanDetail }) {
  const t = useTranslations('agents.spanDetail');

  // Declarative rather than five conditional pushes: the order of these rows
  // is a design decision, and in a list it is visible as one.
  const rows = [
    { key: 'agent', label: t('agentName'), value: span.gen_ai_agent_name },
    {
      key: 'model',
      label: t('model'),
      value: span.gen_ai_response_model ?? span.gen_ai_request_model,
    },
    { key: 'tool', label: t('toolName'), value: span.gen_ai_tool_name },
    {
      key: 'reasoningEffort',
      label: t('reasoningEffort'),
      value: span.attributes['gen_ai.request.reasoning_effort'],
    },
    {
      key: 'conversation',
      label: t('conversationId'),
      value: span.gen_ai_conversation_id,
    },
  ].flatMap((row) => {
    const value = asText(row.value);
    return value ? [{ ...row, value }] : [];
  });

  if (rows.length === 0) return null;

  return (
    <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1 text-xs">
      {rows.map((row) => (
        <div key={row.key} className="contents">
          <dt className="text-muted-foreground">{row.label}</dt>
          <dd className="break-all font-mono">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function TokensSection({
  tokens,
  looksWrong,
}: {
  tokens: NonNullable<ReturnType<typeof tokenBreakdown>>;
  looksWrong: boolean;
}) {
  const t = useTranslations('agents.spanDetail');

  return (
    <Section title={t('tokens')}>
      {looksWrong && (
        // Said out loud rather than hidden: a reader comparing token counts
        // across spans would otherwise trust a number nothing in the span
        // supports.
        <p className="rounded-md border border-amber-500/40 bg-amber-500/10 px-2 py-1.5 text-xs">
          {t('tokenMismatch')}
        </p>
      )}
      <TokenBreakdownList tokens={tokens} />
    </Section>
  );
}

function ToolsSection({ tools }: { tools: string[] }) {
  const t = useTranslations('agents.spanDetail');

  return (
    <Section title={t('availableTools')}>
      <div className="flex flex-wrap gap-1">
        {tools.map((tool) => (
          <Badge key={tool} variant="outline" className="font-mono">
            {tool}
          </Badge>
        ))}
      </div>
    </Section>
  );
}

/** What the model was given: its system instructions and the messages. */
function InputSection({
  input,
}: {
  input: NonNullable<ReturnType<typeof aiInput>>;
}) {
  const t = useTranslations('agents.spanDetail');

  return (
    <Section title={t('input')}>
      {input.systemInstructions && (
        <LabelledPayload
          label={t('systemInstructions')}
          value={input.systemInstructions}
        />
      )}
      <Payload value={input.messages} />
    </Section>
  );
}

/** A payload under its own small caption, inside a section. */
function LabelledPayload({ label, value }: { label: string; value: string }) {
  return (
    <div className="space-y-1">
      <p className="text-[10px] uppercase tracking-wide text-muted-foreground">
        {label}
      </p>
      <Payload value={value} />
    </div>
  );
}

/** A value worth putting on a row, or `null` when there is nothing to say. */
function asText(value: unknown): string | null {
  return typeof value === 'string' && value !== '' ? value : null;
}

/**
 * The token counts, with cached input shown only when there was any.
 *
 * `netNewInput` rather than raw input: a cache read is not a prompt the model
 * processed, and adding the two would report a number the bill does not match.
 */
function TokenBreakdownList({
  tokens,
}: {
  tokens: NonNullable<ReturnType<typeof tokenBreakdown>>;
}) {
  const t = useTranslations('agents.spanDetail');
  const format = useFormatter();

  return (
    <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1 text-xs">
      <dt className="text-muted-foreground">{t('inputTokens')}</dt>
      <dd className="font-mono tabular-nums">
        {format.number(tokens.netNewInput)}
      </dd>
      {tokens.cached > 0 && (
        <>
          <dt className="text-muted-foreground">{t('cachedTokens')}</dt>
          <dd className="font-mono tabular-nums">
            {format.number(tokens.cached)}
          </dd>
        </>
      )}
      <dt className="text-muted-foreground">{t('outputTokens')}</dt>
      <dd className="font-mono tabular-nums">{format.number(tokens.output)}</dd>
      <dt className="text-muted-foreground">{t('totalTokens')}</dt>
      <dd className="font-mono font-semibold tabular-nums">
        {format.number(tokens.total)}
      </dd>
    </dl>
  );
}

/** What the model answered: free text, a structured object, tool calls. */
function OutputSection({
  output,
}: {
  output: NonNullable<ReturnType<typeof aiOutput>>;
}) {
  const t = useTranslations('agents.spanDetail');

  return (
    <Section title={t('output')}>
      {output.text && <Payload value={output.text} />}
      {output.object && (
        <LabelledPayload label={t('responseObject')} value={output.object} />
      )}
      {output.toolCalls && (
        <LabelledPayload
          label={t('requestedToolCalls')}
          value={output.toolCalls}
        />
      )}
    </Section>
  );
}

/** Every attribute as sent, sorted, as the escape hatch for the rest. */
function AttributesTable({
  attributes,
}: {
  attributes: Record<string, unknown>;
}) {
  return (
    <dl className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)] gap-x-3 gap-y-1 text-[11px]">
      {Object.entries(attributes)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([key, value]) => (
          <div key={key} className="contents">
            <dt className="break-all font-mono text-muted-foreground">{key}</dt>
            <dd className="break-all font-mono">
              {typeof value === 'string' ? value : JSON.stringify(value)}
            </dd>
          </div>
        ))}
    </dl>
  );
}
