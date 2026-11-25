import { describe, expect, it } from 'vitest';
import { newRules, type Rule, ruleKeys } from './styles';

const rule = (cssText: string, cssRules?: Rule[]): Rule => ({
  cssText,
  cssRules,
});
const layer = (name: string, rules: Rule[]) =>
  rule(`@layer ${name} { ${rules.map((r) => r.cssText).join(' ')} }`, rules);

const dashboard = [
  layer('utilities', [
    rule('.flex-col { flex-direction: column; }'),
    rule(
      '.\\@xl\\:flex-row { @container (width >= 36rem) { flex-direction: row; } }',
      [],
    ),
  ]),
];

describe('newRules', () => {
  it("drops what the dashboard already has, so a plugin's copy cannot reorder it", () => {
    const plugin = [
      rule('@layer base, components;'),
      layer('utilities', [
        rule('.flex-col { flex-direction: column; }'),
        rule('.text-fg-brand { color: var(--fg-brand); }'),
      ]),
    ];
    expect(newRules(plugin, ruleKeys(dashboard))).toBe(
      '@layer base, components;\n@layer utilities {\n.text-fg-brand { color: var(--fg-brand); }\n}',
    );
  });

  it('keeps a rule the dashboard has only under another condition', () => {
    const plugin = [
      layer('theme', [rule('.flex-col { flex-direction: column; }')]),
    ];
    expect(newRules(plugin, ruleKeys(dashboard))).toContain('.flex-col');
  });

  it('leaves out a group with nothing new in it', () => {
    const plugin = [
      layer('utilities', [rule('.flex-col { flex-direction: column; }')]),
    ];
    expect(newRules(plugin, ruleKeys(dashboard))).toBe('');
  });
});
