import type { RustrakError } from '../errors.js';
import { Ok, type Result } from '../result.js';
import {
  acceptInvitationSchema,
  invitationInfoSchema,
} from '../schemas/invitation.js';
import {
  authResponseSchema,
  loginRequestSchema,
  registerRequestSchema,
  ssoConfigSchema,
  ssoStartSchema,
  updatePreferencesRequestSchema,
  userSchema,
} from '../schemas/user.js';
import type { AcceptInvitation, InvitationInfo } from '../types/invitation.js';
import type {
  LoginRequest,
  LoginResult,
  RegisterRequest,
  SsoConfig,
  UpdatePreferencesRequest,
  User,
} from '../types/user.js';
import { BaseResource, discardBody } from './base.js';

/**
 * Every `Set-Cookie` on a response, without assuming `Headers.getSetCookie`.
 *
 * `getSetCookie` landed in Node 18.16, well below the supported floor, but
 * `engines` is advisory: npm does not enforce it unless the consumer opted in.
 * These are the session-establishing calls, so an older runtime would throw a
 * `TypeError` out of `login` precisely, from a method whose whole contract is
 * that it returns a `Result` instead of throwing. The fallback reads the folded
 * header, which is all the platform offers there.
 *
 * Exported for `tests/unit/base-resource.test.ts` only: the fallback is
 * unreachable on a runtime that has `getSetCookie`, which is every runtime the
 * suite can run on. It is not re-exported from `src/index.ts`.
 */
export function readSetCookie(response: Response): string[] {
  if (typeof response.headers.getSetCookie === 'function') {
    return response.headers.getSetCookie();
  }

  const folded = response.headers.get('set-cookie');
  return folded === null ? [] : [folded];
}

/**
 * Authentication API resource
 * Handles user registration, login, logout, and session management
 */
export class AuthResource extends BaseResource {
  /** Return the public SSO login configuration. */
  async getSsoConfig(): Promise<Result<SsoConfig, RustrakError>> {
    return this.request(
      () => this.http.get('auth/sso/config'),
      ssoConfigSchema,
    );
  }

  /** Begin SSO and return the provider URL plus the encrypted state cookie. */
  async startSso(): Promise<
    Result<{ authorizationUrl: string; cookies: string[] }, RustrakError>
  > {
    return this.requestResponse(
      () => this.http.post('auth/sso/start'),
      async (response) => {
        const cookies = readSetCookie(response);
        const body = await this.readJson(response);
        if (!body.success) return body;

        const data = this.validate(body.data, ssoStartSchema);
        if (!data.success) return data;

        return Ok({
          authorizationUrl: data.data.authorization_url,
          cookies,
        });
      },
    );
  }

  /** Complete an OIDC callback and return the normal Rustrak session cookie. */
  async completeSso(callback: {
    code?: string;
    state?: string;
    error?: string;
  }): Promise<Result<LoginResult, RustrakError>> {
    const searchParams = new URLSearchParams();
    if (callback.code) searchParams.set('code', callback.code);
    if (callback.state) searchParams.set('state', callback.state);
    if (callback.error) searchParams.set('error', callback.error);

    return this.requestResponse(
      () => this.http.get('auth/sso/callback', { searchParams }),
      (response) => this.readLoginResult(response),
    );
  }

  /**
   * Register a new user account
   * Creates a new user and automatically logs them in (sets session cookie)
   * @param credentials - Email and password for the new account
   * @returns LoginResult with user information and session cookies
   */
  async register(
    credentials: RegisterRequest,
  ): Promise<Result<LoginResult, RustrakError>> {
    const validatedInput = this.validateInput(
      credentials,
      registerRequestSchema,
    );
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.requestResponse(
      () => this.http.post('auth/register', { json: validatedInput.data }),
      (response) => this.readLoginResult(response),
    );
  }

