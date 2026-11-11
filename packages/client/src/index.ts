/**
 * @rustrak/client - TypeScript client for Rustrak API
 *
 * A type-safe, fully-featured client for interacting with Rustrak error tracking API.
 *
 * @packageDocumentation
 */

export { RustrakClient } from './client.js';

export type { ClientConfig } from './config.js';
export {
  FIELD_ERROR_CODES,
  type FieldError,
  type FieldErrorCode,
  isRetryable,
  NETWORK_ERROR_MESSAGE,
  type RustrakError,
  type RustrakErrorKind,
  SERVER_ERROR_MESSAGE,
  TIMEOUT_ERROR_MESSAGE,
} from './errors.js';
// Every request and response type in openapi.json, named as the spec names
// them: `IssueResponse`, `ProjectResponse`, `IssuesListData`.
export type * from './generated/types.gen.js';
export {
  Err,
  mapResult,
  Ok,
  type Result,
  unwrap,
  unwrapOr,
} from './result.js';
