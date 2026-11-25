import { definePage, type PageProps } from '@rustrak/plugin/ui';
import { EventsChart } from '../../../components/events-chart';
import Breakdown from '../breakdown/[by]/page';

export const config = definePage({
  sidebar: { label: 'nav.events', icon: 'notification', rank: 3 },
});

/** Every event the site reported over the window; each opens its details. */
export default function Events({ projectId }: PageProps<'project'>) {
  return (
    <Breakdown projectId={projectId} params={{ by: 'event' }} inSection>
      <EventsChart title="events.overTime" height={120} />
    </Breakdown>
  );
}
