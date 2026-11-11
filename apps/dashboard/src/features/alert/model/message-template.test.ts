import { describe, expect, it } from 'vitest';
import { formatTemplate, templateParses } from '../lib/format-template';
import {
  isKnownVariable,
  TEMPLATE_PRESETS,
  TEMPLATE_VARIABLES,
  templatePlaceholder,
} from './message-template';

describe('TEMPLATE_VARIABLES', () => {
  it('inserts every field bare, since the server escapes it for its position', () => {
    // The whole point: the reader never learns a filter. `| tojson` in a
    // snippet would be the old contract leaking back in.
    for (const variable of TEMPLATE_VARIABLES) {
      expect(variable.snippet).toBe(`{{ ${variable.path} }}`);
    }
  });

  it('shows the same discipline in the placeholder', () => {
    expect(templatePlaceholder).not.toContain('|');
    expect(templatePlaceholder).toContain('{{ issue.title }}');
  });

  it('knows its own paths and nothing else', () => {
    expect(isKnownVariable('issue.title')).toBe(true);
    expect(isKnownVariable('issue.titel')).toBe(false);
  });
});

describe('TEMPLATE_PRESETS', () => {
  it('only references fields the payload has', () => {
    // A preset with a typo would be caught at save time by the server, but
    // it should never get that far.
    for (const preset of TEMPLATE_PRESETS) {
      for (const match of preset.body.matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)) {
        expect(
          isKnownVariable(match[1] as string),
          `${preset.name} references ${match[1]}`,
        ).toBe(true);
      }
    }
  });

  it('is JSON around its fields, so it lands in the editor already valid', () => {
    for (const preset of TEMPLATE_PRESETS) {
      expect(templateParses(preset.body), preset.name).toBe(true);
    }
  });

  it('is stored the way the formatter would write it', () => {
    // Otherwise the first press of the wand rewrites a body nobody touched.
    for (const preset of TEMPLATE_PRESETS) {
      expect(formatTemplate(preset.body), preset.name).toBe(preset.body);
    }
  });
});
