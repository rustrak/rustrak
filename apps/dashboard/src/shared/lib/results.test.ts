import type { RustrakError } from '@rustrak/client';
import { Err, Ok } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import { combine, loadAll } from './results';

const notFound = { kind: 'not_found', message: 'gone' } as RustrakError;

describe('combine', () => {
  it('keeps the tuple when every member succeeded', () => {
    const combined = combine([Ok(1), Ok('two')] as const);

    expect(combined).toEqual(Ok([1, 'two']));
  });

  it('answers the first failure', () => {
    const combined = combine([Ok(1), Err(notFound), Err({ ...notFound })]);

    expect(combined.success).toBe(false);
    expect(!combined.success && combined.error).toBe(notFound);
  });
});

describe('loadAll', () => {
  it('awaits the members and combines them', async () => {
    const loaded = await loadAll([
      Promise.resolve(Ok(1)),
      Promise.resolve(Err(notFound)),
    ]);

    expect(loaded.success).toBe(false);
  });
});
