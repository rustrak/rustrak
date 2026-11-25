/**
 * The settings form Rustrak draws for every plugin, from the schema in its
 * manifest: which fields go where, how they are labelled, and what a save
 * sends.
 */
import type { SettingSpec } from '@rustrak/client';

/**
 * The instance shows every field (a project field there is the default for
 * every project); a project shows only its own.
 */
export function fieldsAt(
  specs: SettingSpec[],
  level: 'instance' | 'project',
): SettingSpec[] {
  return level === 'instance'
    ? specs
    : specs.filter((s) => s.scope === 'project');
}

type Messages = { [key: string]: unknown };

function lookup(messages: unknown, path: string[]): string | undefined {
  let at: unknown = messages;
  for (const part of path) {
    if (!at || typeof at !== 'object') return undefined;
    at = (at as Messages)[part];
  }
  return typeof at === 'string' ? at : undefined;
}

/**
 * `settings.<key>.label` (or `.description`) in the plugin's messages: the
 * reader's language, then English. A label falls back to the key itself.
 */
export function settingText(
  messages: Messages,
  locale: string,
  key: string,
  part: 'label',
): string;
export function settingText(
  messages: Messages,
  locale: string,
  key: string,
  part: 'label' | 'description',
): string | undefined;
export function settingText(
  messages: Messages,
  locale: string,
  key: string,
  part: 'label' | 'description',
): string | undefined {
  const path = ['settings', key, part];
  return (
    lookup(messages[locale], path) ??
    lookup(messages.en, path) ??
    (part === 'label' ? key : undefined)
  );
}

/** A form's raw value as the API types it; `''` clears. */
function typed(spec: SettingSpec, raw: unknown): unknown {
  if (raw === '' || raw === undefined) return null;
  return spec.type === 'number' ? Number(raw) : raw;
}

/**
 * What changed between the saved values and the form, typed for the API:
 * an emptied field is `null` (the default comes back), and a secret is only
 * sent when something is typed in it.
 */
export function settingsPatch(
  specs: SettingSpec[],
  saved: Record<string, unknown>,
  draft: Record<string, unknown>,
): Record<string, unknown> {
  const patch: Record<string, unknown> = {};
  for (const spec of specs.filter((s) => s.key in draft)) {
    const raw = draft[spec.key];
    // A secret is never read back: only what is typed is sent.
    if (spec.type === 'secret') {
      if (raw) patch[spec.key] = raw;
      continue;
    }
    const value = typed(spec, raw);
    if (value !== (saved[spec.key] ?? null)) patch[spec.key] = value;
  }
  return patch;
}
