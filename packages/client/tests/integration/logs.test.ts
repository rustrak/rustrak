import { describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/index.js';
import { expectOk } from '../helpers/result.js';

const client = new RustrakClient({
  baseUrl: 'http://localhost:8080',
  token: 'test-token',
});

describe('LogsResource', () => {
  describe('list()', () => {
    it('returns paginated log list', async () => {
      const result = expectOk(await client.logs.list(1));

      expect(result.total_count).toBe(2);
      expect(result.items).toHaveLength(2);
      expect(result.total_pages).toBe(1);
    });

    it('returns log with correct shape', async () => {
      const result = expectOk(await client.logs.list(1));
      const log = result.items[0];

      expect(log.id).toBe('a1b2c3d4-e89b-12d3-a456-426614174000');
      expect(log.level).toBe('info');
      expect(log.body).toBe('ok');
      expect(log.environment).toBeNull();
      expect(log.trace_id).toBe('bbbb');
      expect(log.severity_number).toBe(9);
      expect(log.timestamp).toBe('2026-06-18T12:00:01.000Z');
    });

    it('accepts pagination options', async () => {
      const result = expectOk(
        await client.logs.list(1, { page: 1, per_page: 10 }),
      );
      expect(result).toBeDefined();
    });

    it('accepts filter options', async () => {
      const result = expectOk(
        await client.logs.list(1, {
          level: 'error',
          trace_id: 'aaaa',
        }),
      );
      expect(result).toBeDefined();
    });
  });
});
