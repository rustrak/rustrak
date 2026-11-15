import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';
import {
  IssuesIcon,
  MenuIcon,
  OverviewIcon,
  PerformanceIcon,
  SearchIcon,
} from '../icon/icon-catalog';
import { TabBar, TabBarItem } from './tab-bar';

const meta = {
  title: 'Shell/TabBar',
  component: TabBar,
  parameters: { layout: 'fullscreen' },
  args: { children: null },
} satisfies Meta<typeof TabBar>;

export default meta;
type Story = StoryObj<typeof meta>;

/** A phone-width frame, so the bar is seen at the size it is drawn for. */
function Phone({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto flex h-96 w-[390px] flex-col justify-end border border-border-subtle bg-canvas">
      {children}
    </div>
  );
}

/** The project's bar: three places, search and More, Issues active with a badge. */
export const States: Story = {
  render: () => (
    <Phone>
      <TabBar aria-label="Tabs">
        <TabBarItem icon={OverviewIcon} label="Overview" href="#overview" />
        <TabBarItem
          icon={IssuesIcon}
          label="Issues"
          href="#issues"
          active
          badge={12}
        />
        <TabBarItem
          icon={PerformanceIcon}
          label="Performance"
          href="#performance"
          badge={240}
        />
        <TabBarItem
          icon={SearchIcon}
          label="Search"
          render={<button type="button" />}
        />
        <TabBarItem
          icon={MenuIcon}
          label="More"
          render={<button type="button" />}
        />
      </TabBar>
    </Phone>
  ),
};

/** The active tab says so to assistive technology, not only in colour. */
export const MarksTheCurrentPage: Story = {
  render: States.render,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('link', { name: /Issues/ })).toHaveAttribute(
      'aria-current',
      'page',
    );
    await expect(
      canvas.getByRole('link', { name: /Overview/ }),
    ).not.toHaveAttribute('aria-current');
    // A badge past 99 is capped: "there is something", not the figure.
    await expect(canvas.getByText('99+')).toBeInTheDocument();
  },
};

/** Every tab is reachable with Tab, in order, link or button alike. */
export const KeyboardPath: Story = {
  render: States.render,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.tab();
    await expect(canvas.getByRole('link', { name: /Overview/ })).toHaveFocus();
    await userEvent.tab();
    await userEvent.tab();
    await userEvent.tab();
    await expect(canvas.getByRole('button', { name: /Search/ })).toHaveFocus();
  },
};
