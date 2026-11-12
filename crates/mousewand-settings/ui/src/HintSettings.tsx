// The hint: on or off, how it looks, and an example drawn the way the
// overlay draws it.

import { useState, type CSSProperties } from "react";
import type { Hint } from "./api";
import { Row, Section, Segmented, Switch } from "./ui";

type Look = Pick<Hint, "text" | "glyph" | "background" | "radius">;

/** Ready-made looks. Dark is the built-in default, as the daemon writes it. */
const STYLES: [string, Look][] = [
  ["Dark", { text: "#f2f2f7", glyph: "#9ec4ff", background: "#1c1f26", radius: 14 }],
  ["Light", { text: "#1d1d1f", glyph: "#007aff", background: "#f5f5f7", radius: 14 }],
  ["Glass", { text: "#ffffff", glyph: "#ffffff", background: "#00000080", radius: 20 }],
  ["Ember", { text: "#fff4ec", glyph: "#ff8a3d", background: "#2a1a12", radius: 8 }],
];

const VERTICAL = ["top", "center", "bottom"] as const;
const HORIZONTAL = ["left", "center", "right"] as const;

/** "top right" → [0, 2]; a missing word is center, as the daemon reads it. */
function cell(position: string): [number, number] {
  const words = position.split(/\s+/);
  const v = VERTICAL.findIndex((w) => w !== "center" && words.includes(w));
  const h = HORIZONTAL.findIndex((w) => w !== "center" && words.includes(w));
  return [v < 0 ? 1 : v, h < 0 ? 1 : h];
}

function position(v: number, h: number): string {
  return v === 1 && h === 1 ? "center" : `${VERTICAL[v]} ${HORIZONTAL[h]}`;
}

/** A "#rrggbb(aa)" color with its alpha scaled, as CSS. */
function dim(color: string, by: number): string {
  const hex = color.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const a = hex.length === 8 ? parseInt(hex.slice(6), 16) / 255 : 1;
  return `rgb(${r} ${g} ${b} / ${a * by})`;
}

export default function HintSettings({ hint, onChange }: { hint: Hint; onChange: (hint: Hint) => void }) {
  const set = (patch: Partial<Hint>) => onChange({ ...hint, ...patch });
  const [example, setExample] = useState<"drawing" | "failed">("drawing");
  const [v, h] = cell(hint.position);
  const chosen = STYLES.find(([, look]) => (Object.keys(look) as (keyof Look)[]).every((k) => look[k] === hint[k]));

  /** A color picker that keeps the color's alpha. */
  const color = (key: "text" | "glyph" | "failed" | "background", label: string, title: string) => (
    <label className="flex items-center gap-1.5 text-[12px] text-ink-2" title={title}>
      <input
        type="color"
        className="h-6 w-8 cursor-pointer rounded border border-line bg-control p-0.5"
        value={hint[key].slice(0, 7)}
        onChange={(e) => set({ [key]: e.target.value + hint[key].slice(7) })}
      />
      {label}
    </label>
  );

  return (
    <Section title="Hint" note="Hints show the stroke as you draw it, and what it does.">
      <Row label="Show a hint while drawing">
        <Switch label="Show a hint" on={hint.enabled} onChange={(enabled) => set({ enabled })} />
      </Row>
      <div className={`flex flex-col divide-y divide-line transition-opacity ${hint.enabled ? "" : "opacity-50"}`}>
        <div className="flex flex-col items-center gap-2 p-3">
          <Preview hint={hint} failed={example === "failed"} />
          <Segmented
            value={example}
            options={[
              ["drawing", "Drawing"],
              ["failed", "No match"],
            ]}
            onChange={setExample}
          />
        </div>
        <Row label="Style">
          <div className="flex gap-1.5">
            {STYLES.map(([name, look]) => (
              <button
                key={name}
                type="button"
                title={`Use the ${name} look`}
                className={`flex h-7 items-center gap-1.5 rounded-md border px-2 text-[12px] font-medium ${
                  chosen?.[0] === name ? "border-accent ring-[3px] ring-accent/25" : "border-line"
                }`}
                style={{ background: look.background, color: look.text }}
                onClick={() => set(look)}
              >
                <span style={{ color: look.glyph }}>↓</span>
                {name}
              </button>
            ))}
          </div>
        </Row>
        <Row label="Colors">
          {color("text", "Text", "What the gesture does")}
          {color("glyph", "Arrows", "The stroke so far")}
          {color("failed", "Failed", "The arrows when nothing matches")}
          {color("background", "Background", "Behind it all; its see-through part is kept")}
        </Row>
        <Row label="Position" hint="Where on the screen it shows">
          <div className="grid grid-cols-3 gap-0.5 rounded-md border border-line bg-ink/[0.04] p-0.5">
            {VERTICAL.map((_, row) =>
              HORIZONTAL.map((_, col) => {
                const here = row === v && col === h;
                return (
                  <button
                    key={`${row}${col}`}
                    type="button"
                    aria-label={position(row, col)}
                    title={position(row, col)}
                    className={`grid h-4 w-6 place-items-center rounded-[3px] ${here ? "bg-accent" : "hover:bg-ink/10"}`}
                    onClick={() => set({ position: position(row, col) })}
                  >
                    <span className={`size-1 rounded-full ${here ? "bg-white" : "bg-ink/30"}`} />
                  </button>
                );
              }),
            )}
          </div>
        </Row>
        <Row label="Size">
          <input
            className="field w-20 text-right"
            type="number"
            min={8}
            title="Text size"
            value={hint.size}
            onChange={(e) => set({ size: Number(e.target.value) })}
          />
          <span className="w-5 text-ink-2">px</span>
        </Row>
        <Row label="Corner radius">
          <input
            className="field w-20 text-right"
            type="number"
            min={0}
            title="0 for square corners"
            value={hint.radius}
            onChange={(e) => set({ radius: Number(e.target.value) })}
          />
          <span className="w-5 text-ink-2">px</span>
        </Row>
        <Row label="Font" hint="Any installed family, or sans-serif for the system's">
          <input className="field w-40" value={hint.font} onChange={(e) => set({ font: e.target.value })} />
        </Row>
        <Row label="List what's next" hint="The gestures the stroke could still become">
          <Switch label="List what's next" on={hint.next} onChange={(next) => set({ next })} />
        </Row>
      </div>
    </Section>
  );
}

