import type { AlertIntegration, ProviderType } from '@rustrak/client';
import { Bell, ChevronDown } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import {
  deleteIntegration,
  testIntegration,
} from '@/features/alert/api/mutations';
import { alertProviders } from '@/features/alert/model/providers';
import { ConfirmDeleteDialog } from '@/features/alert/ui/components/confirm-delete-dialog';
import { IntegrationConfigDialog } from '@/features/alert/ui/components/integration-config-dialog/integration-config-dialog';
import { ProviderIcon } from '@/features/alert/ui/components/provider-icon';
import { invalidate, scope } from '@/shared/api/query-client';
import type { Translate } from '@/shared/lib/error-copy';
import { cn } from '@/shared/lib/utils';
import { Badge } from '@/shared/ui/components/shadcn/badge';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/shared/ui/components/shadcn/collapsible';
import { ManageDialog } from './manage-dialog';

// Active alert notification providers
interface IntegrationsListProps {
  initialIntegrations: AlertIntegration[];
}

export function IntegrationsList({
  initialIntegrations,
}: IntegrationsListProps) {
  const t = useTranslations('alerts');
  const [isPending, startTransition] = useTransition();
  const [configureType, setConfigureType] = useState<ProviderType | null>(null);
  const [manageType, setManageType] = useState<ProviderType | null>(null);
  const [editIntegration, setEditIntegration] =
    useState<AlertIntegration | null>(null);
  const [deleteIntegrationItem, setDeleteIntegrationItem] =
    useState<AlertIntegration | null>(null);

  const getIntegrationsByType = (type: ProviderType) =>
    initialIntegrations.filter((c) => c.provider_type === type);

  const handleTest = (
    integration: AlertIntegration,
    routingOverride?: Record<string, unknown>,
  ) => {
    startTransition(async () => {
      const result = await testIntegration(integration.id, routingOverride);

      if (!result.success) {
        // The request itself failed. Distinct from the delivery result below,
        // which is a successful request reporting that the provider refused.
        toast.error(t('integrations.testFailed'), {
          description: result.error.message,
        });
        return;
      }

      if (!result.data.success) {
        toast.error(t('integrations.testFailedResult'), {
          description: result.data.message,
        });
        return;
      }

      // The request reached the endpoint. Whether the endpoint was happy about
      // it is its own business: Rustrak judges delivery by the HTTP status and
      // never interprets the body, so an answer it cannot read must not be
      // dressed as a success. A tick over `{"errcode":93000}` reads as a lie.
      // Neutral when there is something to read, success only when the 2xx is
      // genuinely all there was.
      if (result.data.response_body) {
        toast.info(t('integrations.testAnswered'), {
          description: result.data.response_body,
        });
        return;
      }

      toast.success(t('integrations.testSent'));
    });
  };

  const handleDelete = () => {
    if (!deleteIntegrationItem) return;
    startTransition(async () => {
      const result = await deleteIntegration(deleteIntegrationItem.id);

      if (!result.success) {
        toast.error(t('integrations.deleteFailed'), {
          description: result.error.message,
        });
        return;
      }

      toast.success(t('integrations.deleted'));
      const remaining = initialIntegrations.filter(
        (i) =>
          i.provider_type === deleteIntegrationItem.provider_type &&
          i.id !== deleteIntegrationItem.id,
      );
      if (remaining.length === 0) setManageType(null);
      setDeleteIntegrationItem(null);
      setConfigureType(null);
      setEditIntegration(null);
      void invalidate(scope.integrations);
    });
  };

  const handleCardClick = (type: ProviderType) => {
    const instances = getIntegrationsByType(type);
    if (instances.length >= 1) {
      setManageType(type);
    } else {
      setEditIntegration(null);
      setConfigureType(type);
    }
  };

  const handleEditFromManage = (integration: AlertIntegration) => {
    setManageType(null);
    setEditIntegration(integration);
    setConfigureType(integration.provider_type);
  };

  const handleAddFromManage = (type: ProviderType) => {
    setManageType(null);
    setEditIntegration(null);
    setConfigureType(type);
  };

  const closeConfigure = () => {
    setConfigureType(null);
    setEditIntegration(null);
  };

  const alertConfiguredCount = alertProviders.filter(
    (p) => getIntegrationsByType(p.type).length > 0,
  ).length;

  return (
    <div className="space-y-3">
      <Collapsible defaultOpen className="group/section">
        <CollapsibleTrigger className="w-full">
          <div className="flex items-center justify-between rounded-lg border bg-card px-4 py-3.5 transition-colors hover:bg-accent/50 group-data-open/section:rounded-b-none group-data-open/section:border-b-0">
            <div className="flex items-center gap-3">
              <div className="flex size-8 items-center justify-center rounded-md bg-orange-500/10 text-orange-500">
                <Bell className="size-4" />
              </div>
              <div className="text-left">
                <p className="text-sm font-semibold leading-none">
                  {t('integrations.title')}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t('integrations.description')}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2.5">
              {alertConfiguredCount > 0 && (
                <Badge
                  variant="secondary"
                  className="text-[10px] font-bold uppercase tracking-wider"
                >
                  {t('integrations.configuredCount', {
                    count: alertConfiguredCount,
                  })}
                </Badge>
              )}
              <ChevronDown className="size-4 text-muted-foreground transition-transform duration-200 group-data-open/section:rotate-180" />
            </div>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent className="rounded-b-lg border border-t-0 bg-card/50 p-5">
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {alertProviders.map((providerDef) => (
              <ProviderCard
                key={providerDef.type}
                providerDef={providerDef}
                instances={getIntegrationsByType(providerDef.type)}
                onOpen={handleCardClick}
              />
            ))}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* Manage dialog — shown when 1+ instances exist */}
      {alertProviders.map((providerDef) => {
        const instances = getIntegrationsByType(providerDef.type);
        return (
          <ManageDialog
            key={providerDef.type}
            open={manageType === providerDef.type}
            onOpenChange={(open) => !open && setManageType(null)}
            providerDef={providerDef}
            instances={instances}
            onEdit={handleEditFromManage}
            onAdd={() => handleAddFromManage(providerDef.type)}
            onDelete={(i) => setDeleteIntegrationItem(i)}
          />
        );
      })}

      <IntegrationConfigDialog
        provider={configureType}
        onOpenChange={(open) => !open && closeConfigure()}
        existingIntegration={editIntegration}
        onTest={handleTest}
        onDelete={(integration) => setDeleteIntegrationItem(integration)}
        isPending={isPending}
      />

      <ConfirmDeleteDialog
        open={!!deleteIntegrationItem}
        title={t('integrations.deleteTitle')}
        description={t('integrations.deleteDescription', {
          name: deleteIntegrationItem?.name ?? '',
        })}
        isPending={isPending}
        onConfirm={handleDelete}
        onClose={() => setDeleteIntegrationItem(null)}
      />
    </div>
  );
}

