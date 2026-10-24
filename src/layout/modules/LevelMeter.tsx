import { useEffect, useRef, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { bus } from '../../lib/bus.ts';
import { showContextMenu } from '../../lib/menu.ts';
import { METER_STYLES, type MeterStyle } from '../../lib/settings.ts';
import { useAppearance } from '../../stores/appearance.ts';
import { useSettings } from '../../stores/settings.ts';
import { mixColors } from '../../theme/color.ts';
import { Dynamics, barCount, idleLevels, resample } from './spectrum.ts';
import styles from './modules.module.css';

/** Without a reading for this long nothing is coming (a pause, silence, a stall). */
const STALE_MS = 900;

/**
 * Room for the glow: the canvas reaches this far into the module's padding and draws this
 * far inside its own edges, so the blur isn't cut off at the sides.
 */
const GLOW = 8;

interface Palette {
  accent: string;
  light: string;
  text: string;
}

/** The theme's colors where the meter is (the accent may follow the artwork). */
function palette(element: HTMLElement): Palette {
  const css = getComputedStyle(element);
  const accent = css.getPropertyValue('--vitra-color-accent').trim() || '#3FA9F5';
  const text = css.getPropertyValue('--vitra-color-text').trim() || '#FFFFFF';
  return { accent, light: mixColors(accent, text, 0.45), text };
}

interface Frame {
  width: number;
  height: number;
  values: readonly number[];
  peaks: readonly number[] | null;
  colors: Palette;
}

/** Capsules side by side, with a gap in proportion to their width. */
function columns(width: number, count: number) {
  const gap = Math.max(2, Math.min(6, (width / count) * 0.32));
  const bar = Math.max(1, (width - gap * (count - 1)) / count);
  return { gap, bar };
}

function drawBars(ctx: CanvasRenderingContext2D, frame: Frame, mirror: boolean): void {
  const { width, height, values, peaks, colors } = frame;
  const { gap, bar } = columns(width, values.length);
  const cap = Math.max(2, Math.min(3, bar * 0.4));
  // Room for the caps above the tallest bar (and below it when mirrored).
  const room = height - (cap + 3) * (mirror ? 2 : 1);
  const middle = height / 2;
  const body = new Path2D();
  values.forEach((value, i) => {
    const x = i * (bar + gap);
    // A silent bar is a dot, never nothing.
    const length = Math.max(bar, value * room);
    const y = mirror ? middle - length / 2 : height - length;
    body.roundRect(x, y, bar, length, bar / 2);
  });
  const gradient = mirror
    ? ctx.createLinearGradient(0, 0, 0, height)
    : ctx.createLinearGradient(0, height, 0, 0);
  if (mirror) {
    gradient.addColorStop(0, colors.light);
    gradient.addColorStop(0.5, colors.accent);
    gradient.addColorStop(1, colors.light);
  } else {
    gradient.addColorStop(0, colors.accent);
    gradient.addColorStop(1, colors.light);
  }
  ctx.save();
  // One fill for all bars, so the glow is drawn once.
  ctx.shadowColor = colors.accent;
  ctx.shadowBlur = Math.min(12, bar * 1.6);
  ctx.fillStyle = gradient;
  ctx.fill(body);
  ctx.restore();

  if (!peaks) return;
  const caps = new Path2D();
  peaks.forEach((peak, i) => {
    const value = values[i] ?? 0;
    if (peak - value < 0.015) return;
    const x = i * (bar + gap);
    const reach = Math.max(bar, peak * room);
    if (mirror) {
      caps.roundRect(x, middle - reach / 2 - cap - 2, bar, cap, cap / 2);
      caps.roundRect(x, middle + reach / 2 + 2, bar, cap, cap / 2);
    } else {
      caps.roundRect(x, height - reach - cap - 2, bar, cap, cap / 2);
    }
  });
  ctx.save();
  ctx.globalAlpha = 0.8;
  ctx.fillStyle = colors.light;
  ctx.fill(caps);
  ctx.restore();
}

function drawWave(ctx: CanvasRenderingContext2D, frame: Frame): void {
  const { width, height, values, peaks, colors } = frame;
  const room = height - 6;
  const step = values.length > 1 ? width / (values.length - 1) : width;
  const point = (i: number, value: number) => ({
    x: i * step,
    y: height - 2 - Math.max(0.03, value) * room,
  });
  // A smooth line through the bars' tops: quadratic curves between midpoints.
  const curve = (levels: readonly number[]) => {
    const path = new Path2D();
    const first = point(0, levels[0] ?? 0);
    path.moveTo(first.x, first.y);
    for (let i = 1; i < levels.length; i++) {
      const from = point(i - 1, levels[i - 1] ?? 0);
      const to = point(i, levels[i] ?? 0);
      path.quadraticCurveTo(from.x, from.y, (from.x + to.x) / 2, (from.y + to.y) / 2);
    }
    const last = point(levels.length - 1, levels.at(-1) ?? 0);
    path.lineTo(last.x, last.y);
    return path;
  };
  const line = curve(values);
  const area = new Path2D(line);
  area.lineTo(width, height);
  area.lineTo(0, height);
  area.closePath();

  // The fill fades from the line's highest point, so a quiet wave still has one.
  const top = Math.min(...values.map((value, i) => point(i, value).y));
  const fill = ctx.createLinearGradient(0, top, 0, height);
  fill.addColorStop(0, colors.accent);
  fill.addColorStop(1, 'transparent');
  ctx.save();
  ctx.globalAlpha = 0.45;
  ctx.fillStyle = fill;
  ctx.fill(area);
  ctx.restore();

  if (peaks) {
    ctx.save();
    ctx.globalAlpha = 0.35;
    ctx.strokeStyle = colors.light;
    ctx.lineWidth = 1;
    ctx.stroke(curve(peaks));
    ctx.restore();
  }

  ctx.save();
  ctx.shadowColor = colors.accent;
  ctx.shadowBlur = 10;
  ctx.strokeStyle = colors.light;
  ctx.lineWidth = 2;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  ctx.stroke(line);
  ctx.restore();
}

/**
 * The level meter module: the music's spectrum (`player:spectrum`, twenty bands from 40 Hz
 * to 16 kHz) drawn on a canvas sixty times a second. Bars ease towards each reading (up
 * fast, down slowly), peak caps hold and fall, and a quiet wave runs while nothing plays.
 * As many bars as the width has room for; three styles, chosen from its context menu.
 * With Reduce Motion it draws a few times a second and stays still when nothing plays.
 */
export function LevelMeter({ className }: { className?: string }) {
  const { t } = useTranslation();
  const canvas = useRef<HTMLCanvasElement>(null);
  const style = useSettings((s) => s.settings.meter.style);
  const update = useSettings((s) => s.update);
  const motion = useAppearance((s) => s.effects.motion);

  useEffect(() => {
    const element = canvas.current;
    const ctx = element?.getContext('2d');
    if (!element || !ctx) return;

    let bands: number[] = [];
    let readAt = 0;
    const off = bus.on('player:spectrum', (values) => {
      bands = values;
      readAt = performance.now();
    });

    let width = 0;
    let height = 0;
    const resize = () => {
      const rect = element.getBoundingClientRect();
      const ratio = window.devicePixelRatio || 1;
      width = rect.width;
      height = rect.height;
      element.width = Math.max(1, Math.round(width * ratio));
      element.height = Math.max(1, Math.round(height * ratio));
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    };
    const observer = new ResizeObserver(resize);
    observer.observe(element);
    resize();

    const dynamics = new Dynamics();
    let colors = palette(element);
    let colorsAt = performance.now();
    let last = performance.now();
    let frame = 0;
    const loop = (now: number) => {
      frame = requestAnimationFrame(loop);
      const live = bands.length > 0 && now - readAt < STALE_MS;
      // Reduce Motion: a calm meter, redrawn a few times a second. While nothing plays, a
      // lighter thirty frames a second are enough for the quiet wave.
      const interval = !motion ? 120 : live ? 0 : 30;
      if (now - last < interval) return;
      const dt = Math.min(0.12, (now - last) / 1000);
      last = now;
      if (now - colorsAt > 500) {
        colors = palette(element);
        colorsAt = now;
      }
      const count = barCount(width - GLOW * 2);
      const targets = live
        ? resample(bands, count)
        : motion
          ? idleLevels(count, now / 1000)
          : resample([0.05], count);
      if (!motion && !live && dynamics.settled(targets)) return;
      dynamics.step(targets, dt);
      ctx.clearRect(0, 0, width, height);
      ctx.save();
      ctx.translate(GLOW, GLOW);
      const drawn: Frame = {
        width: width - GLOW * 2,
        height: height - GLOW * 2,
        values: dynamics.values,
        peaks: motion && live ? dynamics.peaks : null,
        colors,
      };
      if (style === 'wave') drawWave(ctx, drawn);
      else drawBars(ctx, drawn, style === 'mirror');
      ctx.restore();
    };
    frame = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      off();
    };
  }, [style, motion]);

  const chooseStyle = (event: MouseEvent) => {
    event.preventDefault();
    const choose = (next: MeterStyle) =>
      void update((s) => ({ ...s, meter: { ...s.meter, style: next } })).catch((err: unknown) =>
        console.error('Choosing the meter style failed', err),
      );
    showContextMenu(
      METER_STYLES.map((option) => ({
        label: t(`layout.meterStyles.${option}`),
        checked: option === style,
        action: () => choose(option),
      })),
    ).catch((err: unknown) => console.error('The meter menu failed', err));
  };

  return (
    <Glass
      as="section"
      className={`${styles.levelMeter} ${className ?? ''}`}
      aria-label={t('layout.modules.level')}
      priority={1}
      onContextMenu={chooseStyle}
    >
      <div className={styles.spectrumPlace}>
        <canvas
          ref={canvas}
          className={styles.spectrum}
          style={{
            inset: -GLOW,
            width: `calc(100% + ${GLOW * 2}px)`,
            height: `calc(100% + ${GLOW * 2}px)`,
          }}
          role="img"
          aria-label={t('layout.spectrum')}
        />
      </div>
    </Glass>
  );
}
