import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { LanguageSelector } from '@/features/user/ui/components/language-selector';
import { TimeZoneSelector } from '@/features/user/ui/components/time-zone-selector';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { ThemeSelector } from '@/shared/ui/components/theme-selector';

export const Route = createFileRoute('/_authenticated/settings/preferences')({
  component: PreferencesPage,
});

/**
 * How Rustrak reads to the reader: language and time zone, saved on the
 * account, and the theme, kept on this device.
 */
function PreferencesPage() {
  const t = useTranslations('settings');

  return (
    <>
      <title>{`${t('preferences.title')} | Rustrak`}</title>
      <SettingsHeader
        title={t('preferences.title')}
        description={t('preferences.subtitle')}
      />

      <SettingSection title={t('account.regional')}>
        <LanguageSelector />
        <TimeZoneSelector />
      </SettingSection>

      <SettingSection title={t('preferences.display')}>
        <SettingRow
          title={t('preferences.theme')}
          description={t('preferences.themeDescription')}
        >
          <ThemeSelector />
        </SettingRow>
      </SettingSection>
    </>
  );
}
