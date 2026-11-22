import { createProject, expect, sendError, test, unique } from './support';

// Below 768 px the sidebar gives way to a tab bar: the only way around on a
// phone, so it has to reach an issue.
test.use({
  viewport: { width: 390, height: 844 },
  isMobile: true,
  hasTouch: true,
});

test('on a phone, the tab bar leads to an issue', async ({ page, request }) => {
  const project = await createProject(request);
  const type = unique('PhoneError');
  await sendError(request, project.dsn, { type, value: 'tapped' });

  await page.goto('/projects');
  await expect(
    page.getByRole('navigation', { name: 'Main navigation' }),
  ).toBeHidden();
  await page
    .getByRole('navigation', { name: 'Tabs' })
    .getByRole('link', { name: 'Issues' })
    .tap();

  await expect(page).toHaveURL(/\/issues$/);
  await page.getByRole('link', { name: `${type}: tapped` }).tap();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(type);
});

// #579: a settings header with actions squeezed its title to "S..." and its
// line to one word per row. The actions have to wrap under it instead.
test('on a phone, a settings header keeps its title whole', async ({
  page,
}) => {
  await page.goto('/settings/storage');
  const title = page.getByRole('heading', { level: 1, name: 'Storage' });
  const action = page.getByRole('button', { name: 'Configure retention' });
  await expect(action).toBeVisible();

  // Not truncated: the heading is as wide as its text.
  expect(
    await title.evaluate((h) => h.scrollWidth <= h.clientWidth),
  ).toBeTruthy();
  // The actions sit below the title, not beside it.
  const t = await title.boundingBox();
  const a = await action.boundingBox();
  expect(a!.y).toBeGreaterThanOrEqual(t!.y + t!.height);
});
