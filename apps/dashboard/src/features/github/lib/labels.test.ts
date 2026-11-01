import { describe, expect, it } from 'vitest';
import { parseLabels } from './labels';

describe('parseLabels', () => {
  it('splits on commas, trims and drops empty and repeated labels', () => {
    expect(parseLabels(' bug, needs triage,, bug ,')).toEqual([
      'bug',
      'needs triage',
    ]);
    expect(parseLabels('')).toEqual([]);
  });
});
