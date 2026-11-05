import type { AuthTokenResponse } from '@rustrak/client';
import {
  cn,
  DeleteIcon,
  focusRing,
  interactiveTransition,
  Menu,
  MoreIcon,
} from '@rustrak/ui';
import { useTranslations } from 'use-intl';

/**
 * Delete for one token, behind the row's ⋯ menu. There is no copy: tokens
 * are stored hashed, and the full value exists only when it is created.
 *
 * A token is identified by its description where it has one and by its
 * prefix otherwise, which is why the action carries a label.
 */
export function TokenActions({
  token,
  busy,
  onDelete,
}: {
  token: AuthTokenResponse;
  busy: boolean;
  onDelete: (token: AuthTokenResponse) => void;
}) {
  const t = useTranslations('tokens');
  const label = token.description || token.token_prefix;

  return (
    <Menu
      align="end"
      trigger={
        <button
          type="button"
          disabled={busy}
          aria-label={t('actionsLabel')}
          className={cn(
            'flex size-control-xs items-center justify-center rounded-md text-fg-meta',
            'hover:bg-surface-hover hover:text-fg disabled:opacity-30',
            'data-popup-open:bg-surface-hover',
            interactiveTransition,
            focusRing,
          )}
        >
          <MoreIcon size="lg" />
        </button>
      }
      actions={[
        {
          id: 'delete',
          label: t('deleteTokenLabel', { label }),
          icon: DeleteIcon,
          tone: 'danger',
          disabled: busy,
          onSelect: () => onDelete(token),
        },
      ]}
    />
  );
}
