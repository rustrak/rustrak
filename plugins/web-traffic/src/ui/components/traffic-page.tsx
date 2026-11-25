import { PluginViews } from '@rustrak/plugin/ui';
import { Page, PageHeader } from '@rustrak/ui';
import type { ReactNode } from 'react';
import { TrafficFilters } from './traffic-filters';

/**
 * Every page of the report: the dashboard's header with its saved views,
 * fixed above a scrolling page that starts with the filters.
 */
export function TrafficPage({
  title,
  summary,
  filters = true,
  fill = false,
  children,
}: {
  title: ReactNode;
  summary?: ReactNode[];
  /** Off on a page about one visitor: there is nothing to narrow. */
  filters?: boolean;
  /**
   * A list page: the page stays put and its table takes the height left,
   * scrolling inside itself, as every list in Rustrak does.
   */
  fill?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title={title}
        summary={summary}
        actions={filters ? <PluginViews /> : null}
      />
      <Page scroll={!fill}>
        {/* One column that grows with its content: straight in the scroller,
            a panel that clips its overflow would shrink to the viewport. A
            list page instead fills the height, for its table to scroll in. */}
        <div
          className={
            fill ? 'flex min-h-0 flex-1 flex-col gap-4' : 'flex flex-col gap-4'
          }
        >
          {filters ? <TrafficFilters /> : null}
          {children}
        </div>
      </Page>
    </div>
  );
}
