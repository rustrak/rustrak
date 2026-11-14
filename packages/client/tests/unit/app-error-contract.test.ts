import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { FIELD_ERROR_CODES } from '../../src/errors.js';
import {
  APP_ERROR_PREFIXES,
  APP_ERROR_STATUS,
  type AppErrorType,
  INTERNAL_ERROR_MESSAGE,
} from '../mocks/handlers.js';

/**
 * The rest of this suite is a closed loop: every assertion restates the literal
 * the fixture on the previous screen sent, so a fixture and its assertion can
 * drift away from the server together and stay green forever.
 *
 * This file is the one place that reads the other side of the contract. It
 * parses `apps/server/src/error.rs` and asserts that the prefix and status
 * tables the fixtures are built from still match the Rust `enum`. Reword an
 * `#[error("...")]` or remap a `StatusCode` and this test fails, which is the
 * only mechanism that can catch that class of drift from TypeScript.
 *
 * Resolved from `import.meta.url`, so it does not depend on the cwd vitest ran
 * from.
 */
const ERROR_RS = fileURLToPath(
  new URL('../../../../apps/server/src/error.rs', import.meta.url),
);

const source = existsSync(ERROR_RS) ? readFileSync(ERROR_RS, 'utf8') : null;

if (source === null) {
  // `@rustrak/client` is publishable on its own, so a consumer may have the
  // package without the Rust workspace beside it. Skipping loudly beats failing
  // with an unexplained ENOENT.
  console.warn(
    `[app-error-contract] skipped: ${ERROR_RS} not found. ` +
      'The AppError coupling check only runs inside the Rustrak monorepo.',
  );
}

/** The `StatusCode` constants `AppError::status_code` can name. */
const STATUS_CODE_NUMBERS: Record<string, number> = {
  BAD_REQUEST: 400,
  UNAUTHORIZED: 401,
  FORBIDDEN: 403,
  NOT_FOUND: 404,
  CONFLICT: 409,
  PAYLOAD_TOO_LARGE: 413,
  INTERNAL_SERVER_ERROR: 500,
  SERVICE_UNAVAILABLE: 503,
};

/** `#[error("<display>")]` immediately above `VariantName(`. */
function parseDisplayStrings(rust: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = /#\[error\("([^"]+)"\)\]\s*(\w+)\(/g;
  for (const m of rust.matchAll(re)) {
    const [, display, variant] = m;
    if (display !== undefined && variant !== undefined) {
      out.set(variant, display);
    }
  }
  return out;
}

/** `AppError::Variant(_) => "TypeLiteral",` inside `error_response`. */
function parseTypeLiterals(rust: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = /AppError::(\w+)\(_\)\s*=>\s*"(\w+)"/g;
  for (const m of rust.matchAll(re)) {
    const [, variant, literal] = m;
    if (variant !== undefined && literal !== undefined) {
      out.set(variant, literal);
    }
  }
  return out;
}

/**
 * The `pub enum FieldErrorCode { ... }` block, plus the attributes above it.
 *
 * Sliced by index rather than matched wholesale: the attribute list contains
 * `#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]`, whose nested
 * brackets defeat any reasonable regex.
 */
function parseFieldErrorCodeEnum(
  rust: string,
): { attributes: string; body: string; variants: string[] } | null {
  const header = 'pub enum FieldErrorCode {';
  const start = rust.indexOf(header);
  if (start === -1) return null;

  const end = rust.indexOf('\n}', start);
  if (end === -1) return null;

  const body = rust.slice(start + header.length, end);
  // `    Variant,` on its own line. Doc comments and attributes cannot match.
  const variants = [...body.matchAll(/^ {4}(\w+),$/gm)].map((m) => m[1] ?? '');

  return {
    attributes: rust.slice(Math.max(0, start - 400), start),
    body,
    variants,
  };
}

/** `AlreadyExists` -> `already_exists`, which is what serde's rename does. */
function toSnakeCase(variant: string): string {
  return variant.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
}

/** `AppError::Variant(_) => StatusCode::NAME,` inside `status_code`. */
function parseStatusCodes(rust: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = /AppError::(\w+)\(_\)\s*=>\s*StatusCode::(\w+)/g;
  for (const m of rust.matchAll(re)) {
    const [, variant, status] = m;
    if (variant !== undefined && status !== undefined) {
      out.set(variant, status);
    }
  }
  return out;
}

