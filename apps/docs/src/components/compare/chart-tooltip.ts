import { valueAt } from '../../lib/chart';
import { mb, num } from '../../lib/compare';

/*
 * The readout under the pointer on a `line-chart.astro`, as a charting
 * library draws it: the pointer snaps to the nearest second, a hairline
 * marks it, every line gets a dot where it crosses it, and a panel beside
 * the pointer lists them all. The panel glides rather than jumps, and is
 * only rewritten when the second changes.
 */

type Line = {
  id: string;
  label: string;
  color: string;
  strong: boolean;
  points: [number, number][];
  done?: number;
  finish?: string;
};
type Axis = { domain: [number, number]; range: [number, number] };
type Data = {
  tooltip: 'race' | 'values';
  x: Axis & { scale: 'log' | 'linear' };
  y: Axis;
  series: Line[];
};

const lerp = ([d0, d1]: number[], [r0, r1]: number[], v: number) =>
  r0 + ((v - d0) / (d1 - d0)) * (r1 - r0);

/** A second to screen x and back, on the chart's own scale. */
function xScale({ scale, domain, range }: Data['x']) {
  if (scale === 'linear') {
    return {
      to: (t: number) => lerp(domain, range, t),
      from: (px: number) => lerp(range, domain, px),
    };
  }
  const logs = domain.map(Math.log10);
  return {
    to: (t: number) => lerp(logs, range, Math.log10(t)),
    from: (px: number) => 10 ** lerp(range, logs, px),
  };
}

/** Rows for the race: finished first, by finish, then the rest by how far they got. */
function raceRows(lines: Line[], t: number) {
  return lines
    .map((line) => ({
      line,
      done: (line.done ?? Number.POSITIVE_INFINITY) <= t,
      n: valueAt(line.points, t) ?? 0,
    }))
    .sort((a, b) =>
      a.done && b.done
        ? (a.line.done ?? 0) - (b.line.done ?? 0)
        : Number(b.done) - Number(a.done) || b.n - a.n,
    )
    .map(({ line, done, n }) => ({
      line,
      value: done ? (line.finish ?? '') : `${num(n)} stored`,
    }));
}

/** Rows for a plain chart: each line's value, highest first. */
function valueRows(lines: Line[], t: number) {
  return lines
    .flatMap((line) => {
      const v = valueAt(line.points, t);
      return v == null ? [] : [{ line, v }];
    })
    .sort((a, b) => b.v - a.v)
    .map(({ line, v }) => ({ line, value: mb(v) }));
}

function render(
  tip: HTMLElement,
  title: string,
  rows: { line: Line; value: string }[],
) {
  const head = document.createElement('p');
  head.className = 'mb-1.5 font-mono text-fg-meta text-mono';
  head.textContent = title;
  const list = document.createElement('ul');
  list.className = 'flex flex-col gap-1';
  for (const { line, value } of rows) {
    const item = document.createElement('li');
    item.className = 'flex items-center gap-2 whitespace-nowrap text-meta';
    const dot = document.createElement('span');
    dot.className = 'size-2 shrink-0 rounded-full';
    dot.style.background = line.color;
    const name = document.createElement('span');
    name.className = line.strong ? 'text-fg' : 'text-fg-subtle';
    name.textContent = line.label;
    const figure = document.createElement('span');
    figure.className = 'ms-auto ps-4 font-mono text-fg tabular-nums';
    figure.textContent = value;
    item.append(dot, name, figure);
    list.append(item);
  }
  tip.replaceChildren(head, list);
}

for (const chart of document.querySelectorAll<HTMLElement>(
  '[data-line-chart]',
)) {
  const data: Data = JSON.parse(
    chart.querySelector('script[type="application/json"]')?.textContent ?? '{}',
  );
  const svg = chart.querySelector('svg');
  const plot = chart.querySelector<SVGRectElement>('[data-plot]');
  const hover = chart.querySelector<SVGGElement>('[data-hover]');
  const cursor = chart.querySelector<SVGLineElement>('[data-cursor]');
  const tip = chart.querySelector<HTMLElement>('[data-tip]');
  if (!svg || !plot || !hover || !cursor || !tip) continue;

  const x = xScale(data.x);
  const y = (v: number) => lerp(data.y.domain, data.y.range, v);
  const [t0, t1] = data.x.domain;
  const dots = new Map(
    data.series.map((line) => [
      line.id,
      chart.querySelector<SVGCircleElement>(`[data-dot="${line.id}"]`),
    ]),
  );
  let shown: number | null = null;

  const hide = () => {
    shown = null;
    hover.setAttribute('visibility', 'hidden');
    tip.style.opacity = '0';
  };

  /** The second under the pointer, and the hover drawn for it. */
  const show = (t: number) => {
    if (t === shown) return;
    shown = t;
    const px = x.to(t);
    cursor.setAttribute('x1', String(px));
    cursor.setAttribute('x2', String(px));
    for (const line of data.series) {
      const dot = dots.get(line.id);
      const v = valueAt(line.points, t);
      if (!dot) continue;
      dot.setAttribute('visibility', v == null ? 'hidden' : 'visible');
      if (v == null) continue;
      dot.setAttribute('cx', String(px));
      dot.setAttribute('cy', String(y(v)));
    }
    hover.setAttribute('visibility', 'visible');
    render(
      tip,
      data.tooltip === 'race' ? `${t} s after the first request` : `${t} s`,
      data.tooltip === 'race'
        ? raceRows(data.series, t)
        : valueRows(data.series, t),
    );
  };

  /** The panel beside the pointer, on whichever side has the room. */
  const place = (event: PointerEvent) => {
    const box = chart.getBoundingClientRect();
    const px = event.clientX - box.left;
    const py = event.clientY - box.top;
    const { offsetWidth: w, offsetHeight: h } = tip;
    const left = px + 20 + w > box.width ? px - 20 - w : px + 20;
    const top = Math.min(Math.max(py - h / 2, 0), box.height - h);
    // Appearing, it starts where the pointer is instead of gliding in from
    // wherever it was last.
    const appearing = tip.style.opacity !== '1';
    if (appearing) tip.style.transition = 'none';
    tip.style.translate = `${left}px ${top}px`;
    if (appearing) {
      tip.getBoundingClientRect();
      tip.style.transition = '';
    }
    tip.style.opacity = '1';
  };

  const move = (event: PointerEvent) => {
    const matrix = svg.getScreenCTM();
    if (!matrix) return;
    const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
      matrix.inverse(),
    );
    const t = Math.min(t1, Math.max(t0, Math.round(x.from(point.x))));
    show(t);
    place(event);
  };

  plot.addEventListener('pointermove', move);
  plot.addEventListener('pointerdown', move);
  plot.addEventListener('pointerleave', hide);
}
