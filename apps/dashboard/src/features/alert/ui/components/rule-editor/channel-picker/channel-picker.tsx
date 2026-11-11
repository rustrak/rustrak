import type { AlertIntegration } from '@rustrak/client';
import { useState } from 'react';
import { routingNeedsOf } from '@/features/alert/lib/routing';
import type { ChannelRouting } from '@/features/alert/ui/hooks/use-channel-routing';
import { ChannelCard } from './channel-card';
import { ChannelRoutingFields } from './channel-routing-fields';

/**
 * Which integrations this rule sends through, and how each one is routed.
 *
 * The selection lives in the form (`value`, the rule's
 * `selected_integration_ids`, and its `error`); the routing lives in `useChannelRouting`, because what
 * a channel needs depends on its provider and cannot be a fixed schema. This
 * component is where those two meet, which is why selecting a channel writes
 * to both.
 */
export function ChannelPicker({
  integrations,
  routing,
  disabled,
  value: selectedIds,
  onChange,
  error: selectionError,
}: {
  integrations: AlertIntegration[];
  routing: ChannelRouting;
  disabled: boolean;
  value: number[];
  onChange: (ids: number[]) => void;
  error: string | undefined;
}) {
  // Built once per render rather than scanning the array inside the map: the
  // two lists grow together, so a linear lookup per row is quadratic in the
  // number of integrations a project has configured.
  const selected = new Set(selectedIds);
  // A disabled channel the rule had when the editor opened stays pickable,
  // so one deselected by mistake can be picked again before saving.
  const [linked] = useState(() => new Set(selectedIds));

  const toggle = (id: number, selected: boolean) => {
    onChange(
      selected ? [...selectedIds, id] : selectedIds.filter((i) => i !== id),
    );
    routing.select(id, selected);
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-2">
        {/* A disabled channel never delivers, so it cannot be picked, but it
            is shown, marked, so an empty-looking list explains itself; one
            the rule already used stays pickable, so it can be removed. */}
        {integrations.map((integration) => {
          const isSelected = selected.has(integration.id);
          const error = routing.errors[integration.id];
          const { hasRoutingFields } = routingNeedsOf(integration);

          return (
            <div key={integration.id}>
              <ChannelCard
                integration={integration}
                selected={isSelected}
                disabled={
                  disabled ||
                  (!integration.is_enabled && !linked.has(integration.id))
                }
                onSelect={(next) => toggle(integration.id, next)}
              />

              {isSelected && hasRoutingFields && (
                <ChannelRoutingFields
                  integration={integration}
                  routing={routing.routingMap[integration.id] ?? {}}
                  error={error}
                  disabled={disabled}
                  onChange={(field, value) =>
                    routing.setField(integration.id, field, value)
                  }
                />
              )}

              {/* A channel with nothing to route can still fail validation,
                  and with no block underneath it there is nowhere for that
                  message to land. */}
              {isSelected && error && !hasRoutingFields && (
                <p className="mx-3 text-sev-error-fg text-meta">{error}</p>
              )}
            </div>
          );
        })}
      </div>

      {selectionError && (
        <p className="text-sev-error-fg text-meta">{selectionError}</p>
      )}
    </div>
  );
}
