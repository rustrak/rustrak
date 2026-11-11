import { useTheme } from 'next-themes';
import { Toaster as Sonner } from 'sonner';
import { useTranslations } from 'use-intl';

export function Toaster() {
  const { resolvedTheme } = useTheme();
  const t = useTranslations('common');

  return (
    <Sonner
      theme={resolvedTheme as 'light' | 'dark' | 'system'}
      position="bottom-right"
      containerAriaLabel={t('notifications')}
      toastOptions={{
        classNames: {
          toast:
            'group toast group-[.toaster]:bg-panel group-[.toaster]:text-fg group-[.toaster]:border-border group-[.toaster]:shadow-lg',
          description: 'group-[.toast]:text-fg-subtle',
          actionButton:
            'group-[.toast]:bg-surface-brand group-[.toast]:text-fg-on-brand',
          cancelButton:
            'group-[.toast]:bg-surface-hover group-[.toast]:text-fg-subtle',
          error:
            'group-[.toaster]:bg-danger group-[.toaster]:text-fg-inverse group-[.toaster]:border-danger',
          success: 'group-[.toaster]:border-success',
        },
      }}
    />
  );
}
