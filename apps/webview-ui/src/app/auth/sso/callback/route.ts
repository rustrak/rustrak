import { NextRequest, NextResponse } from 'next/server';
import { applySetCookies, createClient } from '@/shared/api/rustrak';

/**
 * Browser-facing OIDC callback. The Rust API may only be reachable inside the
 * Docker network, so the dashboard exchanges the callback server-side and
 * relays the resulting encrypted session cookie to the browser.
 */
export async function GET(request: NextRequest) {
  const loginUrl = new URL('/auth/login', request.url);

  const params = request.nextUrl.searchParams;
  const client = await createClient();
  const result = await client.auth.completeSso({
    code: params.get('code') ?? undefined,
    state: params.get('state') ?? undefined,
    error: params.get('error') ?? undefined,
  });

  if (!result.success) {
    console.error('SSO callback failed:', result.error);
    loginUrl.searchParams.set('error', 'sso');
    return NextResponse.redirect(loginUrl);
  }

  await applySetCookies(result.data.cookies);
  return NextResponse.redirect(new URL('/', request.url));
}
