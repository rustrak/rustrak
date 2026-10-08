import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

describe('TeamResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('list()', () => {
    it('should list team members', async () => {
      const members = expectOk(await client.team.list());

      expect(members).toHaveLength(3);
      expect(members[0]?.email).toBe('test@example.com');
      expect(members[0]?.role).toBe('member');
      expect(members[0]?.is_active).toBe(true);
      expect(members[1]?.role).toBe('admin');
      expect(members[1]?.last_login).toBeNull();
    });

    it('should reject malformed response', async () => {
      server.use(
        http.get('http://localhost:8080/api/team', () => {
          return HttpResponse.json([{ id: 'not-a-number' }]);
        }),
      );

      const result = await client.team.list();

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });
  });

  describe('updateRole()', () => {
    it('should update a user role', async () => {
      const result = await client.team.updateRole(1, 'admin');

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should reject an invalid role client-side', async () => {
      const result = await client.team.updateRole(
        1,
        // @ts-expect-error - testing runtime validation
        'superuser',
      );

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should surface 404 for unknown user', async () => {
      const result = await client.team.updateRole(999, 'member');

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('not_found');
      expect(error).toHaveProperty('status', 404);
    });

    // User 3 is the non-primary admin. `routes/team.rs:118-137` checks the
    // primary guard first, so user 2 (primary) can never reach the 409.
    it('should surface 409 when demoting the last admin', async () => {
      const result = await client.team.updateRole(3, 'member');

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error).toMatchObject({ status: 409 });
      // 409 used to fall through to a bare `RustrakError` with only a
      // `statusCode`. The union gives it a name of its own.
      expect(error.kind).toBe('conflict');
      expect(error.message).toBe('Conflict: Cannot demote the last admin');
    });

    // Successor to "should throw a RustrakError on 409". There is no base
    // class to be an instance of any more, and that is the point: what the
    // 409 hands back must be a plain object, or it cannot cross the RSC
    // boundary.
    it('should hand back a plain object on 409, not a class instance', async () => {
      const error = expectErr(await client.team.updateRole(3, 'member'));

      expect(error).not.toBeInstanceOf(Error);
      expect(Object.getPrototypeOf(error)).toBe(Object.prototype);
      expect(structuredClone(error)).toEqual(error);
    });

    it('should surface 403, not 409, when changing the primary admin role', async () => {
      const result = await client.team.updateRole(2, 'member');

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error).toMatchObject({ status: 403 });
      expect(error.kind).toBe('forbidden');
    });
  });

  describe('list() is_primary', () => {
    it('should expose is_primary on the roster', async () => {
      const members = expectOk(await client.team.list());
      expect(members[1]?.is_primary).toBe(true);
      expect(members[0]?.is_primary).toBe(false);
      expect(members[2]?.is_primary).toBe(false);
    });
  });

  describe('remove()', () => {
    it('should delete a user', async () => {
      const result = await client.team.remove(1);

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should surface 404 for unknown user', async () => {
      const result = await client.team.remove(999);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should surface 403 when deleting the primary user', async () => {
      const result = await client.team.remove(2);

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error).toMatchObject({ status: 403 });
      expect(error.kind).toBe('forbidden');
    });
  });
});
