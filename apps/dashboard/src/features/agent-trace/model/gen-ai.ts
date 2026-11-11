/**
 * Reading the `gen_ai.*` attribute bag a span carries.
 *
 * Every function here takes the flat attribute object `spans.get()` returns and answers one question about it.
 *
 * Two conventions coexist in the wild and both are read wherever they differ.
 * Relay dropped the `.cached` / `.reasoning` attribute spellings (only
 * `gen_ai.usage.cache_read.input_tokens` and
 * `gen_ai.usage.reasoning.output_tokens` survive its normalization), but
 * Sentry's own frontend still falls back to the old names for data ingested
 * before that, and so does this.
 */

export type SpanAttributes = Record<string, unknown>;

export interface TokenBreakdown {
  /** Input tokens that were not served from the prompt cache. */
  netNewInput: number;
  cached: number;
  /** Output tokens, reasoning included. */
  output: number;
  total: number;
}

function num(attributes: SpanAttributes, ...keys: string[]): number {
  for (const key of keys) {
    const raw = attributes[key];
    if (typeof raw === 'number' && Number.isFinite(raw)) return raw;
    if (typeof raw === 'string' && raw.trim() !== '') {
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) return parsed;
    }
  }
  return 0;
}

function str(attributes: SpanAttributes, ...keys: string[]): string | null {
  for (const key of keys) {
    const raw = attributes[key];
    if (typeof raw === 'string' && raw !== '') return raw;
    if (raw != null && typeof raw === 'object') return JSON.stringify(raw);
  }
  return null;
}

export interface AiInput {
  /** The conversation, tool-call arguments, or embeddings input, verbatim. */
  messages: string;
  /** Sent alongside the conversation rather than inside it, when present. */
  systemInstructions: string | null;
}

/**
 * The input side of a span: the prompt for an LLM call, the arguments for a
 * tool call, or the embeddings input.
 *
 * The attribute chain is Sentry's, in its order — a span instrumented across
 * an SDK upgrade can carry several of these at once, and the newest wins.
 */
export function aiInput(attributes: SpanAttributes): AiInput | null {
  const messages = str(
    attributes,
    'gen_ai.input.messages',
    'gen_ai.request.messages',
    'ai.input_messages',
    'ai.prompt',
    'gen_ai.tool.call.arguments',
    'gen_ai.tool.input',
    'gen_ai.embeddings.input',
  );

  if (messages == null) return null;

  return {
    messages,
    systemInstructions: str(attributes, 'gen_ai.system_instructions'),
  };
}

export interface AiOutput {
  /** The response text or message array, verbatim. */
  text: string | null;
  /** A structured object the model was asked to produce, when it was. */
  object: string | null;
  /** Tool calls the model requested — often the whole point of the span. */
  toolCalls: string | null;
}

/**
 * The output side of a span: the model's response, the structured object it
 * produced, the tool calls it asked for, or a tool's own result.
 *
 * All three parts are independent — a planning step commonly returns tool
 * calls and no prose at all — so this reports each rather than picking one.
 */
export function aiOutput(attributes: SpanAttributes): AiOutput | null {
  const text = str(
    attributes,
    'gen_ai.output.messages',
    'gen_ai.response.text',
    'gen_ai.tool.call.result',
    'gen_ai.tool.output',
  );
  const object = str(attributes, 'gen_ai.response.object');
  const toolCalls = str(attributes, 'gen_ai.response.tool_calls');

  if (text == null && object == null && toolCalls == null) return null;

  return { text, object, toolCalls };
}

/**
 * Names of the tools an agent span was given to choose from.
 *
 * These arrive as an SDK-serialized JSON array; a truncated or malformed one
 * yields `null` rather than throwing, because it must not take the details
 * panel down with it.
 */
export function availableTools(attributes: SpanAttributes): string[] | null {
  const raw = str(
    attributes,
    'gen_ai.tool.definitions',
    'gen_ai.request.available_tools',
  );
  if (raw == null) return null;

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!Array.isArray(parsed)) return null;

  const names = parsed.flatMap((tool) =>
    tool != null &&
    typeof tool === 'object' &&
    'name' in tool &&
    typeof tool.name === 'string'
      ? [tool.name]
      : [],
  );

  return names.length > 0 ? names : null;
}

function cachedInputTokens(attributes: SpanAttributes): number {
  return num(
    attributes,
    'gen_ai.usage.cache_read.input_tokens',
    'gen_ai.usage.input_tokens.cached',
  );
}

/**
 * The convention says `input_tokens` includes cached and `output_tokens`
 * includes reasoning. Providers disagree in practice, so both of these pick
 * whichever reading brings `input + output` closest to the reported total.
 *
 * Getting this wrong is not cosmetic: subtracting cached from an input that
 * never contained it silently loses tokens the user was billed for.
 */
function adjustedInput(
  input: number,
  cached: number,
  output: number,
  total: number,
): number {
  if (cached <= 0) return input;
  const without = input + output;
  const withCached = without + cached;
  return Math.abs(withCached - total) < Math.abs(without - total)
    ? input + cached
    : input;
}

function adjustedOutput(
  input: number,
  output: number,
  reasoning: number,
  total: number,
): number {
  if (reasoning <= 0) return output;
  const without = input + output;
  const withReasoning = without + reasoning;
  return Math.abs(withReasoning - total) < Math.abs(without - total)
    ? output + reasoning
    : output;
}

