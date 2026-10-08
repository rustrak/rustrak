import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

const BASE_URL = 'http://localhost:8080';

describe('SourceMapsResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: BASE_URL,
      token: 'test-token',
    });
  });

  describe('getChunkUploadCapability()', () => {
    it('should return upload URL and limits', async () => {
      const caps = expectOk(
        await client.sourceMaps.getChunkUploadCapability('my-org'),
      );

      expect(caps.url).toContain('my-org');
      expect(caps.chunkSize).toBe(2097152);
      expect(caps.chunksPerRequest).toBe(64);
      expect(caps.maxRequestSize).toBe(33554432);
      expect(caps.hashAlgorithm).toBe('sha1');
      expect(caps.accept).toContain('artifact_bundles');
    });

    it('should validate response schema', async () => {
      server.use(
        http.get(`${BASE_URL}/api/0/organizations/bad-org/chunk-upload/`, () =>
          HttpResponse.json({ unexpected: true }),
        ),
      );

      const result =
        await client.sourceMaps.getChunkUploadCapability('bad-org');

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });
  });

  describe('uploadChunks()', () => {
    it('should upload chunks without error', async () => {
      const chunk = {
        hash: 'aabbcc1122334455aabbcc1122334455aabbcc11',
        data: new Blob(['source map data'], {
          type: 'application/octet-stream',
        }),
      };
      const result = await client.sourceMaps.uploadChunks('my-org', [chunk]);

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should upload multiple chunks', async () => {
      const chunks = [
        {
          hash: 'aaaa0000000000000000000000000000aaaa0001',
          data: new Blob(['chunk1']),
        },
        {
          hash: 'aaaa0000000000000000000000000000aaaa0002',
          data: new Blob(['chunk2']),
        },
        {
          hash: 'aaaa0000000000000000000000000000aaaa0003',
          data: new Blob(['chunk3']),
        },
      ];
      const result = await client.sourceMaps.uploadChunks('my-org', chunks);

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should batch chunks into multiple requests when count exceeds chunksPerRequest', async () => {
      let requestCount = 0;
      server.use(
        http.post(
          `${BASE_URL}/api/0/organizations/batch-org/chunk-upload/`,
          () => {
            requestCount++;
            return new HttpResponse(null, { status: 200 });
          },
        ),
      );

      // 3 chunks with chunksPerRequest=2 → must send 2 requests (batch of 2 + batch of 1)
      const chunks = [
        {
          hash: 'bbbb0000000000000000000000000000bbbb0001',
          data: new Blob(['a']),
        },
        {
          hash: 'bbbb0000000000000000000000000000bbbb0002',
          data: new Blob(['b']),
        },
        {
          hash: 'bbbb0000000000000000000000000000bbbb0003',
          data: new Blob(['c']),
        },
      ];
      expectOk(await client.sourceMaps.uploadChunks('batch-org', chunks, 2));

      expect(requestCount).toBe(2);
    });

    // `chunksPerRequest` used to `throw new Error(...)`. It is caller input
    // checked before any request, so it is now the matrix's `invalid_request`
    // row and no bytes leave the process.
    it('should reject a non-positive chunksPerRequest without sending anything', async () => {
      let requestCount = 0;
      server.use(
        http.post(
          `${BASE_URL}/api/0/organizations/my-org/chunk-upload/`,
          () => {
            requestCount++;
            return new HttpResponse(null, { status: 200 });
          },
        ),
      );

      const result = await client.sourceMaps.uploadChunks(
        'my-org',
        [{ hash: 'a'.repeat(40), data: new Blob(['x']) }],
        0,
      );

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
      expect(requestCount).toBe(0);
    });
  });

  describe('assembleBundle()', () => {
    it('should return ok state when all chunks are present', async () => {
      const result = expectOk(
        await client.sourceMaps.assembleBundle('my-org', {
          checksum: 'abc123',
          chunks: ['sha1hash1', 'sha1hash2'],
          projects: ['my-project'],
        }),
      );

      expect(result.state).toBe('ok');
      expect(result.missingChunks).toHaveLength(0);
    });

    it('should return not_found with missing chunks', async () => {
      const result = expectOk(
        await client.sourceMaps.assembleBundle('my-org', {
          checksum: 'missing-abc123',
          chunks: ['sha1hash1', 'sha1hash2'],
          projects: ['my-project'],
        }),
      );

      expect(result.state).toBe('not_found');
      expect(result.missingChunks.length).toBeGreaterThan(0);
    });

    // `assembleBundle` used to be the only input-taking method that skipped
    // `validateInput`, so an empty `projects` was a round trip that came back
    // 400. It is now the same `invalid_request` every other method reports, and
    // the assertion that the handler was never reached is the point: nothing
    // was sent.
    it('rejects an empty projects array locally, without sending it', async () => {
      let reached = false;
      server.use(
        http.post(
          `${BASE_URL}/api/0/organizations/my-org/artifactbundle/assemble/`,
          () => {
            reached = true;
            return HttpResponse.json({ state: 'ok', missingChunks: [] });
          },
        ),
      );

      const result = await client.sourceMaps.assembleBundle('my-org', {
        checksum: 'abc123',
        chunks: ['sha1hash1'],
        projects: [],
      });

      const error = expectErr(result);
      expect(error.kind).toBe('invalid_request');
      // No status: nothing reached the server.
      expect(error).not.toHaveProperty('status');
      expect(reached).toBe(false);
    });

    it('rejects an empty checksum locally', async () => {
      const error = expectErr(
        await client.sourceMaps.assembleBundle('my-org', {
          checksum: '',
          chunks: ['sha1hash1'],
          projects: ['my-project'],
        }),
      );

      expect(error.kind).toBe('invalid_request');
    });

    it('should validate response schema', async () => {
      server.use(
        http.post(
          `${BASE_URL}/api/0/organizations/bad-org/artifactbundle/assemble/`,
          () => HttpResponse.json({ unexpected: true }),
        ),
      );

      const result = await client.sourceMaps.assembleBundle('bad-org', {
        checksum: 'abc',
        chunks: [],
        projects: ['p'],
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });
  });

  describe('list()', () => {
    it('should return list of source map files', async () => {
      const result = expectOk(
        await client.sourceMaps.list('my-org', 'my-project'),
      );

      expect(result.data).toHaveLength(1);
      expect(result.data[0]?.debugId).toBe(
        'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
      );
      expect(result.data[0]?.fileType).toBe('source_map');
      expect(result.data[0]?.size).toBe(15234);
      expect(result.data[0]?.timesUsed).toBe(3);
    });

    it('should report not_found for an unknown project', async () => {
      const result = await client.sourceMaps.list('my-org', 'not-found');

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should validate response schema', async () => {
      server.use(
        http.get(
          `${BASE_URL}/api/0/projects/my-org/bad-project/files/source-maps/`,
          () => HttpResponse.json({ wrong: 'shape' }),
        ),
      );

      const result = await client.sourceMaps.list('my-org', 'bad-project');

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });
  });
});
