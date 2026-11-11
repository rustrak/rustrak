/**
 * Every operation in `openapi.json`, called through the client.
 *
 * Walks the spec rather than a list, so an endpoint added to the server is
 * covered the day the client is regenerated. For each one it checks that the
 * method sends the documented HTTP method, path, required query and body, and that
 * a response built from the spec's own schema comes back as `success: true`:
 * what the server is held to by `openapi_contract_test.rs`, the client's
 * validators must accept.
 */
import { HttpResponse, http } from 'msw';
import { describe, expect, it } from 'vitest';
import { type Operation, type Schema, spec } from '../../scripts/spec.ts';
import { RustrakClient } from '../../src/client.js';
import { BINARY } from '../../src/generated/binary.gen.js';
import { server } from '../setup.js';

const BASE = 'http://rustrak.test';
const CHUNK_NAME = 'da39a3ee5e6b4b0d3255bfef95601890afd80709';

/** A string that satisfies `schema`'s pattern, format or length. */
function sampleString(schema: Schema): string {
  if (typeof schema.pattern === 'string') {
    const value = ({ '^[0-9]+$': '1' } as Record<string, string>)[
      schema.pattern
    ];
    if (value === undefined) {
      throw new Error(`no sample for pattern ${schema.pattern}`);
    }
    return value;
  }
  return (
    (
      {
        uuid: '3f2b8c1e-9a4d-4c6b-8e2f-1a2b3c4d5e6f',
        'date-time': '2026-01-01T00:00:00Z',
        email: 'contract@example.com',
        uri: 'https://example.com',
      } as Record<string, string>
    )[schema.format as string] ?? 'x'.repeat(Number(schema.minLength ?? 1))
  );
}

/** A value that satisfies `schema`, built from the spec alone. */
function sample(schema: Schema | undefined): unknown {
  if (!schema) return {};
  if (typeof schema.$ref === 'string') {
    return sample(spec.components.schemas[schema.$ref.split('/').pop()!]);
  }
  for (const key of ['oneOf', 'anyOf', 'allOf'] as const) {
    const options = schema[key] as Schema[] | undefined;
    if (!options) continue;
    if (key === 'allOf') {
      return Object.assign({}, ...options.map((s) => sample(s)));
    }
    return sample(options.find((s) => s.type !== 'null') ?? options[0]);
  }
  if (Array.isArray(schema.enum)) return schema.enum[0];
  const types = [schema.type].flat().filter((t) => t !== 'null');
  switch (types[0]) {
    case 'string':
      return sampleString(schema);
    case 'integer':
    case 'number':
      return Number(schema.minimum ?? 1);
    case 'boolean':
      return true;
    case 'array':
      return [sample(schema.items as Schema)];
    case 'object':
    case undefined: {
      const properties = (schema.properties ?? {}) as Record<string, Schema>;
      return Object.fromEntries(
        Object.entries(properties).map(([k, v]) => [k, sample(v)]),
      );
    }
    default:
      return null;
  }
}

/** What to call the method with, and the URL and query it must produce. */
function request(path: string, op: Operation) {
  const pathParams: Record<string, unknown> = {};
  const query: Record<string, unknown> = {};
  let url = path;
  for (const p of op.parameters ?? []) {
    const value = sample(p.schema);
    if (p.in === 'path') {
      pathParams[p.name] = value;
      url = url.replace(`{${p.name}}`, String(value));
    } else if (p.in === 'query' && p.required) {
      query[p.name] = value;
    }
  }
  const json = op.requestBody?.content['application/json'];
  const multipart = op.requestBody?.content['multipart/form-data'];
  const options = {
    ...(Object.keys(pathParams).length && { path: pathParams }),
    ...(Object.keys(query).length && { query }),
    ...(json && { body: sample(json.schema) }),
    // A sentry-cli chunk: the part's field name is its SHA-1.
    ...(multipart && { body: { [CHUNK_NAME]: new Blob(['chunk']) } }),
  };
  return { options, url, query };
}

const operations = Object.entries(spec.paths).flatMap(([path, item]) =>
  Object.entries(item).map(([method, op]) => ({ path, method, op })),
);

/** The documented answer: bytes for a file, else the schema's sample or nothing. */
function reply(status: number, schema: Schema | undefined, binary: boolean) {
  if (binary) return new HttpResponse(new Uint8Array([1, 2, 3]), { status });
  return status === 204 || (status >= 300 && status < 400) || !schema
    ? new HttpResponse(null, { status })
    : HttpResponse.json(sample(schema), { status });
}

describe('every operation in openapi.json', () => {
  const client = new RustrakClient({ baseUrl: BASE, maxRetries: 0 });

  it.each(operations)('$method $path', async ({ path, method, op }) => {
    const [resource, name] = op.operationId.split('.');
    const call = (
      client as unknown as Record<string, Record<string, Function>>
    )[resource!]?.[name!];
    expect(call, `client.${resource}.${name}`).toBeTypeOf('function');

    const { options, url, query } = request(path, op);

    // Redirects and the invite-only `register` document no 2xx: answer with
    // the first status they do document and check only the request.
    const [status, response] =
      Object.entries(op.responses).find(([s]) => s.startsWith('2')) ??
      Object.entries(op.responses)[0]!;
    const responseSchema = response.content?.['application/json']?.schema;
    const binary = (BINARY as readonly string[]).includes(op.operationId);

    let seen: Request | undefined;
    let sent: unknown;
    server.use(
      http.all(`${BASE}*`, async ({ request }) => {
        seen = request;
        const type = request.headers.get('content-type') ?? '';
        if (type.startsWith('application/json')) sent = await request.json();
        if (type.startsWith('multipart/form-data')) {
          const part = (await request.formData()).get(CHUNK_NAME);
          sent = part instanceof Blob ? await part.text() : part;
        }
        return reply(Number(status), responseSchema, binary);
      }),
    );

    const result = await call!(options);

    expect(result).toMatchObject({ success: status.startsWith('2') });
    if (binary) expect(result.data).toBeInstanceOf(Blob);
    expect(seen?.method).toBe(method.toUpperCase());
    const sentUrl = new URL(seen!.url);
    expect(decodeURIComponent(sentUrl.pathname)).toBe(url);
    for (const [k, v] of Object.entries(query)) {
      expect(sentUrl.searchParams.get(k)).toBe(String(v));
    }
    const { requestBody } = op;
    if (requestBody?.content['application/json']) {
      expect(sent).toEqual(options.body);
    } else if (requestBody?.content['multipart/form-data']) {
      expect(sent).toBe('chunk');
    }
  });
});
