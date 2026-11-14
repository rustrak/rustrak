import type { AuthTokenResponse } from '@rustrak/client';
import { TokensIcon } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { useRelativeTime } from '@/shared/i18n/use-now';
import { SettingsListItem } from '@/shared/ui/components/setting-row';
import { TokenActions } from './token-actions';

export interface TokenListProps {
  tokens: AuthTokenResponse[];
  isBusy: (token: AuthTokenResponse) => boolean;
  onDelete: (token: AuthTokenResponse) => void;
}

/**
 * One row per token: its name, the prefix it can be recognised by, when it
 * was made and last used, and its actions.
 *
 * The row is the same at every width. The facts on the right fold away on a
 * phone, and the created / last-used line under the name carries them there.
 */
export function TokenRows({ tokens, isBusy, onDelete }: TokenListProps) {
  const t = useTranslations('tokens');
  const format = useFormatter();
  const relativeTime = useRelativeTime();

  return tokens.map((token) => {
    const created = t('createdOn', {
      date: format.dateTime(new Date(token.created_at), 'date'),
    });
    // The server already appends "..." to the prefix.
    const prefix = token.token_prefix.replace(/\.+$/, '');
    const expired =
      token.expires_at !== null && new Date(token.expires_at) <= new Date();
    const scope = [
      token.permissions === null
        ? t('scope.allPermissions')
        : t('scope.permissions', { count: token.permissions.length }),
      token.project_ids === null
        ? t('scope.allProjects')
        : t('scope.projects', { count: token.project_ids.length }),
      token.expires_at === null
        ? t('scope.noExpiry')
        : expired
          ? t('scope.expired')
          : t('scope.expires', {
              date: format.dateTime(new Date(token.expires_at), 'date'),
            }),
    ].join(' · ');
    const used = token.last_used_at
      ? t('usedOn', { time: relativeTime(new Date(token.last_used_at)) })
      : t('neverUsed');

    return (
      <SettingsListItem
        key={token.id}
        leading={
          <span className="flex size-8 items-center justify-center rounded-lg border border-border-subtle text-fg-muted">
            <TokensIcon size="lg" />
          </span>
        }
        title={
          token.description || (
            <span className="font-mono text-mono">{prefix}…</span>
          )
        }
        description={
          <>
            <span className="font-mono text-mono-sm">{prefix}••••••••</span>
            {' · '}
            {scope}
            <span className="sm:hidden">
              {' · '}
              {used}
            </span>
          </>
        }
        meta={
          <>
            <span>{created}</span>
            <span>{used}</span>
          </>
        }
        actions={
          <TokenActions
            token={token}
            busy={isBusy(token)}
            onDelete={onDelete}
          />
        }
      />
    );
  });
}
