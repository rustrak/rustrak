import { expect, test as setup } from '@playwright/test';
import { ADMIN, ADMIN_STATE } from './support';

// Signs in once through the API and saves the session cookie; every test
// starts from it. The login *form* is tested in `auth.spec.ts`, once.
setup('sign in as the admin', async ({ request }) => {
  const response = await request.post('/auth/login', { data: ADMIN });
  expect(response.ok()).toBe(true);
  await request.storageState({ path: ADMIN_STATE });
});
