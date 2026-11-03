'use client';

import { useEffect, useState } from 'react';
import { fetchRoadmap, PROJECT_URL, type RoadmapItem } from '@/lib/roadmap';
import { Overview } from './overview';
import { Stages } from './stages';

/**
 * The board and the horizons, drawn from what the build fetched and then
 * refreshed from the project itself once the page is open. A failed refresh
 * keeps the build's copy: stale beats empty.
 */
export function RoadmapLive({ initial }: { initial: RoadmapItem[] }) {
  const [items, setItems] = useState(initial);

  useEffect(() => {
    fetchRoadmap()
      .then(setItems)
      .catch(() => {});
  }, []);

  if (items.length === 0) {
    return (
      <p className="mx-auto w-full max-w-6xl px-6 py-24 text-fg-subtle text-value">
        The roadmap could not be loaded. It lives on{' '}
        <a href={PROJECT_URL} className="text-fg-brand hover:underline">
          GitHub Projects
        </a>
        .
      </p>
    );
  }

  const open = items.filter((item) => item.status !== 'Shipped');
  const shipped = items.filter((item) => item.status === 'Shipped');

  return (
    <>
      <div className="mx-auto w-full max-w-7xl px-4 pt-16 pb-12 sm:px-6">
        <Overview items={open} />
      </div>
      <Stages open={open} shipped={shipped} />
    </>
  );
}
