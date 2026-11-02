import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { Select, SelectItem } from './select';

const RETENTION = { '7': '7 days', '30': '30 days', '90': '90 days' };

const meta = {
  title: 'Components/Select',
  component: Select,
  args: {
    'aria-label': 'Keep events for',
    items: RETENTION,
    placeholder: 'Choose a period',
    onValueChange: fn(),
    children: Object.entries(RETENTION).map(([value, label]) => (
      <SelectItem key={value} value={value}>
        {label}
      </SelectItem>
    )),
  },
  parameters: { layout: 'padded' },
} satisfies Meta<typeof Select>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Every state side by side: empty, chosen, small, invalid, disabled. */
export const States: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="grid w-80 gap-3">
      <Select {...args} />
      <Select {...args} defaultValue="30" />
      <Select {...args} size="sm" defaultValue="7" />
      <Select {...args} invalid />
      <Select {...args} disabled defaultValue="90" />
    </div>
  ),
};

/** Opens from the keyboard, picks with Enter, and reports the value. */
export const PicksFromTheKeyboard: Story = {
  play: async ({ args, canvasElement }) => {
    const trigger = within(canvasElement).getByRole('combobox', {
      name: 'Keep events for',
    });
    trigger.focus();
    await userEvent.keyboard('{Enter}');
    const option = await within(document.body).findByRole('option', {
      name: '30 days',
    });
    await userEvent.click(option);
    await waitFor(() => expect(args.onValueChange).toHaveBeenCalledWith('30'));
    await expect(trigger).toHaveTextContent('30 days');
  },
};
