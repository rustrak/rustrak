import { createMember, expect, test, unique } from './support';

// A member of their own, so their default view moves no other test's page.
test('a page reopens on its default view from the logo, but not after the reader picks another order or leaves', async ({
  request,
  browser,
}) => {
  const { page } = await createMember(request, browser, 'admin');
  const view = await page.request.post('/api/views', {
    data: { page: 'projects', name: unique('recent'), query: 'sort=recent' },
  });
  expect(view.status()).toBe(201);
  const { id } = await view.json();
  const setDefault = await page.request.put('/api/views/default', {
    data: { page: 'projects', view_id: id, scope: 'user' },
  });
  expect(setDefault.status()).toBe(204);

  await page.goto('/projects');
  await expect(page).toHaveURL(/\/projects\?sort=recent$/);

  // Picking another order on the page is a choice the default must not undo.
  await page.getByRole('button', { name: 'Recent activity' }).click();
  await page.getByRole('menuitem', { name: 'Newest' }).click();
  await expect(page).toHaveURL(/\/projects$/);
  await expect(page.getByRole('button', { name: 'Newest' })).toBeVisible();

  // Coming back in through the logo is a new visit: the default again.
  await page.getByRole('link', { name: 'Rustrak' }).first().click();
  await expect(page).toHaveURL(/\/projects\?sort=recent$/);

  // Leaving by a link goes where it points: the page on its way out must not
  // take that link's arrival for its own and send the reader back.
  await page.getByRole('button', { name: 'Recent activity' }).click();
  await page.getByRole('menuitem', { name: 'Newest' }).click();
  await expect(page).toHaveURL(/\/projects$/);
  await page.getByRole('link', { name: 'Settings' }).first().click();
  await expect(page).toHaveURL(/\/settings/);
});
