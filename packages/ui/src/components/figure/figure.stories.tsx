import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, within } from 'storybook/test';
import { FactList, Figure, FigureStrip } from './figure';

const meta = {
  title: 'Components/Figure',
  component: Figure,
  args: { label: 'Events', value: '1,204' },
} satisfies Meta<typeof Figure>;

export default meta;
type Story = StoryObj<typeof meta>;

/** A strip of figures, with and without footnotes, and a list of facts. */
export const States: Story = {
  parameters: { controls: { disable: true }, layout: 'padded' },
  render: () => (
    <div className="flex flex-col gap-6 p-page-gutter">
      <FigureStrip className="grid-cols-3">
        <Figure label="Events" value="1,204" footnote="+12% vs last week" />
        <Figure label="Users" value="87" footnote={null} />
        <Figure
          label="Crash free"
          value="99.2%"
          valueClassName="text-success-fg"
        />
      </FigureStrip>
      <FactList
        className="gap-y-2"
        valueClassName="text-fg"
        facts={[
          ['Release', '1.4.0'],
          ['Environment', 'production'],
        ]}
      />
    </div>
  ),
};

/** Label and value are a term and its description. */
export const ItIsATermAndItsValue: Story = {
  render: (args) => (
    <FigureStrip className="grid-cols-1">
      <Figure {...args} />
    </FigureStrip>
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('term')).toHaveTextContent('Events');
    await expect(canvas.getByRole('definition')).toHaveTextContent('1,204');
  },
};
