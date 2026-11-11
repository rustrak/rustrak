import type { SpanResponse } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  resolveSelectedSpan,
  summarizeTrace,
  waterfallSpans,
} from './trace-summary';

function span(over: Partial<SpanResponse> & { id: string }): SpanResponse {
  return {
    gen_ai_operation_type: null,
    gen_ai_agent_name: null,
    gen_ai_request_model: null,
    gen_ai_response_model: null,
    gen_ai_usage_total_tokens: null,
    start_timestamp: null,
    timestamp: null,
    status: null,
    ...over,
  } as unknown as SpanResponse;
}

describe('summarizeTrace', () => {
  it('spans the whole trace, earliest start to latest end', () => {
    const summary = summarizeTrace([
      span({
        id: 'a',
        start_timestamp: '2026-01-01T00:00:00.000Z',
        timestamp: '2026-01-01T00:00:01.000Z',
      }),
      span({
        id: 'b',
        start_timestamp: '2026-01-01T00:00:00.500Z',
        timestamp: '2026-01-01T00:00:03.000Z',
      }),
    ]);

    expect(summary.duration).toBe(3000);
    expect(summary.startedAt).toBe(Date.parse('2026-01-01T00:00:00.000Z'));
  });

  it('has no duration when nothing carries a timestamp', () => {
    const summary = summarizeTrace([span({ id: 'a' })]);

    expect(summary.duration).toBeNull();
    expect(summary.startedAt).toBeNull();
  });

  /**
   * The one rule here that is not arithmetic. An agent span reports the total
   * for everything under it, so counting it alongside its children bills the
   * same tokens twice. The Traces list query excludes them for the same reason.
   */
  it('excludes agent spans from the token total', () => {
    const summary = summarizeTrace([
      span({
        id: 'agent',
        gen_ai_operation_type: 'agent',
        gen_ai_usage_total_tokens: 300,
      }),
      span({
        id: 'llm',
        gen_ai_operation_type: 'ai_client',
        gen_ai_usage_total_tokens: 200,
      }),
      span({
        id: 'tool',
        gen_ai_operation_type: 'tool',
        gen_ai_usage_total_tokens: 100,
      }),
    ]);

    expect(summary.totalTokens).toBe(300);
  });

  it('counts calls by operation type', () => {
    const summary = summarizeTrace([
      span({ id: '1', gen_ai_operation_type: 'ai_client' }),
      span({ id: '2', gen_ai_operation_type: 'ai_client' }),
      span({ id: '3', gen_ai_operation_type: 'tool' }),
    ]);

    expect(summary.llmCallCount).toBe(2);
    expect(summary.toolCallCount).toBe(1);
  });

  it('counts any status that is not ok as an error, but not a missing one', () => {
    const summary = summarizeTrace([
      span({ id: '1', status: 'ok' }),
      span({ id: '2', status: 'internal_error' }),
      span({ id: '3', status: null }),
    ]);

    expect(summary.errorCount).toBe(1);
  });

  it('takes the agent name from whichever span reports one', () => {
    const summary = summarizeTrace([
      span({ id: '1' }),
      span({ id: '2', gen_ai_agent_name: 'researcher' }),
    ]);

    expect(summary.agentName).toBe('researcher');
  });

  it('names the requested model when a failed call reported no response one', () => {
    // A call that threw has no response model but still names what it tried.
    const summary = summarizeTrace([
      span({ id: '1', gen_ai_request_model: 'claude-opus-4' }),
    ]);

    expect(summary.models).toEqual(['claude-opus-4']);
  });

  it('prefers the response model and lists each one once', () => {
    const summary = summarizeTrace([
      span({
        id: '1',
        gen_ai_request_model: 'asked-for',
        gen_ai_response_model: 'answered-with',
      }),
      span({ id: '2', gen_ai_response_model: 'answered-with' }),
    ]);

    expect(summary.models).toEqual(['answered-with']);
  });
});

