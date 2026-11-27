import { describe, expect, it } from 'vitest';
import { levelTone } from './levels';

describe('levelTone', () => {
  it('reads the levels it knows', () => {
    expect(levelTone('fatal')).toBe('error');
    expect(levelTone('warn')).toBe('warning');
    expect(levelTone('debug')).toBeUndefined();
  });

  it('has no tone for a level named after an object property', () => {
    expect(levelTone('constructor')).toBeUndefined();
    expect(levelTone('__proto__')).toBeUndefined();
  });
});
