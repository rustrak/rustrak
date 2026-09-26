import { HttpResponse, http } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { SERVER_ERROR_MESSAGE } from '../../src/errors.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

describe('AuthResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      // No token needed for session-based auth
    });
  });

  describe('OpenID Connect SSO', () => {
    it('reads the public provider configuration', async () => {
      const result = await client.auth.getSsoConfig();
      expect(expectOk(result)).toEqual({
        enabled: true,
        provider_name: 'Pocket ID',
      });
    });

    it('returns an error when the provider configuration request fails', async () => {
      server.use(
        http.get('http://localhost:8080/auth/sso/config', () =>
          HttpResponse.json(
            {
              error: {
                type: 'Internal',
                message: 'Internal server error',
              },
            },
            { status: 500 },
          ),
        ),
      );

      const error = expectErr(await client.auth.getSsoConfig());
      expect(error.kind).toBe('server_error');
      expect(error.message).toBe(SERVER_ERROR_MESSAGE);
    });

    it('starts SSO and returns the state cookie', async () => {
      const result = expectOk(await client.auth.startSso());
      expect(result.authorizationUrl).toContain('id.example.com/authorize');
      expect(result.cookies[0]).toContain('oidc-state');
    });

    it('returns an error when starting SSO fails', async () => {
      server.use(
        http.post('http://localhost:8080/auth/sso/start', () =>
          HttpResponse.json(
            {
              error: {
                type: 'NotFound',
                message: 'Resource not found: SSO is not configured',
              },
            },
            { status: 404 },
          ),
        ),
      );

      const error = expectErr(await client.auth.startSso());
      expect(error.kind).toBe('not_found');
      expect(error.message).toBe('Resource not found: SSO is not configured');
    });

    it('completes SSO and returns the authenticated session', async () => {
      const result = expectOk(
        await client.auth.completeSso({
          code: 'authorization-code',
          state: 'test',
        }),
      );
      expect(result.user.email).toBe('test@example.com');
      expect(result.cookies[0]).toContain('authenticated');
    });

    it('returns an authentication failure for an incomplete callback', async () => {
      const result = await client.auth.completeSso({ state: 'test' });
      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('unauthenticated');
    });

    it('returns an authentication failure when callback state is missing', async () => {
      const result = await client.auth.completeSso({
        code: 'authorization-code',
      });
      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('unauthenticated');
    });
  });

  // `POST /auth/register` is invite-only: `routes/auth.rs:106-115` returns
  // `AppError::Forbidden("Registration is invite-only")` for every input,
  // regardless of body. There is no success path and no server-side validation
  // branch, so the only things worth testing are the client-side schema (which
  // never reaches the network) and that single 403.
  describe('register()', () => {
    it('should always be rejected with 403 Forbidden', async () => {
      const result = await client.auth.register({
        email: 'newuser@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('forbidden');
      expect(error.message).toBe('Forbidden: Registration is invite-only');
      expect(error).toHaveProperty('status', 403);
    });

    it('should be rejected the same way for an already-registered email', async () => {
      const result = await client.auth.register({
        email: 'existing@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('forbidden');
    });

    it('should validate email format client-side, before any request', async () => {
      const result = await client.auth.register({
        email: 'not-an-email',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      // `invalid_request`: the caller's input, rejected before the network.
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should reject an empty password client-side (required, no length policy)', async () => {
      const result = await client.auth.register({
        email: 'test@example.com',
        password: '',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should validate email format strictly, client-side', async () => {
      const invalidEmails = [
        'no-at-sign',
        '@no-local-part.com',
        'no-domain@',
        'spaces in@email.com',
        'double@@at.com',
      ];

      for (const email of invalidEmails) {
        const result = await client.auth.register({
          email,
          password: 'password123',
        });

        expect(result.success, `${email} should not validate`).toBe(false);
        expect(expectErr(result).kind).toBe('invalid_request');
      }
    });
  });

  describe('login()', () => {
    it('should login with valid credentials', async () => {
      const result = expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );

      expect(result.user.id).toBe(1);
      expect(result.user.email).toBe('test@example.com');
      expect(result.user.is_admin).toBe(false);
      expect(result.cookies).toBeDefined();
    });

    it('should login admin user', async () => {
      const result = expectOk(
        await client.auth.login({
          email: 'admin@example.com',
          password: 'adminpass123',
        }),
      );

      expect(result.user.id).toBe(2);
      expect(result.user.email).toBe('admin@example.com');
      expect(result.user.is_admin).toBe(true);
    });

    it('should reject invalid credentials', async () => {
      const result = await client.auth.login({
        email: 'test@example.com',
        password: 'wrongpassword',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('unauthenticated');
      expect(error.message).toBe('Unauthorized: Invalid credentials');
    });

    it('should reject non-existent user', async () => {
      const result = await client.auth.login({
        email: 'nonexistent@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('unauthenticated');
    });

    it('should reject inactive user account', async () => {
      const result = await client.auth.login({
        email: 'inactive@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('unauthenticated');
      expect(error.message).toBe('Unauthorized: Account is disabled');
    });

    it('should be case-sensitive for email', async () => {
      // Assuming email is case-sensitive
      const result = await client.auth.login({
        email: 'TEST@EXAMPLE.COM',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('unauthenticated');
    });

    it('should validate input before sending request', async () => {
      // Invalid email should fail validation
      const result = await client.auth.login({
        email: 'not-an-email',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should handle empty password', async () => {
      const result = await client.auth.login({
        email: 'test@example.com',
        password: '',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });
  });

  describe('logout()', () => {
    it('should logout successfully', async () => {
      const cookies = expectOk(await client.auth.logout());
      expect(Array.isArray(cookies)).toBe(true);
    });

    it('should return cookies array on successful logout', async () => {
      const cookies = expectOk(await client.auth.logout());
      expect(Array.isArray(cookies)).toBe(true);
    });

    it('should work even without active session', async () => {
      // Logout should succeed even if not logged in
      const cookies = expectOk(await client.auth.logout());
      expect(Array.isArray(cookies)).toBe(true);
    });
  });

  describe('updatePreferences()', () => {
    it('should send the chosen language and timezone and return the updated user', async () => {
      expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );

      const user = expectOk(
        await client.auth.updatePreferences({
          language: 'zh',
          timezone: 'Asia/Tokyo',
        }),
      );

      expect(user.language).toBe('zh');
      expect(user.timezone).toBe('Asia/Tokyo');
    });

    it('should clear a preference when it is sent as null', async () => {
      expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );

      const user = expectOk(
        await client.auth.updatePreferences({ language: null }),
      );

      expect(user.language).toBeNull();
    });
  });

  describe('getCurrentUser()', () => {
    it('should get current authenticated user', async () => {
      // First login to set session
      expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );

      // Then get current user
      const user = expectOk(await client.auth.getCurrentUser());

      expect(user.id).toBe(1);
      expect(user.email).toBe('test@example.com');
      expect(user.is_admin).toBe(false);
    });

    // NOTE: This test is skipped because MSW (Mock Service Worker) in Node.js doesn't
    // properly simulate cookie handling. The credentials: 'include' option works in
    // browsers but not in Node.js test environments. Testing cookie-based auth requires
    // either:
    // 1. E2E tests with a real browser (Playwright/Cypress)
    // 2. Integration tests against a real server
    // The auth flow itself is verified via the login/register tests.
    it.skip('should report unauthenticated for a request with no session', async () => {
      // Create new client without session cookie
      const unauthClient = new RustrakClient({
        baseUrl: 'http://localhost:8080',
      });

      const result = await unauthClient.auth.getCurrentUser();

      // No session is a failed Result with `unauthenticated`, deliberately not
      // a successful Result carrying `null`. Consumers redirect to login on
      // this kind and on this kind only.
      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('unauthenticated');
    });

    it('should validate response schema', async () => {
      // Login first
      expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );

      const user = expectOk(await client.auth.getCurrentUser());

      // Validate structure
      expect(user).toHaveProperty('id');
      expect(user).toHaveProperty('email');
      expect(user).toHaveProperty('is_admin');
      expect(typeof user.id).toBe('number');
      expect(typeof user.email).toBe('string');
      expect(typeof user.is_admin).toBe('boolean');
    });
  });

  describe('getInvitation()', () => {
    it('should fetch invitation details by token', async () => {
      const info = expectOk(
        await client.auth.getInvitation('invite-token-abc123'),
      );

      expect(info.email).toBe('invitee@example.com');
      expect(info.role).toBe('member');
      expect(info.status).toBe('pending');
      expect(info.expires_at).toBeDefined();
    });

    it('should reject an unknown token (404)', async () => {
      const result = await client.auth.getInvitation('does-not-exist');

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('not_found');
      expect(error.message).toBe('Resource not found: Invitation not found');
    });

    it('should reject an expired/used invitation (400)', async () => {
      const result = await client.auth.getInvitation('expired-token');

      expect(result.success).toBe(false);
      const error = expectErr(result);
      // The server's 400 is `AppError::Validation`, which is `validation` here,
      // not `invalid_request`: it came back from the network.
      expect(error.kind).toBe('validation');
      expect(error.message).toBe(
        'Validation error: Invitation is expired or already used',
      );
    });

    it('should reject malformed response', async () => {
      server.use(
        http.get('http://localhost:8080/auth/invitation/:token', () => {
          return HttpResponse.json({ email: 'x' });
        }),
      );

      const result = await client.auth.getInvitation('invite-token-abc123');

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });
  });

  describe('acceptInvitation()', () => {
    it('should accept an invitation and return user + cookies', async () => {
      const result = expectOk(
        await client.auth.acceptInvitation({
          token: 'invite-token-abc123',
          password: 'password123',
        }),
      );

      expect(result.user.email).toBe('invitee@example.com');
      expect(result.user.role).toBe('member');
      expect(result.cookies).toBeDefined();
      expect(Array.isArray(result.cookies)).toBe(true);
    });

    it('should reject an empty password client-side (required)', async () => {
      const result = await client.auth.acceptInvitation({
        token: 'invite-token-abc123',
        password: '',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should validate token is non-empty client-side', async () => {
      const result = await client.auth.acceptInvitation({
        token: '',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should reject an invalid invitation (400)', async () => {
      const result = await client.auth.acceptInvitation({
        token: 'invalid-token',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('validation');
      expect(error.message).toBe('Validation error: Invalid invitation token');
    });
  });

  describe('Error Handling', () => {
    it('should handle malformed response from register', async () => {
      server.use(
        http.post('http://localhost:8080/auth/register', () => {
          return HttpResponse.json({ invalid: 'response' });
        }),
      );

      const result = await client.auth.register({
        email: 'test@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    it('should handle malformed response from login', async () => {
      server.use(
        http.post('http://localhost:8080/auth/login', () => {
          return HttpResponse.json({ invalid: 'response' });
        }),
      );

      const result = await client.auth.login({
        email: 'test@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    // The other failure `readLoginResult` has to report: a 2xx whose body is
    // not JSON at all, rather than JSON of the wrong shape. It reaches the
    // `readJson` arm, which the two tests above do not.
    it('should report invalid_response when the login body is not JSON', async () => {
      server.use(
        http.post('http://localhost:8080/auth/login', () => {
          return new HttpResponse('<html>login page</html>', {
            headers: { 'Content-Type': 'application/json' },
          });
        }),
      );

      const result = await client.auth.login({
        email: 'test@example.com',
        password: 'password123',
      });

      expect(expectErr(result).kind).toBe('invalid_response');
    });

    it('should handle network errors gracefully', async () => {
      server.use(
        http.post('http://localhost:8080/auth/login', () => {
          return HttpResponse.error();
        }),
      );

      const result = await client.auth.login({
        email: 'test@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('network');
      // No `cause`: it embeds the resolved host and port.
      expect(error).not.toHaveProperty('cause');
    });

    it('should handle server errors (500)', async () => {
      server.use(
        http.post('http://localhost:8080/auth/register', () => {
          return HttpResponse.json(
            { error: 'connection string leaked here' },
            { status: 500 },
          );
        }),
      );

      const result = await client.auth.register({
        email: 'test@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('server_error');
      // Redacted at construction: nothing from the server body survives.
      expect(error.message).toBe(SERVER_ERROR_MESSAGE);
      expect(error.message).not.toContain('connection string');
    });
  });

  describe('Session Cookie Handling', () => {
    // `acceptInvitation`, not `register`, is the endpoint that creates an
    // account and sets a session cookie (`routes/auth.rs:297-307`).
    it('should return cookies from acceptInvitation', async () => {
      const result = expectOk(
        await client.auth.acceptInvitation({
          token: 'invite-token-abc123',
          password: 'password123',
        }),
      );

      expect(result.cookies).toBeDefined();
      expect(Array.isArray(result.cookies)).toBe(true);
    });

    it('should return cookies from login', async () => {
      const result = expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );

      // Login should return cookies
      expect(result.cookies).toBeDefined();
      expect(Array.isArray(result.cookies)).toBe(true);
    });
  });

  describe('Edge Cases', () => {
    it('should handle concurrent login requests', async () => {
      const promises = [
        client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
        client.auth.login({
          email: 'admin@example.com',
          password: 'adminpass123',
        }),
      ];

      const results = await Promise.all(promises);
      expect(results).toHaveLength(2);
      expect(expectOk(results[0]!).user.email).toBe('test@example.com');
      expect(expectOk(results[1]!).user.email).toBe('admin@example.com');
    });

    it('should handle rapid accept-invitation/logout/login sequence', async () => {
      // Accept an invitation (the only account-creating endpoint)
      const registered = expectOk(
        await client.auth.acceptInvitation({
          token: 'invite-token-abc123',
          password: 'password123',
        }),
      );
      expect(registered.user.email).toBe('invitee@example.com');

      // Logout
      expectOk(await client.auth.logout());

      // Login again with same credentials would work in real scenario
      // (mocked here as different email since we don't persist state)
      const loggedIn = expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );
      expect(loggedIn.user.email).toBe('test@example.com');
    });

    it('should handle unicode characters in email', async () => {
      // Zod's email() validator doesn't accept unicode characters by default
      // This is a known limitation - unicode in local part is technically valid
      // but not widely supported. Test that it's rejected.
      const result = await client.auth.register({
        email: 'tëst@example.com',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should handle extremely long passwords gracefully', async () => {
      const veryLongPassword = 'a'.repeat(10000);

      // Should not crash: there is no length policy server-side.
      const result = expectOk(
        await client.auth.acceptInvitation({
          token: 'invite-token-abc123',
          password: veryLongPassword,
        }),
      );

      expect(result.user.email).toBe('invitee@example.com');
    });

    it('should handle whitespace in credentials', async () => {
      // Email with leading/trailing whitespace is invalid email format
      // Client-side validation should catch this
      const result = await client.auth.login({
        email: ' test@example.com ',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });
  });

  describe('TypeScript Type Safety', () => {
    it('should return properly typed LoginResult from acceptInvitation', async () => {
      const result = expectOk(
        await client.auth.acceptInvitation({
          token: 'invite-token-abc123',
          password: 'password123',
        }),
      );

      // TypeScript should infer these properties
      const _id: number = result.user.id;
      const _email: string = result.user.email;
      const _isAdmin: boolean = result.user.is_admin;
      const _cookies: string[] = result.cookies;

      expect(result).toBeDefined();
    });

    it('should return properly typed LoginResult from login', async () => {
      const result = expectOk(
        await client.auth.login({
          email: 'test@example.com',
          password: 'password123',
        }),
      );

      // TypeScript should infer these properties
      const _id: number = result.user.id;
      const _email: string = result.user.email;
      const _isAdmin: boolean = result.user.is_admin;
      const _cookies: string[] = result.cookies;

      expect(result).toBeDefined();
    });

    it('should enforce LoginRequest schema', () => {
      // This tests compile-time type safety
      const validRequest = {
        email: 'test@example.com',
        password: 'password123',
      };

      expect(validRequest).toBeDefined();

      const _invalidEmail = { email: 123, password: 'test' };

      const _missingPassword = { email: 'test@example.com' };
    });
  });

  describe('Input Validation (Zod)', () => {
    it('should validate email is string', async () => {
      const result = await client.auth.register({
        // @ts-expect-error - testing runtime validation
        email: 123,
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should validate password is string', async () => {
      const result = await client.auth.register({
        email: 'test@example.com',
        // @ts-expect-error - testing runtime validation
        password: 12345678,
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should validate email format at runtime', async () => {
      const result = await client.auth.register({
        email: 'not-an-email',
        password: 'password123',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should reject an empty password at runtime (required)', async () => {
      const result = await client.auth.register({
        email: 'test@example.com',
        password: '',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });
  });
});
