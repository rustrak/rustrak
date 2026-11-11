import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, waitFor, within } from 'storybook/test';
import { Text } from '../text/text';
import { TimeSeriesChart } from './time-series-chart';

/** The same deterministic wiggle the design mocks used: no clock, no RNG. */
function seed(count: number, low: number, high: number, start: number) {
  const out: number[] = [];
  let x = start;
  for (let i = 0; i < count; i++) {
    x = (x * 9301 + 49297) % 233280;
    out.push(Math.round(low + (x / 233280) * (high - low)));
  }
  return out;
}

const HOURS = Array.from({ length: 24 }, (_, i) => i);
const events = seed(24, 400, 2400, 5);
const users = seed(24, 60, 380, 17);

const DATA = HOURS.map((hour, i) => ({
  hour,
  events: events[i] ?? 0,
  users: users[i] ?? 0,
}));

const adoptionNew = seed(24, 5, 20, 7).map((v, i) => Math.min(96, v + i * 4));
const ADOPTION = HOURS.map((hour, i) => {
  const current = adoptionNew[i] ?? 0;
  return { hour, current, previous: 100 - current };
});

const formatHour = (value: string | number) => `${value}:00`;
const formatCount = (value: number) =>
  value >= 1000 ? `${(value / 1000).toFixed(1)}k` : String(value);

const meta = {
  title: 'Charts/TimeSeries',
  component: TimeSeriesChart,
  parameters: { layout: 'padded' },
} satisfies Meta<typeof TimeSeriesChart>;

export default meta;
type Story = StoryObj;

/** One series needs no legend: the card's title names it. */
export const SingleSeries: Story = {
  render: () => (
    <div className="w-160 rounded-xl border border-border-subtle bg-surface p-4">
      <Text variant="card-title" render={<h3 />} className="pb-3">
        Events, last 24 h
      </Text>
      <TimeSeriesChart
        data={DATA}
        series={[{ key: 'events', label: 'Events' }]}
        xKey="hour"
        height={180}
        formatX={formatHour}
        formatY={formatCount}
        label="Events over the last 24 hours"
      />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // The drawing arrives once the container has measured itself.
    await waitFor(() =>
      expect(
        canvasElement.querySelector('.recharts-area-curve'),
      ).toBeInTheDocument(),
    );
    // One series: the title carries the name, no legend box.
    await expect(canvas.queryByText('Events')).not.toBeInTheDocument();
  },
};

/**
 * One bucket of data, padded with empty buckets, draws no area, so it gets a
 * dot, and counts tick in evenly spaced whole numbers rather than 0.4.
 */
export const SinglePoint: Story = {
  render: () => (
    <div className="w-160 rounded-xl border border-border-subtle bg-surface p-4">
      <TimeSeriesChart
        data={[
          { hour: 2, events: null },
          { hour: 3, events: 3 },
          { hour: 4, events: null },
        ]}
        series={[{ key: 'events', label: 'Events' }]}
        xKey="hour"
        height={180}
        formatX={formatHour}
        integer
        label="Events in one hour"
      />
    </div>
  ),
  play: async ({ canvasElement }) => {
    await waitFor(() =>
      expect(canvasElement.querySelector('.recharts-area-dot')).not.toBeNull(),
    );
    const ticks = () =>
      [
        ...canvasElement.querySelectorAll('.recharts-yAxis-tick-labels text'),
      ].map((tick) => tick.textContent ?? '');
    await waitFor(() => expect(ticks()).toEqual(['0', '1', '2', '3']));
    // The axis is sized to its labels, so none starts left of the chart.
    const left = canvasElement
      .querySelector('.recharts-wrapper')
      ?.getBoundingClientRect().left;
    await waitFor(() =>
      expect(
        [
          ...canvasElement.querySelectorAll('.recharts-yAxis-tick-labels text'),
        ].every((tick) => tick.getBoundingClientRect().left >= (left ?? 0)),
      ).toBe(true),
    );
  },
};

/** Two series overlay; the legend appears on its own, in fixed order. */
export const TwoSeries: Story = {
  render: () => (
    <div className="w-160 rounded-xl border border-border-subtle bg-surface p-4">
      <Text variant="card-title" render={<h3 />} className="pb-3">
        Volume, last 24 h
      </Text>
      <TimeSeriesChart
        data={DATA}
        series={[
          { key: 'events', label: 'Events' },
          { key: 'users', label: 'Affected users' },
        ]}
        xKey="hour"
        height={180}
        formatX={formatHour}
        formatY={formatCount}
        label="Events and affected users over the last 24 hours"
      />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('Events')).toBeInTheDocument();
    await expect(canvas.getByText('Affected users')).toBeInTheDocument();
  },
};

/** Stacked areas: the release-adoption drawing from the redesign. */
export const StackedAdoption: Story = {
  render: () => (
    <div className="w-160 rounded-xl border border-border-subtle bg-surface p-4">
      <Text variant="card-title" render={<h3 />} className="pb-3">
        Release adoption
      </Text>
      <TimeSeriesChart
        data={ADOPTION}
        series={[
          { key: 'previous', label: '2.10.0' },
          { key: 'current', label: '2.11.0' },
        ]}
        xKey="hour"
        height={160}
        stacked
        formatX={formatHour}
        formatY={(value) => `${value} %`}
        yDomain={[0, 100]}
        label="Share of sessions per release over the last 24 hours"
      />
    </div>
  ),
};
