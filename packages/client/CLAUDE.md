# @rustrak/client

TypeScript client for the Rustrak REST API, **generated from
`apps/server/openapi.json`** with `@hey-api/openapi-ts`. Usable from Node or a
browser: HTTP is the platform's `fetch`, Zod validates both ways, tsdown builds,
Vitest and MSW test. Root context: `/CLAUDE.md`.

```
apps/server (utoipa) ──▶ openapi.json ──▶ pnpm generate ──▶ src/generated/  (SDK, types, zod)
                                                      ├──▶ src/generated/binary.gen.ts (file endpoints)
                                                      └──▶ API reference in README.md
                                                           and apps/docs/content/sdks/client.mdx
```

## Never edit `src/generated/` or a generated API reference

Both are output; the API reference is whatever sits between the `api:start`
and `api:end` markers of README.md and `apps/docs/content/sdks/client.mdx`.
Change the server (its handler, its `utoipa::path`, its `ToSchema` types and
doc comments), regenerate the spec, then:

```bash
pnpm --filter @rustrak/client generate        # SDK, README and apps/docs/content/sdks/client.mdx
pnpm --filter @rustrak/client generate:check  # what CI runs: fails if any of it is stale
```

All of it is committed: a server change shows up in the PR as the client change it
causes, and building or type-checking never needs the generator.

Every method is named in Rust, on its handler: `operation_id = "issues.list"`
is `client.issues.list()`. hey-api splits the id on the dot; nothing in this
package derives a name. The server's `every_operation_names_its_client_method`
test fails on a missing, malformed or repeated id. **Changing an
`operation_id` renames a client method**, which is a breaking change.

## The one invariant: no exceptions

Every method returns a `Result<T, RustrakError>`:

```ts
type Result<T, E> =
  | { readonly success: true;  readonly data: T }
  | { readonly success: false; readonly error: E }
```

The shape mirrors Zod's `safeParse`. Everything inside is a plain object with
`Object.prototype`: no class instances. React's Flight serializer refuses to send
anything else across the server/client boundary, so a failure that crosses from a
Server Component to the browser has to survive `structuredClone`. A thrown
`Error` subclass does not.

`RustrakError` is a discriminated union keyed on `kind`, covering the transport
(`network`, `invalid_response`), the request (`invalid_request`, `validation`),
authorization (`unauthenticated`, `forbidden`), and the response status
(`not_found`, `conflict`, `gone`, `rate_limited`, `payload_too_large`,
`client_error`, `server_error`). `TRANSIENT_STATUSES` in `errors.ts` is the one
retry rule: `isRetryable` reads it, and so does the transport, less a 429 and a
timeout, whose wait only the caller can choose.

## Layout

```
src/
├── index.ts       public API surface
├── client.ts      RustrakClient: the generated SDK with every method wrapped in toResult
├── config.ts      ClientConfig
├── result.ts      Result, Ok, Err, unwrap, unwrapOr, mapResult
├── errors.ts      the RustrakError union and isRetryable
├── utils/http.ts  the fetch every request goes through (timeout, retries,
│                  transport failures) and the status -> RustrakError mapping
└── generated/     hey-api output, plus binary.gen.ts. Do not edit.
scripts/
├── spec.ts        the parsed openapi.json, shared by generator and tests
└── generate.ts    README/docs API reference and binary.gen.ts
```

`toResult` in `client.ts` is the single boundary: only failures it recognises
become an `Err`; anything else is a bug and is rethrown.

## Tests

```bash
pnpm --filter=@rustrak/client test
```

- `tests/integration/operations.test.ts` walks `openapi.json` and calls every
  operation, checking the request it sends and that a response built from the
  spec validates. A new endpoint is covered the day it is generated.
- `error-handling.test.ts` and `tests/unit/transport.test.ts` cover the error
  half: status mapping, redaction, retries, timeouts, cancellation.
- The server side of the contract is `apps/server/tests/integration/openapi_contract_test.rs`:
  real responses validated against the same spec.

MSW intercepts at the network layer, so tests exercise the real `fetch` pipeline.
