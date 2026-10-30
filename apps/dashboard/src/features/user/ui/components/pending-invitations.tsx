import type { InvitationResponse } from '@rustrak/client';
import {
  Avatar,
  Button,
  CloseIcon,
  CopyIcon,
  EmptyIcon,
  RefreshIcon,
  Tag,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { userMutations } from '@/features/user/api/mutations';
import { copyToClipboard } from '@/shared/lib/clipboard';
import { type Translate } from '@/shared/lib/error-copy';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';

interface PendingInvitationsProps {
  invitations: InvitationResponse[];
  /** Whether any mail transport exists; without one resending can only fail. */
  emailAvailable: boolean;
}

/**
 * Put an invite link on the clipboard, or show it when there is no clipboard.
 *
 * Module scope: it reads  and the invitation it is handed, and closes
 * over nothing in the component.
 */
async function handleCopy(invitation: InvitationResponse, t: Translate) {
  const link = `${window.location.origin}/invite/${invitation.token}`;
  if (await copyToClipboard(link)) {
    toast.success(t('toast.linkCopied'));
  } else {
    toast.info(t('toast.copyLink'), { description: link });
  }
}

export function PendingInvitations({
  invitations,
  emailAvailable,
}: PendingInvitationsProps) {
  const t = useTranslations('user');
  const format = useFormatter();
  const revoke = useMutation(userMutations.revokeInvitation());
  const resend = useMutation(userMutations.resendInvitation());
  const isPending = revoke.isPending;

  const handleRevoke = (invitation: InvitationResponse) => {
    revoke.mutate(invitation.token, {
      onSuccess: () => toast.success(t('toast.revoked')),
      // No form here, so there is no input to attach a `fields` entry to;
      // the message is what the row can show.
      onError: (error) =>
        toast.error(t('toast.revokeFailed'), {
          description: error.error.message,
        }),
    });
  };

  /**
   * Re-deliver the invite email. The button only exists on rows with no
   * delivered email (`email_sent_at` null) — the server would demand
   * `force` for a delivered one, and this state is what "send it again"
   * means without asking anyone. It stays disabled while no transport is
   * configured: a click could only ever come back as a server error.
   */
  const handleResend = (invitation: InvitationResponse) => {
    resend.mutate(
      { token: invitation.token },
      {
        onSuccess: () =>
          toast.success(t('toast.resendSent'), {
            description: t('toast.resendSentHint', {
              email: invitation.email,
            }),
          }),
        onError: (error) =>
          toast.error(t('toast.resendFailed'), {
            description: error.error.message,
          }),
      },
    );
  };

  return (
    <SettingSection
      title={t('pending.title')}
      description={t('pending.description')}
    >
      {invitations.length === 0 ? (
        <SettingsEmpty
          icon={<EmptyIcon size="2xl" />}
          title={t('pending.empty')}
        />
      ) : (
        invitations.map((invitation) => (
          <SettingsListItem
            key={invitation.token}
            leading={<Avatar name={invitation.email} />}
            title={
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate">{invitation.email}</span>
                <Tag
                  variant="soft"
                  tone={invitation.role === 'admin' ? 'brand' : 'neutral'}
                >
                  {invitation.role === 'admin'
                    ? t('roles.admin')
                    : t('roles.member')}
                </Tag>
                <Tag
                  variant="soft"
                  tone={invitation.email_sent_at ? 'brand' : 'neutral'}
                >
                  {invitation.email_sent_at
                    ? t('pending.emailSent')
                    : t('pending.emailNotSent')}
                </Tag>
              </span>
            }
            description={t('pending.expires', {
              date: format.dateTime(new Date(invitation.expires_at), 'date'),
            })}
            actions={
              <>
                {!invitation.email_sent_at && (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={RefreshIcon}
                    onClick={() => handleResend(invitation)}
                    disabled={resend.isPending || !emailAvailable}
                    aria-label={t('pending.resendAria', {
                      email: invitation.email,
                    })}
                  />
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  icon={CopyIcon}
                  onClick={() => handleCopy(invitation, t)}
                  aria-label={t('pending.copyAria', {
                    email: invitation.email,
                  })}
                />
                <Button
                  variant="ghost"
                  size="sm"
                  icon={CloseIcon}
                  onClick={() => handleRevoke(invitation)}
                  disabled={isPending}
                  aria-label={t('pending.revokeAria', {
                    email: invitation.email,
                  })}
                />
              </>
            }
          />
        ))
      )}
    </SettingSection>
  );
}
