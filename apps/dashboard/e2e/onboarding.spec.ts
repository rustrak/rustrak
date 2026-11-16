import { expect, sendError, test, unique } from './support';

// The first thing anyone does with Rustrak, end to end: create a project,
// copy the DSN it shows, point an SDK at it, see the error.
test('a new project hands out a DSN that receives errors', async ({
  page,
  request,
}) => {
  const name = unique('checkout-api');
  await page.goto('/projects/new');
  await page
    .getByRole('button', { name: 'python Python', exact: true })
    .click();
  await page.getByRole('textbox', { name: 'Project name' }).fill(name);
  await page.getByRole('button', { name: 'Create Project' }).click();

  await expect(page).toHaveURL(/\/projects\/\d+\/settings\/client-keys$/);
  await expect(
    page.getByRole('navigation', { name: 'Breadcrumb' }),
  ).toContainText(name);
  await expect(page.getByText('pip install "sentry-sdk"')).toBeVisible();

  // The DSN as the page shows it, not as the API returns it: this is the
  // string a user pastes into their SDK.
  const dsn = await page
    .getByRole('code')
    .filter({ hasText: /^https?:\/\// })
    .textContent();
  const { projectId } = await sendError(request, dsn ?? '', {
    type: 'PaymentDeclined',
    value: 'card expired',
  });
  expect(page.url()).toContain(`/projects/${projectId}/`);

  await page.goto(`/projects/${projectId}/issues`);
  await expect(
    page.getByRole('link', { name: 'PaymentDeclined: card expired' }),
  ).toBeVisible();
});
