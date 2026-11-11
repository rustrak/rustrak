'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import type { Release, ReleaseChunk } from '@/lib/release';

/**
 * The archive: which releases are on the page, and how to go and get the rest.
 *
 * It exists for one reason. The page ships the ten most recent releases and
 * nothing else, because rendering every release on one URL was 604KB of HTML
 * growing by a release a fortnight. Older ones are fetched a chunk at a time
 * from static JSON generated at build time (`changelog/releases/[chunk]`),
 * which keeps the first response the same size in two years as it is today.
 *
 * So the reveal is a real fetch, not a stack of hidden markup with a button
 * over it. That distinction is the entire exercise: hiding it in CSS would
 * have left all 604KB in the document and only moved the scrollbar.
 *
 * Split out of `changelog-feed.tsx` when the spectrum landed: two consumers
 * need to jump to a release that may not be loaded (a deep link and a click on
 * the spectrum), and the component that renders the feed should not also be the
 * place that owns a fetch loop and two effects' worth of scroll restoration.
 */

export type ArchiveStatus = 'idle' | 'loading' | 'error';

type Options = {
  initial: Release[];
  /**
   * Which chunk every anchor lives in, including the legacy filename anchors,
   * so a deep link into a release that has not been fetched yet can pull the
   * chunks it needs instead of landing on the wrong entry.
   */
  chunkByAnchor: Record<string, number>;
};

export function useReleaseArchive({ initial, chunkByAnchor }: Options) {
  const [releases, setReleases] = useState<Release[]>(initial);
  const [loadedChunks, setLoadedChunks] = useState(1);
  const [status, setStatus] = useState<ArchiveStatus>('idle');
  const [pendingAnchor, setPendingAnchor] = useState<string | null>(null);

  // Mirrors of the values the async loop reads between awaits. State would be
  // stale inside the loop and would also make `loadThrough` a new function on
  // every render, which the hash listener below depends on it not being.
  const loadedRef = useRef(1);
  const busyRef = useRef(false);
  // How far the loop has been asked to go. A ref rather than an argument
  // because a second caller has to be able to raise it while the first is
  // still running. See the note in `loadThrough`.
  const targetRef = useRef(1);

  const loadThrough = useCallback(async (target: number) => {
    // Raised before the busy check, and never lowered. A request that arrives
    // mid-flight used to be dropped on the floor: it returned here because the
    // loop was busy, and the loop it returned to was bounded by the target of
    // whoever started it. So a deep link into chunk four landing while chunk
    // two was in the air left `pendingAnchor` set on a release that would
    // never be fetched, and the page simply never scrolled. Now the running
    // loop reads this on every pass and carries on to the further target.
    targetRef.current = Math.max(targetRef.current, target);
    if (busyRef.current || targetRef.current <= loadedRef.current) return;
    busyRef.current = true;
    setStatus('loading');

    try {
      while (loadedRef.current < targetRef.current) {
        const chunk = loadedRef.current + 1;
        const response = await fetch(`/changelog/releases/${chunk}`);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const payload = (await response.json()) as ReleaseChunk;

        // Appended per chunk rather than once at the end: a failure on the
        // third request should keep the two that succeeded on the page.
        loadedRef.current = chunk;
        setReleases((current) => [...current, ...payload.releases]);
        setLoadedChunks(chunk);
      }
      setStatus('idle');
    } catch {
      setStatus('error');
    } finally {
      busyRef.current = false;
    }
  }, []);

  /**
   * A jump from the spectrum.
   *
   * Returns whether it took over the navigation. For a release already on the
   * page it returns `false` and the browser follows the `href` itself, which is
   * the better outcome in every way: native scroll, a real history entry, and a
   * hash the reader can copy out of the address bar. Only an unfetched release
   * is handled here, because there is nothing for the browser to scroll to yet.
   */
  const jumpTo = useCallback(
    (anchor: string) => {
      const chunk = chunkByAnchor[anchor];
      if (!chunk || chunk <= loadedRef.current) return false;

      // The hash is still written, so the address bar and the back button
      // behave as if the browser had done it. `pendingAnchor` does the scroll
      // once the release exists.
      window.history.pushState(null, '', `#${anchor}`);
      setPendingAnchor(anchor);
      void loadThrough(chunk);
      return true;
    },
    [chunkByAnchor, loadThrough],
  );

  /**
   * Deep links. Every release used to be in the document, so any `#v0-4-1`
   * shared before today has to keep working, including the ones that point
   * into a chunk this page has not fetched. The browser has already given up
   * on an unknown hash by the time this runs, so the chunk is pulled and the
   * scroll is done by hand once the entry exists.
   */
  useEffect(() => {
    const resolve = () => {
      const anchor = decodeURIComponent(window.location.hash.slice(1));
      const chunk = anchor ? chunkByAnchor[anchor] : undefined;

      /*
        Every path clears the pending anchor, including the ones with nothing
        to do. The last hash is the only one the reader asked for, and the
        cases that return here (a release already on the page, an unknown
        anchor, an empty hash) are all cases the browser has already handled
        or that mean nothing. Leaving a previous anchor standing through them
        meant the next chunk to arrive would scroll the page off whatever the
        reader had just navigated to, back to a release they had abandoned.
      */
      if (!chunk || chunk <= loadedRef.current) {
        setPendingAnchor(null);
        return;
      }

      setPendingAnchor(anchor);
      void loadThrough(chunk);
    };

    resolve();
    window.addEventListener('hashchange', resolve);
    return () => window.removeEventListener('hashchange', resolve);
  }, [chunkByAnchor, loadThrough]);

  /** Scrolls once the release the hash asked for is actually in the DOM. */
  useEffect(() => {
    if (!pendingAnchor) return;
    const target = document.getElementById(pendingAnchor);
    if (!target) return;
    target.scrollIntoView();
    setPendingAnchor(null);
  }, [pendingAnchor, releases]);

  return { releases, loadedChunks, status, loadThrough, jumpTo };
}
