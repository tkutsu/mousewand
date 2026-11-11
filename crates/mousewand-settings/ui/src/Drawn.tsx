// The two ways to draw: a stroke typed as directions, or a shape picked by name.

import { arrows } from "./keys";
import { cleanStroke } from "./check";

/** A stroke field, with its arrows on a line of their own under it unless they show elsewhere. */
export function StrokeInput({
  value,
  arrows: showArrows = true,
  onChange,
}: {
  value: string;
  arrows?: boolean;
  onChange: (stroke: string) => void;
}) {
  if (!showArrows) return <StrokeField value={value} onChange={onChange} />;
  return (
    <div className="flex flex-col gap-1">
      <StrokeField value={value} onChange={onChange} />
      <span className="h-4 text-[15px] leading-4 tracking-[0.2em] text-accent" aria-label="as arrows">
        {arrows(value)}
      </span>
    </div>
  );
}

function StrokeField({ value, onChange }: { value: string; onChange: (stroke: string) => void }) {
  return (
    <input
      className={`field w-24 font-mono uppercase ${value ? "" : "invalid"}`}
      value={value}
      placeholder="DR"
      title="U D L R, and diagonals 7 ↖ 9 ↗ 1 ↙ 3 ↘"
      onChange={(e) => onChange(cleanStroke(e.target.value))}
    />
  );
}

/** A shape picker; choosing "Draw a New One…" calls `onDraw`. */
export function ShapeSelect({
  value,
  shapes,
  drawing,
  onChange,
  onDraw,
}: {
  value: string;
  shapes: string[];
  drawing: boolean;
  onChange: (shape: string) => void;
  onDraw: () => void;
}) {
  return (
    <select
      className="field"
      value={drawing ? "" : value}
      onChange={(e) => (e.target.value ? onChange(e.target.value) : onDraw())}
    >
      {shapes.map((s) => (
        <option key={s}>{s}</option>
      ))}
      <option value="">Draw a New One…</option>
    </select>
  );
}