/** An arrow pointing right in a unit square, turned, as the overlay draws it. */
function Arrow({ angle, size, color }: { angle: number; size: number; color: string }) {
  return (
    <svg viewBox="0 0 1 1" width={size} height={size} className="shrink-0" style={{ marginRight: size * 0.05 }}>
      <path
        d="M0.18 0.5H0.82M0.56 0.24L0.82 0.5L0.56 0.76"
        transform={`rotate(${angle} 0.5 0.5)`}
        fill="none"
        stroke={color}
        strokeWidth={0.12}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

const ANGLES: Record<string, number> = { R: 0, D: 90, L: 180, U: 270 };

const PADDING = 14;
/** The mock screen's margin, smaller than the real 64 px. */
const MARGIN = 20;

/**
 * A mock screen with the hint for a stroke that's half drawn. As on screen,
 * the main line goes where the position says, and the next moves hang off
 * it: above at the bottom, below elsewhere.
 */
function Preview({ hint, failed }: { hint: Hint; failed: boolean }) {
  const size = hint.size;
  const small = size * 0.82;
  // A stroke that can't become any gesture: only its arrows, in the failed color.
  const main = failed
    ? { stroke: "DL", label: "", size, arrow: dim(hint.failed, 1), text: "", next: false }
    : { stroke: "D", label: "Reload", size, arrow: dim(hint.glyph, 1), text: dim(hint.text, 0.75), next: false };
  const next = hint.next && !failed
    ? [
        { stroke: "R", label: "Close tab" },
        { stroke: "RU", label: "Reopen closed tab" },
      ].map((r) => ({ ...r, size: small, arrow: dim(hint.glyph, 0.85), text: dim(hint.text, 0.7), next: true }))
    : [];
  const [v, h] = cell(hint.position);
  const above = v === 2;
  const rows = above ? [...next, main] : [main, ...next];
  // Next moves line their labels up after the longest stroke.
  const nextArrows = Math.max(0, ...next.map((r) => r.stroke.length * r.size * 1.05));
  // The panel the main line alone would make, placed like the real one.
  const mainHeight = size * 1.45 + PADDING * 2;
  const panel: CSSProperties = {
    background: hint.background,
    borderRadius: hint.radius,
    padding: PADDING,
    ...[{ left: MARGIN }, { left: "50%", translate: "-50% 0" }, { right: MARGIN }][h],
    ...[{ top: MARGIN }, { top: `calc(50% - ${mainHeight / 2}px)` }, { bottom: MARGIN }][v],
  };
  return (
    <div
      className="relative h-56 w-full overflow-hidden rounded-lg border border-line bg-gradient-to-br from-[#5b7fa8] via-[#8a76a6] to-[#d29a7a]"
      style={{ fontFamily: hint.font }}
    >
      <div className="absolute flex flex-col opacity-90" style={panel}>
        {rows.map((row, i) => (
          <div
            key={row.stroke}
            className="flex items-center whitespace-nowrap"
            style={{ height: row.size * 1.45, marginTop: i > 0 && row.next !== rows[i - 1].next ? size * 0.35 : 0 }}
          >
            <div className="flex" style={{ width: row.next ? nextArrows : undefined }}>
              {[...row.stroke].map((d, j) => (
                <Arrow key={j} angle={ANGLES[d]} size={row.size} color={row.arrow} />
              ))}
            </div>
            {row.label && (
              <span style={{ fontSize: row.size, color: row.text, marginLeft: row.size * 0.45, lineHeight: 1 }}>
                {row.label}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
