/**
 * Streaks of light from the orbiting apps into the wordmark: the effect of the
 * docs' `home/flow.tsx`, aimed at the mark. One canvas, at most 60 frames a
 * second, and only while the panel is on screen. The tiles keep turning on the
 * compositor; a streak only reads where its tile is at the moment it leaves.
 */

import {
  type CurvePoint,
  clearOf,
  type Point,
  rectEdge,
  sampleCurve,
} from './strike-path';

/**
 * The letters' bodies inside the wordmark's box, as shares of its height: the
 * x-height down to the baseline. Ascenders are too thin to land on, so a streak
 * that stopped at the box would stop in the air above the `u`.
 */
const BODY_TOP = 0.325;
const BODY_BOTTOM = 0.983;
/** How many letters to either side a hit lights up. */
const GLOW_REACH = 3;
/** How far a hit moves the whole word, in pixels. */
const THUD = 1.2;

/** How curved a streak may be, as shares of its length, gentlest first. */
const BENDS = [0.2, 0.32, 0.45, 0.6, 0.75];
/**
 * How far a streak keeps from another tile, as a share of a tile's width: its
 * half-diagonal, plus how far one drifts in a streak's flight. Tiles and orbits
 * are sized in rem, so all of it grows with the browser's font size.
 */
const CLEARANCE = 1.1;
/** Where a streak leaves its own tile, as a share of a tile's width. */
const TILE_EDGE = 0.55;

const TONES = ['--sev-error', '--sev-warning', '--fg-subtle'];
const TAIL = 0.4;
const IMPACT_LIFE = 0.5;
/** A streak's width, opacity, and share of the tail, back to front. */
const COMET = [
  [1, 0.08, 6],
  [1, 0.3, 1],
  [0.5, 0.55, 1.4],
  [0.18, 0.9, 1.8],
];

interface Strike {
  path: Path2D;
  points: CurvePoint[];
  length: number;
  tone: number;
  start: number;
  duration: number;
  land: Point;
  heading: number;
  hit: boolean;
}

interface Impact {
  at: Point;
  born: number;
  sparks: { vx: number; vy: number }[];
}

function pointAt(points: Strike['points'], s: number): Point {
  const index = points.findIndex((point) => point.s >= s);
  if (index < 1) return points[0];
  const a = points[index - 1];
  const b = points[index];
  const k = (s - a.s) / (b.s - a.s);
  return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
}

/** The gentlest curve from `from` to `to` that passes no other tile, if any. */
function findCurve(
  from: Point,
  to: Point,
  obstacles: Point[],
  clearance: number,
) {
  const heading = Math.atan2(to.y - from.y, to.x - from.x);
  const distance = Math.hypot(to.x - from.x, to.y - from.y);
  const side = Math.random() < 0.5 ? -1 : 1;

  for (const bend of BENDS) {
    for (const direction of [side, -side]) {
      const offset = distance * bend * direction;
      const control = {
        x: (from.x + to.x) / 2 - Math.sin(heading) * offset,
        y: (from.y + to.y) / 2 + Math.cos(heading) * offset,
      };
      const points = sampleCurve(from, control, to);
      if (clearOf(points, obstacles, clearance)) return { control, points };
    }
  }
  return null;
}

/** A soft round light, drawn once and stamped wherever a head or a hit is. */
function glowSprite(color: string, light: boolean) {
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
  // On a light page a white core is a hole, and fading through transparent
  // black is a grey halo: keep the gradient's alpha and repaint it one colour.
  if (light) {
    g.globalCompositeOperation = 'source-in';
    g.fillStyle = color;
    g.fillRect(0, 0, 64, 64);
  }
  return sprite;
}

