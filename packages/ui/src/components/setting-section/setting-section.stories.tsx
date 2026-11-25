import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';
import { Button } from '../button/button';
import { Input } from '../input/input';
import { Switch } from '../switch/switch';
import {
  SettingRow,
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
  SettingsListSkeleton,
} from './setting-section';

const meta = {
  title: 'Components/SettingSection',
  component: SettingSection,
  args: { title: 'General', children: null },
} satisfies Meta<typeof SettingSection>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * Every body a section holds, and the destructive one, side by side: rows,
 * a list, a list loading, and nothing yet.
 */
export const States: Story = {
  parameters: { controls: { disable: true }, layout: 'padded' },
  render: () => (
    <div className="flex flex-col gap-2 p-page-gutter">
      <SettingSection
        title="General"
        description="How this plugin behaves everywhere."
      >
        <SettingRow
          title="Name"
          description="As it appears in the sidebar."
          htmlFor="name"
        >
          <Input id="name" defaultValue="Analytics" />
        </SettingRow>
        <SettingRow title="Enabled" description="Run it in every project.">
          <Switch aria-label="Enabled" defaultChecked />
        </SettingRow>
      </SettingSection>
      <SettingSection
        title="Installed"
        actions={<Button size="sm">Install</Button>}
      >
        <SettingsListItem
          title="Hello"
          description="The smallest Rustrak plugin."
          meta="0.1.0"
        />
        <SettingsListItem
          title="Analytics"
          description="Web analytics without cookies."
          meta="0.2.0"
        />
      </SettingSection>
      <SettingSection title="Loading">
        <SettingsListSkeleton rows={2} />
      </SettingSection>
      <SettingSection title="Empty">
        <SettingsEmpty
          title="No plugins yet"
          description="Install one from a .rtp file."
        />
      </SettingSection>
      <SettingSection title="Danger zone" destructive>
        <SettingRow
          title="Uninstall"
          description="Removes the plugin and, if you ask, its data."
        >
          <Button size="sm" variant="danger">
            Uninstall
          </Button>
        </SettingRow>
      </SettingSection>
    </div>
  ),
};

/** A row's title points at its control: clicking it focuses the input. */
export const TheTitleLabelsItsControl: Story = {
  render: () => (
    <SettingSection title="General">
      <SettingRow title="Name" htmlFor="plugin-name">
        <Input id="plugin-name" />
      </SettingRow>
    </SettingSection>
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByText('Name'));
    await expect(canvas.getByRole('textbox')).toHaveFocus();
  },
};

/** The section's title is a heading, so a page reads as its groups. */
export const ItsTitleIsAHeading: Story = {
  render: () => (
    <SettingSection title="Danger zone" destructive>
      <SettingRow title="Uninstall">x</SettingRow>
    </SettingSection>
  ),
  play: async ({ canvasElement }) => {
    const heading = within(canvasElement).getByRole('heading', { level: 2 });
    await expect(heading).toHaveTextContent('Danger zone');
  },
};
