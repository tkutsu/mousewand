// A trail as the overlay draws it, mid-stroke: glow around a solid body
// with a lighter middle, narrowing and fading toward the tail, its colors
// blending from head to tail.

import { useId } from "react";
import type { ThemeInfo } from "./api";

type Rgb = [number, number, number];

const POINTS = 64;
const [WIDTH, HEIGHT] = [480, 112];

/** A gentle wave from the tail on the left to the head on the right. */
const path = Array.from({ length: POINTS }, (_, i) => {
  const t = i / (POINTS - 1);
  return [48 + t * (WIDTH - 96), HEIGHT / 2 + Math.sin(t * Math.PI * 1.7 - 0.5) * 24] as const;
});

/** The color at `life`: the first at the head (1), the last at the tail (0). */
function colorAt(colors: Rgb[], life: number): Rgb {
  const last = colors.length - 1;
  const x = (1 - life) * last;
  const i = Math.min(Math.floor(x), Math.max(last - 1, 0));
  const [a, b] = [colors[i], colors[Math.min(i + 1, last)]];
  return a.map((c, k) => c + (b[k] - c) * (x - i)) as Rgb;
}

const css = (rgb: Rgb) => `rgb(${rgb.map((c) => Math.round(c * 255)).join(" ")})`;
const lighter = (rgb: Rgb) => rgb.map((c) => c + (1 - c) * 0.3) as Rgb;

export default function TrailPreview({ theme }: { theme: ThemeInfo | undefined }) {
  const fade = useId();
  const colors = theme?.colors ?? [];

  // One pass of segments at `scale` of the width. Each pass is opaque inside
  // its group, so joints don't stack; the mask fades the whole trail.
  const pass = (scale: number, opacity: number, tint: (rgb: Rgb) => Rgb = (c) => c) => (
    <g key={scale} opacity={opacity}>
      {path.slice(1).map(([x, y], i) => {
        const life = (i + 1) / (POINTS - 1);
        const [px, py] = path[i];
        return (
          <line
            key={i}
            x1={px}
            y1={py}
            x2={x}
            y2={y}
            stroke={css(tint(colorAt(colors, life)))}
            strokeWidth={Math.max(theme!.width * Math.pow(life, 0.7) * scale, 0.5)}
            strokeLinecap="round"
          />
        );
      })}
    </g>
  );

  return (
    <div className="h-28 w-full overflow-hidden rounded-lg border border-line bg-gradient-to-br from-[#5b7fa8] via-[#8a76a6] to-[#d29a7a]">
      {colors.length > 0 && (
        <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} className="size-full" aria-label={`The ${theme!.name} trail`}>
          <defs>
            <linearGradient id={`${fade}-g`}>
              <stop offset="0.08" stopColor="white" stopOpacity={0} />
              <stop offset="0.92" stopColor="white" stopOpacity={1} />
            </linearGradient>
            <mask id={fade}>
              <rect width={WIDTH} height={HEIGHT} fill={`url(#${fade}-g)`} />
            </mask>
          </defs>
          <g mask={`url(#${fade})`}>
            {[
              [1.0, 0.08],
              [0.7, 0.14],
              [0.45, 0.25],
            ].map(([scale, alpha]) => pass(scale, theme!.glow * alpha))}
            {pass(0.32, 1)}
            {pass(0.12, 1, lighter)}
          </g>
        </svg>
      )}
    </div>
  );
}
