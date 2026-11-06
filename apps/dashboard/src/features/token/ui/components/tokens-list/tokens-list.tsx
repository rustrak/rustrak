import type {
  AuthTokenResponse,
  CatalogResource,
  CreateAuthToken,
} from '@rustrak/client';
import {
  Button,
  Dialog,
  DialogClose,
  DialogFooter,
  DialogHeader,
  NewIcon,
  TokensIcon,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { tokenMutations } from '@/features/token/api/mutations';
import { copyToClipboard } from '@/shared/lib/clipboard';
import {
  SettingSection,
  SettingsEmpty,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
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
  const commonT = useTranslations('common');
  const creation = useMutation(tokenMutations.create());
  const removal = useMutation(tokenMutations.delete());
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  // The full value, shown once: the creation's answer, until it is closed.
  const newToken = creation.data?.token ?? null;

  const [pendingDelete, setPendingDelete] = useState<AuthTokenResponse | null>(
    null,
  );

  const closeCreate = () => {
    setIsCreateOpen(false);
    creation.reset();
  };

  const create = (input: CreateAuthToken) =>
    creation.mutate(input, {
      onError: (error) =>
        toast.error(t('toasts.createFailed'), {
          description: error.error.message,
        }),
      onSuccess: () =>
        toast.success(t('toasts.created'), {
          description: t('toasts.createdHint'),
        }),
    });

  const confirmDelete = () => {
    if (!pendingDelete) return;
    const id = pendingDelete.id;
    setPendingDelete(null);
    removal.mutate(id, {
      onError: (error) =>
        toast.error(t('toasts.deleteFailed'), {
          description: error.error.message,
        }),
      onSuccess: () => toast.success(t('toasts.deleted')),
    });
  };

  const copyNewToken = async () => {
    if (!newToken) return false;
    if (await copyToClipboard(newToken)) return true;

    toast.info(commonT('clipboardUnavailable'), {
      description: commonT('clipboardUnavailableHint'),
    });
    return false;
  };

  const rowProps = {
    tokens,
    isBusy: (token: AuthTokenResponse) =>
      removal.isPending && removal.variables === token.id,
    onDelete: setPendingDelete,
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
              onCopyToken={copyNewToken}
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

      <Dialog
        alert
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <DialogHeader
          dismissible={false}
          title={t('deleteDialog.title')}
          description={t('deleteDialog.description')}
        />
        <DialogFooter>
          <DialogClose
            disabled={removal.isPending}
            render={
              <Button variant="secondary">{t('deleteDialog.cancel')}</Button>
            }
          />
          <Button
            variant="danger-primary"
            onClick={confirmDelete}
            disabled={removal.isPending}
          >
            {removal.isPending
              ? t('deleteDialog.deleting')
              : t('deleteDialog.confirm')}
          </Button>
        </DialogFooter>
      </Dialog>
    </>
  );
}
