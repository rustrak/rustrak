/**
 * The server's `openapi.json`: what the client is generated from and what
 * `tests/integration/operations.test.ts` walks.
 */
import { readFileSync } from 'node:fs';

export type Schema = Record<string, unknown>;

export type Operation = {
  operationId: string;
  tags: string[];
  summary?: string;
  description?: string;
  parameters?: {
    name: string;
    in: string;
    required?: boolean;
    schema: Schema;
  }[];
  requestBody?: { content: Record<string, { schema: Schema }> };
  responses: Record<string, { content?: Record<string, { schema: Schema }> }>;
};

export const specText = readFileSync(
  new URL('../../../apps/server/openapi.json', import.meta.url),
  'utf8',
);

export const spec: {
  paths: Record<string, Record<string, Operation>>;
  components: { schemas: Record<string, Schema> };
} = JSON.parse(specText);
