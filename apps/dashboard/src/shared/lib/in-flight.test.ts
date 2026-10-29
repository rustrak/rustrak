import { describe, expect, it } from 'vitest';
import { shareInFlight } from './in-flight';

describe('shareInFlight', () => {
  it('gives callers asking at once the same request', async () => {
    let calls = 0;
    const read = () =>
      shareInFlight('same', async () => {
        calls += 1;
        return calls;
      });

    const [a, b] = await Promise.all([read(), read()]);

    expect([a, b, calls]).toEqual([1, 1, 1]);
  });

  it('asks again once the last answer is in', async () => {
    let calls = 0;
    const read = () => shareInFlight('again', async () => ++calls);

    await read();

    expect(await read()).toBe(2);
  });

  it('asks again after a failure', async () => {
    let calls = 0;
    const read = () =>
      shareInFlight('failing', async () => {
        calls += 1;
        throw new Error('down');
      });

    await expect(read()).rejects.toThrow('down');
    await expect(read()).rejects.toThrow('down');

    expect(calls).toBe(2);
  });
});