  /**
   * Login with email and password
   * Authenticates the user and sets a session cookie
   * @param credentials - Email and password
   * @returns LoginResult with user information and session cookies
   */
  async login(
    credentials: LoginRequest,
  ): Promise<Result<LoginResult, RustrakError>> {
    const validatedInput = this.validateInput(credentials, loginRequestSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.requestResponse(
      () => this.http.post('auth/login', { json: validatedInput.data }),
      (response) => this.readLoginResult(response),
    );
  }

  /**
   * Logout the current user
   * Clears the session cookie
   * @returns Array of Set-Cookie headers (typically clearing cookies)
   */
  async logout(): Promise<Result<string[], RustrakError>> {
    return this.requestResponse(
      () => this.http.post('auth/logout'),
      async (response) => {
        const cookies = readSetCookie(response);
        // Only the headers matter here, and an unread body holds its socket out
        // of Node's keep-alive pool.
        discardBody(response);
        return Ok(cookies);
      },
    );
  }

  /**
   * Get current authenticated user
   * Requires a valid session cookie
   *
   * No session is `{success: false, kind: 'unauthenticated'}`, not a `null`
   * user: it is the literal reading of the server's 401. Callers must send the
   * visitor to login on `unauthenticated` **only**. Treating `network` or
   * `server_error` the same way turns a flaky connection into a login loop.
   *
   * @returns The current user, or `unauthenticated` when there is no session
   */
  async getCurrentUser(): Promise<Result<User, RustrakError>> {
    return this.request(() => this.http.get('auth/me'), userSchema);
  }

  /**
   * Stores how this reader wants the dashboard presented to them.
   *
   * A key left out is left alone; a key sent as `null` is cleared. That is why
   * the request type is optional *and* nullable rather than one or the other:
   * "do not touch" and "forget my choice" are different instructions.
   */
  async updatePreferences(
    preferences: UpdatePreferencesRequest,
  ): Promise<Result<User, RustrakError>> {
    const validatedInput = this.validateInput(
      preferences,
      updatePreferencesRequestSchema,
    );
    if (!validatedInput.success) return validatedInput;

    return this.request(
      () => this.http.patch('auth/me', { json: validatedInput.data }),
      userSchema,
    );
  }

  /**
   * Get the details of a pending invitation by its token (public endpoint)
   * Used by the accept-invitation page before the user has an account
   * @param token - Invitation token
   * @returns Invitation info (email, role, status, expiry)
   */
  async getInvitation(
    token: string,
  ): Promise<Result<InvitationInfo, RustrakError>> {
    return this.request(
      () => this.http.get(`auth/invitation/${token}`),
      invitationInfoSchema,
    );
  }

  /**
   * Accept a pending invitation, creating the user account and logging in
   * @param input - Invitation token and the new account's password
   * @returns LoginResult with user information and session cookies
   */
  async acceptInvitation(
    input: AcceptInvitation,
  ): Promise<Result<LoginResult, RustrakError>> {
    const validatedInput = this.validateInput(input, acceptInvitationSchema);
    if (!validatedInput.success) {
      return validatedInput;
    }

    return this.requestResponse(
      () =>
        this.http.post('auth/accept-invitation', {
          json: validatedInput.data,
        }),
      (response) => this.readLoginResult(response),
    );
  }

  /**
   * The three session-establishing endpoints share a body and need the
   * `Set-Cookie` headers off the raw `Response`, which is why they go through
   * `requestResponse` rather than the plain JSON path.
   */
  private async readLoginResult(
    response: Response,
  ): Promise<Result<LoginResult, RustrakError>> {
    const cookies = readSetCookie(response);

    // `readJson` rather than a bare try/catch: a connection dying mid-body is
    // `network`, not `invalid_response`, and a genuine bug still throws.
    const body = await this.readJson(response);
    if (!body.success) {
      return body;
    }

    const authResponse = this.validate(body.data, authResponseSchema);
    if (!authResponse.success) {
      return authResponse;
    }

    return Ok({ user: authResponse.data.user, cookies });
  }
}
