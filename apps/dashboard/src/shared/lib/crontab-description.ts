import cronstrue from 'cronstrue';
import 'cronstrue/locales/es';
import 'cronstrue/locales/fr';
import 'cronstrue/locales/ro';
import 'cronstrue/locales/zh_CN';

/**
 * Only the words for a crontab live here. Whether it is valid and when it
 * runs come from the server (`schedules.preview`), whose parser is the one
 * that runs it: a second parser in the browser could only disagree with it.
 */

/** cronstrue names Chinese `zh_CN`; the rest match the dashboard's locales. */
const CRONSTRUE_LOCALE: Record<string, string> = { zh: 'zh_CN' };

/** "At 03:00", in the reader's language, or null when cronstrue can't read it. */
export function describeCrontab(value: string, locale: string): string | null {
  if (!value.trim()) return null;
  try {
    return cronstrue.toString(value, {
      locale: CRONSTRUE_LOCALE[locale] ?? locale,
      use24HourTimeFormat: true,
      verbose: false,
    });
  } catch {
    return null;
  }
}
