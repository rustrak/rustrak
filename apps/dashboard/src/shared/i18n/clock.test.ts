import { describe, expect, it, vi } from 'vitest';
import { createClock } from './clock';

describe('createClock', () => {
  it('moves now to the present and tells only its own listeners', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    try {
      vi.setSystemTime(new Date('2026-01-01T06:00:00Z'));
      const clock = createClock();
      const listener = vi.fn();
      clock.subscribe(listener);
      const before = clock.now();

      vi.setSystemTime(new Date('2026-01-01T06:34:00Z'));
      expect(clock.now()).toBe(before);

      clock.refreshNow();

      expect(clock.now()).toEqual(new Date('2026-01-01T06:34:00Z'));
      expect(listener).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it('stops telling a listener that unsubscribed', () => {
    const clock = createClock();
    const listener = vi.fn();
    clock.subscribe(listener)();

    clock.refreshNow();

    expect(listener).not.toHaveBeenCalled();
  });
});
