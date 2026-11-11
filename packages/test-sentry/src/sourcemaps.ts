import * as crypto from 'node:crypto';
import * as Sentry from '@sentry/node';

// ============================================================================
// CRC-32 (needed for ZIP format)
// ============================================================================

function makeCrc32Table(): Uint32Array {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[i] = c;
  }
  return table;
}

const CRC32_TABLE = makeCrc32Table();

function crc32(data: Buffer): number {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) {
    crc = (CRC32_TABLE[(crc ^ data[i]!) & 0xff]! ^ (crc >>> 8)) >>> 0;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

// ============================================================================
// Minimal ZIP writer (stored, no compression)
// ============================================================================

function buildZip(files: { name: string; data: Buffer }[]): Buffer {
  const localParts: Buffer[] = [];
  const centralParts: Buffer[] = [];
  let currentOffset = 0;

  for (const file of files) {
    const nameBytes = Buffer.from(file.name, 'utf8');
    const checksum = crc32(file.data);
    const size = file.data.length;

    const local = Buffer.alloc(30 + nameBytes.length);
    local.writeUInt32LE(0x04034b50, 0); // local file header signature
    local.writeUInt16LE(20, 4); // version needed
    local.writeUInt16LE(0, 6); // flags
    local.writeUInt16LE(0, 8); // compression: stored
    local.writeUInt16LE(0, 10); // mod time
    local.writeUInt16LE(0, 12); // mod date
    local.writeUInt32LE(checksum, 14); // crc-32
    local.writeUInt32LE(size, 18); // compressed size
    local.writeUInt32LE(size, 22); // uncompressed size
    local.writeUInt16LE(nameBytes.length, 26); // filename length
    local.writeUInt16LE(0, 28); // extra field length
    nameBytes.copy(local, 30);

    const central = Buffer.alloc(46 + nameBytes.length);
    central.writeUInt32LE(0x02014b50, 0); // central dir signature
    central.writeUInt16LE(20, 4); // version made by
    central.writeUInt16LE(20, 6); // version needed
    central.writeUInt16LE(0, 8); // flags
    central.writeUInt16LE(0, 10); // compression: stored
    central.writeUInt16LE(0, 12); // mod time
    central.writeUInt16LE(0, 14); // mod date
    central.writeUInt32LE(checksum, 16); // crc-32
    central.writeUInt32LE(size, 20); // compressed size
    central.writeUInt32LE(size, 24); // uncompressed size
    central.writeUInt16LE(nameBytes.length, 28); // filename length
    central.writeUInt16LE(0, 30); // extra field length
    central.writeUInt16LE(0, 32); // file comment length
    central.writeUInt16LE(0, 34); // disk start
    central.writeUInt16LE(0, 36); // internal attr
    central.writeUInt32LE(0, 38); // external attr
    central.writeUInt32LE(currentOffset, 42); // local header offset
    nameBytes.copy(central, 46);

    currentOffset += local.length + size;
    localParts.push(local, file.data);
    centralParts.push(central);
  }

  const cdOffset = currentOffset;
  const cdSize = centralParts.reduce((s, b) => s + b.length, 0);

  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0); // end of central dir signature
  eocd.writeUInt16LE(0, 4); // disk number
  eocd.writeUInt16LE(0, 6); // disk with CD start
  eocd.writeUInt16LE(files.length, 8); // entries on disk
  eocd.writeUInt16LE(files.length, 10); // total entries
  eocd.writeUInt32LE(cdSize, 12); // CD size
  eocd.writeUInt32LE(cdOffset, 16); // CD offset
  eocd.writeUInt16LE(0, 20); // comment length

  return Buffer.concat([...localParts, ...centralParts, eocd]);
}

// ============================================================================
// Artifact bundle
// ============================================================================

const CODE_FILE = 'https://test.rustrak.local/static/app.min.js';
const SOURCE_FILE = 'src/lib/users.ts';
const SOURCE_CONTENT = [
  'export async function getUser(id: string) {',
  "  const user = await db.find('users', id);",
  "  if (!user) throw new Error('User not found: ' + id);",
  '  return user;',
  '}',
  '',
].join('\n');

function generateDebugId(): string {
  const b = crypto.randomBytes(16);
  b[6] = (b[6]! & 0x0f) | 0x40;
  b[8] = (b[8]! & 0x3f) | 0x80;
  const h = b.toString('hex');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

function createArtifactBundle(debugId: string): Buffer {
  const sourceMap = Buffer.from(
    JSON.stringify({
      version: 3,
      sources: [SOURCE_FILE],
      sourcesContent: [SOURCE_CONTENT],
      mappings: 'AAAA',
      names: [],
      debugId,
    }),
    'utf8',
  );

  const manifest = Buffer.from(
    JSON.stringify({
      files: {
        '~/app.min.js.map': {
          url: '~/app.min.js.map',
          type: 'source_map',
          headers: { 'debug-id': debugId },
        },
      },
      debugIdMap: { [debugId]: '~/app.min.js.map' },
    }),
    'utf8',
  );

  return buildZip([
    { name: '~/app.min.js.map', data: sourceMap },
    { name: 'manifest.json', data: manifest },
  ]);
}

// ============================================================================
// Upload protocol — mirrors src/api/mod.rs in sentry-cli
// ============================================================================

interface ChunkUploadCapability {
  url: string;
  chunkSize: number;
  chunksPerRequest: number;
  maxRequestSize: number;
  hashAlgorithm: string;
  accept: string[];
}

async function getChunkUploadCapability(
  serverUrl: string,
  org: string,
  token: string,
): Promise<ChunkUploadCapability> {
  const res = await fetch(
    `${serverUrl}/api/0/organizations/${org}/chunk-upload/`,
    {
      headers: { Authorization: `Bearer ${token}` },
    },
  );
  if (!res.ok) {
    throw new Error(
      `Capability check failed: HTTP ${res.status} — ${await res.text()}`,
    );
  }
  return res.json() as Promise<ChunkUploadCapability>;
}

async function uploadChunks(
  uploadUrl: string,
  token: string,
  chunks: { hash: string; data: Buffer }[],
): Promise<void> {
  const form = new FormData();
  for (const chunk of chunks) {
    form.append(
      chunk.hash,
      new Blob([new Uint8Array(chunk.data)], {
        type: 'application/octet-stream',
      }),
      chunk.hash,
    );
  }
  const res = await fetch(uploadUrl, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
    body: form,
  });
  if (!res.ok) {
    throw new Error(
      `Chunk upload failed: HTTP ${res.status} — ${await res.text()}`,
    );
  }
}

