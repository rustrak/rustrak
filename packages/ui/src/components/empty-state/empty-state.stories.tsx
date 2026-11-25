import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, within } from 'storybook/test';
import { EmptyState } from './empty-state';

const meta = {
  title: 'Components/EmptyState',
  component: EmptyState,
  args: {
    title: 'No issues yet',
    description: 'Send an error from your app and it shows up here.',
  },
} satisfies Meta<typeof EmptyState>;

export default meta;
type Story = StoryObj<typeof meta>;

/** One page with nothing in it: the only state it has. */
export const States: Story = {
  parameters: { layout: 'padded' },
};

/** It says what is missing and how to get it, as text a reader hears. */
export const ItSaysWhatIsMissing: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('No issues yet')).toBeInTheDocument();
    await expect(canvas.getByText(/Send an error/)).toBeInTheDocument();
  },
};
