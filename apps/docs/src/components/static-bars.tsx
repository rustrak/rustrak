import { ChartLegend } from '@rustrak/ui';

/**
 * `@rustrak/ui`'s `BarsChart`, drawn for a picture of the product.
 *
 * recharts measures its container in the browser and draws nothing on the
 * server, and these shots are rendered once at build time and never hydrate.
 * So the same chart is laid out here by hand: the bars are one SVG stretched
 * to the plot, the labels and the dashed grid are HTML, so text never
 * stretches with it. Same props and same look as the real one; no tooltip,
 * because nothing in a screenshot is pointed at.
 */
interface Series {
  key: string;
  label: string;
  color: string;
}

const H = 100;

/** Three or four round steps from zero to the top, as recharts picks them. */
function niceTicks(max: number) {
  const raw = max / 3;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = Math.ceil(raw / power) * power;
  const ticks = [];
  for (let t = 0; t < max + step; t += step) ticks.push(t);
  return ticks;
}

export function BarsChart<TDatum extends Record<string, unknown>>({
  data,
  series,
  xKey,
  height,
  stacked = true,
  formatX = String,
  formatY = String,
  label,
}: {
  data: TDatum[];
  series: Series[];
  xKey: string;
  height: number;
  stacked?: boolean;
  formatX?: (value: string | number) => string;
  formatY?: (value: number) => string;
  label: string;
}) {
  const value = (d: TDatum, key: string) => Number(d[key]);
  const totals = data.map((d) =>
    stacked
      ? series.reduce((sum, s) => sum + value(d, s.key), 0)
      : Math.max(...series.map((s) => value(d, s.key))),
  );
  const ticks = niceTicks(Math.max(...totals));
  const top = ticks[ticks.length - 1];
  const step = 100 / data.length;
  const width = step * 0.76; // recharts' barCategoryGap="24%"
  // About one date per 120 px of a desktop-wide chart, ends included.
  const labelEvery = Math.ceil(data.length / 7);

  return (
    <figure aria-label={label}>
      <div className="flex gap-2 pr-5" style={{ height }}>
        <div className="flex flex-col-reverse justify-between pb-5 text-right font-mono text-fg-ghost text-mono-sm tabular-nums">
          {ticks.map((t) => (
            <span key={t} className="-my-2 leading-4">
              {formatY(t)}
            </span>
          ))}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="relative flex-1">
            <div className="absolute inset-0 flex flex-col justify-between">
              {ticks.map((t) => (
                <div
                  key={t}
                  className="border-border-divider border-t border-dashed"
                />
              ))}
            </div>
            <svg
              viewBox={`0 0 100 ${H}`}
              preserveAspectRatio="none"
              className="absolute inset-0 size-full"
              aria-hidden="true"
            >
              {data.flatMap((d, i) => {
                let base = 0;
                return series.map((s) => {
                  const h = (value(d, s.key) / top) * H;
                  const y = H - (stacked ? base : 0) - h;
                  if (stacked) base += h;
                  return (
                    <rect
                      key={`${i}-${s.key}`}
                      x={i * step + (step - width) / 2}
                      y={y}
                      width={width}
                      height={h}
                      fill={s.color}
                      // The 1 px surface seam between stacked bands.
                      stroke="var(--surface)"
                      strokeWidth={1}
                      vectorEffect="non-scaling-stroke"
                    />
                  );
                });
              })}
            </svg>
          </div>
          <div className="flex h-5 items-end justify-between font-mono text-fg-ghost text-mono-sm">
            {data
              .filter((_, i) => i % labelEvery === 0 || i === data.length - 1)
              .map((d) => (
                <span key={String(d[xKey])}>{formatX(d[xKey] as number)}</span>
              ))}
          </div>
        </div>
      </div>
      <ChartLegend series={series} />
    </figure>
  );
}
