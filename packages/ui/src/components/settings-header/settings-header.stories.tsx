import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, within } from 'storybook/test';
import { Button } from '../button/button';
import { SettingsHeader } from './settings-header';

const meta = {
  title: 'Components/SettingsHeader',
  component: SettingsHeader,
  args: {
    title: 'Plugins',
    description:
      'Extend Rustrak with plugins. Each one runs sandboxed, with its own database.',
  },
} satisfies Meta<typeof SettingsHeader>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * Title alone, with its line, and with actions: the three a page uses. Each
 * in its own article, as each sits in its own page: three bare headers in one
 * document would be three banners.
 */
export const States: Story = {
  parameters: { controls: { disable: true }, layout: 'padded' },
  render: () => (
    <div className="flex flex-col gap-10 p-page-gutter">
      <article>
        <SettingsHeader title="Instance" />
      </article>
      <article>
        <SettingsHeader
          title="Storage"
          description="What Rustrak keeps, and for how long."
        />
      </article>
      <article>
        <SettingsHeader
          title="Plugins"
          description="Extend Rustrak with plugins."
          actions={<Button size="sm">Install</Button>}
        />
      </article>
    </div>
  ),
};

/** The title is the page's one heading. */
export const ItIsThePagesHeading: Story = {
  play: async ({ canvasElement }) => {
    const heading = within(canvasElement).getByRole('heading', { level: 1 });
    await expect(heading).toHaveTextContent('Plugins');
  },
};
