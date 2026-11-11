import { Kbd } from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useTranslations } from 'use-intl';

function Hint({ keys, children }: { keys: string; children: ReactNode }) {
  return (
    <span className="flex items-center gap-1.5">
      <Kbd tone="muted">{keys}</Kbd>
      {children}
    </span>
  );
}

/** The key legend under the list. */
export function CommandBarFooter({
  hasPreview,
  previewOpen,
}: {
  hasPreview: boolean;
  previewOpen: boolean;
}) {
  const t = useTranslations();

  return (
    <div className="flex h-10 shrink-0 items-center gap-4 border-border-divider border-t px-4 text-fg-subtle text-hint">
      <Hint keys="↑↓">{t('commands.navigate')}</Hint>
      <Hint keys="↵">{t('commands.open')}</Hint>

      {hasPreview ? (
        <Hint keys="⇥">
          {previewOpen ? t('commands.hidePages') : t('commands.pages')}
        </Hint>
      ) : null}

      <span className="ms-auto">
        <Hint keys="esc">{t('commands.close')}</Hint>
      </span>
    </div>
  );
}
