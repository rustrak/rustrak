import type {
  AuthTokenResponse,
  CatalogResource,
  CreateAuthToken,
} from '@rustrak/client';
import { Button, confirm, NewIcon, TokensIcon } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { tokenMutations } from '@/features/token/api/mutations';
import {
  SettingSection,
  SettingsEmpty,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { CreateTokenDialog } from './create-token-dialog';
import { TokenRows } from './token-rows';

interface TokensListProps {
  tokens: AuthTokenResponse[];
  /** `api-tokens:create`: without it only the reader's own existing tokens. */
  canCreate: boolean;
  /** The page's header is drawn here: its action, "New token", is this list's. */
  title: string;
  subtitle: string;
  /** Under the list: where the API these tokens open is documented. */
  footer?: React.ReactNode;
  /** For narrowing a new token: what a permission can name. */
  catalog: CatalogResource[];
  /** For binding a new token to some projects. */
  projects: { id: number; name: string }[];
}

export function TokensList({
  tokens,
  canCreate,
  title,
  subtitle,
  footer,
  catalog,
  projects,
}: TokensListProps) {
  const t = useTranslations('tokens');
  const toastFailure = useToastFailure();
  const creation = useMutation(tokenMutations.create());
  const removal = useMutation(tokenMutations.delete());
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  // The full value, shown once: the creation's answer, until it is closed.
  const newToken = creation.data?.token ?? null;

  const closeCreate = () => {
    setIsCreateOpen(false);
    creation.reset();
  };

  const create = (input: CreateAuthToken) =>
    creation.mutate(input, {
      onError: (error) => toastFailure(t('toasts.createFailed'), error),
      onSuccess: () =>
        toast.success(t('toasts.created'), {
          description: t('toasts.createdHint'),
        }),
    });

  const confirmDelete = async (token: AuthTokenResponse) => {
    const ok = await confirm({
      title: t('deleteDialog.title'),
      description: t('deleteDialog.description'),
      tone: 'danger',
      confirmLabel: t('deleteDialog.confirm'),
      cancelLabel: t('deleteDialog.cancel'),
    });
    if (!ok) return;
    removal.mutate(token.id, {
      onError: (error) => toastFailure(t('toasts.deleteFailed'), error),
      onSuccess: () => toast.success(t('toasts.deleted')),
    });
  };

  const rowProps = {
    tokens,
    isBusy: (token: AuthTokenResponse) =>
      removal.isPending && removal.variables === token.id,
    onDelete: confirmDelete,
  };

  return (
    <>
      <SettingsHeader
        title={title}
        description={subtitle}
        actions={
          canCreate ? (
            <CreateTokenDialog
              open={isCreateOpen}
              onOpenChange={(open) =>
                open ? setIsCreateOpen(true) : closeCreate()
              }
              newToken={newToken}
              isPending={creation.isPending}
              onCreate={create}
              onDone={closeCreate}
              catalog={catalog}
              projects={projects}
            />
          ) : null
        }
      />

      <SettingSection
        title={t('list.title')}
        description={t('list.description')}
      >
        {tokens.length === 0 ? (
          <SettingsEmpty
            icon={<TokensIcon size="2xl" />}
            title={t('empty.title')}
            action={
              canCreate ? (
                <Button icon={NewIcon} onClick={() => setIsCreateOpen(true)}>
                  {t('empty.createFirst')}
                </Button>
              ) : null
            }
          />
        ) : (
          <TokenRows {...rowProps} />
        )}
      </SettingSection>

      {footer}
    </>
  );
}
