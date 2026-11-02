'use client';

import { useReducedMotion } from 'motion/react';
import { PlatformIcon } from 'platformicons';
import { type CSSProperties, useEffect, useRef, useState } from 'react';
import { Reveal, Words } from './reveal';

/**
 * Many apps in, one stream out.
 *
 * Left of centre, a grid of apps. Each reporting app sends streaks of light
 * to the wall in the middle of the section, in its own colour; they hit the
 * wall at their own height with a lime flash, and leave as Rustrak's lime
 * through the narrow bundle under the words. The tile flashes as its streak
 * leaves (`flow-ping` in home.css, on the same clock as the canvas). Every
 * second and a half one app goes quiet and another starts near it.
 *
 * The streaks are drawn on one canvas at up to 60 frames a second, and only
 * while the section is on screen: as SVG, animating a few hundred strokes made
 * the browser repaint the whole drawing every frame. Under reduced motion
 * nothing moves.
 */

const W = 1200;
const H = 440;
const SEAM = W / 2;
const MID = H / 2;

const COLS = 9;
const ROWS = 9;
const CELL = 30;
const GAP = 12;
const GRID_X = 40;
const GRID_Y = (H - (ROWS * CELL + (ROWS - 1) * GAP)) / 2;
/** Reporting apps live in the columns nearest the seam. */
const FIRST_LIVE_COL = 3;
/** How many apps are reporting at once. */
const LIVE = 20;
/** The wall's top, where the incoming lines land, and the bundle out. */
const WALL_TOP = MID - 115;
const WALL_BOTTOM = MID + 115;
const LAND_STEP = 10;
const BEAM_Y = MID;

/** What arrives: errors, warnings and noise. Never the brand: that is what
 * Rustrak makes of them, on the other side of the wall. */
const TONES = [
  { id: 'error', color: 'var(--sev-error)' },
  { id: 'warning', color: 'var(--sev-warning)' },
  { id: 'neutral', color: 'var(--fg-subtle)' },
];

/** Where the wall sits along the gradients, which run across the drawing. */
const AT_WALL = SEAM / W;

const PLATFORMS = [
  'javascript-react',
  'python-django',
  'node-express',
  'go',
  'rust',
  'ruby-rails',
  'php-laravel',
  'java-spring-boot',
  'dotnet',
  'apple-ios',
  'android',
  'javascript-nextjs',
  'python-fastapi',
  'javascript-vue',
  'elixir',
  'flutter',
  'react-native',
  'python-flask',
  'javascript-angular',
  'kotlin',
  'php-symfony',
  'node-nestjs',
  'javascript-svelte',
  'unity',
  'javascript-remix',
  'bun',
  'deno',
  'go-gin',
  'rust-actix',
  'python-celery',
  'ruby-sinatra',
  'electron',
  'capacitor',
  'java',
  'python',
];

interface Source {
  cell: number;
  platform: string;
  /** Joined after the page loaded, at this `performance.now()`: its streaks
   * start from the app instead of mid-flight. */
  born?: number;
}

/** A fixed starting arrangement, so the server and the client agree. */
const INITIAL: Source[] = (() => {
  // Cells near the seam, in a scrambled but fixed order.
  const cells = Array.from({ length: COLS * ROWS }, (_, i) => i)
    .filter((cell) => cell % COLS >= FIRST_LIVE_COL)
    .sort((a, b) => ((a * 37) % 101) - ((b * 37) % 101))
    .slice(0, LIVE)
    // Top to bottom, so each app's slot on the wall matches where it sits.
    .sort((a, b) => a - b);
  return cells.map((cell, index) => ({ cell, platform: PLATFORMS[index] }));
})();

const colOf = (cell: number) => cell % COLS;
const rowOf = (cell: number) => Math.floor(cell / COLS);
const cellX = (col: number) => GRID_X + col * (CELL + GAP);
const cellY = (row: number) => GRID_Y + row * (CELL + GAP);

const pct = (x: number, y: number) => ({
  left: `${(x / W) * 100}%`,
  top: `${(y / H) * 100}%`,
});

/** A stable per-app rhythm, so a moved app keeps its own pace. */
function rhythm(platform: string) {
  let hash = 0;
  for (const char of platform) hash = (hash * 31 + char.charCodeAt(0)) % 997;
  return { duration: 3.4 + (hash % 9) * 0.3, delay: -(hash % 13) * 0.45 };
}

/** The grid row an app in this wall slot naturally sits on. */
const rowForSlot = (slot: number) =>
  Math.round((slot * (ROWS - 1)) / (LIVE - 1));

