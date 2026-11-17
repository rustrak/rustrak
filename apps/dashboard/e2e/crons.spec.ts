import { createProject, expect, test } from './support';

// The crons page hands out a check-in URL; a job that uses it must show up
// as a monitor, and its latest run decides whether it is failing.
test('a job checking in through the page’s URL is failing after a failed run', async ({
  page,
  request,
}) => {
  const project = await createProject(request);
  await page.goto(`/projects/${project.id}/crons`);

  // The URL as the snippet shows it: what a user pastes into their job.
  const snippet = await page
    .getByRole('code')
    .filter({ hasText: 'RUSTRAK_CRONS=' })
    .textContent();
  const url = snippet?.match(/RUSTRAK_CRONS="([^"]+)"/)?.[1] ?? '';
  const monitorConfig = { schedule: { type: 'crontab', value: '0 * * * *' } };
  for (const status of ['ok', 'error']) {
    const checkIn = await request.post(url, {
      data: { monitor_config: monitorConfig, status },
    });
    expect(checkIn.ok()).toBe(true);
  }

  await page.reload();
  await expect(page.getByText('1 failing')).toBeVisible();
  await page.getByRole('link', { name: /^my-job/ }).click();

  await expect(page.getByRole('heading', { level: 1 })).toHaveText('my-job');
  await expect(page.getByText('Failing', { exact: true })).toBeVisible();
  const runs = page.getByRole('table').getByRole('row');
  await expect(runs.nth(1).getByRole('cell').first()).toHaveText('Failed');
  await expect(runs.nth(2).getByRole('cell').first()).toHaveText('OK');
});
