import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, within } from 'storybook/test';
import { Button } from '../button/button';
import { Panel, PanelPlaceholder, RailBlock } from './panel';

const meta = {
  title: 'Components/Panel',
  component: Panel,
  args: { title: 'Error volume', children: null },
} satisfies Meta<typeof Panel>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Plain, with a hint and actions, flush, with a footer, and loading. */
export const States: Story = {
  parameters: { controls: { disable: true }, layout: 'padded' },
  render: () => (
    <div className="grid gap-4 p-page-gutter md:grid-cols-2">
      <Panel title="Issues">
        <p className="text-value text-fg-secondary">12 new this week.</p>
      </Panel>
      <Panel
        title="Error volume"
        hint="Events per hour"
        actions={<Button size="sm">Compare</Button>}
      >
        <PanelPlaceholder height={96} />
      </Panel>
      <Panel title="Latest" flush footer="Showing 3 of 40">
        <ul>
          {['/home', '/pricing', '/docs'].map((path) => (
            <li
              key={path}
              className="border-border-divider border-b px-5 py-2 text-value text-fg last:border-b-0"
            >
              {path}
            </li>
          ))}
        </ul>
      </Panel>
      <div className="rounded-2xl border border-border-subtle bg-surface">
        <RailBlock title="Environment">
          <p className="text-value text-fg">production</p>
        </RailBlock>
      </div>
    </div>
  ),
};

/** The title is a heading, so a page of panels can be read by headings. */
export const ItsTitleIsAHeading: Story = {
  args: { title: 'Error volume', children: 'content' },
  play: async ({ canvasElement }) => {
    const heading = within(canvasElement).getByRole('heading', { level: 2 });
    await expect(heading).toHaveTextContent('Error volume');
  },
};

/** Flush rows run to the panel's edges; the rest keep its gutter. */
export const FlushRunsToTheEdges: Story = {
  args: { title: 'Rows', flush: true, children: <p data-testid="body">row</p> },
  play: async ({ canvasElement }) => {
    const body = within(canvasElement).getByTestId('body').parentElement!;
    await expect(getComputedStyle(body).paddingLeft).toBe('0px');
  },
};
