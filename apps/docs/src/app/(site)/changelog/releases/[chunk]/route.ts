import { getChunkCount, getReleaseChunk } from '@/lib/changelog';

/**
 * The older halves of the changelog, as static JSON.
 *
 * This is what makes "load more" load anything. The site is a static export,
 * so there is no server to ask at runtime — but a GET route handler with no
 * dynamic input is rendered at build time like any page, landing in `out/` as
 * `changelog/releases/2`, `changelog/releases/3` and so on. The feed fetches
 * one when the reader asks for it, and a visitor who never scrolls past the
 * tenth release never pays for the other thirty.
 *
 * Chunk 1 is deliberately absent: the page already inlines it, and generating
 * it here would ship the same ten releases twice.
 */
export const dynamicParams = false;

export function generateStaticParams() {
  return Array.from({ length: getChunkCount() - 1 }, (_, index) => ({
    chunk: String(index + 2),
  }));
}

export async function GET(
  _request: Request,
  context: { params: Promise<{ chunk: string }> },
) {
  const { chunk } = await context.params;
  return Response.json(getReleaseChunk(Number(chunk)));
}
