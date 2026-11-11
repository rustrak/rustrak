import { SegmentedControl, SegmentedItem } from '@rustrak/ui';
import { useTheme } from 'next-themes';
import { useTranslations } from 'use-intl';

/** Light, dark, or whatever the system says, as one segmented control. */
export function ThemeSelector() {
  const { theme, setTheme } = useTheme();
  const t = useTranslations('theme');

  return (
    <SegmentedControl
      aria-label={t('label')}
      value={theme}
      onValueChange={setTheme}
    >
      <SegmentedItem value="light">{t('light')}</SegmentedItem>
      <SegmentedItem value="dark">{t('dark')}</SegmentedItem>
      <SegmentedItem value="system">{t('system')}</SegmentedItem>
    </SegmentedControl>
  );
}
