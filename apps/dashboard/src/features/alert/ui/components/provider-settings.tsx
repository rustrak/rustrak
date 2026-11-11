import type {
  AlertIntegration,
  ProviderType,
  TestChannelResponse,
} from '@rustrak/client';
import {
  BackIcon,
  Button,
  confirm,
  DeleteIcon,
  Menu,
  MoreIcon,
  NewIcon,
  Tag,
  TestIcon,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { alertMutations } from '@/features/alert/api/mutations';
import { providerEditing } from '@/features/alert/lib/provider-view';
import { slackDefaults } from '@/features/alert/model/integration-forms';
import { ProviderForm } from '@/features/alert/ui/components/integration-config-dialog/provider-form';
import { ProviderIcon } from '@/features/alert/ui/components/provider-icon';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

/** What `?edit=` asks for: a new integration, or the one with this id. */
export type EditParam = 'new' | number | undefined;

/**
 * One alert provider's settings page: the integrations configured for it, or
 * the form for one of them -- never both, so the form has the page.
 *
 * Which one is in the URL (`?edit=new`, `?edit=3`) rather than in state, so
 * the browser's back button leaves the form the way it leaves any page, and a
 * link can open an integration directly. With nothing configured yet the form
 * is what the page opens on, since adding the first is the only thing to do.
 */
export function ProviderSettings({
  provider,
  name,
  description,
  integrations,
  edit,
  can,
  onEdit,
  catalogHref,
}: {
  provider: ProviderType;
  name: string;
  description: string;
  integrations: AlertIntegration[];
  edit: EditParam;
  /** What the reader may do to this provider's integrations. */
  can: { create: boolean; update: boolean; delete: boolean };
  /** Moves the page to the list (`undefined`) or to a form. */
  onEdit: (edit: EditParam) => void;
  /** The integrations catalogue, keeping the project it was opened from. */
  catalogHref: string;
}) {
  const t = useTranslations('alerts');
  const toastFailure = useToastFailure();
  const router = useRouter();
  const test = useMutation(alertMutations.testIntegration());
  const remove = useMutation(alertMutations.deleteIntegration());
  const isPending = test.isPending || remove.isPending;

  const editing = providerEditing(edit, integrations, can);

  const handleTest = (
    integration: AlertIntegration,
    routingOverride?: Record<string, unknown>,
  ) =>
    test.mutate(
      { id: integration.id, routingOverride },
      {
        // The request itself failed, which is not the same as the provider
        // refusing the message: that comes back as a successful request.
        onError: (error) => toastFailure(t('integrations.testFailed'), error),
        onSuccess: (answer) => announceTest(answer, t),
      },
    );

  const confirmDelete = async (integration: AlertIntegration) => {
    const ok = await confirm({
      title: t('integrations.deleteTitle'),
      description: t('integrations.deleteDescription', {
        name: integration.name,
      }),
      tone: 'danger',
      confirmLabel: t('common.delete'),
      cancelLabel: t('common.cancel'),
    });
    if (!ok) return;
    remove.mutate(integration.id, {
      onError: (error) => toastFailure(t('integrations.deleteFailed'), error),
      onSuccess: () => {
        toast.success(t('integrations.deleted'));
        if (editing !== null) onEdit(undefined);
      },
    });
  };

  const closeForm = () => {
    // With nothing configured the list is the form again, so the way out of
    // a form nobody filled in is the catalogue.
    if (integrations.length === 0) {
      router.push(catalogHref);
      return;
    }
    onEdit(undefined);
  };

  return (
    <>
      <ProviderHeader
        provider={provider}
        name={name}
        description={description}
        editing={editing}
        hasIntegrations={integrations.length > 0}
        canCreate={can.create}
        onEdit={onEdit}
      />

      {editing !== null ? (
        <ProviderForm
          // A new form per integration, so each seeds its defaults once.
          key={editing === 'new' ? 'new' : editing.id}
          provider={provider}
          existingIntegration={editing === 'new' ? null : editing}
          onOpenChange={(open) => {
            if (!open) closeForm();
          }}
          onTest={handleTest}
          onDelete={can.delete ? confirmDelete : undefined}
          isPending={isPending}
        />
      ) : integrations.length > 0 ? (
        <SettingSection
          title={t('manage.configuredCount', { count: integrations.length })}
        >
          {integrations.map((integration) => (
            <IntegrationRow
              key={integration.id}
              integration={integration}
              provider={provider}
              can={can}
              onConfigure={() => onEdit(integration.id)}
              onTest={() =>
                asksForDestination(provider, integration)
                  ? onEdit(integration.id)
                  : handleTest(integration)
              }
              onDelete={() => confirmDelete(integration)}
            />
          ))}
        </SettingSection>
      ) : (
        <SettingSection title={name}>
          <SettingsEmpty
            icon={<ProviderIcon type={provider} className="size-6" />}
            title={t('integrations.notConfigured')}
            action={
              can.create ? <AddButton name={name} onEdit={onEdit} /> : null
            }
          />
        </SettingSection>
      )}
    </>
  );
}

/**
 * Email and a Slack bot are told where a test goes, and their form is where
 * that is asked: from the list, Test opens it rather than sending nowhere.
 */
function asksForDestination(
  provider: ProviderType,
  integration: AlertIntegration,
): boolean {
  return (
    provider === 'email' ||
    (provider === 'slack' && slackDefaults(integration).method === 'bot_token')
  );
}

/** One configured integration: its state, and the ways into it. */
function IntegrationRow({
  integration,
  provider,
  can,
  onConfigure,
  onTest,
  onDelete,
}: {
  integration: AlertIntegration;
  provider: ProviderType;
  can: { update: boolean; delete: boolean };
  onConfigure: () => void;
  onTest: () => void;
  onDelete: () => void;
}) {
  const t = useTranslations('alerts');
  return (
    <SettingsListItem
      leading={
        <span className="flex size-8 items-center justify-center rounded-lg border border-border-subtle text-fg-secondary">
          <ProviderIcon type={provider} className="size-4" />
        </span>
      }
      title={integration.name}
      meta={
        <Tag variant="soft" tone={integration.is_enabled ? 'brand' : 'neutral'}>
          {integration.is_enabled
            ? t('integrations.connected')
            : t('integrations.disabled')}
        </Tag>
      }
      actions={
        <>
          {can.update ? (
            <Button
              variant="secondary"
              size="sm"
              onClick={onConfigure}
              aria-label={t('integrations.configureAria', {
                name: integration.name,
              })}
            >
              {t('integrations.configure')}
            </Button>
          ) : null}
          {can.update || can.delete ? (
            <Menu
              align="end"
              trigger={
                <Button
                  variant="ghost"
                  size="sm"
                  icon={MoreIcon}
                  aria-label={t('manage.more', {
                    name: integration.name,
                  })}
                />
              }
              actions={[
                ...(can.update
                  ? [
                      {
                        id: 'test',
                        label: t('common.test'),
                        icon: TestIcon,
                        onSelect: onTest,
                      },
                    ]
                  : []),
                ...(can.delete
                  ? [
                      {
                        id: 'delete',
                        label: t('common.delete'),
                        icon: DeleteIcon,
                        tone: 'danger' as const,
                        separated: true,
                        onSelect: onDelete,
                      },
                    ]
                  : []),
              ]}
            />
          ) : null}
        </>
      }
    />
  );
}

/** Says how a test the server carried out went. */
function announceTest(
  answer: TestChannelResponse,
  t: ReturnType<typeof useTranslations<'alerts'>>,
) {
  if (!answer.success) {
    toast.error(t('integrations.testFailedResult'), {
      description: answer.message,
    });
    return;
  }
  // Delivery is judged by the HTTP status and the body is never
  // interpreted, so a body to read is shown as neutral, not as success.
  if (answer.response_body) {
    toast.info(t('integrations.testAnswered'), {
      description: answer.response_body,
    });
    return;
  }
  toast.success(t('integrations.testSent'));
}

function AddButton({
  name,
  onEdit,
}: {
  name: string;
  onEdit: (edit: EditParam) => void;
}) {
  const t = useTranslations('alerts');
  return (
    <Button variant="primary" icon={NewIcon} onClick={() => onEdit('new')}>
      {t('manage.addProvider', { name })}
    </Button>
  );
}

/** The page's title: the provider, or the integration being edited. */
function ProviderHeader({
  provider,
  name,
  description,
  editing,
  hasIntegrations,
  canCreate,
  onEdit,
}: {
  provider: ProviderType;
  name: string;
  description: string;
  editing: AlertIntegration | 'new' | null;
  hasIntegrations: boolean;
  canCreate: boolean;
  onEdit: (edit: EditParam) => void;
}) {
  const t = useTranslations('alerts');
  const title =
    editing === null
      ? name
      : editing === 'new'
        ? t('manage.addProvider', { name })
        : editing.name;

  return (
    <SettingsHeader
      title={
        <span className="flex min-w-0 items-center gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-xl border border-border-subtle text-fg-secondary">
            <ProviderIcon type={provider} className="size-5" />
          </span>
          <span className="truncate">{title}</span>
        </span>
      }
      description={description}
      actions={
        // Nothing to go back to, or to add beside, while the form is the
        // only thing the provider has.
        !hasIntegrations ? null : editing === null ? (
          canCreate ? (
            <AddButton name={name} onEdit={onEdit} />
          ) : null
        ) : (
          <Button
            variant="ghost"
            icon={BackIcon}
            onClick={() => onEdit(undefined)}
          >
            {t('page.back', { name })}
          </Button>
        )
      }
    />
  );
}