/** Starts the effect and returns how to stop it. */
export function runStrikes(canvas: HTMLCanvasElement, stage: HTMLElement) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const styles = getComputedStyle(document.documentElement);
  const light = document.documentElement.dataset.theme === 'light';
  // Multiplied, a hit tints the page and leaves the black letters readable.
  if (light) canvas.style.mixBlendMode = 'multiply';
  // The brand as a line or a letter, and as a glow: one colour in the dark
  // theme, while the light theme needs a deeper lime to read on white.
  const brand = styles.getPropertyValue('--fg-brand').trim();
  const glow = styles.getPropertyValue('--surface-brand').trim();
  const fg = styles.getPropertyValue('--fg').trim();
  const easing = styles.getPropertyValue('--ease-entrance').trim();
  const colors = TONES.map((name) => styles.getPropertyValue(name).trim());
  const sprites = colors.map((color) => glowSprite(color, light));
  const brandSprite = glowSprite(glow, light);
  const tiles = [...stage.querySelectorAll<HTMLElement>('[data-orbit-tile]')];
  const mark = stage.querySelector('[data-orbit-mark] svg');
  if (!mark) return;
  const letters = [...mark.querySelectorAll('path')];

  // The widest pass is a glow on a dark page. On white its hard edge reads as a
  // dark outline around the streak.
  const comet = light ? COMET.slice(1) : COMET;

  const strikes: Strike[] = [];
  const impacts: Impact[] = [];
  let nextLaunch = 0;

  const launch = (time: number) => {
    const box = canvas.getBoundingClientRect();
    const centers = tiles.map((tile) => {
      const rect = tile.getBoundingClientRect();
      return {
        x: rect.left + rect.width / 2 - box.left,
        y: rect.top + rect.height / 2 - box.top,
      };
    });
    const index = Math.floor(Math.random() * tiles.length);
    const tileCenter = centers[index];
    // Cropped by the panel, or behind its copy: nothing to see it leave.
    if (
      tileCenter.x < 0 ||
      tileCenter.x > box.width ||
      tileCenter.y < 0 ||
      tileCenter.y > box.height * 0.85
    ) {
      return;
    }

    const rect = mark.getBoundingClientRect();
    const land = rectEdge(
      tileCenter,
      {
        x: rect.left + rect.width / 2 - box.left,
        y: rect.top + rect.height * ((BODY_TOP + BODY_BOTTOM) / 2) - box.top,
      },
      rect.width / 2,
      (rect.height * (BODY_BOTTOM - BODY_TOP)) / 2,
    );
    const away = Math.atan2(land.y - tileCenter.y, land.x - tileCenter.x);
    const tileWidth = tiles[index].offsetWidth;
    const from = {
      x: tileCenter.x + Math.cos(away) * tileWidth * TILE_EDGE,
      y: tileCenter.y + Math.sin(away) * tileWidth * TILE_EDGE,
    };

    // Boxed in by the others for now: this one waits for another turn.
    const curve = findCurve(
      from,
      land,
      centers.filter((_, i) => i !== index),
      tileWidth * CLEARANCE,
    );
    if (!curve) return;
    const { control, points } = curve;

    const path = new Path2D();
    path.moveTo(from.x, from.y);
    path.quadraticCurveTo(control.x, control.y, land.x, land.y);

    const end = points[points.length - 1];
    const before = points[points.length - 2];
    const tone = Math.floor(Math.random() * TONES.length);
    tiles[index].animate([{ borderColor: colors[tone] }, {}], {
      duration: 500,
    });
    strikes.push({
      path,
      points,
      length: end.s,
      tone,
      start: time,
      duration: 0.7 + end.s / 700,
      land,
      // The way it arrives, which a curve makes different from the way it left.
      heading: Math.atan2(end.y - before.y, end.x - before.x),
      hit: false,
    });
  };

  /**
   * The word takes the hit as one heavy block: it gives a pixel and is back at
   * once, so nothing wobbles, while the lime runs out from the letter struck
   * through its neighbours, weaker and a beat later. Only the mark repaints.
   */
  const absorb = ({ land, heading }: Strike) => {
    mark.animate(
      [
        {
          translate: `${Math.cos(heading) * THUD}px ${Math.sin(heading) * THUD}px`,
        },
        { translate: '0 0' },
      ],
      { duration: 180, easing },
    );

    const box = canvas.getBoundingClientRect();
    const rect = mark.getBoundingClientRect();
    const across = (land.x + box.left - rect.left) / rect.width;
    const struck = Math.round(
      Math.min(Math.max(across, 0), 1) * (letters.length - 1),
    );

    letters.forEach((letter, i) => {
      const away = Math.abs(i - struck);
      const strength = 1 - away / (GLOW_REACH + 1);
      if (strength <= 0) return;
      letter.animate(
        [
          {
            fill: `color-mix(in oklab, ${brand} ${strength * 100}%, ${fg})`,
          },
          // Spelled out: Chrome interpolates a `color-mix()` towards the
          // implicit `currentcolor` into near-black.
          { fill: fg },
        ],
        { duration: 600, delay: away * 40, easing },
      );
    });
  };

  const drawStrike = (strike: Strike, head: number) => {
    const span = strike.length * TAIL;
    ctx.strokeStyle = colors[strike.tone];
    for (const [part, alpha, width] of comet) {
      const dash = span * part;
      ctx.setLineDash([dash, strike.length * 2]);
      ctx.lineDashOffset = dash - head;
      ctx.globalAlpha = alpha;
      ctx.lineWidth = width;
      ctx.stroke(strike.path);
    }
    if (head < strike.length) {
      const at = pointAt(strike.points, head);
      ctx.globalAlpha = 0.75;
      ctx.drawImage(sprites[strike.tone], at.x - 5, at.y - 5, 10, 10);
    }
  };

  const drawImpact = ({ at, born, sparks }: Impact, time: number) => {
    const age = time - born;
    const e = age / IMPACT_LIFE;
    const fade = 1 - e;

    ctx.setLineDash([]);
    ctx.globalAlpha = fade ** 3 * 0.8;
    const flash = 34 * (1 - e * 0.5);
    ctx.drawImage(
      brandSprite,
      at.x - flash / 2,
      at.y - flash / 2,
      flash,
      flash,
    );

    ctx.globalAlpha = fade ** 2 * 0.45;
    ctx.strokeStyle = brand;
    ctx.lineWidth = 0.8;
    ctx.beginPath();
    ctx.arc(at.x, at.y, 3 + 22 * e, 0, Math.PI * 2);
    ctx.stroke();

    ctx.globalAlpha = fade ** 1.5 * 0.7;
    for (const { vx, vy } of sparks) {
      const x = at.x + vx * age;
      const y = at.y + vy * age;
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x - vx * 0.035, y - vy * 0.035);
      ctx.stroke();
    }
  };

  /** Moves a strike on and draws it; true once its tail has landed too. */
  const advance = (strike: Strike, time: number) => {
    const span = strike.length * TAIL;
    // Accelerating: it falls into the mark.
    const k = Math.min((time - strike.start) / strike.duration, 1);
    const head = k ** 1.8 * (strike.length + span);
    if (head >= strike.length && !strike.hit) {
      strike.hit = true;
      absorb(strike);
      impacts.push({
        at: strike.land,
        born: time,
        sparks: Array.from({ length: 6 }, () => {
          const angle = strike.heading + Math.PI + (Math.random() - 0.5) * 1.8;
          const speed = 50 + Math.random() * 80;
          return { vx: Math.cos(angle) * speed, vy: Math.sin(angle) * speed };
        }),
      });
    }
    if (k < 1) drawStrike(strike, head);
    return k === 1;
  };

  // Light adds up where streaks cross, as light does. On white that only washes
  // them out.
  const blend = light ? 'source-over' : 'lighter';

  let frame = 0;
  let last = 0;
  const draw = (now: number) => {
    frame = requestAnimationFrame(draw);
    if (now - last < 1000 / 60 - 2) return;
    last = now;
    const time = now / 1000;

    if (time >= nextLaunch) {
      launch(time);
      nextLaunch = time + 0.2 + Math.random() * 0.35;
    }

    ctx.globalCompositeOperation = 'source-over';
    ctx.globalAlpha = 1;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.globalCompositeOperation = blend;
    ctx.lineCap = 'round';

    for (let i = strikes.length - 1; i >= 0; i--) {
      if (advance(strikes[i], time)) strikes.splice(i, 1);
    }

    for (let i = impacts.length - 1; i >= 0; i--) {
      if (time - impacts[i].born > IMPACT_LIFE) impacts.splice(i, 1);
      else drawImpact(impacts[i], time);
    }
  };

  const resize = () => {
    const { width, height } = canvas.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio, 2);
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    // What is in flight was aimed at where the wordmark used to be.
    strikes.length = 0;
    impacts.length = 0;
  };
  const sizes = new ResizeObserver(resize);
  sizes.observe(canvas);

  // Below `lg` the panel is not displayed, and nothing should be drawing.
  const visibility = new IntersectionObserver(([entry]) => {
    cancelAnimationFrame(frame);
    if (entry.isIntersecting) frame = requestAnimationFrame(draw);
  });
  visibility.observe(canvas);

  return () => {
    cancelAnimationFrame(frame);
    sizes.disconnect();
    visibility.disconnect();
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    canvas.style.mixBlendMode = '';
  };
}
