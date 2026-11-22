import { createProject, expect, sendError, test, unique } from './support';

test.describe('issues', () => {
  test('an issue shows the exception and where it was thrown', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    await sendError(request, project.dsn, {
      type: 'KeyError',
      value: "'user_id'",
      fn: 'load_profile',
    });

    await page.goto(`/projects/${project.id}/issues`);
    await page.getByRole('link', { name: "KeyError: 'user_id'" }).click();

    await expect(page.getByRole('heading', { level: 1 })).toHaveText(
      'KeyError',
    );
    const stack = page.getByRole('group').filter({
      has: page.getByRole('heading', { name: 'Stack Trace' }),
    });
    await expect(stack.getByText("'user_id'")).toBeVisible();
    await expect(
      stack.getByRole('button', { name: /load_profile app\.py:42/ }),
    ).toBeVisible();
  });

  test('resolving an issue moves it from Open to Resolved', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const title = 'TimeoutError: upstream took 30s';
    const { issueId } = await sendError(request, project.dsn, {
      type: 'TimeoutError',
      value: 'upstream took 30s',
    });

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await page.getByRole('button', { name: 'Resolve', exact: true }).click();
    await expect(
      page.getByRole('button', { name: /^Unresolve/ }),
    ).toBeVisible();

    // From the server, not the cache the click just wrote.
    await page.goto(`/projects/${project.id}/issues`);
    await expect(page.getByText('No issues found')).toBeVisible();
    await expect(page.getByRole('link', { name: title })).toBeHidden();

    await page.getByRole('tab', { name: 'Resolved' }).click();
    await expect(page.getByRole('link', { name: title })).toBeVisible();
  });

  test('a search narrows the list and survives a reload', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const wanted = unique('Wanted');
    await sendError(request, project.dsn, { type: wanted, value: 'match me' });
    await sendError(request, project.dsn, {
      type: 'OtherError',
      value: 'noise',
    });

    await page.goto(`/projects/${project.id}/issues`);
    await expect(
      page.getByRole('link', { name: 'OtherError: noise' }),
    ).toBeVisible();

    const search = page.getByRole('combobox', { name: /Filter by/ });
    await search.fill(wanted);
    await search.press('Enter');

    for (const _ of ['before reload', 'after reload']) {
      await expect(page).toHaveURL(new RegExp(`[?&]q=${wanted}`));
      await expect(
        page.getByRole('link', { name: `${wanted}: match me` }),
      ).toBeVisible();
      await expect(
        page.getByRole('link', { name: 'OtherError: noise' }),
      ).toBeHidden();
      await page.reload();
    }
  });

  test('the all-projects list shows issues from every project', async ({
    page,
    request,
  }) => {
    const marker = unique('cross');
    const first = await createProject(request);
    const second = await createProject(request);
    await sendError(request, first.dsn, { type: 'FirstError', value: marker });
    await sendError(request, second.dsn, {
      type: 'SecondError',
      value: marker,
    });

    await page.goto(`/issues?q=${marker}`);
    const rows = page.getByRole('row');
    await expect(
      rows.filter({ hasText: `FirstError: ${marker}` }),
    ).toContainText(first.name);
    await expect(
      rows.filter({ hasText: `SecondError: ${marker}` }),
    ).toContainText(second.name);
  });

  test('resolving the selected issues leaves the rest open', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    for (const type of ['FirstBulk', 'SecondBulk', 'KeptOpen']) {
      await sendError(request, project.dsn, { type, value: 'bulk' });
    }

    await page.goto(`/projects/${project.id}/issues`);
    for (const type of ['FirstBulk', 'SecondBulk']) {
      await page
        .getByRole('row')
        .filter({ hasText: `${type}: bulk` })
        .getByRole('checkbox', { name: 'Select row' })
        .check();
    }
    await page
      .getByRole('toolbar', { name: 'Actions for the selected rows' })
      .getByRole('button', { name: 'Resolve' })
      .click();
    // Gone from Open on screen, then from the server after a reload.
    await expect(
      page.getByRole('link', { name: 'FirstBulk: bulk' }),
    ).toBeHidden();

    await page.reload();
    await expect(
      page.getByRole('link', { name: 'KeptOpen: bulk' }),
    ).toBeVisible();
    await expect(
      page.getByRole('link', { name: 'FirstBulk: bulk' }),
    ).toBeHidden();
    await expect(
      page.getByRole('link', { name: 'SecondBulk: bulk' }),
    ).toBeHidden();

    await page.getByRole('tab', { name: 'Resolved' }).click();
    await expect(
      page.getByRole('link', { name: 'FirstBulk: bulk' }),
    ).toBeVisible();
    await expect(
      page.getByRole('link', { name: 'SecondBulk: bulk' }),
    ).toBeVisible();
  });

  test('a deleted issue is gone, not just hidden', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const { issueId } = await sendError(request, project.dsn, {
      type: 'DeleteMe',
      value: 'once',
    });

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await page.getByRole('button', { name: 'Issue actions' }).click();
    await page.getByRole('menuitem', { name: 'Delete issue' }).click();
    await expect(page.getByRole('menu')).toBeHidden();
    await page
      .getByRole('alertdialog')
      .getByRole('button', { name: 'Delete' })
      .click();

    await expect(page).toHaveURL(new RegExp(`/projects/${project.id}/issues$`));
    await page.getByRole('tab', { name: 'All' }).click();
    await expect(
      page.getByRole('link', { name: 'DeleteMe: once' }),
    ).toBeHidden();
    const gone = await request.get(
      `/api/projects/${project.id}/issues/${issueId}`,
    );
    expect(gone.status()).toBe(404);
  });

  test('assigning an issue to yourself shows you as its assignee', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const { issueId } = await sendError(request, project.dsn, {
      type: 'Unowned',
      value: 'needs someone',
    });

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await page.getByRole('button', { name: 'Unassigned' }).click();
    await page.getByRole('menuitem', { name: 'Assign to me' }).click();
    await expect(page.getByRole('button', { name: 'Unassigned' })).toBeHidden();

    await page.goto(`/projects/${project.id}/issues`);
    const row = page
      .getByRole('row')
      .filter({ hasText: 'Unowned: needs someone' });
    await expect(
      row.getByRole('cell', { name: 'Me', exact: true }),
    ).toBeVisible();
  });

  test('a comment is kept in the issue activity', async ({ page, request }) => {
    const project = await createProject(request);
    const { issueId } = await sendError(request, project.dsn, {
      type: 'Discussed',
      value: 'needs context',
    });
    const comment = unique('Rolled back the deploy');

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await page
      .getByRole('textbox', { name: 'Leave a comment...' })
      .fill(comment);
    await page.getByRole('button', { name: 'Comment' }).click();
    const activity = page.getByRole('complementary', { name: 'Details' });
    await expect(activity.getByText(comment)).toBeVisible();

    await page.reload();
    await expect(activity.getByText(comment)).toBeVisible();
  });

  test('every event of an issue can be read, not only the latest', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const error = { type: 'Recurring', value: 'twice' };
    const { issueId } = await sendError(request, project.dsn, {
      ...error,
      user: 'first-user',
    });
    await sendError(request, project.dsn, { ...error, user: 'second-user' });

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await expect(page.getByText('Event 2 of 2')).toBeVisible();
    await expect(page.getByText('second-user').first()).toBeVisible();

    await page.getByRole('button', { name: 'Previous event' }).click();
    await expect(page.getByText('Event 1 of 2')).toBeVisible();
    await expect(page.getByText('first-user').first()).toBeVisible();
    await expect(page.getByText('second-user')).toHaveCount(0);
  });
  test('resolved in the current release, only that release or a later one reopens it', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const error = { type: 'TenantError', value: unique('staggered') };
    await sendError(request, project.dsn, { ...error, release: 'app@1.0.0' });
    const { issueId } = await sendError(request, project.dsn, {
      ...error,
      release: 'app@2.0.0',
    });

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await page.getByRole('button', { name: 'More resolve options' }).click();
    await page
      .getByRole('menuitem', { name: 'Resolve in current release' })
      .click();
    await expect(page.getByText('In app@2.0.0', { exact: true })).toBeVisible();

    // A tenant still on the old build.
    await sendError(request, project.dsn, { ...error, release: 'app@1.0.0' });
    await page.reload();
    await expect(
      page.getByRole('button', { name: /^Unresolve/ }),
    ).toBeVisible();
    await expect(page.getByText('In app@2.0.0', { exact: true })).toBeVisible();

    await sendError(request, project.dsn, { ...error, release: 'app@2.0.0' });
    await page.reload();
    await expect(
      page.getByRole('button', { name: 'Resolve', exact: true }),
    ).toBeVisible();
    await expect(
      page.getByText('detected a regression in app@2.0.0'),
    ).toBeVisible();
  });

  test('resolving in a release chosen from the list', async ({
    page,
    request,
  }) => {
    const project = await createProject(request);
    const error = { type: 'PickedError', value: unique('picked') };
    for (const release of ['app@1.0.0', 'app@2.0.0']) {
      await sendError(request, project.dsn, { ...error, release });
    }
    const { issueId } = await sendError(request, project.dsn, {
      ...error,
      release: 'app@3.0.0',
    });

    await page.goto(`/projects/${project.id}/issues/${issueId}`);
    await page.getByRole('button', { name: 'More resolve options' }).click();
    await page
      .getByRole('menuitem', { name: 'Resolve in another release…' })
      .click();
    const dialog = page.getByRole('dialog');
    await dialog.getByRole('combobox', { name: 'Release' }).click();
    await page.getByRole('option', { name: 'app@2.0.0' }).click();
    await dialog.getByRole('button', { name: 'Resolve' }).click();
    await expect(page.getByText('In app@2.0.0', { exact: true })).toBeVisible();
    await expect(
      page.getByText('marked this issue as resolved in app@2.0.0'),
    ).toBeVisible();

    await sendError(request, project.dsn, { ...error, release: 'app@1.0.0' });
    await page.reload();
    await expect(page.getByText('In app@2.0.0', { exact: true })).toBeVisible();

    await sendError(request, project.dsn, { ...error, release: 'app@3.0.0' });
    await page.reload();
    await expect(
      page.getByRole('button', { name: 'Resolve', exact: true }),
    ).toBeVisible();
  });
});
