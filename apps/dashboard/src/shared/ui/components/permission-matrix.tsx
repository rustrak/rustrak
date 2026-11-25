import type { Permission } from '@rustrak/client';
import { Checkbox } from '@rustrak/ui';
import { useLocale, useTranslations } from 'use-intl';
import {
  ACTIONS,
  type EditorResource,
  hasPermission,
  rowState,
  toggle,
  toggleRow,
} from '@/shared/lib/permission-matrix';

interface PermissionMatrixProps {
  /** One module's resources: the rows. */
  resources: EditorResource[];
  value: Permission[];
  onChange: (next: Permission[]) => void;
  readOnly?: boolean;
}

/**
 * One module's permissions as a grid: a row per resource, a column per
 * action. A cell the resource does not accept is a disabled checkbox, so
 * every row lines up. The row's own checkbox turns the whole row
 * on or off.
 */
export function PermissionMatrix({
  resources,
  value,
  onChange,
  readOnly = false,
}: PermissionMatrixProps) {
  const t = useTranslations('access');
  const locale = useLocale();
  // A plugin's row brings its own texts; Rustrak's come from the messages.
  const own = (texts?: Record<string, string>) => texts?.[locale] ?? texts?.en;
  return (
    <table className="w-full text-body">
      <thead className="sticky top-0 bg-surface">
        <tr className="border-border-subtle border-b text-fg-secondary text-label">
          <th className="px-5 py-2 text-left font-medium">{t('resource')}</th>
          {ACTIONS.map((action) => (
            <th key={action} className="w-24 px-2 py-2 text-center font-medium">
              {t(`actions.${action}`)}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {resources.map((resource) => {
          const state = rowState(value, resource);
          const name = resource.label
            ? (own(resource.label) ?? resource.key)
            : t(`resources.${resource.key}`);
          const help = resource.label
            ? own(resource.help)
            : t(`resourceHelp.${resource.key}`);
          return (
            <tr
              key={resource.key}
              className="border-border-subtle border-b last:border-b-0"
            >
              <th scope="row" className="px-5 py-3 text-left font-normal">
                <span className="flex items-start gap-3">
                  <span className="pt-0.5">
                    <Checkbox
                      aria-label={t('wholeRow', { resource: name })}
                      checked={state === 'all'}
                      indeterminate={state === 'some'}
                      disabled={readOnly}
                      onCheckedChange={() =>
                        onChange(toggleRow(value, resource))
                      }
                    />
                  </span>
                  <span className="flex min-w-0 flex-col gap-0.5">
                    <span className="text-body text-fg">{name}</span>
                    {help ? (
                      <span className="text-fg-subtle text-meta">{help}</span>
                    ) : null}
                  </span>
                </span>
              </th>
              {ACTIONS.map((action) => {
                const applies = resource.actions.includes(action);
                return (
                  <td key={action} className="px-2 py-3 align-middle">
                    <span className="flex justify-center">
                      <Checkbox
                        aria-label={`${name}: ${t(`actions.${action}`)}`}
                        checked={
                          applies && hasPermission(value, resource.key, action)
                        }
                        disabled={readOnly || !applies}
                        onCheckedChange={() =>
                          onChange(toggle(value, resource.key, action))
                        }
                      />
                    </span>
                  </td>
                );
              })}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