/**
 * One step of the shuffle: an app goes quiet and another starts in its slot,
 * in a free cell near the same row. The slot is what fixes a line's place on
 * the wall, so nothing else in the scene moves when this happens.
 */
function shuffle(sources: Source[]): Source[] {
  const used = new Set(sources.map((s) => s.cell));
  const out = Math.floor(Math.random() * sources.length);
  const row = rowForSlot(out);
  const free = Array.from({ length: COLS * ROWS }, (_, i) => i).filter(
    (cell) =>
      colOf(cell) >= FIRST_LIVE_COL &&
      !used.has(cell) &&
      Math.abs(rowOf(cell) - row) <= 1,
  );
  if (free.length === 0) return sources;
  const idle = PLATFORMS.filter((p) => !sources.some((s) => s.platform === p));
  const next = [...sources];
  next[out] = {
    cell: free[Math.floor(Math.random() * free.length)],
    platform: idle[Math.floor(Math.random() * idle.length)],
    born: performance.now(),
  };
  return next;
}

/** The streak's length, and how far its head travels in one cycle. */
const DASH = 0.22;
const TRAVEL = 1 + DASH;

/** Launched hard, then easing off: fast out of the app, slower to settle. */
const launch = (t: number) => 1 - (1 - t) ** 2.2;

interface Point {
  x: number;
  y: number;
  /** Distance along the path to here. */
  s: number;
}

interface Streak {
  key: string;
  path: Path2D;
  points: Point[];
  /** Path length in drawing units. */
  length: number;
  /** Distance along the path to the wall. */
  wall: number;
  tone: (typeof TONES)[number]['id'];
  land: number;
  duration: number;
  /** When its first cycle starts, in `performance.now()` seconds. */
  start: number;
  /** Where the head was on the previous frame, to catch the impact; null
   * until the streak has been drawn once. */
  previous: number | null;
}

interface Impact {
  y: number;
  born: number;
  sparks: { vx: number; vy: number }[];
}

/** How long an impact on the wall takes to die out, in seconds. */
const IMPACT_LIFE = 0.5;

/** The path as points, for finding where a streak's head is. */
function sample(
  x: number,
  y: number,
  bend: number,
  land: number,
  lane: number,
): Point[] {
  const points: Point[] = [];
  let s = 0;
  const push = (px: number, py: number) => {
    const prev = points.at(-1);
    if (prev) s += Math.hypot(px - prev.x, py - prev.y);
    points.push({ x: px, y: py, s });
  };
  const end = SEAM - 60;
  for (let i = 0; i <= 32; i++) {
    const t = i / 32;
    const u = 1 - t;
    push(
      u * u * u * x +
        3 * u * u * t * bend +
        3 * u * t * t * bend +
        t * t * t * end,
      u * u * u * y +
        3 * u * u * t * y +
        3 * u * t * t * land +
        t * t * t * land,
    );
  }
  push(SEAM, land);
  // The jump from the impact to the bundle costs no distance: the streak
  // reappears in the bundle the moment it hits the wall.
  points.push({ x: SEAM, y: lane, s });
  push(W, lane);
  return points;
}

function pointAt(points: Point[], s: number): Point {
  for (let i = 1; i < points.length; i++) {
    const b = points[i];
    if (b.s >= s) {
      const a = points[i - 1];
      const k = b.s === a.s ? 0 : (s - a.s) / (b.s - a.s);
      return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k, s };
    }
  }
  return points[points.length - 1];
}

/** A soft round light, drawn once and stamped wherever a head or a hit is. */
function glowSprite(color: string) {
  const sprite = document.createElement('canvas');
  sprite.width = 64;
  sprite.height = 64;
  const g = sprite.getContext('2d');
  if (!g) return sprite;
  const gradient = g.createRadialGradient(32, 32, 0, 32, 32, 32);
  gradient.addColorStop(0, '#ffffff');
  gradient.addColorStop(0.18, color);
  gradient.addColorStop(1, 'transparent');
  g.fillStyle = gradient;
  g.fillRect(0, 0, 64, 64);
  return sprite;
}

/**
 * A hit on the wall: a flash, a shock ring spreading up and down the wall,
 * the wall lighting up around it, and a few sparks thrown back.
 */
