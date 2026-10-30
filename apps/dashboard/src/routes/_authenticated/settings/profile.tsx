import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { ChangePasswordDialog } from '@/features/user/ui/components/change-password-dialog';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute('/_authenticated/settings/profile')({
  component: ProfilePage,
});

/** Who the reader is here, and how they sign in. */
function ProfilePage() {
  const t = useTranslations('settings');
  const user = useSessionUser();

  return (
    <>
      <title>{`${t('profile.title')} | Rustrak`}</title>
      <SettingsHeader
        title={t('profile.title')}
        description={t('profile.subtitle')}
      />

      <SettingSection title={t('account.profile')}>
        <SettingRow title={t('account.email')}>
          <p className="text-value text-fg">{user.email}</p>
        </SettingRow>
        <SettingRow title={t('account.role')}>
          <p className="text-value text-fg">
            {user.is_admin ? t('account.administrator') : user.role}
          </p>
        </SettingRow>
        <SettingRow title={t('account.password.title')}>
          <ChangePasswordDialog />
        </SettingRow>
      </SettingSection>
    </>
  );
}
