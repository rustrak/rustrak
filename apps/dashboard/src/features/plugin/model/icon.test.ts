import { PluginsIcon } from '@rustrak/ui';
import { describe, expect, it } from 'vitest';
import { pluginIcon } from './icon';

describe('pluginIcon', () => {
  it('is the puzzle piece for a name only the prototype has', () => {
    expect(pluginIcon('constructor')).toBe(PluginsIcon);
  });
});