async function assembleBundle(
  serverUrl: string,
  org: string,
  project: string,
  token: string,
  checksum: string,
  chunkHashes: string[],
): Promise<{ state: string; missingChunks: string[] }> {
  const res = await fetch(
    `${serverUrl}/api/0/organizations/${org}/artifactbundle/assemble/`,
    {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        checksum,
        chunks: chunkHashes,
        projects: [project],
      }),
    },
  );
  if (!res.ok) {
    throw new Error(
      `Assembly failed: HTTP ${res.status} — ${await res.text()}`,
    );
  }
  return res.json() as Promise<{ state: string; missingChunks: string[] }>;
}

// ============================================================================
// Public test function
// ============================================================================

export interface SourceMapTestConfig {
  serverUrl: string;
  org: string;
  project: string;
  token: string;
}

export async function testSourceMaps(
  config: SourceMapTestConfig,
): Promise<void> {
  const debugId = generateDebugId();

  console.log(`[testSourceMaps] debug_id:  ${debugId}`);
  console.log(`[testSourceMaps] code_file: ${CODE_FILE}`);

  // Build the artifact bundle ZIP
  const bundle = createArtifactBundle(debugId);
  const bundleChecksum = crypto.createHash('sha1').update(bundle).digest('hex');
  console.log(
    `[testSourceMaps] Bundle: ${bundle.length} bytes  sha1:${bundleChecksum}`,
  );

  // Step 1 — capability check
  console.log('[testSourceMaps] Step 1/3  Getting upload capability...');
  const caps = await getChunkUploadCapability(
    config.serverUrl,
    config.org,
    config.token,
  );
  console.log(
    `[testSourceMaps]   chunkSize=${caps.chunkSize}  maxChunks=${caps.chunksPerRequest}  url=${caps.url}`,
  );

  // Step 2 — split into chunks and upload
  console.log('[testSourceMaps] Step 2/3  Uploading chunks...');
  const chunks: { hash: string; data: Buffer }[] = [];
  for (let i = 0; i < bundle.length; i += caps.chunkSize) {
    const data = Buffer.from(bundle.subarray(i, i + caps.chunkSize));
    chunks.push({
      hash: crypto.createHash('sha1').update(data).digest('hex'),
      data,
    });
  }
  await uploadChunks(caps.url, config.token, chunks);
  console.log(`[testSourceMaps]   Uploaded ${chunks.length} chunk(s)`);

  // Step 3 — assemble (poll until worker finishes, mirrors sentry-cli behaviour)
  console.log('[testSourceMaps] Step 3/3  Assembling bundle...');
  const POLL_INTERVAL_MS = 1_000;
  const POLL_TIMEOUT_MS = 30_000;
  const pollStart = Date.now();
  let result: { state: string; missingChunks: string[] };
  while (true) {
    result = await assembleBundle(
      config.serverUrl,
      config.org,
      config.project,
      config.token,
      bundleChecksum,
      chunks.map((c) => c.hash),
    );
    console.log(`[testSourceMaps]   state=${result.state}`);
    if (result.state === 'ok' || result.state === 'error') break;
    if (Date.now() - pollStart > POLL_TIMEOUT_MS) {
      throw new Error(
        `Assembly timed out after ${POLL_TIMEOUT_MS}ms (last state=${result.state})`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
  if (result.state !== 'ok') {
    const missing =
      result.missingChunks.length > 0
        ? ` Missing chunks: ${result.missingChunks.join(', ')}`
        : '';
    throw new Error(
      `Artifact bundle assembly did not complete successfully (state=${result.state}).${missing}`,
    );
  }

  // Send error event referencing the uploaded source map via debug_meta
  console.log('[testSourceMaps] Sending error event with debug_meta...');
  const eventId = Sentry.captureEvent({
    level: 'error',
    exception: {
      values: [
        {
          type: 'Error',
          value: 'Source map test — user not found',
          stacktrace: {
            frames: [
              {
                filename: '~/app.min.js',
                abs_path: CODE_FILE,
                lineno: 1,
                colno: 0,
                function: '?',
                in_app: true,
              },
            ],
          },
        },
      ],
    },
    debug_meta: {
      images: [{ type: 'sourcemap', code_file: CODE_FILE, debug_id: debugId }],
    },
  } as any);
  console.log(`[testSourceMaps] Event sent: ${eventId}`);
  console.log('[testSourceMaps]');
  console.log(
    '[testSourceMaps] Open the Rustrak dashboard and find this issue.',
  );
  console.log('[testSourceMaps] The stack frame should show:');
  console.log(`[testSourceMaps]   ${SOURCE_FILE}:1  getUser`);
  console.log('[testSourceMaps] instead of:');
  console.log('[testSourceMaps]   app.min.js:1:0  ?');
}
