import type { Product, ProjectResponse } from '@rustrak/client';
import {
  AgentsIcon,
  ChevronDownIcon,
  ChevronUpIcon,
  CronsIcon,
  cn,
  DragIcon,
  FeedbackIcon,
  focusRing,
  type IconComponent,
  IssuesIcon,
  interactiveTransition,
  LogsIcon,
  PerformanceIcon,
  ReleasesIcon,
  Switch,
  Text,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { projectMutations } from '@/features/project/api/mutations';
import {
  keepsProduct,
  LOCKED_PRODUCT,
  orderedProducts,
  PRODUCTS,
} from '@/features/project/model/products';
import { SettingSection } from '@/shared/ui/components/setting-row';

const ICONS: Record<Product, IconComponent> = {
  issues: IssuesIcon,
  feedback: FeedbackIcon,
  releases: ReleasesIcon,
  performance: PerformanceIcon,
  logs: LogsIcon,
  crons: CronsIcon,
  agents: AgentsIcon,
};

const GROUP_OF = new Map(PRODUCTS.map((p) => [p.key, p.group]));

interface ProductsState {
  order: Product[];
  disabled: Product[];
}

/**
 * Which products a project keeps, and the order its sidebar lists them in.
 *
 * Every change is saved the moment it is made -- a switch is the setting,
 * not a draft -- and shown at once: the request runs behind the new state and
 * puts the old one back if the server refuses it. A saved change refreshes
 * the project, so the sidebar follows without a reload.
 *
 * Reordering works by dragging a row, and from the keyboard with the arrows
 * beside each one: a list that only reorders by drag is a list a keyboard
 * cannot reorder at all.
 */
export function ProductsSettings({
  project,
  readOnly = false,
}: {
  project: ProjectResponse;
  /** Without `project:update`: shown, not changeable. */
  readOnly?: boolean;
}) {
  const t = useTranslations('settings');
  const tNav = useTranslations('projects');
  const update = useMutation(projectMutations.update(project.id));
  const [state, setState] = useState<ProductsState>({
    // Grouped as the sidebar shows them (a stable sort keeps the saved order
    // inside each group), so an order saved interleaved through the API still
    // reorders from the keyboard.
    order: orderedProducts(project.product_order).sort(
      (a, b) =>
        Number(GROUP_OF.get(a) === 'monitor') -
        Number(GROUP_OF.get(b) === 'monitor'),
    ),
    disabled: project.disabled_products,
  });
  // The row being dragged, and the state from before the drag began: the
  // list reorders live under the pointer, and is saved once, on drop.
  const [drag, setDrag] = useState<{
    product: Product;
    before: ProductsState;
  } | null>(null);
  // One save at a time: each sends both lists whole, so two in flight could
  // land out of order and the older one would win.
  // Read-only rides on the same flag: nothing moves or toggles.
  const saving = update.isPending || readOnly;

  const save = (next: ProductsState, previous = state) => {
    setState(next);
    update.mutate(
      { product_order: next.order, disabled_products: next.disabled },
      {
        onError: (error) => {
          setState(previous);
          toast.error(t('products.saveFailed'), {
            description: error.error.message,
          });
        },
      },
    );
  };

  const toggle = (product: Product, on: boolean) =>
    save({
      ...state,
      disabled: on
        ? state.disabled.filter((p) => p !== product)
        : [...state.disabled, product],
    });

  const reorder = (product: Product, to: number) => {
    const order = state.order.filter((p) => p !== product);
    order.splice(Math.max(0, Math.min(to, order.length)), 0, product);
    return order;
  };

  // A product moves within its sidebar group: the sidebar lists Triage and
  // Monitor under their own headings, so an order across them could not show.
  const sameGroup = (a: Product, b: Product | undefined) =>
    b !== undefined && GROUP_OF.get(a) === GROUP_OF.get(b);

  const move = (product: Product, to: number) => {
    const order = reorder(product, to);
    if (order.join() !== state.order.join()) save({ ...state, order });
  };

  const performanceOff = state.disabled.includes('performance');

  return (
    <SettingSection
      title={t('products.title')}
      description={t('products.note')}
    >
      <ul>
        {state.order.map((product, index) => {
          const Icon = ICONS[product];
          const name = tNav(`nav.${product}`);
          const locked = product === LOCKED_PRODUCT;
          const on = keepsProduct(state.disabled, product);
          const group =
            GROUP_OF.get(product) === 'triage' ? 'groupTriage' : 'groupMonitor';

          return (
            <li
              key={product}
              draggable={!saving}
              onDragStart={(event) => {
                event.dataTransfer.effectAllowed = 'move';
                setDrag({ product, before: state });
              }}
              onDragOver={(event) => {
                event.preventDefault();
                if (
                  drag &&
                  drag.product !== product &&
                  sameGroup(drag.product, product)
                ) {
                  setState({ ...state, order: reorder(drag.product, index) });
                }
              }}
              onDragEnd={() => {
                if (drag && state.order.join() !== drag.before.order.join()) {
                  save(state, drag.before);
                }
                setDrag(null);
              }}
              className={cn(
                'flex items-center gap-3 border-b border-border-divider px-5 py-3.5 last:border-b-0',
                'hover:bg-surface-hover',
                interactiveTransition,
                drag?.product === product && 'bg-surface-selected',
              )}
            >
              <DragIcon
                size="lg"
                aria-label={t('products.drag', { name })}
                className="shrink-0 cursor-grab text-fg-ghost active:cursor-grabbing"
              />
              <span
                className={cn(
                  'flex size-8 shrink-0 items-center justify-center rounded-lg border border-border-subtle',
                  on ? 'text-fg-secondary' : 'text-fg-ghost',
                  interactiveTransition,
                )}
              >
                <Icon size="xl" />
              </span>
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="flex items-center gap-2">
                  <Text
                    variant="value"
                    className={cn('font-medium', !on && 'text-fg-subtle')}
                  >
                    {name}
                  </Text>
                  <Text variant="hint" tone="meta">
                    {tNav(`nav.${group}`)}
                  </Text>
                </span>
                <Text variant="hint" tone="subtle" truncate>
                  {t(`products.items.${product}`)}
                </Text>
              </div>

              <div className="flex shrink-0 items-center gap-0.5">
                <ReorderButton
                  label={t('products.moveUp', { name })}
                  icon={ChevronUpIcon}
                  disabled={
                    saving || !sameGroup(product, state.order[index - 1])
                  }
                  onClick={() => move(product, index - 1)}
                />
                <ReorderButton
                  label={t('products.moveDown', { name })}
                  icon={ChevronDownIcon}
                  disabled={
                    saving || !sameGroup(product, state.order[index + 1])
                  }
                  onClick={() => move(product, index + 1)}
                />
              </div>

              {locked ? (
                <Text variant="hint" tone="meta" className="w-20 text-end">
                  {t('products.alwaysOn')}
                </Text>
              ) : (
                <span className="flex w-20 justify-end">
                  <Switch
                    aria-label={name}
                    checked={on}
                    // Agents rides on performance: with that off there is
                    // nothing for it to keep, so it waits for performance.
                    disabled={
                      saving || (product === 'agents' && performanceOff)
                    }
                    onCheckedChange={(checked) => toggle(product, checked)}
                  />
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </SettingSection>
  );
}

function ReorderButton({
  label,
  icon: Icon,
  disabled,
  onClick,
}: {
  label: string;
  icon: IconComponent;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        'flex size-control-xs items-center justify-center rounded-md text-fg-meta',
        'hover:bg-surface-active hover:text-fg disabled:opacity-30',
        interactiveTransition,
        focusRing,
      )}
    >
      <Icon size="md" />
    </button>
  );
}
