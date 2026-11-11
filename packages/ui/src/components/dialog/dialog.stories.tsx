import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { Button } from '../button/button';
import { Text } from '../text/text';
import { alert, confirm } from './confirm';
import {
  Dialog,
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
} from './dialog';
import { DialogProvider } from './dialog-manager';
import { createDialog } from './dialog-store';

const meta = {
  title: 'Components/Dialog',
  component: Dialog,
  parameters: { layout: 'padded' },
  decorators: [
    (Story) => (
      <DialogProvider closeLabel="Close">
        <Story />
      </DialogProvider>
    ),
  ],
} satisfies Meta<typeof Dialog>;

export default meta;
type Story = StoryObj;

/** The box on its own, controlled: header, scrolling body, footer. */
export const Composed: Story = {
  render: function ComposedStory() {
    const [open, setOpen] = useState(false);

    return (
      <>
        <Button variant="secondary" onClick={() => setOpen(true)}>
          Create alert rule
        </Button>
        <Dialog open={open} onOpenChange={setOpen} size="lg">
          <DialogHeader
            title="Create alert rule"
            description="It fires when the conditions hold for five minutes."
          />
          <DialogBody>
            <Text variant="body" tone="secondary">
              The form would live here, scrolling on its own while the header
              and the footer stay put.
            </Text>
          </DialogBody>
          <DialogFooter hint="Esc cancels">
            <DialogClose
              render={<Button variant="secondary" aria-label="Cancel" />}
            >
              Cancel
            </DialogClose>
            <Button variant="primary" onClick={() => setOpen(false)}>
              Create rule
            </Button>
          </DialogFooter>
        </Dialog>
      </>
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);
    const trigger = canvas.getByRole('button', { name: 'Create alert rule' });

    await userEvent.click(trigger);
    const dialog = await body.findByRole('dialog');
    await expect(
      within(dialog).getByText('Create alert rule'),
    ).toBeInTheDocument();

    // Escape closes, and the focus comes home after the exit transition.
    await userEvent.keyboard('{Escape}');
    await waitFor(() =>
      expect(body.queryByRole('dialog')).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(trigger).toHaveFocus());
  },
};

/**
 * `confirm` asks and waits. It answers `true` only when the action is
 * pressed; Escape, like Cancel, is a no.
 */
export const Confirm: Story = {
  render: function ConfirmStory() {
    const [answer, setAnswer] = useState<string>('');

    return (
      <div className="flex items-center gap-3">
        <Button
          variant="danger"
          onClick={async () => {
            const confirmed = await confirm({
              title: 'Delete the RUSTRAK-1042 issue',
              description:
                'Its 4,086 events go with it. Deleting cannot be undone.',
              confirmLabel: 'Delete',
            });
            setAnswer(confirmed ? 'deleted' : 'kept');
          }}
        >
          Delete issue
        </Button>
        {answer ? <span data-testid="answer">{answer}</span> : null}
      </div>
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);

    // Declining: Escape answers no.
    await userEvent.click(canvas.getByRole('button', { name: 'Delete issue' }));
    await body.findByRole('alertdialog');
    await userEvent.keyboard('{Escape}');
    await waitFor(() =>
      expect(canvas.getByTestId('answer')).toHaveTextContent('kept'),
    );

    // Enter is a no too: focus starts on the way out, and the action
    // promises no ⏎ it would not keep.
    await userEvent.click(canvas.getByRole('button', { name: 'Delete issue' }));
    let dialog = await body.findByRole('alertdialog');
    await expect(
      within(dialog).getByRole('button', { name: 'Delete' }),
    ).toBeInTheDocument();
    await userEvent.keyboard('{Enter}');
    await waitFor(() =>
      expect(body.queryByRole('alertdialog')).not.toBeInTheDocument(),
    );
    await expect(canvas.getByTestId('answer')).toHaveTextContent('kept');

    // Accepting: the action, and only the action, answers yes.
    await userEvent.click(canvas.getByRole('button', { name: 'Delete issue' }));
    dialog = await body.findByRole('alertdialog');
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Delete' }),
    );
    await waitFor(() =>
      expect(canvas.getByTestId('answer')).toHaveTextContent('deleted'),
    );
  },
};

/** The irreversible asks to be typed: the phrase is what forces reading. */
export const ConfirmWithPhrase: Story = {
  render: function PhraseStory() {
    const [answer, setAnswer] = useState('');

    return (
      <div className="flex items-center gap-3">
        <Button
          variant="danger"
          onClick={async () => {
            const confirmed = await confirm({
              title: 'Delete the Checkout API project',
              description:
                'Every issue, event and alert rule in it is destroyed.',
              confirmLabel: 'Delete project',
              phrase: 'checkout-api',
            });
            setAnswer(confirmed ? 'deleted' : 'kept');
          }}
        >
          Delete project
        </Button>
        {answer ? <span data-testid="answer">{answer}</span> : null}
      </div>
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);

    await userEvent.click(
      canvas.getByRole('button', { name: 'Delete project' }),
    );
    const dialog = await body.findByRole('alertdialog');
    const action = within(dialog).getByRole('button', {
      name: /Delete project/,
    });

    // Locked until the phrase is typed, and typing it unlocks.
    await expect(action).toBeDisabled();
    await userEvent.click(within(dialog).getByRole('textbox'));
    await userEvent.keyboard('checkout-api');
    await expect(action).toBeEnabled();

    await userEvent.click(action);
    await waitFor(() =>
      expect(canvas.getByTestId('answer')).toHaveTextContent('deleted'),
    );
  },
};

/** `alert` tells something that must be read, and waits to be dismissed. */
export const Alert: Story = {
  render: () => (
    <Button
      variant="secondary"
      onClick={() =>
        alert({
          title: 'Export finished',
          description: '12,403 events written to events.ndjson.',
        })
      }
    >
      Finish export
    </Button>
  ),
};

/*
 * A modal defined once and awaited from anywhere: the manager pattern the
 * dashboard will use for pickers.
 */
const pickRelease = createDialog<Record<string, never>, string>(
  function PickRelease({ close }) {
    return (
      <>
        <DialogHeader
          title="Choose a release"
          description="The regression is marked against it."
        />
        <DialogBody>
          <div className="flex flex-col gap-1 pt-1">
            {['2.11.0', '2.10.0', '2.9.0'].map((release) => (
              <Button
                key={release}
                variant="ghost"
                onClick={() => close(release)}
              >
                {release}
              </Button>
            ))}
          </div>
        </DialogBody>
      </>
    );
  },
  { size: 'sm' },
);

export const AwaitedPicker: Story = {
  render: function PickerStory() {
    const [picked, setPicked] = useState('');

    return (
      <div className="flex items-center gap-3">
        <Button
          variant="secondary"
          onClick={async () => {
            const release = await pickRelease.open();
            setPicked(release ?? 'none');
          }}
        >
          Mark regression
        </Button>
        {picked ? <span data-testid="picked">{picked}</span> : null}
      </div>
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);

    await userEvent.click(
      canvas.getByRole('button', { name: 'Mark regression' }),
    );
    const dialog = await body.findByRole('dialog');
    await userEvent.click(
      within(dialog).getByRole('button', { name: '2.10.0' }),
    );
    await waitFor(() =>
      expect(canvas.getByTestId('picked')).toHaveTextContent('2.10.0'),
    );
  },
};
