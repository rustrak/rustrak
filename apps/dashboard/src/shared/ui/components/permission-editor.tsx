import type { CatalogResource, Permission } from '@rustrak/client';
import { Count, cn, Tab, TabList, TabPanel, Tabs } from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useTranslations } from 'use-intl';
import { modulesOf } from '@/shared/lib/permission-matrix';
import { PermissionMatrix } from './permission-matrix';

interface PermissionEditorProps {
  catalog: CatalogResource[];
  value: Permission[];
  onChange: (next: Permission[]) => void;
  readOnly?: boolean;
  /** Tabs before the modules, e.g. a role's name or its projects. */
  leading?: { value: string; label: ReactNode; content: ReactNode }[];
  /** Places the tab strip, e.g. inside a page header. */
  header?: (tabs: ReactNode) => ReactNode;
  /** Sizes each module's grid, e.g. a fixed height that scrolls in a dialog. */
  panelClassName?: string;
}

/**
 * Permissions split into one tab per module, each a matrix. The draft lives
 * in the caller, so switching tabs keeps every change until it is saved.
 */
export function PermissionEditor({
  catalog,
  value,
  onChange,
  readOnly,
  leading = [],
  header = (tabs) => tabs,
  panelClassName,
}: PermissionEditorProps) {
  const t = useTranslations('access');
  const modules = modulesOf(catalog);
  const granted = (resources: CatalogResource[]) =>
    value.filter((p) =>
      resources.some(
        (r) => r.key === p.resource && r.actions.includes(p.action),
      ),
    ).length;

  const list = (
    <TabList>
      {leading.map((tab) => (
        <Tab key={tab.value} value={tab.value}>
          {tab.label}
        </Tab>
      ))}
      {modules.map(({ module, resources }) => (
        <Tab key={module} value={module}>
          {t(`modules.${module}`)}
          <Count>{granted(resources)}</Count>
        </Tab>
      ))}
    </TabList>
  );

  return (
    <Tabs defaultValue={leading[0]?.value ?? modules[0]?.module}>
      {header(list)}
      {leading.map((tab) => (
        <TabPanel key={tab.value} value={tab.value}>
          {tab.content}
        </TabPanel>
      ))}
      {modules.map(({ module, resources }) => (
        <TabPanel key={module} value={module}>
          <div
            className={cn(
              'mt-4 overflow-hidden rounded-2xl border border-border-subtle bg-surface',
              panelClassName,
            )}
          >
            <PermissionMatrix
              resources={resources}
              value={value}
              onChange={onChange}
              readOnly={readOnly}
            />
          </div>
        </TabPanel>
      ))}
    </Tabs>
  );
}
