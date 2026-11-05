import { SecurityIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { SettingsEmpty } from './setting-row';

/**
 * Where a page's content would be, for a reader whose roles do not reach it
 * on this project: said plainly, instead of a load that can only fail.
 */
export function NotAuthorized() {
  const t = useTranslations('settings');
  return (
    <div className="overflow-hidden rounded-2xl border border-border-subtle bg-surface">
      <SettingsEmpty
        icon={<SecurityIcon size="2xl" />}
        title={t('notAuthorized')}
        description={t('projectNotAuthorizedDescription')}
      />
    </div>
  );
}