/** What a provider's tile has to say, from the integrations it holds. */
interface ProviderStatus {
  count: number;
  /**
   * The one integration this provider has, or `null`.
   *
   * A tile with several has room for a count and nothing else, so it cannot
   * name one or report whether it is on.
   */
  single: AlertIntegration | null;
  isConnected: boolean;
  isDisabled: boolean;
}

function providerStatus(
  instances: readonly AlertIntegration[],
): ProviderStatus {
  const count = instances.length;
  const single = count === 1 ? (instances[0] ?? null) : null;

  return {
    count,
    single,
    isConnected: !!single && single.is_enabled,
    isDisabled: !!single && !single.is_enabled,
  };
}

function statusLabel(status: ProviderStatus, t: Translate): string {
  if (status.count === 0) return t('integrations.notConfigured');
  if (status.count > 1) {
    return t('integrations.configuredCount', { count: status.count });
  }
  return status.isConnected
    ? t('integrations.connected')
    : t('integrations.disabled');
}

interface ProviderCardProps {
  providerDef: (typeof alertProviders)[number];
  instances: readonly AlertIntegration[];
  onOpen: (type: ProviderType) => void;
}

/** One provider tile: its mark, its state, and the way into its dialog. */
function ProviderCard({ providerDef, instances, onOpen }: ProviderCardProps) {
  const t = useTranslations('alerts');
  const status = providerStatus(instances);
  const open = () => onOpen(providerDef.type);

  return (
    /* Not a <button>: the card holds its own Manage button, and a button
       inside a button is invalid HTML. So it takes the role and the keyboard
       handling a button would have given it for free — without this the card
       was mouse-only and unreachable by Tab. */
    // biome-ignore lint/a11y/useSemanticElements: see above
    <div
      className={cn(
        'group relative bg-card border rounded-lg p-5 flex flex-col justify-between h-48',
        'hover:border-primary/50 transition-all cursor-pointer',
        status.count === 0 && 'opacity-70 hover:opacity-100',
      )}
      role="button"
      tabIndex={0}
      aria-label={t('integrations.configureAria', { name: providerDef.name })}
      onClick={open}
      onKeyDown={(e) => {
        if (e.key !== 'Enter' && e.key !== ' ') return;
        e.preventDefault();
        open();
      }}
    >
      <div className="flex justify-between items-start">
        <div
          className={cn(
            'size-10 rounded flex items-center justify-center text-white',
            providerDef.color,
          )}
        >
          <ProviderIcon type={providerDef.type} className="size-5" />
        </div>

        <div className="flex items-center gap-1.5">
          <Badge
            variant={status.isConnected ? 'default' : 'secondary'}
            className={cn(
              'text-[10px] font-bold uppercase tracking-wider',
              status.isConnected &&
                'bg-green-500/10 text-green-500 border-green-500/20 hover:bg-green-500/20',
              status.isDisabled && 'opacity-60',
            )}
          >
            {statusLabel(status, t)}
          </Badge>
        </div>
      </div>

      <div>
        <h3 className="font-bold text-base">{providerDef.name}</h3>
        <p className="text-xs text-muted-foreground mt-1 line-clamp-1">
          {status.single ? status.single.name : t(providerDef.descriptionKey)}
        </p>
      </div>

      <Button
        variant="outline"
        className="w-full text-xs font-bold uppercase tracking-wide"
        onClick={(e) => {
          e.stopPropagation();
          open();
        }}
      >
        {status.count === 0
          ? t('integrations.configure')
          : t('integrations.manage')}
      </Button>
    </div>
  );
}
