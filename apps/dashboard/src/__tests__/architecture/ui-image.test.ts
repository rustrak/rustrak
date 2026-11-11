import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/**
 * The standalone `rustrak-ui` image configures itself at start rather than at
 * build: its entrypoint script normalises `RUSTRAK_API_URL` before nginx's
 * envsubst renders the proxy, so the browser still talks to one origin.
 */

const here = dirname(fileURLToPath(import.meta.url));

/**
 * `RUSTRAK_API_URL` after the entrypoint has looked at it: the value the
 * template will see, or the reason the container refused to start.
 *
 * The script is sourced by nginx's entrypoint before envsubst runs, so this
 * sources it the same way, in `sh`, with the variable set as an operator
 * would set it.
 */
function apiUrlSeenByNginx(value: string): {
  url: string;
  exit: number;
  stderr: string;
} {
  const script = resolve(here, '../../../docker/15-rustrak-api-url.envsh');
  const result = spawnSync(
    'sh',
    ['-c', `. "${script}" && printf '%s' "$RUSTRAK_API_URL"`],
    { env: { PATH: process.env.PATH ?? '', RUSTRAK_API_URL: value } },
  );
  return {
    url: result.stdout.toString(),
    exit: result.status ?? -1,
    stderr: result.stderr.toString(),
  };
}

describe('the standalone dashboard image reads RUSTRAK_API_URL the way an operator writes it', () => {
  it('drops a trailing slash rather than proxying every request to /', () => {
    // `proxy_pass` with a variable sends the request URI as-is only when the
    // value has no path of its own. `https://api.example.com/` has one, `/`,
    // and every `/api/...` would reach the server as `/`.
    expect(apiUrlSeenByNginx('https://api.example.com/').url).toBe(
      'https://api.example.com',
    );
    expect(apiUrlSeenByNginx('http://server:8080//').url).toBe(
      'http://server:8080',
    );
    expect(apiUrlSeenByNginx('http://server:8080').url).toBe(
      'http://server:8080',
    );
  });

  it('refuses a value that carries a path', () => {
    // There is no right answer for `https://example.com/rustrak`: the
    // dashboard would need every prefix rewritten, which the template does
    // not do. Refusing at start is the honest failure.
    const refused = apiUrlSeenByNginx('https://example.com/rustrak');
    expect(refused.exit).not.toBe(0);
    expect(refused.stderr).toMatch(/RUSTRAK_API_URL/);
  });

  it('refuses a value that is not an http(s) origin', () => {
    expect(apiUrlSeenByNginx('server:8080').exit).not.toBe(0);
    expect(apiUrlSeenByNginx('').exit).not.toBe(0);
  });
});
