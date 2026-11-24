import { ICONS, PLATFORMS } from './icons';

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
 * The grid, the wall and the first twenty tiles are HTML from the build
 * (`flow.astro`, from the geometry below); `startFlow` adds the light. The
 * streaks are drawn on one canvas at up to 60 frames a second, and only while
 * the section is on screen. Under reduced motion nothing moves.
 */

export const W = 1200;
export const H = 440;
export const SEAM = W / 2;
const MID = H / 2;

export const COLS = 9;
export const ROWS = 9;
export const CELL = 30;
const GAP = 12;
const GRID_X = 40;
const GRID_Y = (H - (ROWS * CELL + (ROWS - 1) * GAP)) / 2;
/** Reporting apps live in the columns nearest the seam. */
const FIRST_LIVE_COL = 3;
/** How many apps are reporting at once. */
const LIVE = 20;
/** The wall's top, where the incoming lines land, and the bundle out. */
export const WALL_TOP = MID - 115;
export const WALL_BOTTOM = MID + 115;
const LAND_STEP = 10;
export const BEAM_Y = MID;

/** What arrives: errors, warnings and noise. Never the brand: that is what
 * Rustrak makes of them, on the other side of the wall. */
const TONES = ['error', 'warning', 'neutral'] as const;
type Tone = (typeof TONES)[number];

/** Where the wall sits along the gradients, which run across the drawing. */
const AT_WALL = SEAM / W;

export interface Source {
  cell: number;
  platform: string;
  /** Joined after the page loaded, at this `performance.now()`: its streaks
   * start from the app instead of mid-flight. */
  born?: number;
}

/** A fixed starting arrangement: the one the build draws. */
export const INITIAL: Source[] = Array.from(
  { length: COLS * ROWS },
  (_, i) => i,
)
  // Cells near the seam, in a scrambled but fixed order.
  .filter((cell) => cell % COLS >= FIRST_LIVE_COL)
  .sort((a, b) => ((a * 37) % 101) - ((b * 37) % 101))
  .slice(0, LIVE)
  // Top to bottom, so each app's slot on the wall matches where it sits.
  .sort((a, b) => a - b)
  .map((cell, index) => ({ cell, platform: PLATFORMS[index] }));

export const colOf = (cell: number) => cell % COLS;
export const rowOf = (cell: number) => Math.floor(cell / COLS);
export const cellX = (col: number) => GRID_X + col * (CELL + GAP);
export const cellY = (row: number) => GRID_Y + row * (CELL + GAP);

/** A tile's place and its flash's clock, as inline style. */
export function tileStyle(source: Source) {
  const { duration, delay } = rhythm(source.platform);
  const start = source.born === undefined ? delay : 0;
  return [
    `left:${(cellX(colOf(source.cell)) / W) * 100}%`,
    `top:${(cellY(rowOf(source.cell)) / H) * 100}%`,
    `animation-duration:0.6s,${duration}s`,
    `animation-delay:0s,${start}s`,
  ].join(';');
}

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
 * the wall, so nothing else in the scene moves when this happens. Returns the
 * slot that changed, or -1.
 */
function shuffle(sources: Source[]) {
  const used = new Set(sources.map((s) => s.cell));
  const out = Math.floor(Math.random() * sources.length);
  const row = rowForSlot(out);
  const free = Array.from({ length: COLS * ROWS }, (_, i) => i).filter(
    (cell) =>
      colOf(cell) >= FIRST_LIVE_COL &&
      !used.has(cell) &&
      Math.abs(rowOf(cell) - row) <= 1,
  );
  if (free.length === 0) return -1;
  const idle = PLATFORMS.filter((p) => !sources.some((s) => s.platform === p));
  sources[out] = {
    cell: free[Math.floor(Math.random() * free.length)],
    platform: idle[Math.floor(Math.random() * idle.length)],
    born: performance.now(),
  };
  return out;
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
  tone: Tone;
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
  gradients: Record<Tone, CanvasGradient>,
  sprites: Record<Tone, HTMLCanvasElement>,
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

/**
 * Three streaks per app, a third of a cycle apart, so the scene is always
 * busy without any one line being loud. Each leaves its app, eases flat, hits
 * the wall at its own height, runs along the wall into the middle and leaves
 * through the narrow bundle on the right. Streaks of apps that stayed keep
 * where they were, so their next hit is still caught.
 */
function buildStreaks(sources: Source[], epoch: number, before: Streak[]) {
  const previous = new Map(before.map((s) => [s.key, s.previous]));
  return sources.flatMap((source, slot) => {
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
    const tone = TONES[(slot * 7) % TONES.length];
    const { duration, delay } = rhythm(source.platform);
    const born = (source.born ?? 0) / 1000;
    return [0, 1 / 3, 2 / 3].map((phase): Streak => {
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
            ? epoch + delay - duration * phase
            : born + duration * phase,
        previous: previous.get(key) ?? null,
      };
    });
  });
}

