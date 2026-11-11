/**
 * What every script here sends to unless told otherwise: `SENTRY_DSN`, else
 * project 1 on a local server.
 */
export const DSN =
  process.env.SENTRY_DSN ??
  'http://0cabb1ed295f4cf6b41411220caa8eb6@localhost:8080/1';
export const RELEASE = process.env.SENTRY_RELEASE ?? 'demo@1.0.0';
export const ENV = process.env.SENTRY_ENV ?? 'production';
