import type { AuthToken } from '@rustrak/client';
import { Key, Plus } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import {
  createToken,
  deleteToken,
  getToken,
} from '@/features/token/api/mutations';
import { invalidate, scope } from '@/shared/api/query-client';
import { copyToClipboard } from '@/shared/lib/clipboard';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/shared/ui/components/shadcn/alert-dialog';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import { CreateTokenDialog } from './create-token-dialog';
import { TokenCards, TokenTable } from './token-rows';

interface TokensListProps {
  initialTokens: AuthToken[];
}

export function TokensList({ initialTokens }: TokensListProps) {
  const t = useTranslations('tokens');
  const commonT = useTranslations('common');
  const [isPending, startTransition] = useTransition();
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [newToken, setNewToken] = useState<string | null>(null);

  // Which single token a request is in flight for, so only its own buttons go
  // quiet rather than the whole table.
  const [busyId, setBusyId] = useState<number | null>(null);
  const [pendingDelete, setPendingDelete] = useState<AuthToken | null>(null);

  const closeCreate = () => {
    setIsCreateOpen(false);
    setNewToken(null);
    void invalidate(scope.tokens);
  };

  const create = (description: string) => {
    startTransition(async () => {
      const result = await createToken({
        description: description || undefined,
      });

      if (!result.success) {
        toast.error(t('toasts.createFailed'), {
          description: result.error.message,
        });
        return;
      }

      setNewToken(result.data.token);
      toast.success(t('toasts.created'), {
        description: t('toasts.createdHint'),
      });
    });
  };

  const confirmDelete = () => {
    if (!pendingDelete) return;
    const id = pendingDelete.id;
    setBusyId(id);
    setPendingDelete(null);

    startTransition(async () => {
      const result = await deleteToken(id);

      if (result.success) {
        toast.success(t('toasts.deleted'));
        void invalidate(scope.tokens);
      } else {
        toast.error(t('toasts.deleteFailed'), {
          description: result.error.message,
        });
      }

      setBusyId(null);
    });
  };

  const copy = (token: AuthToken) => {
    setBusyId(token.id);
    startTransition(async () => {
      const result = await getToken(token.id);

      if (!result.success) {
        toast.error(t('toasts.getFailed'), {
          description: result.error.message,
        });
        setBusyId(null);
        return;
      }

      if (await copyToClipboard(result.data.token)) {
        toast.success(t('toasts.copied'));
      } else {
        // No clipboard (an insecure origin, usually). Showing the value is the
        // fallback: it is the one thing the user came here for.
        toast.info(t('toasts.copyTitle'), { description: result.data.token });
      }
      setBusyId(null);
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
    tokens: initialTokens,
    isBusy: (token: AuthToken) => isPending || busyId === token.id,
    onCopy: copy,
    onDelete: setPendingDelete,
  };

  return (
    <div className="space-y-6">
      <CreateTokenDialog
        open={isCreateOpen}
        onOpenChange={(open) => (open ? setIsCreateOpen(true) : closeCreate())}
        newToken={newToken}
        isPending={isPending}
        onCreate={create}
        onDone={closeCreate}
        onCopyToken={copyNewToken}
      />

      {initialTokens.length === 0 ? (
        <Card className="border-dashed">
          <CardContent className="flex flex-col items-center justify-center py-12">
            <Key className="size-12 text-muted-foreground/50 mb-4" />
            <p className="text-muted-foreground mb-4">{t('empty.title')}</p>
            <Button variant="outline" onClick={() => setIsCreateOpen(true)}>
              <Plus className="mr-2 size-4" />
              {t('empty.createFirst')}
            </Button>
          </CardContent>
        </Card>
      ) : (
        <Card>
          <CardHeader>
            <CardTitle>{t('list.title')}</CardTitle>
            <CardDescription>{t('list.description')}</CardDescription>
          </CardHeader>
          <CardContent>
            <TokenCards {...rowProps} />
            <TokenTable {...rowProps} />
          </CardContent>
        </Card>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('deleteDialog.title')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('deleteDialog.description')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isPending}>
              {t('deleteDialog.cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={confirmDelete}
              disabled={isPending}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {isPending
                ? t('deleteDialog.deleting')
                : t('deleteDialog.confirm')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
