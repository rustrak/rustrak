import { describe, expect, it } from 'vitest';
import { describeCrontab } from './crontab-description';

describe('describeCrontab', () => {
  it('reads a crontab in the reader language', () => {
    expect(describeCrontab('0 3 * * *', 'en')).toBe('At 03:00');
    expect(describeCrontab('0 3 * * *', 'es')).toBe('A las 03:00');
  });

  it('maps the Chinese locale to cronstrue name for it', () => {
    expect(describeCrontab('0 3 * * *', 'zh')).not.toBe('At 03:00');
  });

  it('is null for something it cannot read', () => {
    expect(describeCrontab('nope', 'en')).toBeNull();
    expect(describeCrontab('', 'en')).toBeNull();
  });
});
