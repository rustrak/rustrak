import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, within } from 'storybook/test';
import { Count } from '../count/count';
import { Text } from '../text/text';
import { Tag } from './tag';

const meta = {
  title: 'Components/Tag',
  component: Tag,
  args: { children: 'error', tone: 'error' },
  argTypes: {
    tone: {
      control: 'inline-radio',
      options: ['error', 'warning', 'info', 'brand', 'neutral'],
    },
    variant: { control: 'inline-radio', options: ['text', 'soft'] },
  },
} satisfies Meta<typeof Tag>;

export default meta;
type Story = StoryObj<typeof meta>;

const TONES = ['error', 'warning', 'info', 'brand', 'neutral'] as const;

/**
 * Both variants against every tone, in one frame. `text` and `soft` share a
 * palette and differ only in whether it fills, so a tone that has drifted in
 * one and not the other is invisible on separate pages.
 */
export const States: Story = {
  parameters: { controls: { disable: true }, layout: 'padded' },
  render: () => (
    <div className="flex flex-col gap-4">
      {(['text', 'soft'] as const).map((variant) => (
        <div key={variant} className="flex items-center gap-3">
          <span className="w-20 shrink-0 font-mono text-column text-fg-meta uppercase">
            {variant}
          </span>
          {TONES.map((tone) => (
            <Tag key={tone} variant={variant} tone={tone}>
              {tone}
            </Tag>
          ))}
        </div>
      ))}
    </div>
  ),
};

export const Severity: Story = {
  parameters: { layout: 'padded' },
  render: () => (
    <div className="flex flex-col gap-2">
      <Tag tone="error">fatal</Tag>
      <Tag tone="error">error</Tag>
      <Tag tone="warning">warn</Tag>
      <Tag tone="info">info</Tag>
    </div>
  ),
};

/**
 * How it reads down a column, which is the only test that matters: a filled
 * pill on every row turned severity into confetti, which is why the default is
 * plain coloured text.
 */
export const DownAColumn: Story = {
  parameters: { layout: 'padded' },
  render: () => (
    <div className="w-[520px] rounded-xl border border-border-subtle bg-panel">
      {(
        [
          ['error', 'error', 'TypeError: Cannot read properties of undefined'],
          ['error', 'fatal', 'ConnectionTimeout: pool exhausted after 30000ms'],
          ['error', 'error', 'PaymentDeclined: issuer rejected authorization'],
          ['warning', 'warn', 'ValidationError: coupon code not applicable'],
          ['info', 'info', 'DeprecationWarning: legacy shipping API called'],
        ] as const
      ).map(([tone, label, title]) => (
        <div
          key={title}
          className="flex items-center gap-3 border-b border-border-divider px-4 py-3 last:border-0"
        >
          <span className="w-12 shrink-0">
            <Tag tone={tone}>{label}</Tag>
          </span>
          <Text variant="value" truncate className="flex-1">
            {title}
          </Text>
          <Count>1.204</Count>
        </div>
      ))}
    </div>
  ),
};

/** `soft` is for a label on its own, with nothing nearby to read it against. */
export const Soft: Story = {
  parameters: { layout: 'padded' },
  render: () => (
    <div className="flex items-center gap-3">
      <Tag variant="soft" tone="brand">
        Owner
      </Tag>
      <Tag variant="soft" tone="neutral">
        Admin
      </Tag>
      <Tag variant="soft" tone="warning">
        regression
      </Tag>
      <Tag variant="soft" tone="error">
        crashing
      </Tag>
    </div>
  ),
};

/**
 * A value as it was sent -- a release, an environment -- is drawn verbatim:
 * sentence case is for words, and `web@2.0.0` is not `Web@2.0.0`.
 */
export const Verbatim: Story = {
  parameters: { layout: 'padded' },
  render: () => (
    <div className="flex items-center gap-3">
      <Tag variant="soft" verbatim className="font-mono">
        web@2.0.0
      </Tag>
      <Tag variant="soft" verbatim>
        production
      </Tag>
    </div>
  ),
  play: async ({ canvasElement }) => {
    const value = within(canvasElement).getByText('web@2.0.0');
    await expect(getComputedStyle(value).textTransform).toBe('none');
  },
};
