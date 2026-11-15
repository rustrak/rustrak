import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { Button } from '../button/button';
import {
  DocsIcon,
  IssuesIcon,
  LogsIcon,
  SettingsIcon,
  SignOutIcon,
} from '../icon/icon-catalog';
import { Sheet, SheetRow, SheetSection } from './sheet';

const meta = {
  title: 'Components/Sheet',
  component: Sheet,
  parameters: { layout: 'fullscreen' },
  args: {
    open: false,
    onOpenChange: () => {},
    title: 'More',
    closeLabel: 'Close',
    children: null,
  },
} satisfies Meta<typeof Sheet>;

export default meta;
type Story = StoryObj<typeof meta>;

function Demo({ initiallyOpen = false }: { initiallyOpen?: boolean }) {
  const [open, setOpen] = useState(initiallyOpen);
  return (
    <div className="flex h-[640px] items-start p-6">
      <Button onClick={() => setOpen(true)}>Open sheet</Button>
      <Sheet open={open} onOpenChange={setOpen} title="More" closeLabel="Close">
        <SheetSection label="Monitor">
          <SheetRow icon={IssuesIcon} label="Issues" active />
          <SheetRow icon={LogsIcon} label="Logs" />
        </SheetSection>
        <SheetSection label="Workspace">
          <SheetRow
            icon={SettingsIcon}
            label="Settings"
            detail="Account, workspace and projects"
            chevron
          />
          <SheetRow icon={DocsIcon} label="Docs" selected />
        </SheetSection>
        <SheetSection>
          <SheetRow icon={SignOutIcon} label="Sign out" danger />
        </SheetSection>
      </Sheet>
    </div>
  );
}

/** Every row variant at once: active, with a detail and a chevron, ticked, danger. */
export const States: Story = {
  render: () => <Demo initiallyOpen />,
};

/** Opens as a dialog with its title, and Escape puts focus back on the opener. */
export const KeyboardPath: Story = {
  render: () => <Demo />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const opener = canvas.getByRole('button', { name: 'Open sheet' });
    await userEvent.click(opener);
    const page = within(canvasElement.ownerDocument.body);
    await expect(
      await page.findByRole('dialog', { name: 'More' }),
    ).toBeInTheDocument();
    await expect(
      page.getByRole('button', { name: 'Close' }),
    ).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(opener).toHaveFocus());
  },
};
