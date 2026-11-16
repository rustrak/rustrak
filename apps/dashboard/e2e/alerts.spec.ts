import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { createProject, expect, sendError, test, unique } from './support';

/** An HTTP endpoint that records what Rustrak POSTs to it. */
async function webhookReceiver() {
  const bodies: string[] = [];
  const server = createServer((req, res) => {
    let body = '';
    req.on('data', (chunk) => {
      body += chunk;
    });
    req.on('end', () => {
      bodies.push(body);
      res.end('ok');
    });
  });
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  const { port } = server.address() as AddressInfo;
  return {
    url: `http://127.0.0.1:${port}/hook`,
    bodies,
    close: () => server.close(),
  };
}

// The point of an error tracker: someone hears about a new error without
// watching the dashboard.
test('a new-issue alert posts the issue to its webhook', async ({
  page,
  request,
}) => {
  const receiver = await webhookReceiver();
  const project = await createProject(request);
  const channel = unique('ops-hook');
  const alert = unique('New issues to ops');

  // Channels are shared by every project; the journey is the alert, so the
  // channel is setup.
  const created = await request.post('/api/integrations', {
    data: {
      name: channel,
      provider_type: 'webhook',
      credentials: { url: receiver.url },
    },
  });
  expect(created.ok()).toBe(true);

  await page.goto(`/projects/${project.id}/alerts/new?type=new_issue`);
  await page
    .getByRole('textbox', { name: 'e.g. New issues in production by email' })
    .fill(alert);
  await page.getByRole('button', { name: 'Every time' }).click();
  await page.getByText(channel).click();
  await page.getByRole('button', { name: 'Create alert' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(alert);

  await sendError(request, project.dsn, {
    type: 'PagerWorthy',
    value: 'database unreachable',
  });

  await expect
    .poll(() => receiver.bodies.join('\n'), { timeout: 20_000 })
    .toContain('PagerWorthy');
  receiver.close();
});
