import { DialogProvider } from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useTranslations } from 'use-intl';

/** Hosts `confirm()` and `alert()` from @rustrak/ui, in the reader's language. */
export function DialogHost({ children }: { children: ReactNode }) {
  const t = useTranslations('common');
  return <DialogProvider closeLabel={t('close')}>{children}</DialogProvider>;
}
