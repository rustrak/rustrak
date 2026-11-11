import { Button } from '@rustrak/ui';
import { useSelector } from '@tanstack/react-form';
import type { ReactNode } from 'react';
import { useTranslations } from 'use-intl';
import { useFormContext } from '@/shared/ui/components/app-form';

/**
 * The bar a provider's form ends with, held to the bottom of the page.
 *
 * Sticky because the form is a page now, and a long one -- a custom webhook's
 * body alone is a screen -- so Save has to be reachable wherever the reader
 * has scrolled to, and so does a failure that named no field (`error`, the
 * form's `RootError`). It bleeds over the scroller's padding the way
 * `SettingsHeader` does at the top.
 *
 * Test and Delete are not here: they act on the stored record, not on the
 * draft, and live in their own rows above.
 */
export function ConfigFooter({
  submitLabel,
  isLoading,
  onCancel,
  error,
}: {
  submitLabel: string;
  isLoading: boolean;
  onCancel: () => void;
  error: ReactNode;
}) {
  const t = useTranslations('alerts');
  const form = useFormContext();
  const isDirty = useSelector(form.store, (state) => !state.isDefaultValue);

  return (
    <div className="sticky -bottom-page-gutter z-20 -mx-page-gutter mt-4 -mb-page-gutter flex flex-wrap items-center justify-end gap-x-4 gap-y-2 border-t border-border-divider bg-panel px-page-gutter py-4">
      <div className="me-auto min-w-0">
        {error}
        {isDirty ? (
          <p className="text-meta text-fg-subtle">{t('page.unsaved')}</p>
        ) : null}
      </div>
      <div className="flex gap-2">
        <Button
          type="button"
          variant="secondary"
          onClick={onCancel}
          disabled={isLoading}
        >
          {t('common.cancel')}
        </Button>
        <Button type="submit" variant="primary" loading={isLoading}>
          {submitLabel}
        </Button>
      </div>
    </div>
  );
}
