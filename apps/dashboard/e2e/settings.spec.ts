import {
  createMember,
  createProject,
  expect,
  freshContext,
  newPage,
  sendError,
  test,
  unique,
} from './support';

test.describe('project settings', () => {
  test('a renamed project is listed by its new name', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const name = unique('renamed');

    await page.goto(`/projects/${project.id}/settings/general`);
    const field = page.getByRole('textbox', { name: 'Name' });
    await field.fill(name);
    await field.press('Enter');
    await expect(
      page.getByRole('navigation', { name: 'Breadcrumb' }),
    ).toContainText(name);

    await page.goto('/projects');
    const list = page.getByRole('main');
    await expect(list.getByRole('link', { name, exact: true })).toBeVisible();
    await expect(
      list.getByRole('link', { name: project.name, exact: true }),
    ).toHaveCount(0);
  });

  test('a deleted project is gone and its DSN stops accepting events', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    await sendError(request, project.dsn, { type: 'Before', value: 'removal' });

    await page.goto(`/projects/${project.id}/settings/general`);
    await page.getByRole('button', { name: 'Remove Project' }).click();
    await page
      .getByRole('alertdialog', { name: `Delete "${project.name}"?` })
      .getByRole('button', { name: 'Delete' })
      .click();

    await expect(page).toHaveURL(/\/projects$/);
    await expect(
      page.getByRole('main').getByRole('link', { name: project.name }),
    ).toHaveCount(0);
    const { username: key } = new URL(project.dsn);
    const ingest = await request.post(`/api/${project.id}/store/`, {
      headers: {
        'X-Sentry-Auth': `Sentry sentry_version=7, sentry_key=${key}`,
      },
      data: { message: 'after removal' },
    });
    expect(ingest.ok()).toBe(false);
  });

  test('a member sees a project only once added to it', async ({
    page,
    browser,
    request,
  }) => {
    const project = await createProject(request);
    const member = await createMember(request, browser);
    const role = unique('Viewer');
    const created = await request.post('/api/roles', {
      data: {
        name: role,
        permissions: [{ resource: 'issues', action: 'read' }],
      },
    });
    expect(created.ok()).toBe(true);

    await member.page.goto(`/projects/${project.id}/issues`);
    await expect(member.page.getByRole('heading', { level: 1 })).toHaveText(
      'Page not found',
    );

    await page.goto(`/projects/${project.id}/settings/members`);
    await page.getByRole('button', { name: 'Add Member' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add Member' });
    await dialog
      .getByRole('combobox', { name: 'Select a user to add' })
      .click();
    await page.getByRole('option', { name: new RegExp(member.email) }).click();
    await expect(page.getByRole('listbox')).toBeHidden();
    await dialog.getByRole('combobox', { name: 'Role for new member' }).click();
    await page.getByRole('option', { name: role }).click();
    await expect(page.getByRole('listbox')).toBeHidden();
    await dialog.getByRole('button', { name: 'Add' }).click();
    await expect(dialog).toBeHidden();

    await member.page.reload();
    await expect(member.page.getByRole('heading', { level: 1 })).toHaveText(
      'Issues',
    );
  });
});

test.describe('your account', () => {
  test('an API token authenticates until it is revoked', async ({
    page,
    playwright,
  }) => {
    const name = unique('ci-token');
    await page.goto('/settings/tokens');
    await page.getByRole('button', { name: 'New Token' }).click();
    await page
      .getByRole('dialog', { name: 'Create API Token' })
      .getByRole('textbox', { name: 'Description (optional)' })
      .fill(name);
    await page.getByRole('button', { name: 'Create Token' }).click();

    // Shown once, for the user to copy.
    const created = page.getByRole('dialog', { name: 'Token Created' });
    const token = (await created.getByRole('code').textContent()) ?? '';
    await created.getByRole('button', { name: 'Done' }).click();

    const api = await playwright.request.newContext({
      baseURL: test.info().project.use.baseURL,
      storageState: { cookies: [], origins: [] },
      extraHTTPHeaders: { Authorization: `Bearer ${token}` },
    });
    expect((await api.get('/api/projects')).status()).toBe(200);

    // A token row has no landmark of its own: the smallest block holding its
    // description and an "Actions" button is the row.
    const row = page
      .getByRole('main')
      .locator('div')
      .filter({
        hasText: name,
        has: page.getByRole('button', { name: 'Actions' }),
      })
      .last();
    await row.getByRole('button', { name: 'Actions' }).click();
    await page.getByRole('menuitem', { name: `Delete token ${name}` }).click();
    await page
      .getByRole('alertdialog', { name: 'Delete Token?' })
      .getByRole('button', { name: 'Delete' })
      .click();
    await expect(page.getByRole('alertdialog')).toBeHidden();

    await expect(page.getByText(name)).toHaveCount(0);
    expect((await api.get('/api/projects')).status()).toBe(401);
    await api.dispose();
  });

  test('a changed password replaces the old one', async ({
    browser,
    request,
  }) => {
    const member = await createMember(request, browser);
    const newPassword = 'a-new-password-42';

    await member.page.goto('/settings/profile');
    await member.page.getByRole('button', { name: 'Change password' }).click();
    const dialog = member.page.getByRole('dialog', { name: 'Change password' });
    await dialog
      .getByRole('textbox', { name: 'Current password' })
      .fill(member.password);
    await dialog
      .getByRole('textbox', { name: 'New password', exact: true })
      .fill(newPassword);
    await dialog
      .getByRole('textbox', { name: 'Confirm new password' })
      .fill(newPassword);
    await dialog.getByRole('button', { name: 'Change password' }).click();
    await expect(dialog).toBeHidden();

    // A context of its own, so signing in here leaves the admin's alone.
    const anonymous = await freshContext(browser);
    const login = (password: string) =>
      anonymous.request.post('/auth/login', {
        data: { email: member.email, password },
      });
    expect((await login(member.password)).status()).toBe(401);
    expect((await login(newPassword)).ok()).toBe(true);
    await anonymous.close();
  });

  test('the chosen language is kept on the account', async ({
    browser,
    request,
  }) => {
    // Its own user: the language is saved on the account, not the browser.
    const member = await createMember(request, browser);
    await member.page.goto('/settings/preferences');
    await member.page.getByRole('combobox', { name: 'Language' }).click();
    await member.page.getByRole('option', { name: 'Español' }).click();
    await expect(member.page.getByRole('heading', { level: 1 })).toHaveText(
      'Preferencias',
    );

    // Another browser, signed in fresh: nothing local carries the choice.
    const elsewhere = await freshContext(browser);
    await elsewhere.request.post('/auth/login', {
      data: { email: member.email, password: member.password },
    });
    const page = await newPage(elsewhere);
    await page.goto('/settings/profile');
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Perfil');
  });
});
