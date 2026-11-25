// A plugin's settings: declared once in rustrak.config.ts, drawn by Rustrak
// as one form for every plugin, and read typed on both sides. What the
// browser gets leaves secrets out; what the server gets has them.

import type { SettingField, Settings } from './config.ts';

/** The manifest's `[[settings]]` tables, in the order they were declared. */
export function settingsManifest(
  settings: Settings,
): Record<string, unknown>[] {
  return Object.entries(settings).map(([key, f]) => {
    const field = f as SettingField & Record<string, unknown>;
    return {
      key,
      type: field.type,
      scope: field.scope ?? 'instance',
      required: field.required ?? false,
      default: field.default,
      options: field.options,
      min: field.min,
      max: field.max,
      integer: field.integer,
    };
  });
}

/** Never empty: `required`, or a default, means the host always has a value. */
const alwaysSet = (f: SettingField) =>
  f.required || ('default' in f && f.default !== undefined);

/** `full-funnel` and `full_funnel` → `FullFunnel`. */
export const pascal = (s: string) =>
  s.replace(/(^|[-_])([a-z0-9])/g, (_, __, c: string) => c.toUpperCase());
const snake = (s: string) => s.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

/** `interface PluginSettings` for the browser: secrets are not there. */
export function settingsTs(settings: Settings): string {
  const lines = Object.entries(settings)
    .filter(([, f]) => f.type !== 'secret')
    .map(([key, f]) => {
      const type =
        f.type === 'number'
          ? 'number'
          : f.type === 'boolean'
            ? 'boolean'
            : f.type === 'select'
              ? f.options.map((o) => `'${o}'`).join(' | ')
              : 'string';
      return `    ${key}: ${type}${alwaysSet(f) ? '' : ' | null'};`;
    });
  return ['  interface PluginSettings {', ...lines, '  }'].join('\n');
}

/** `struct Settings` (and an enum per select) for the server. */
export function settingsRust(settings: Settings): string {
  const enums: string[] = [];
  const fields = Object.entries(settings).map(([key, f]) => {
    let type: string;
    switch (f.type) {
      case 'number':
        type = f.integer ? 'i64' : 'f64';
        break;
      case 'boolean':
        type = 'bool';
        break;
      case 'select': {
        type = pascal(key);
        const variants = f.options.map(
          (o) => `    #[serde(rename = "${o}")]\n    ${pascal(o)},`,
        );
        enums.push(
          `#[derive(Debug, Clone, Copy, PartialEq, Eq, ::rustrak_plugin::serde::Deserialize)]\n#[serde(crate = "::rustrak_plugin::serde")]\npub enum ${type} {\n${variants.join('\n')}\n}`,
        );
        break;
      }
      default:
        type = 'String';
    }
    if (!alwaysSet(f)) type = `Option<${type}>`;
    return `    pub ${snake(key)}: ${type},`;
  });
  const body = fields.length ? `{\n${fields.join('\n')}\n}` : '{}';
  return [
    "/// The plugin's settings: the project's value, then the instance's, then",
    '/// the default. `Settings::get(ctx)?` in a route.',
    '#[derive(Debug, Clone, ::rustrak_plugin::serde::Deserialize)]',
    '#[serde(crate = "::rustrak_plugin::serde", rename_all = "camelCase")]',
    `pub struct Settings ${body}`,
    '',
    'impl Settings {',
    '    pub fn get(ctx: &::rustrak_plugin::Ctx) -> ::rustrak_plugin::Result<Self> {',
    '        ctx.settings()',
    '    }',
    '}',
    ...enums.map((e) => `\n${e}`),
    '',
  ].join('\n');
}
