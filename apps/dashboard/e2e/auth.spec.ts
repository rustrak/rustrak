import {
  ADMIN,
  adminRequest,
  createProject,
  expect,
  sendError,
  test,
  unique,
} from './support';

// Every test here starts signed out.
test.use({ storageState: { cookies: [], origins: [] } });

test.describe('signing in', () => {
  test('a link to an issue opens that issue once the reader signs in', async ({
    page,
    playwright,
  }) => {
    const admin = await adminRequest(playwright);
    const project = await createProject(admin);
    const type = unique('DeepLinkError');
    const { issueId } = await sendError(admin, project.dsn, {
      type,
      value: 'from an alert email',
    });

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await expect(page).toHaveURL(/\/login$/);

    await page.getByLabel('Email').fill(ADMIN.email);
    await page.getByLabel('Password', { exact: true }).fill(ADMIN.password);
    await page.getByRole('button', { name: 'Log in' }).click();

    await expect(page).toHaveURL(new RegExp(`/issues/${issueId}`));
    await expect(page.getByRole('heading', { level: 1 })).toHaveText(type);
  });

  test('a wrong password is refused without saying which field was wrong', async ({
    page,
  }) => {
    await page.goto('/login');
    await page.getByLabel('Email').fill(ADMIN.email);
    await page.getByLabel('Password', { exact: true }).fill('not-the-password');
    await page.getByRole('button', { name: 'Log in' }).click();

    await expect(page.getByText('Invalid email or password')).toBeVisible();
    await page.goto('/projects');
    await expect(page).toHaveURL(/\/login$/);
  });

  test('signing out ends the session on the server', async ({
    page,
    playwright,
  }) => {
    // Its own session: signing out the shared one would sign out every test.
    await page.request.post('/auth/login', { data: ADMIN });
    await page.goto('/projects');
    // The cookie as it was, to replay after the browser has dropped it.
    const signedIn = await page.context().storageState();

    await page.getByRole('button', { name: `Account: ${ADMIN.email}` }).click();
    await page.getByRole('menuitem', { name: 'Sign out' }).click();
    await expect(page).toHaveURL(/\/login$/);

    const replay = await playwright.request.newContext({
      baseURL: test.info().project.use.baseURL,
      storageState: signedIn,
    });
    expect((await replay.get('/auth/me')).status()).toBe(401);
    await replay.dispose();
  });
});
