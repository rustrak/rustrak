'use client';

import dynamic from 'next/dynamic';

/*
 * The charts and Recharts with them, loaded when the page needs them rather
 * than in its first bundle. Each placeholder holds the chart's height so the
 * page does not jump when it arrives.
 */
const hold = (height: number) => () => <div style={{ height }} />;

export const RaceChart = dynamic(
  () => import('./charts').then((m) => m.RaceChart),
  { ssr: false, loading: hold(440) },
);
export const MemoryChart = dynamic(
  () => import('./charts').then((m) => m.MemoryChart),
  { ssr: false, loading: hold(360) },
);
export const RankBars = dynamic(
  () => import('./charts').then((m) => m.RankBars),
  { ssr: false, loading: hold(330) },
);
