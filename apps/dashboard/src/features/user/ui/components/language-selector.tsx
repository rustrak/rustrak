import { Select, SelectItem } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useId } from 'react';
import { toast } from 'sonner';
import { useLocale, useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import { intl } from '@/shared/i18n/intl';
import { isLocale, LOCALES } from '@/shared/i18n/routing';
import { SettingRow } from '@/shared/ui/components/setting-row';
import { useRouter } from '@/shared/ui/hooks/use-router';

/**
 * The language control.
 *
 * **Changing the language no longer changes the URL.** It used to replace the
 * locale prefix and navigate; now it writes the preference and asks the server
 * to render the page again. The address bar does not move, which is the point:
 * a link to an issue is a link to that issue, not to that issue in Spanish.
 *
 * The choice is written to `users.language`, so it follows the reader to any
 * browser they log in from. `intl.reload()` is what applies it: it resolves
 * the language again from the row `updatePreferences` just published and
 * swaps the catalogue under the provider, so the page re-renders in place
 * with scroll position, open dialogs and client state intact. The route
 * refresh after it is for what is translated outside React, the tab title
 * each route's `head` builds.
 *
 * **Each language names itself.** `简体中文`, not "Chinese" -- a reader who
 * needs this control is by definition not reading the current language well,
 * and the one string they will recognise is their own. No flags: a language is
 * not a country, and `zh` in particular is spoken across several.
 */
export function LanguageSelector() {
  const t = useTranslations('locale');
  const locale = useLocale();
  const router = useRouter();
  // Pending until the new language is on screen, not only until it is saved.
  const save = useMutation({
    ...userMutations.updatePreferences(),
    onSuccess: async () => {
      await intl.reload();
      // Not to see the write: to rebuild the tab title, which each route's
      // `head` translates outside React.
      await router.refresh();
    },
  });
  const isPending = save.isPending;
  const fieldId = useId();

  const switchTo = (next: string) => {
    if (!isLocale(next) || next === locale) return;
    save.mutate(
      { language: next },
      { onError: () => toast.error(t('saveFailed')) },
    );
  };

  return (
    <SettingRow
      title={t('label')}
      description={t('storedOnAccount')}
      htmlFor={fieldId}
    >
      <Select
        id={fieldId}
        value={locale}
        onValueChange={switchTo}
        // The re-render is a server round trip. Without the pending state a
        // slow response reads as a dead control and invites a second pick.
        disabled={isPending}
        items={Object.fromEntries(LOCALES.map((l) => [l, t(l)]))}
      >
        {LOCALES.map((candidate) => (
          <SelectItem key={candidate} value={candidate}>
            {t(candidate)}
          </SelectItem>
        ))}
      </Select>
    </SettingRow>
  );
}