/** CSS custom properties, resolved for a canvas that cannot read them. */
const resolveColor = (name: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** A tile for an app that just joined, cut from the template in the page. */
function makeTile(template: HTMLTemplateElement, source: Source) {
  const tile = template.content.firstElementChild?.cloneNode(
    true,
  ) as HTMLElement;
  tile.setAttribute('style', tileStyle(source));
  tile.querySelector('img')?.setAttribute('src', ICONS[source.platform]);
  return tile;
}

export function startFlow(stage: HTMLElement) {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const canvas = stage.querySelector('canvas');
  const ctx = canvas?.getContext('2d');
  const template = stage.querySelector('template');
  const tiles = [...stage.querySelectorAll<HTMLElement>('[data-tile]')];
  if (!canvas || !ctx || !template) return;

  // The tiles' CSS flashes started counting when the page was styled; the
  // canvas keeps their clock.
  const ping = tiles[0]
    ?.getAnimations()
    .find((a) => (a as CSSAnimation).animationName === 'flow-ping');
  const epoch = Number(ping?.startTime ?? performance.now()) / 1000;

  const sources = INITIAL.map((s) => ({ ...s }));
  let streaks = buildStreaks(sources, epoch, []);

  const brand = resolveColor('--surface-brand');
  const toneColor: Record<Tone, string> = {
    error: resolveColor('--sev-error'),
    warning: resolveColor('--sev-warning'),
    neutral: resolveColor('--fg-subtle'),
  };
  // Each tone up to the wall, lime from the wall on, fading out at the right
  // edge so the bundle does not stop dead.
  const gradients = Object.fromEntries(
    TONES.map((tone) => {
      const g = ctx.createLinearGradient(0, 0, W, 0);
      g.addColorStop(0, toneColor[tone]);
      g.addColorStop(AT_WALL - 0.001, toneColor[tone]);
      g.addColorStop(AT_WALL, brand);
      g.addColorStop(AT_WALL + 0.08, brand);
      g.addColorStop(0.82, `color-mix(in oklab, ${brand} 25%, transparent)`);
      g.addColorStop(1, 'transparent');
      return [tone, g];
    }),
  ) as Record<Tone, CanvasGradient>;
  const sprites = Object.fromEntries(
    TONES.map((tone) => [tone, glowSprite(toneColor[tone])]),
  ) as Record<Tone, HTMLCanvasElement>;
  const brandSprite = glowSprite(brand);
  const impacts: Impact[] = [];

  const resize = () => {
    const { width, height } = canvas.getBoundingClientRect();
    const dpr = Math.min(devicePixelRatio, 2);
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    ctx.setTransform((width * dpr) / W, 0, 0, (height * dpr) / H, 0, 0);
  };
  new ResizeObserver(resize).observe(canvas);

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
    for (const streak of streaks) {
      drawStreak(ctx, streak, time, gradients, sprites, impacts);
    }
    ctx.setLineDash([]);
    for (let i = impacts.length - 1; i >= 0; i--) {
      const age = time - impacts[i].born;
      if (age > IMPACT_LIFE) {
        impacts.splice(i, 1);
        continue;
      }
      drawImpact(ctx, impacts[i], age, brand, brandSprite);
    }
  };

  const step = () => {
    const slot = shuffle(sources);
    if (slot < 0) return;
    const tile = makeTile(template, sources[slot]);
    tiles[slot].replaceWith(tile);
    tiles[slot] = tile;
    streaks = buildStreaks(sources, epoch, streaks);
  };

  // Off screen the canvas stops drawing and the apps stop moving. The CSS
  // flashes keep running: pausing them would drift them off the canvas clock.
  let shuffling = 0;
  new IntersectionObserver(([entry]) => {
    cancelAnimationFrame(frame);
    clearInterval(shuffling);
    if (!entry.isIntersecting) return;
    frame = requestAnimationFrame(draw);
    shuffling = window.setInterval(step, 1500);
  }).observe(stage);
}
