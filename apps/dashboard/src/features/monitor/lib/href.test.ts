import { describe, expect, it } from 'vitest';
import { monitorHref, newMonitorHref } from './href';

describe('monitorHref', () => {
  it('never meets a static crons route, whatever the slug', () => {
    // `new` is a valid slug (the server derives it from the name "New").
    expect(monitorHref(1, 'new')).not.toBe(newMonitorHref(1));
  });
});
