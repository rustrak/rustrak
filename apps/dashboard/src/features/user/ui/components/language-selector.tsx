import { useRouter } from '@tanstack/react-router';
import { useId, useTransition } from 'react';
import { toast } from 'sonner';
import { useLocale, useTranslations } from 'use-intl';
import { updatePreferences } from '@/features/user/api/mutations';
import { isLocale, LOCALES } from '@/shared/i18n/routing';
import { Label } from '@/shared/ui/components/shadcn/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';

/**
 * The language control.
 *
 * **Changing the language no longer changes the URL.** It used to replace the
 * locale prefix and navigate; now it writes the preference and asks the server
 * to render the page again. The address bar does not move, which is the point:
 * a link to an issue is a link to that issue, not to that issue in Spanish.
 *
 * The choice is written to `users.language`, so it follows the reader to any
 * browser they log in from. `router.invalidate()` rather than
 * `location.reload()`: it re-runs the loaders and the route `head` titles
 * against the new row and reconciles, so scroll position, open dialogs and
 * client state survive the change.
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
  const [isPending, startTransition] = useTransition();
  const fieldId = useId();

  const switchTo = (next: string | null) => {
    if (!isLocale(next ?? undefined) || next === locale) return;

    startTransition(async () => {
      const result = await updatePreferences({ language: next });
      if (!result.success) {
        toast.error(t('saveFailed'));
        return;
      }
      router.invalidate();
    });
  };

  return (
    <div className="flex flex-col gap-2 sm:max-w-xs">
      <Label htmlFor={fieldId}>{t('label')}</Label>
      <Select
        value={locale}
        onValueChange={switchTo}
        // The re-render is a server round trip. Without the pending state a
        // slow response reads as a dead control and invites a second pick.
        disabled={isPending}
      >
        <SelectTrigger id={fieldId} className="w-full">
          <SelectValue>{(value) => t(String(value))}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {LOCALES.map((candidate) => (
            <SelectItem key={candidate} value={candidate}>
              {t(candidate)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className="text-xs text-muted-foreground">{t('storedOnAccount')}</p>
    </div>
  );
}
