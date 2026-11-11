import type { AlertIntegration } from '@rustrak/client';
import { cn, Switch } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { getSlackMethod } from '@/features/alert/lib/routing';
import { ProviderIcon } from '@/features/alert/ui/components/provider-icon';

/**
 * One integration, on or off.
 *
 * Selection moves the border and the icon's tint, never the card's background.
 * A card that fills in when selected competes with the routing block that
 * opens underneath it, and the two together read as one lit slab rather than
 * as a control and its detail.
 */
export function ChannelCard({
  integration,
  selected,
  disabled,
  onSelect,
}: {
  integration: AlertIntegration;
  selected: boolean;
  disabled: boolean;
  onSelect: (selected: boolean) => void;
}) {
  const t = useTranslations('alerts');
  const isSlackBot =
    integration.provider_type === 'slack' &&
    getSlackMethod(integration) === 'bot_token';

  return (
    <label
      className={cn(
        'flex cursor-pointer items-center has-disabled:cursor-not-allowed justify-between gap-3 rounded-lg border bg-surface px-3 py-2.5 transition-colors hover:bg-surface-hover',
        selected ? 'border-border-brand' : 'border-border',
      )}
    >
      <span className="flex min-w-0 items-center gap-2.5">
        <span
          className={cn(
            'flex size-7 shrink-0 items-center justify-center rounded-md transition-colors',
            selected
              ? 'bg-success-surface text-fg-brand'
              : 'bg-surface-chip text-fg-subtle',
          )}
        >
          <ProviderIcon type={integration.provider_type} className="size-3.5" />
        </span>
        <span className="flex min-w-0 flex-col gap-0.5">
          <span className="truncate font-medium text-body text-fg">
            {integration.name}
          </span>
          <span className="truncate text-fg-subtle text-hint capitalize">
            {integration.provider_type.replace('_', ' ')}
            {isSlackBot ? ` · ${t('channelCard.botToken')}` : ''}
            {integration.is_enabled ? '' : ` · ${t('integrations.disabled')}`}
          </span>
          {integration.is_enabled ? null : (
            <span className="text-fg-subtle text-hint">
              {t('channelCard.disabledHint')}
            </span>
          )}
        </span>
      </span>
      <Switch
        checked={selected}
        onCheckedChange={onSelect}
        disabled={disabled}
      />
    </label>
  );
}