/**
 * The raw counts, before any convention-reconciling.
 *
 * A provider may report the parts and no total. Relay derives one in that case
 * (`set_total_tokens`, relay-event-normalization/src/normalize/span/ai.rs) and
 * declines to when both parts are absent; this does the same, so a span the
 * server never normalized still reads the way Sentry would show it. Without
 * it the total falls to zero, the mismatch tolerance collapses to 1, and the
 * warning fires on correct instrumentation.
 */
function rawUsage(attributes: SpanAttributes) {
  const input = num(attributes, 'gen_ai.usage.input_tokens');
  const output = num(attributes, 'gen_ai.usage.output_tokens');
  const reported = num(attributes, 'gen_ai.usage.total_tokens');

  return {
    // A reported total wins even when it disagrees with the parts — that
    // disagreement is what the mismatch warning exists to surface.
    total: reported !== 0 ? reported : input + output,
    input,
    output,
    cached: cachedInputTokens(attributes),
    reasoning: num(
      attributes,
      'gen_ai.usage.reasoning.output_tokens',
      'gen_ai.usage.output_tokens.reasoning',
    ),
  };
}

function reportsUsage(usage: ReturnType<typeof rawUsage>): boolean {
  return usage.total !== 0 || usage.input !== 0 || usage.output !== 0;
}

/**
 * Splits a span's reported token usage into the parts worth showing.
 *
 * Returns `null` when the span reports no usage at all, so a caller can tell
 * "this is not an LLM call" from "this call used zero tokens".
 */
export function tokenBreakdown(
  attributes: SpanAttributes,
): TokenBreakdown | null {
  const usage = rawUsage(attributes);
  if (!reportsUsage(usage)) return null;

  const input = adjustedInput(
    usage.input,
    usage.cached,
    usage.output,
    usage.total,
  );
  const output = adjustedOutput(
    input,
    usage.output,
    usage.reasoning,
    usage.total,
  );

  return {
    netNewInput: usage.cached > 0 ? Math.max(0, input - usage.cached) : input,
    cached: usage.cached,
    output,
    total: usage.total,
  };
}

/**
 * Whether the reported counts look wrong: any negative value, or parts that
 * miss the reported total by more than a rounding tolerance.
 *
 * Worth surfacing rather than hiding — a reader comparing token counts across
 * spans would otherwise trust a number nothing in the span supports. A span
 * reporting no usage at all is not broken, just not an LLM call.
 */
export function hasTokenMismatch(attributes: SpanAttributes): boolean {
  const usage = rawUsage(attributes);
  if (!reportsUsage(usage)) return false;

  if (
    usage.input < 0 ||
    usage.output < 0 ||
    usage.total < 0 ||
    usage.cached < 0 ||
    usage.reasoning < 0
  ) {
    return true;
  }

  const input = adjustedInput(
    usage.input,
    usage.cached,
    usage.output,
    usage.total,
  );
  const output = adjustedOutput(
    input,
    usage.output,
    usage.reasoning,
    usage.total,
  );

  // Both the raw sum and the sum of what is actually displayed: netNewInput
  // is clamped at zero when cached exceeds input, so the two can differ even
  // when the raw numbers reconcile.
  const netNewInput =
    usage.cached > 0 ? Math.max(0, input - usage.cached) : input;
  const tolerance = Math.max(1, usage.total * 0.01);

  return (
    Math.abs(input + output - usage.total) > tolerance ||
    Math.abs(netNewInput + usage.cached + output - usage.total) > tolerance
  );
}

export interface ChatMessage {
  role: string;
  content: string;
}

/**
 * A conversation as messages, for drawing as a chat rather than a JSON
 * dump. Reads the two shapes the SDKs send -- OpenAI's `content` and OTel's
 * `parts` -- joining text parts and keeping any other part as JSON. `null`
 * when the payload is not a list of messages: tool arguments, plain text.
 */
export function chatMessages(raw: string): ChatMessage[] | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!Array.isArray(parsed) || parsed.length === 0) return null;

  const messages: ChatMessage[] = [];
  for (const item of parsed) {
    if (item === null || typeof item !== 'object' || !('role' in item)) {
      return null;
    }
    const { role, content, parts, tool_calls } = item as {
      role: unknown;
      content?: unknown;
      parts?: unknown;
      tool_calls?: unknown;
    };
    const body = Array.isArray(parts) ? parts : content;
    // The tools a model asked for are part of what it said, after any text:
    // a message may hold only them, or prose and them.
    const calls =
      Array.isArray(tool_calls) && tool_calls.length
        ? JSON.stringify(tool_calls)
        : '';
    const text = [messageText(body), calls].filter(Boolean).join('\n');
    messages.push({ role: String(role), content: text });
  }
  return messages;
}

function messageText(body: unknown): string {
  if (typeof body === 'string') return body;
  if (Array.isArray(body)) {
    return body
      .map((part) =>
        part &&
        typeof part === 'object' &&
        'text' in part &&
        typeof part.text === 'string'
          ? part.text
          : JSON.stringify(part),
      )
      .join('\n');
  }
  return body == null ? '' : JSON.stringify(body);
}
