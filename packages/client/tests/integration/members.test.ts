import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

describe('MembersResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('list()', () => {
    it('should list project members', async () => {
      const members = expectOk(await client.members.list(1));

      expect(members).toHaveLength(2);
      expect(members[0]?.user_id).toBe(1);
      expect(members[0]?.email).toBe('test@example.com');
      expect(members[0]?.role).toBe('editor');
      expect(members[1]?.role).toBe('admin');
    });

    it('should surface 404 for unknown project', async () => {
      const result = await client.members.list(999);

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('not_found');
      expect(error.message).toBe('Resource not found: Project 999 not found');
    });

    it('should reject malformed response', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/1/members', () => {
          return HttpResponse.json([{ user_id: 'x' }]);
        }),
      );

      const result = await client.members.list(1);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });
  });

  describe('upsert()', () => {
    it('should add or update a member', async () => {
      const result = await client.members.upsert(1, {
        user_id: 1,
        role: 'editor',
      });

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should validate role client-side', async () => {
      const result = await client.members.upsert(1, {
        user_id: 1,
        // @ts-expect-error - testing runtime validation
        role: 'owner',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should surface 404 for unknown project', async () => {
      const result = await client.members.upsert(999, {
        user_id: 1,
        role: 'viewer',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should surface 409 when downgrading the last project admin', async () => {
      const result = await client.members.upsert(1, {
        user_id: 2,
        role: 'viewer',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error).toMatchObject({ status: 409 });
      expect(error.kind).toBe('conflict');
    });
  });

  describe('remove()', () => {
    it('should remove a member', async () => {
      const result = await client.members.remove(1, 1);

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should surface 404 for unknown membership', async () => {
      const result = await client.members.remove(1, 999);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should surface 409 when removing the last project admin', async () => {
      const result = await client.members.remove(1, 2);

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error).toMatchObject({ status: 409 });
      expect(error.kind).toBe('conflict');
    });
  });
});
