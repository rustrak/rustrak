import type { Invitation } from '@rustrak/client';
import { Copy, Mail, X } from 'lucide-react';
import { useTransition } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { revokeInvitation } from '@/features/user/api/mutations';
import { invalidate, scope } from '@/shared/api/query-client';
import { copyToClipboard } from '@/shared/lib/clipboard';
import { type Translate } from '@/shared/lib/error-copy';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';

interface PendingInvitationsProps {
  invitations: Invitation[];
}

/**
 * Put an invite link on the clipboard, or show it when there is no clipboard.
 *
 * Module scope: it reads  and the invitation it is handed, and closes
 * over nothing in the component.
 */
async function handleCopy(invitation: Invitation, t: Translate) {
  const link = `${window.location.origin}/invite/${invitation.token}`;
  if (await copyToClipboard(link)) {
    toast.success(t('toast.linkCopied'));
  } else {
    toast.info(t('toast.copyLink'), { description: link });
  }
}

export function PendingInvitations({ invitations }: PendingInvitationsProps) {
  const t = useTranslations('user');
  const format = useFormatter();
  const [isPending, startTransition] = useTransition();

  const handleRevoke = (invitation: Invitation) => {
    startTransition(async () => {
      const result = await revokeInvitation(invitation.token);
      if (result.success) {
        toast.success(t('toast.revoked'));
        void invalidate(scope.team);
      } else {
        // No form here, so there is no input to attach a `fields` entry to;
        // the message is what the row can show.
        toast.error(t('toast.revokeFailed'), {
          description: result.error.message,
        });
      }
    });
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('pending.title')}</CardTitle>
        <CardDescription>{t('pending.description')}</CardDescription>
      </CardHeader>
      <CardContent>
        {invitations.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-8 text-center">
            <Mail className="size-10 text-muted-foreground/50 mb-3" />
            <p className="text-muted-foreground text-sm">
              {t('pending.empty')}
            </p>
          </div>
        ) : (
          <ul className="space-y-3">
            {invitations.map((invitation) => (
              <li
                key={invitation.token}
                className="flex items-center justify-between gap-3 rounded-lg border p-3"
              >
                <div className="min-w-0 space-y-1">
                  <div className="flex items-center gap-2">
                    <p className="text-sm font-medium truncate">
                      {invitation.email}
                    </p>
                    <Badge
                      variant={
                        invitation.role === 'admin' ? 'default' : 'secondary'
                      }
                    >
                      {invitation.role === 'admin'
                        ? t('roles.admin')
                        : t('roles.member')}
                    </Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t('pending.expires', {
                      date: format.dateTime(
                        new Date(invitation.expires_at),
                        'date',
                      ),
                    })}
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => handleCopy(invitation, t)}
                    aria-label={t('pending.copyAria', {
                      email: invitation.email,
                    })}
                  >
                    <Copy className="size-4" />
                  </Button>
                  <Button
                    variant="destructive"
                    size="icon"
                    onClick={() => handleRevoke(invitation)}
                    disabled={isPending}
                    aria-label={t('pending.revokeAria', {
                      email: invitation.email,
                    })}
                  >
                    <X className="size-4" />
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