describe('resolveSelectedSpan', () => {
  const spans = [
    span({ id: 'a', gen_ai_operation_type: 'ai_client' }),
    span({ id: 'b' }),
  ];

  it('honours a requested span that belongs to this trace', () => {
    expect(resolveSelectedSpan(spans, 'b')).toEqual({
      selectedSpanId: 'b',
      requestedMissing: false,
    });
  });

  /**
   * `getSpan` is scoped to the project, not the trace, so without this a span
   * from another trace would render beside this one's waterfall: the URL and
   * the panel describing different things.
   */
  it('refuses a span from another trace and says it is missing', () => {
    expect(resolveSelectedSpan(spans, 'elsewhere')).toEqual({
      selectedSpanId: undefined,
      requestedMissing: true,
    });
  });

  it('opens the default span when none was requested', () => {
    expect(resolveSelectedSpan(spans, undefined)).toEqual({
      selectedSpanId: 'a',
      requestedMissing: false,
    });
  });

  it('selects nothing for an empty trace', () => {
    expect(resolveSelectedSpan([], undefined)).toEqual({
      selectedSpanId: undefined,
      requestedMissing: false,
    });
  });
});

describe('waterfallSpans', () => {
  it('links children by span id and names each span by what it ran', () => {
    const rows = waterfallSpans([
      span({
        id: 'row-agent',
        span_id: 'a1',
        op: 'gen_ai.invoke_agent',
        gen_ai_operation_type: 'agent',
        gen_ai_agent_name: 'Support Agent',
        description: 'invoke_agent Support Agent',
        start_timestamp: '2026-01-01T00:00:00.000Z',
        timestamp: '2026-01-01T00:00:02.000Z',
      }),
      span({
        id: 'row-llm',
        span_id: 'c1',
        parent_span_id: 'a1',
        op: 'gen_ai.chat',
        gen_ai_operation_type: 'ai_client',
        gen_ai_request_model: 'gpt-4o',
        start_timestamp: '2026-01-01T00:00:00.500Z',
        timestamp: '2026-01-01T00:00:01.500Z',
        status: 'internal_error',
      }),
      span({ id: 'row-untimed', span_id: 'x' }),
    ]);

    expect(rows).toEqual([
      {
        id: 'row-agent',
        parentId: null,
        op: 'gen_ai.invoke_agent',
        description: 'Support Agent',
        startMs: Date.parse('2026-01-01T00:00:00.000Z'),
        endMs: Date.parse('2026-01-01T00:00:02.000Z'),
        status: undefined,
        // What the server classified it as, which the colour follows.
        operationType: 'agent',
        unfinished: false,
        selfMs: 1000,
      },
      {
        id: 'row-llm',
        parentId: 'row-agent',
        op: 'gen_ai.chat',
        description: 'gpt-4o',
        startMs: Date.parse('2026-01-01T00:00:00.500Z'),
        endMs: Date.parse('2026-01-01T00:00:01.500Z'),
        status: 'internal_error',
        operationType: 'ai_client',
        unfinished: false,
        selfMs: 1000,
      },
      // No time at all: kept, unfinished, at the trace's start, so it can
      // still be opened.
      {
        id: 'row-untimed',
        parentId: null,
        op: undefined,
        description: undefined,
        startMs: Date.parse('2026-01-01T00:00:00.000Z'),
        endMs: Date.parse('2026-01-01T00:00:00.000Z'),
        status: undefined,
        operationType: undefined,
        unfinished: true,
        selfMs: 0,
      },
    ]);
  });

  it('places a span with only its end at that end, under its parent', () => {
    const rows = waterfallSpans([
      span({
        id: 'row-agent',
        span_id: 'a1',
        start_timestamp: '2026-01-01T00:00:00.000Z',
        timestamp: '2026-01-01T00:00:02.000Z',
      }),
      span({
        id: 'row-half',
        span_id: 'h',
        parent_span_id: 'a1',
        timestamp: '2026-01-01T00:00:01.000Z',
      }),
    ]);
    expect(rows[1]).toMatchObject({
      parentId: 'row-agent',
      startMs: Date.parse('2026-01-01T00:00:01.000Z'),
      endMs: Date.parse('2026-01-01T00:00:01.000Z'),
      unfinished: true,
    });
  });
});
