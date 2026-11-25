/**
 * A plugin's stylesheet is Tailwind over the same tokens as the dashboard's,
 * so most of its rules are copies of the dashboard's own. Loaded after it, a
 * copy wins the cascade: the plugin's `.flex-col` lands after the dashboard's
 * `@xl:flex-row` and every row that used both stacks. Dropping the copies
 * leaves the dashboard's order alone and adds only what is new.
 *
 * ponytail: a class only the plugin has still lands after every dashboard
 * variant; reorder per property if a plugin ever needs that.
 */

/** The part of a `CSSRule` this needs; a plain object in tests. */
export interface Rule {
  cssText: string;
  cssRules?: ArrayLike<Rule>;
}

/** `@layer utilities`, `@media (…)`: a rule that only groups others. */
function group(rule: Rule): string | null {
  const head = rule.cssText.slice(0, rule.cssText.indexOf('{')).trim();
  return head.startsWith('@') && rule.cssRules ? head : null;
}

/** Every rule under its groups' headers, to look a copy up by. */
export function ruleKeys(rules: ArrayLike<Rule>, under = ''): Set<string> {
  const keys = new Set<string>();
  for (const rule of Array.from(rules)) {
    const head = group(rule);
    if (head) {
      for (const key of ruleKeys(rule.cssRules!, `${under}${head}|`))
        keys.add(key);
    } else {
      keys.add(under + rule.cssText);
    }
  }
  return keys;
}

/** The plugin's rules the dashboard does not have, as a stylesheet. */
export function newRules(
  rules: ArrayLike<Rule>,
  known: Set<string>,
  under = '',
): string {
  const out: string[] = [];
  for (const rule of Array.from(rules)) {
    const head = group(rule);
    if (head) {
      const inner = newRules(rule.cssRules!, known, `${under}${head}|`);
      if (inner) out.push(`${head} {\n${inner}\n}`);
    } else if (!known.has(under + rule.cssText)) {
      out.push(rule.cssText);
    }
  }
  return out.join('\n');
}
