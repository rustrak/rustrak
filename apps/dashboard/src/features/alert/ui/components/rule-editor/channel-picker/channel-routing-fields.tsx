import type { AlertIntegration } from '@rustrak/client';
import { Input, Textarea } from '@rustrak/ui';
import { useId } from 'react';
import { useTranslations } from 'use-intl';
import {
  routesByUrlOverride,
  routingNeedsOf,
} from '@/features/alert/lib/routing';

/**
 * One labelled routing input, so the three below differ only in their control.
 *
 * `children` is a render prop rather than a node because the label has to
 * point at the control by id. These are not form fields — the routing map
 * lives outside the form schema — so nothing else is generating that pairing,
 * and the three labels here were previously attached to nothing at all.
 */
function Row({
  label,
  required,
  hint,
  children,
}: {
  label: string;
  required: boolean;
  hint?: React.ReactNode;
  children: (props: {
    id: string;
    'aria-describedby'?: string;
  }) => React.ReactNode;
}) {
  const t = useTranslations('alerts');
  const id = useId();
  const hintId = `${id}-hint`;

  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-fg-secondary text-meta">
        {label}
        {required ? (
          <span className="text-sev-error-fg"> *</span>
        ) : (
          <span className="text-fg-ghost"> {t('routing.optional')}</span>
        )}
      </label>
      {children({ id, 'aria-describedby': hint ? hintId : undefined })}
      {hint && (
        <p id={hintId} className="text-fg-subtle text-hint">
          {hint}
        </p>
      )}
    </div>
  );
}

/**
 * Where a selected channel is routed, which depends entirely on its provider.
 *
 * Rendered as a block attached under the channel's card rather than inside it,
 * so a channel with three inputs and one with none stay the same height in the
 * list until they are switched on.
 */
export function ChannelRoutingFields({
  integration,
  routing,
  error,
  disabled,
  onChange,
}: {
  integration: AlertIntegration;
  routing: Record<string, string>;
  error?: string;
  disabled: boolean;
  onChange: (field: string, value: string) => void;
}) {
  const t = useTranslations('alerts');
  const { needsChannel, needsRecipients, needsUrl } =
    routingNeedsOf(integration);

  return (
    <div className="mx-3 flex flex-col gap-3 rounded-b-lg border border-border border-t-0 bg-panel px-3 py-3">
      {needsChannel && (
        <Row
          label={t('routing.channelLabel')}
          required
          hint={t.rich('routing.channelHint', {
            code: (chunks) => <code className="font-mono">{chunks}</code>,
          })}
        >
          {(a) => (
            <Input
              {...a}
              placeholder={t('routing.channelPlaceholder')}
              value={routing.channel ?? ''}
              onChange={(e) => onChange('channel', e.target.value)}
              disabled={disabled}
              autoComplete="off"
            />
          )}
        </Row>
      )}

      {needsRecipients && (
        <Row
          label={t('routing.recipientsLabel')}
          required
          hint={t('routing.recipientsHint')}
        >
          {(a) => (
            <Textarea
              {...a}
              placeholder={t('routing.recipientsPlaceholder')}
              value={routing.recipients ?? ''}
              onChange={(e) => onChange('recipients', e.target.value)}
              disabled={disabled}
              autoComplete="off"
            />
          )}
        </Row>
      )}

      {routesByUrlOverride(integration) && (
        <Row
          label={t('routing.overrideUrlLabel')}
          required={needsUrl}
          hint={needsUrl ? undefined : t('routing.overrideUrlHint')}
        >
          {(a) => (
            <Input
              {...a}
              placeholder={t('routing.overrideUrlPlaceholder')}
              value={routing.url ?? ''}
              onChange={(e) => onChange('url', e.target.value)}
              disabled={disabled}
              autoComplete="off"
            />
          )}
        </Row>
      )}

      {error && <p className="text-sev-error-fg text-meta">{error}</p>}
    </div>
  );
}
