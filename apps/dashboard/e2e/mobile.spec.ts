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
