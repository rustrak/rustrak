import { useFormatter } from 'use-intl';

/** A chart's bucket label: the day for daily buckets, the hour otherwise. */
export function useBucketLabel(daily: boolean) {
  const format = useFormatter();
  return (value: string | number) =>
    format.dateTime(new Date(value), daily ? 'axisDay' : 'axisHour');
}