function drawImpact(
  ctx: CanvasRenderingContext2D,
  impact: Impact,
  age: number,
  brand: string,
  sprite: HTMLCanvasElement,
) {
  const e = age / IMPACT_LIFE;
  const fade = 1 - e;

  ctx.globalAlpha = fade ** 3 * 0.8;
  const flash = 20 * (1 - e * 0.5);
  ctx.drawImage(sprite, SEAM - flash / 2, impact.y - flash / 2, flash, flash);

  ctx.globalAlpha = fade ** 2 * 0.45;
  ctx.strokeStyle = brand;
  ctx.lineWidth = 0.8;
  ctx.beginPath();
  ctx.ellipse(SEAM, impact.y, 2 + 9 * e, 4 + 30 * e, 0, 0, Math.PI * 2);
  ctx.stroke();

  const reach = 8 + 30 * e;
  const wall = ctx.createLinearGradient(
    0,
    impact.y - reach,
    0,
    impact.y + reach,
  );
  wall.addColorStop(0, 'transparent');
  wall.addColorStop(0.5, brand);
  wall.addColorStop(1, 'transparent');
  ctx.globalAlpha = fade * 0.55;
  ctx.strokeStyle = wall;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(SEAM, impact.y - reach);
  ctx.lineTo(SEAM, impact.y + reach);
  ctx.stroke();

  ctx.globalAlpha = fade ** 1.5 * 0.7;
  ctx.strokeStyle = brand;
  ctx.lineWidth = 0.8;
  for (const spark of impact.sparks) {
    const x = SEAM + spark.vx * age;
    const y = impact.y + spark.vy * age + 60 * age * age;
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x - spark.vx * 0.035, y - spark.vy * 0.035);
    ctx.stroke();
  }
}

/** A comet: the whole tail faint and wide, the front brighter, ending at a
 * hot head. Width, share of the tail, and opacity, back to front. */
const COMET: [number, number, number][] = [
  [1, 0.08, 6],
  [1, 0.3, 1],
  [0.5, 0.55, 1.4],
  [0.18, 0.9, 1.8],
];

function drawStreak(
  ctx: CanvasRenderingContext2D,
  streak: Streak,
  time: number,
  gradients: Record<Streak['tone'], CanvasGradient>,
  sprites: Record<Streak['tone'], HTMLCanvasElement>,
  impacts: Impact[],
) {
  const elapsed = time - streak.start;
  if (elapsed < 0) return;
  const cycle = (elapsed / streak.duration) % 1;
  const span = DASH * streak.length;
  const head = launch(cycle) * TRAVEL * streak.length;

  // Only a crossing seen between two frames is a hit. A streak drawn for the
  // first time (on load, or rebuilt when an app comes or goes) may already be
  // past the wall, and must not fire then.
  if (
    streak.previous !== null &&
    streak.previous < streak.wall &&
    head >= streak.wall &&
    head > streak.previous
  ) {
    impacts.push({
      y: streak.land,
      born: time,
      sparks: Array.from({ length: 4 }, () => {
        const angle = Math.PI * (0.55 + Math.random() * 0.9);
        const speed = 30 + Math.random() * 60;
        return { vx: Math.cos(angle) * speed, vy: Math.sin(angle) * speed };
      }),
    });
  }
  streak.previous = head;

  ctx.strokeStyle = gradients[streak.tone];
  for (const [part, alpha, width] of COMET) {
    const length = span * part;
    ctx.setLineDash([length, streak.length * 2]);
    ctx.lineDashOffset = length - head;
    ctx.globalAlpha = alpha;
    ctx.lineWidth = width;
    ctx.stroke(streak.path);
  }

  // The hot head, on the way in. Past the wall the bundle runs calm.
  if (head < streak.wall) {
    const at = pointAt(streak.points, head);
    ctx.setLineDash([]);
    ctx.globalAlpha = 0.75;
    ctx.drawImage(sprites[streak.tone], at.x - 5, at.y - 5, 10, 10);
  }
}

/** CSS custom properties, resolved for a canvas that cannot read them. */
function resolveColor(name: string) {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
}

