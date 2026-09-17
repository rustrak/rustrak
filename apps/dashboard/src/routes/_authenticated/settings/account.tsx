import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { LanguageSelector } from '@/features/user/ui/components/language-selector';
import { TimeZoneSelector } from '@/features/user/ui/components/time-zone-selector';
import { translator } from '@/shared/i18n/intl';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import { Label } from '@/shared/ui/components/shadcn/label';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute('/_authenticated/settings/account')({
  head: () => {
    const t = translator('settings');
    return {
      meta: [
        { title: t('account.meta.title') },
        { name: 'description', content: t('account.meta.description') },
      ],
    };
  },
  component: AccountPage,
});

function AccountPage() {
  const t = useTranslations('settings');
  // The page is nothing but a read of the current user, and the gate above
  // already made it. Both failure branches it used to carry — redirect on
  // `anonymous`, render the outage on `unavailable` — are that gate's now.
  const user = useSessionUser();

  return (
    <>
      <div className="mb-6 md:mb-8">
        <h1 className="text-xl md:text-2xl font-extrabold tracking-tight">
          {t('account.title')}
        </h1>
        <p className="text-muted-foreground mt-1">{t('account.subtitle')}</p>
      </div>

      <div className="space-y-6">
        <Card>
          <CardHeader>
            <CardTitle>{t('account.profile')}</CardTitle>
            <CardDescription>{t('account.profileDescription')}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label className="text-muted-foreground">
                {t('account.email')}
              </Label>
              <p className="text-sm font-medium">{user.email}</p>
            </div>
            {user.is_admin && (
              <div className="space-y-2">
                <Label className="text-muted-foreground">
                  {t('account.role')}
                </Label>
                <p className="text-sm font-medium">
                  {t('account.administrator')}
                </p>
              </div>
            )}
          </CardContent>
        </Card>

        {/* The home for everything that is "how I want this read to me" rather
            than "who I am": the language, and the zone every timestamp is
            shown in. One card, not two. */}
        <Card>
          <CardHeader>
            <CardTitle>{t('account.regional')}</CardTitle>
            <CardDescription>
              {t('account.regionalDescription')}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-6">
            <LanguageSelector />
            <TimeZoneSelector />
          </CardContent>
        </Card>
      </div>
    </>
  );
}
