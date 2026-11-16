import { expect, freshContext, newPage, test, unique } from './support';

// The invite link is copied to the clipboard; that copy is what gets shared.
test.use({ permissions: ['clipboard-read', 'clipboard-write'] });

test('an invited member sets a password and is signed in', async ({
  page,
  browser,
}) => {
  const email = `${unique('member')}@e2e.test`;
  await page.goto('/settings/team');
  await page.getByRole('button', { name: 'Invite', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Invite a member' });
  await dialog.getByRole('textbox', { name: 'Email' }).fill(email);
  await dialog.getByRole('button', { name: 'Invite' }).click();

  const copy = page.getByRole('button', {
    name: `Copy invite link for ${email}`,
  });
  await expect(copy).toBeVisible();
  const link = await page.evaluate(() => navigator.clipboard.readText());
  expect(link).toMatch(/\/invite\/\w+$/);

  // The new member, in a browser that has never signed in.
  const member = await freshContext(browser);
  const invited = await newPage(member);
  await invited.goto(link);
  await expect(invited.getByRole('textbox', { name: 'Email' })).toHaveValue(
    email,
  );
  await invited
    .getByRole('textbox', { name: 'Password', exact: true })
    .fill('member-password');
  await invited
    .getByRole('textbox', { name: 'Confirm password' })
    .fill('member-password');
  await invited.getByRole('button', { name: 'Accept & continue' }).click();

  await expect(
    invited.getByRole('button', { name: `Account: ${email}` }),
  ).toBeVisible();

  await page.reload();
  await expect(copy).toBeHidden();
  await expect(page.getByRole('link', { name: email })).toBeVisible();
});