describe.skipIf(source === null)(
  'AppError contract (apps/server/src/error.rs)',
  () => {
    // Non-null inside the block: `describe.skipIf` guarantees it.
    const rust = source as string;

    const displays = parseDisplayStrings(rust);
    const literals = parseTypeLiterals(rust);
    const statuses = parseStatusCodes(rust);

    it('parses all nine variants out of the Rust enum', () => {
      // If the parse silently matches nothing, every table comparison below
      // would trivially pass, so assert the parse itself first.
      expect(displays.size).toBe(9);
      expect(literals.size).toBe(9);
      expect(statuses.size).toBe(9);
      expect([...literals.keys()].sort()).toEqual([...displays.keys()].sort());
      expect([...statuses.keys()].sort()).toEqual([...displays.keys()].sort());
    });

    it('every variant interpolates its detail as a trailing {0}', () => {
      for (const [variant, display] of displays) {
        expect(
          display.endsWith('{0}'),
          `${variant}'s #[error(...)] must end with {0}, got "${display}"`,
        ).toBe(true);
      }
    });

    // -----------------------------------------------------------------------
    // The 5xx redaction (gh-233)
    //
    // A `Display` string is no longer what a 5xx puts on the wire. The tables
    // above still describe `#[error(...)]` faithfully, and still must: that
    // text is what `log::error!` carries, so it is what an operator greps.
    // What changed is which bodies it reaches. These two pin the split itself,
    // because every fixture in this suite now builds a 500 body out of a
    // constant declared in TypeScript, and nothing else would notice the Rust
    // side rewording or dropping it.
    // -----------------------------------------------------------------------

    it('INTERNAL_ERROR_MESSAGE matches the Rust constant', () => {
      const match = rust.match(
        /pub const INTERNAL_ERROR_MESSAGE: &str = "([^"]*)";/,
      );

      expect(match, 'INTERNAL_ERROR_MESSAGE must exist in error.rs').not.toBe(
        null,
      );
      expect(match?.[1]).toBe(INTERNAL_ERROR_MESSAGE);
    });

    it('error_response redacts on the status, not on the variant', () => {
      // Anchored on `is_server_error()` rather than on a variant list: the
      // rule has to keep holding for a tenth variant nobody has written yet,
      // and a `match` naming `Database` and `Internal` would silently exempt
      // it. `_ = INTERNAL_ERROR_MESSAGE;` somewhere unrelated would satisfy a
      // grep for the constant alone, so both halves are asserted together.
      const body = rust.slice(rust.indexOf('fn error_response'));

      expect(body).toContain('is_server_error()');
      expect(body).toContain('INTERNAL_ERROR_MESSAGE');
    });

    it('APP_ERROR_PREFIXES matches the #[error(...)] Display prefixes', () => {
      const expected: Record<string, string> = {};
      for (const [variant, display] of displays) {
        const literal = literals.get(variant);
        if (literal === undefined) continue;
        expected[literal] = display.slice(0, -'{0}'.length);
      }

      expect(APP_ERROR_PREFIXES).toEqual(expected);
    });

    it('APP_ERROR_STATUS matches AppError::status_code', () => {
      const expected: Record<string, number> = {};
      for (const [variant, statusName] of statuses) {
        const literal = literals.get(variant);
        const code = STATUS_CODE_NUMBERS[statusName];
        expect(
          code,
          `unknown StatusCode::${statusName}; add it to STATUS_CODE_NUMBERS`,
        ).toBeDefined();
        if (literal !== undefined && code !== undefined) {
          expected[literal] = code;
        }
      }

      expect(APP_ERROR_STATUS).toEqual(expected);
    });

    it('the AppErrorType union covers exactly the literals Rust can emit', () => {
      const declared = Object.keys(APP_ERROR_PREFIXES) as AppErrorType[];
      expect(declared.sort()).toEqual([...literals.values()].sort());
    });

    // -----------------------------------------------------------------------
    // FieldErrorCode
    //
    // The same coupling one level down. `ErrorDetail.fields[].code` is a closed
    // vocabulary a form maps to its own copy, so a variant added in Rust that
    // never reaches `FIELD_ERROR_CODES` would arrive as a code this client
    // drops on the floor. That has to be a failing test, not a silent gap.
    // -----------------------------------------------------------------------

    const fieldErrorCodes = parseFieldErrorCodeEnum(rust);

    it('parses the FieldErrorCode enum out of the Rust source', () => {
      // Assert the parse before comparing anything against it: a regex that
      // matched nothing would make the equality below trivially satisfiable.
      expect(fieldErrorCodes).not.toBeNull();
      expect(fieldErrorCodes?.variants.length).toBeGreaterThan(0);
    });

    it('FieldErrorCode serialises snake_case, which the comparison assumes', () => {
      // Drop `#[serde(rename_all = "snake_case")]` and every variant would go
      // out PascalCase while this file kept passing against the same names.
      expect(fieldErrorCodes?.attributes).toContain(
        '#[serde(rename_all = "snake_case")]',
      );
    });

    it('no FieldErrorCode variant overrides its name with #[serde(rename)]', () => {
      // `toSnakeCase` below hand-rolls what `rename_all = "snake_case"` does,
      // and it can only see the identifier. A single
      // `#[serde(rename = "duplicate")]` above `AlreadyExists` would keep every
      // assertion in this file green while the server put `"duplicate"` on the
      // wire and `readFieldErrors` dropped it on the floor, degrading every
      // affected form to a form-level error with a fully green suite.
      //
      // The rule is therefore "there is no per-variant rename", which is
      // checkable, rather than "the parser understands renames", which is a
      // second serde implementation nobody will keep correct.
      expect(
        fieldErrorCodes?.body,
        'a per-variant #[serde(rename = "...")] makes the wire name ' +
          'unknowable from the identifier; either drop it or teach ' +
          'FIELD_ERROR_CODES and this parser about it',
      ).not.toContain('#[serde(rename');
    });

    it('FIELD_ERROR_CODES matches the Rust FieldErrorCode variants', () => {
      const expected = (fieldErrorCodes?.variants ?? []).map(toSnakeCase);

      expect([...FIELD_ERROR_CODES].sort()).toEqual([...expected].sort());
    });
  },
);
