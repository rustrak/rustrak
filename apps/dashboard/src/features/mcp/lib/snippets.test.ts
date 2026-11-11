import { describe, expect, it } from 'vitest';
import { mcpSnippet, TOKEN_PLACEHOLDER, VSCODE_TOKEN_INPUT } from './snippets';

const URL = 'https://rustrak.example/mcp';

describe('mcpSnippet', () => {
  it('gives a signing-in client the URL and no credential', () => {
    for (const client of ['claudeCode', 'json', 'vscode'] as const) {
      const { code } = mcpSnippet('oauth', client, URL);
      expect(code).toContain(URL);
      expect(code).not.toContain(TOKEN_PLACEHOLDER);
      expect(code).not.toContain('Authorization');
    }
  });

  it('gives a token client the bearer header', () => {
    for (const client of ['claudeCode', 'json'] as const) {
      const { code } = mcpSnippet('token', client, URL);
      expect(code).toContain(URL);
      expect(code).toContain(`Bearer ${TOKEN_PLACEHOLDER}`);
    }
  });

  it('writes JSON a client can parse', () => {
    const { code } = mcpSnippet('token', 'json', URL);
    expect(JSON.parse(code).mcpServers.rustrak.url).toBe(URL);
  });
});

describe('remote server entries', () => {
  it('declare their transport, which Claude Code reads a URL without as stdio', () => {
    const { code } = mcpSnippet('oauth', 'json', URL);
    expect(JSON.parse(code).mcpServers.rustrak.type).toBe('http');
  });

  it("use VS Code's own `servers` shape for VS Code", () => {
    const { code } = mcpSnippet('oauth', 'vscode', URL);
    expect(JSON.parse(code).servers.rustrak).toEqual({
      type: 'http',
      url: URL,
    });
  });

  it('let VS Code ask for the token, so `.vscode/mcp.json` can be committed', () => {
    const { code } = mcpSnippet('token', 'vscode', URL);
    const config = JSON.parse(code);
    expect(config.inputs).toEqual([
      expect.objectContaining({
        type: 'promptString',
        id: 'rustrak-token',
        password: true,
      }),
    ]);
    expect(config.servers.rustrak.headers.Authorization).toBe(
      `Bearer ${VSCODE_TOKEN_INPUT}`,
    );
    expect(code).not.toContain(TOKEN_PLACEHOLDER);
  });
});
