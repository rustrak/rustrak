import {
  createProject,
  expect,
  sendError,
  sendFeedback,
  sendLogs,
  sendTransaction,
  test,
} from './support';

// What an SDK sends besides errors, each read back where the dashboard
// promises to show it.

test('an error tagged with a release is listed as that release’s new issue', async ({
  page,
  request,
}) => {
  const project = await createProject(request);
  await sendError(request, project.dsn, {
    type: 'CheckoutCrash',
    value: 'after deploy',
    release: 'shop@2.4.0',
  });

  await page.goto(`/projects/${project.id}/releases`);
  await page.getByText('shop@2.4.0').click();

  await expect(page.getByRole('heading', { level: 1 })).toHaveText('2.4.0');
  await expect(
    page.getByRole('link', { name: /CheckoutCrash: after deploy/ }),
  ).toBeVisible();
});

test('a transaction shows up with its duration and its spans', async ({
  page,
  request,
}) => {
  const project = await createProject(request);
  await sendTransaction(request, project.dsn, {
    name: 'GET /checkout',
    durationMs: 800,
    span: 'SELECT * FROM carts',
  });

  await page.goto(`/projects/${project.id}/performance`);
  const row = page.getByRole('row').filter({ hasText: 'GET /checkout' });
  await expect(row.getByRole('cell', { name: '800 ms' }).first()).toBeVisible();

  await row.click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(
    'GET /checkout',
  );
  // The slowest sample opens its trace, where the span is.
  await page.getByRole('link', { name: '800 ms' }).click();
  await expect(page.getByText('SELECT * FROM carts').first()).toBeVisible();
});

test('logs can be searched by their message', async ({ page, request }) => {
  const project = await createProject(request);
  await sendLogs(request, project.dsn, [
    { level: 'error', body: 'payment provider timed out' },
    { level: 'info', body: 'cache warmed' },
  ]);

  await page.goto(`/projects/${project.id}/logs`);
  const lines = page.getByRole('list', { name: 'Log lines' });
  await expect(lines.getByRole('listitem')).toHaveCount(2);

  const search = page.getByRole('combobox', { name: /Filter by/ });
  await search.fill('payment');
  await search.press('Enter');
  await expect(lines.getByRole('listitem')).toHaveCount(1);
  await expect(lines).toContainText('payment provider timed out');
});

test('logs can be narrowed to one environment and a line opens in place', async ({
  page,
  request,
}) => {
  const project = await createProject(request);
  await sendLogs(request, project.dsn, [
    {
      level: 'error',
      body: 'payment provider timed out',
      environment: 'production',
    },
    { level: 'info', body: 'cache warmed', environment: 'staging' },
  ]);

  await page.goto(`/projects/${project.id}/logs`);
  const lines = page.getByRole('list', { name: 'Log lines' });
  await expect(lines.getByRole('listitem')).toHaveCount(2);

  await page.getByRole('button', { name: 'Environment' }).click();
  await page.getByRole('menuitem', { name: 'production' }).click();
  await expect(lines.getByRole('listitem')).toHaveCount(1);
  await expect(lines).toContainText('payment provider timed out');

  // Kept in the address, so a reload or a shared link keeps it.
  await page.reload();
  await expect(lines.getByRole('listitem')).toHaveCount(1);

  const line = lines.getByRole('listitem');
  await line
    .getByRole('button', { name: /payment provider timed out/ })
    .click();
  await expect(
    line.getByRole('definition').getByTitle('production'),
  ).toBeVisible();
});

test('resolved feedback leaves the inbox', async ({ page, request }) => {
  const project = await createProject(request);
  await sendFeedback(request, project.dsn, {
    message: 'the checkout button does nothing',
    email: 'buyer@example.com',
  });

  await page.goto(`/projects/${project.id}/feedback`);
  const inbox = page.getByRole('list', { name: 'User Feedback' });
  await expect(inbox).toContainText('the checkout button does nothing');
  await page.getByRole('button', { name: /^Resolve/ }).click();
  await expect(inbox).toHaveCount(0);

  // And still after a reload, from the server.
  await page.reload();
  await expect(inbox).toHaveCount(0);
  await page.getByRole('tab', { name: 'Resolved' }).click();
  await expect(inbox).toContainText('the checkout button does nothing');
});
