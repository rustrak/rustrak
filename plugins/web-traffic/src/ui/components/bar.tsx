/** A list row: its value, its figures on the right, a bar behind them. */
export const BAR_ROW =
  'relative flex w-full items-center gap-3 px-5 py-1.5 text-meta';

/** The bar behind a row, as long as its share of the largest. */
export function BarFill({ ratio }: { ratio: number }) {
  return (
    <span
      aria-hidden
      className="absolute inset-y-0.5 left-2 rounded bg-surface-selected"
      style={{ width: `calc((100% - 1rem) * ${ratio})` }}
    />
  );
}
