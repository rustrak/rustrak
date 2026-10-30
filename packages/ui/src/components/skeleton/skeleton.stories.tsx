import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect } from 'storybook/test';
import { Text } from '../text/text';
import { Skeleton } from './skeleton';

const meta = {
  title: 'Components/Skeleton',
  component: Skeleton,
  args: { className: 'h-4 w-48' },
  argTypes: {
    shape: { control: 'inline-radio', options: ['block', 'circle'] },
  },
} satisfies Meta<typeof Skeleton>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Both shapes, at the sizes a page asks for: a title, a line, an avatar. */
export const States: Story = {
  parameters: { controls: { disable: true }, layout: 'padded' },
  render: () => (
    <div className="flex flex-col gap-4">
      {(
        [
          ['title', 'h-7 w-48', 'block'],
          ['line', 'h-4 w-72', 'block'],
          ['avatar', 'size-8', 'circle'],
        ] as const
      ).map(([label, size, shape]) => (
        <div key={label} className="flex flex-col gap-2">
          <Text variant="column" tone="meta">
            {label}
          </Text>
          <Skeleton shape={shape} className={size} />
        </div>
      ))}
    </div>
  ),
};

/** A page's place while it loads: the region announces, the bars do not. */
export const InARegion: Story = {
  parameters: { layout: 'padded' },
  render: () => (
    <div
      role="status"
      aria-label="Loading"
      className="flex w-search flex-col gap-3 rounded-lg border border-border-subtle bg-panel p-4"
    >
      <Skeleton className="h-5 w-40" />
      <Skeleton className="h-4 w-full" />
      <Skeleton className="h-4 w-2/3" />
    </div>
  ),
};

/** Hidden from assistive technology, so a page of them is not read out. */
export const ItIsHiddenFromAssistiveTechnology: Story = {
  play: async ({ canvasElement }) => {
    const bar = canvasElement.querySelector('span[aria-hidden]');

    await expect(bar).toHaveAttribute('aria-hidden', 'true');
  },
};