export function Flow() {
  const reduced = useReducedMotion();
  const [sources, setSources] = useState(INITIAL);
  const stage = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const streaks = useRef<Streak[]>([]);
  // The moment the tiles' CSS flashes started counting, so the canvas keeps
  // the same clock as they do.
  const epoch = useRef(0);
  useEffect(() => {
    epoch.current = performance.now() / 1000;
  }, []);
  const [visible, setVisible] = useState(false);

  // Nothing runs while the section is off screen: the canvas stops drawing,
  // the flashes pause and the apps stop moving.
  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    const observer = new IntersectionObserver(([entry]) =>
      setVisible(entry.isIntersecting),
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (reduced || !visible) return;
    const id = setInterval(() => setSources(shuffle), 1500);
    return () => clearInterval(id);
  }, [reduced, visible]);

  // Three streaks per app, a third of a cycle apart, so the scene is always
  // busy without any one line being loud. Each leaves its app, eases flat,
  // hits the wall at its own height, runs along the wall into the middle and
  // leaves through the narrow bundle on the right. Rebuilt only when an app
  // comes or goes; the drawing loop reads them from the ref.
  useEffect(() => {
    // Streaks of apps that stayed keep where they were, so their next hit is
    // still caught.
    const before = new Map(
      streaks.current.map((streak) => [streak.key, streak.previous]),
    );
    streaks.current = sources.flatMap((source, slot) => {
      const x = cellX(colOf(source.cell)) + CELL;
      const y = cellY(rowOf(source.cell)) + CELL / 2;
      const order = slot - (LIVE - 1) / 2;
      const land = MID + order * LAND_STEP;
      const lane = MID + order * 0.6;
      const bend = x + (SEAM - x) * 0.5;
      const path = new Path2D(
        `M${x} ${y} C ${bend} ${y}, ${bend} ${land}, ${SEAM - 60} ${land} L ${SEAM} ${land} M ${SEAM} ${lane} L ${W} ${lane}`,
      );
      const points = sample(x, y, bend, land, lane);
      const length = points[points.length - 1].s;
      const wall = points[points.length - 3].s;
      const tone = TONES[(slot * 7) % TONES.length].id;
      const { duration, delay } = rhythm(source.platform);
      const born = (source.born ?? 0) / 1000;
      return [0, 1 / 3, 2 / 3].map((phase) => {
        const key = `${source.platform}-${source.cell}-${phase}`;
        return {
          key,
          path,
          points,
          length,
          wall,
          tone,
          land,
          duration,
          // On load the scene is already mid-flow; a newcomer starts from its
          // app, a third of a cycle apart.
          start:
            source.born === undefined
              ? epoch.current + delay - duration * phase
              : born + duration * phase,
          previous: before.get(key) ?? null,
        };
      });
    });
  }, [sources]);

  // The drawing loop: one canvas, at most 60 frames a second, only while the
  // section is on screen. Animating hundreds of SVG strokes made the browser
  // repaint the whole drawing every frame; this redraws a few dozen short
  // strokes into one bitmap.
  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext('2d');
    if (!el || !ctx || reduced || !visible) return;

    const brand = resolveColor('--surface-brand');
    const toneColor = {
      error: resolveColor('--sev-error'),
      warning: resolveColor('--sev-warning'),
      neutral: resolveColor('--fg-subtle'),
    };
    // Each tone up to the wall, lime from the wall on, fading out at the
    // right edge so the bundle does not stop dead.
    const gradients = Object.fromEntries(
      Object.entries(toneColor).map(([id, color]) => {
        const g = ctx.createLinearGradient(0, 0, W, 0);
        g.addColorStop(0, color);
        g.addColorStop(AT_WALL - 0.001, color);
        g.addColorStop(AT_WALL, brand);
        g.addColorStop(AT_WALL + 0.08, brand);
        g.addColorStop(0.82, `color-mix(in oklab, ${brand} 25%, transparent)`);
        g.addColorStop(1, 'transparent');
        return [id, g];
      }),
    ) as Record<Streak['tone'], CanvasGradient>;

    const resize = () => {
      const { width, height } = el.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio, 2);
      el.width = Math.round(width * dpr);
      el.height = Math.round(height * dpr);
      ctx.setTransform((width * dpr) / W, 0, 0, (height * dpr) / H, 0, 0);
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(el);

    const sprites = {
      brand: glowSprite(brand),
      error: glowSprite(toneColor.error),
      warning: glowSprite(toneColor.warning),
      neutral: glowSprite(toneColor.neutral),
    };
    const impacts: Impact[] = [];

    let frame = 0;
    let last = 0;
    const draw = (now: number) => {
      frame = requestAnimationFrame(draw);
      if (now - last < 1000 / 60 - 2) return;
      last = now;
      const time = now / 1000;
      ctx.globalCompositeOperation = 'source-over';
      ctx.globalAlpha = 1;
      ctx.clearRect(0, 0, W, H);
      // Light adds up where streaks cross, as light does.
      ctx.globalCompositeOperation = 'lighter';
      ctx.lineCap = 'round';

      for (const streak of streaks.current) {
        drawStreak(ctx, streak, time, gradients, sprites, impacts);
      }

      ctx.setLineDash([]);
      for (let i = impacts.length - 1; i >= 0; i--) {
        const age = time - impacts[i].born;
        if (age > IMPACT_LIFE) {
          impacts.splice(i, 1);
          continue;
        }
        drawImpact(ctx, impacts[i], age, brand, sprites.brand);
      }
      ctx.globalAlpha = 1;
      ctx.globalCompositeOperation = 'source-over';
    };
    frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [reduced, visible]);

  return (
    <section className="overflow-hidden py-16 sm:py-24">
      {/* Wider than the rest of the page: the drawing needs the room. */}
      <Reveal y={48} className="mx-auto w-full max-w-[96rem] px-6">
        <div
          ref={stage}
          data-paused={visible ? undefined : ''}
          // On a phone the drawing is cropped rather than shrunk: shown at
          // well over twice the screen's width, with the reporting apps, the
          // wall and the start of the bundle in view.
          className="flow-stage relative aspect-[1200/440] w-full max-lg:-ml-[60%] max-lg:w-[240%] sm:max-lg:-ml-[25%] sm:max-lg:w-[150%]"
        >
          <svg
            aria-hidden
            viewBox={`0 0 ${W} ${H}`}
            className="absolute inset-0 size-full overflow-visible"
          >
            <defs>
              {/* In user space: a vertical line has no width, so a gradient
                  sized to its bounding box would not paint at all. */}
              <linearGradient
                id="flow-wall"
                gradientUnits="userSpaceOnUse"
                x1={SEAM}
                x2={SEAM}
                y1={WALL_TOP}
                y2={WALL_BOTTOM}
              >
                <stop offset="0" stopColor="var(--fg)" stopOpacity="0" />
                <stop offset="0.5" stopColor="var(--fg)" stopOpacity="0.3" />
                <stop offset="1" stopColor="var(--fg)" stopOpacity="0" />
              </linearGradient>
            </defs>

            {Array.from({ length: COLS * ROWS }, (_, cell) => (
              <rect
                // A fixed grid: the position is the identity.
                // react-doctor-disable-next-line react-doctor/no-array-index-as-key
                key={cell}
                x={cellX(colOf(cell))}
                y={cellY(rowOf(cell))}
                width={CELL}
                height={CELL}
                rx={7}
                className="fill-none stroke-fg"
                strokeOpacity={0.04 + (colOf(cell) / (COLS - 1)) * 0.1}
                vectorEffect="non-scaling-stroke"
              />
            ))}

            <line
              x1={SEAM}
              x2={SEAM}
              y1={WALL_TOP}
              y2={WALL_BOTTOM}
              stroke="url(#flow-wall)"
              strokeWidth={1}
              vectorEffect="non-scaling-stroke"
            />
          </svg>

          <canvas
            ref={canvas}
            aria-hidden
            className="pointer-events-none absolute inset-0 size-full"
          />

          {sources.map((source) => {
            const { duration, delay } = rhythm(source.platform);
            const start = source.born === undefined ? delay : 0;
            return (
              <span
                key={`${source.platform}-${source.cell}`}
                className="flow-enter flow-ping absolute flex aspect-square w-[2.5%] items-center justify-center rounded-md border border-border bg-canvas"
                style={
                  {
                    ...pct(
                      cellX(colOf(source.cell)),
                      cellY(rowOf(source.cell)),
                    ),
                    animationDuration: `0.6s, ${duration}s`,
                    animationDelay: `0s, ${start}s`,
                  } as CSSProperties
                }
              >
                <PlatformIcon
                  platform={source.platform}
                  size={16}
                  className="w-[56%] rounded-sm"
                />
              </span>
            );
          })}

          <div
            className="absolute right-0 left-[56%] max-lg:hidden"
            style={{ bottom: `calc(${((H - BEAM_Y) / H) * 100}% + 3rem)` }}
          >
            <Words
              text="Every app. One place."
              delay={0.4}
              className="text-balance text-fg text-headline"
            />
          </div>
          <div
            className="absolute right-0 left-[56%] max-lg:hidden"
            style={{ top: `calc(${(BEAM_Y / H) * 100}% + 3rem)` }}
          >
            <Reveal delay={0.7}>
              <p className="max-w-md text-pretty text-fg-subtle text-lead">
                Your frontends, APIs, workers and mobile apps report to one
                Rustrak. Thousands of events go in; a short list of issues comes
                out.
              </p>
            </Reveal>
          </div>
        </div>

        <div className="mt-6 flex flex-col gap-4 lg:hidden">
          <Words
            text="Every app. One place."
            className="text-balance text-fg text-headline"
          />
          <p className="text-pretty text-fg-subtle text-lead">
            Your frontends, APIs, workers and mobile apps report to one Rustrak.
            Thousands of events go in; a short list of issues comes out.
          </p>
        </div>
      </Reveal>
    </section>
  );
}
