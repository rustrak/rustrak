// A plugin's texts: src/messages/<locale>.json, en.json required. The dashboard
// shows them in the plugin's pages and in what Rustrak draws for it (its
// settings form, its permissions in Roles), so they live beside ui/, not in it.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { paths } from './config.ts';

/**
 * The languages Rustrak's dashboard speaks (`LOCALES` in
 * apps/dashboard/src/shared/i18n/config.ts): a plugin missing one shows its
 * readers English there.
 */
export const LOCALES = ['en', 'zh', 'fr', 'es', 'ro', 'ja'] as const;

/** One warning per language of Rustrak's that `has` lacks. */
export function missingLocales(
  has: (locale: string) => boolean,
  file: (locale: string) => string,
): string[] {
  return LOCALES.filter((l) => !has(l)).map(
    (l) => `${file(l)} is missing: readers in ${l} see English`,
  );
}

export type Messages = { [key: string]: string | Messages };

/** `{ nav: { a: 'x' } }` → `['nav.a']`. */
export function keys(messages: Messages, prefix = ''): string[] {
  return Object.entries(messages).flatMap(([k, v]) =>
    typeof v === 'string' ? [`${prefix}${k}`] : keys(v, `${prefix}${k}.`),
  );
}

/** `src/messages/*.json` by locale; none at all is fine. */
export function readMessages(dir: string): Record<string, Messages> {
  const folder = paths(dir).messages;
  if (!existsSync(folder)) return {};
  const out: Record<string, Messages> = {};
  for (const file of readdirSync(folder)) {
    if (!file.endsWith('.json')) continue;
    const locale = file.slice(0, -'.json'.length);
    try {
      out[locale] = JSON.parse(
        readFileSync(join(folder, file), 'utf8'),
      ) as Messages;
    } catch (e) {
      throw new Error(
        `src/messages/${file} is not valid JSON: ${(e as Error).message}`,
      );
    }
  }
  if (Object.keys(out).length > 0 && !out.en) {
    throw new Error(
      'src/messages/en.json is missing: English is what every other language falls back to',
    );
  }
  return out;
}

/** The keys each language lacks; English stands in for them. */
export function missingKeys(messages: Record<string, Messages>): string[] {
  const known = keys(messages.en ?? {});
  return Object.entries(messages)
    .filter(([locale]) => locale !== 'en')
    .flatMap(([locale, own]) => {
      const has = new Set(keys(own));
      return known
        .filter((key) => !has.has(key))
        .map(
          (key) =>
            `src/messages/${locale}.json is missing ${key} (English is shown)`,
        );
    });
}
