import { describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/index.js';
import { spanSchema } from '../../src/schemas/index.js';
import { expectOk } from '../helpers/result.js';

const client = new RustrakClient({
  baseUrl: 'http://localhost:8080',
  token: 'test-token',
});

describe('SpansResource', () => {
  describe('list()', () => {
    it('returns paginated span list', async () => {
      const result = expectOk(await client.spans.list(1));

      expect(result.total_count).toBe(1);
      expect(result.items).toHaveLength(1);
    });

    it('returns a span with the shared standalone/transaction shape', async () => {
      const result = expectOk(await client.spans.list(1));
      const span = result.items[0];

      expect(span.op).toBe('gen_ai.invoke_agent');
      expect(span.transaction_id).toBeNull();
      expect(span.is_segment).toBe(true);
      expect(span.duration_ms).toBe(1000.0);
    });

    it('accepts operation_type filter', async () => {
      const result = expectOk(
        await client.spans.list(1, { operation_type: 'tool' }),
      );
      expect(result.items).toHaveLength(0);
    });

    it('accepts pagination and other filters', async () => {
      const result = expectOk(
        await client.spans.list(1, {
          page: 1,
          per_page: 10,
          op: 'gen_ai.invoke_agent',
          status: 'ok',
          trace_id: 'abc',
        }),
      );
      expect(result).toBeDefined();
    });

    it('omits attributes — the list stays light', () => {
      // `spans.data` is never trimmed server-side, so a trace's worth of
      // prompts would dwarf the waterfall the list draws. Attributes come
      // from get() instead, one span at a time.
      expect(spanSchema.shape).not.toHaveProperty('attributes');
    });
  });

  describe('get()', () => {
    it('returns the span with its raw gen_ai attribute bag', async () => {
      const span = expectOk(
        await client.spans.get(1, 'd4e5f6a7-e89b-12d3-a456-426614174000'),
      );

      expect(span.op).toBe('gen_ai.chat');
      expect(span.attributes['gen_ai.request.messages']).toBe(
        '[{"role":"user","content":"what is the weather"}]',
      );
      expect(span.attributes['gen_ai.response.text']).toBe('it is sunny');
    });

    it('carries tags when the producer stored them', async () => {
      const span = expectOk(
        await client.spans.get(1, 'd4e5f6a7-e89b-12d3-a456-426614174000'),
      );

      expect(span.tags).toEqual({ environment: 'production' });
    });

    it('returns a not-found error for an unknown span', async () => {
      const result = await client.spans.get(
        1,
        'ffffffff-0000-0000-0000-000000000000',
      );

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error.kind).toBe('not_found');
      }
    });
  });
});
