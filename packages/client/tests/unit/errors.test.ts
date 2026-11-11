import { describe, expect, it } from 'vitest';
import {
  isRetryable,
  NETWORK_ERROR_MESSAGE,
  type RustrakError,
  SERVER_ERROR_MESSAGE,
  TIMEOUT_ERROR_MESSAGE,
} from '../../src/errors.js';
import { Err, mapResult, unwrap } from '../../src/result.js';

describe('RustrakError', () => {
  it.each<[RustrakError, boolean]>([
    [
      {
        kind: 'network',
        message: NETWORK_ERROR_MESSAGE,
        reason: 'unreachable',
      },
      true,
    ],
    [
      { kind: 'network', message: TIMEOUT_ERROR_MESSAGE, reason: 'timeout' },
      true,
    ],
    [
      { kind: 'server_error', status: 500, message: SERVER_ERROR_MESSAGE },
      true,
    ],
    [
      { kind: 'server_error', status: 503, message: SERVER_ERROR_MESSAGE },
      true,
    ],
    [
      { kind: 'server_error', status: 501, message: SERVER_ERROR_MESSAGE },
      false,
    ],
    [{ kind: 'rate_limited', status: 429, message: 'x' }, true],
    [{ kind: 'client_error', status: 408, message: 'x' }, true],
    [{ kind: 'client_error', status: 418, message: 'x' }, false],
    [{ kind: 'validation', status: 400, message: 'x' }, false],
    [{ kind: 'conflict', status: 409, message: 'x' }, false],
    [{ kind: 'invalid_request', message: 'x' }, false],
    [{ kind: 'invalid_response', message: 'x' }, false],
  ])('isRetryable(%o) is %s', (error, expected) => {
    expect(isRetryable(error)).toBe(expected);
  });

  // The three fixed messages are production values, so asserting on them is
  // asserting on `src/errors.ts`. What matters is that none of them is a
  // template: the whole reason they exist is that the message they replaced
  // interpolated a connection string, a filesystem path, or the request URL.
  describe('fixed messages', () => {
    it('carry no interpolation and nothing environment-specific', () => {
      const fixed = [
        SERVER_ERROR_MESSAGE,
        NETWORK_ERROR_MESSAGE,
        TIMEOUT_ERROR_MESSAGE,
      ];

      for (const message of fixed) {
        expect(message).not.toContain('{');
        expect(message).not.toContain('$');
        expect(message).not.toContain('://');
        expect(message).not.toMatch(/:\d+/);
      }
    });

    it('are three distinct strings, so a consumer can tell them apart', () => {
      expect(
        new Set([
          SERVER_ERROR_MESSAGE,
          NETWORK_ERROR_MESSAGE,
          TIMEOUT_ERROR_MESSAGE,
        ]).size,
      ).toBe(3);
    });
  });
});

describe('Result helpers', () => {
  // `unwrap` is the one place a caller opts back into exceptions, so it must
  // still throw, and the message must name the failure it was handed.
  it('unwrap throws on a failure, naming the error', () => {
    const result = Err<RustrakError>({
      kind: 'not_found',
      status: 404,
      message: 'Resource not found: Project 42',
    });

    expect(() => unwrap(result)).toThrow(
      'unwrap() called on a failed Result: Resource not found: Project 42',
    );
  });

  it('mapResult passes a failure through without calling fn', () => {
    const error: RustrakError = {
      kind: 'network',
      message: 'x',
      reason: 'unreachable',
    };
    const failure = mapResult(Err(error), () => {
      throw new Error('must not run');
    });
    expect(failure).toEqual({ success: false, error });
  });
});
