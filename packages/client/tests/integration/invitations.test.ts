import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

describe('InvitationsResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('create()', () => {
    it('should create an invitation', async () => {
      const invitation = expectOk(
        await client.invitations.create({
          email: 'invitee@example.com',
          role: 'member',
        }),
      );

      expect(invitation.token).toBe('new-invite-token');
      expect(invitation.email).toBe('invitee@example.com');
      expect(invitation.role).toBe('member');
      expect(invitation.status).toBe('pending');
    });

    it('should validate email client-side', async () => {
      const result = await client.invitations.create({
        email: 'not-an-email',
        role: 'member',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should validate role client-side', async () => {
      const result = await client.invitations.create({
        email: 'a@b.com',
        // @ts-expect-error - testing runtime validation
        role: 'owner',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should surface 409 when email is already used', async () => {
      const result = await client.invitations.create({
        email: 'existing@example.com',
        role: 'member',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error).toMatchObject({ status: 409 });
      expect(error.kind).toBe('conflict');
    });
  });

  describe('list()', () => {
    it('should list invitations', async () => {
      const invitations = expectOk(await client.invitations.list());

      expect(invitations).toHaveLength(1);
      expect(invitations[0]?.email).toBe('invitee@example.com');
      expect(invitations[0]?.role).toBe('member');
    });

    it('should reject malformed response', async () => {
      server.use(
        http.get('http://localhost:8080/api/invitations', () => {
          return HttpResponse.json([{ token: 123 }]);
        }),
      );

      const result = await client.invitations.list();

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });
  });

  describe('revoke()', () => {
    it('should revoke an invitation', async () => {
      const result = await client.invitations.revoke('invite-token-abc123');

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should surface 404 for unknown invitation', async () => {
      const result = await client.invitations.revoke('unknown-token');

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('not_found');
      expect(error.message).toBe(
        'Resource not found: Pending invitation not found',
      );
    });

    // Successor to "should throw a RustrakError subclass on failure": there is
    // no class hierarchy left, so what matters is that the failure is a plain
    // serializable object carrying a `kind` from the closed union.
    it('should hand back a plain object from the union on failure', async () => {
      const error = expectErr(await client.invitations.revoke('unknown-token'));

      expect(error).not.toBeInstanceOf(Error);
      expect(Object.getPrototypeOf(error)).toBe(Object.prototype);
      expect(typeof error.kind).toBe('string');
    });
  });
});
