import { useRef, useState, type PointerEvent } from "react";
import { alike, saveShape } from "./api";
import { sentence } from "./check";
import { Problem } from "./ui";

type Point = [number, number];

/**
 * How many drawings teach a shape. Each covers another way of drawing it;
 * past a handful they mostly widen what matches.
 */
const EXAMPLES = 5;

/**
 * Teaches a shape by drawing it a few times, like enrolling a fingerprint:
 * a bar fills with each drawing, and one that doesn't look like the others
 * is turned away. With `name`, it redraws that shape.
 */
export default function ShapePad({
  name: fixed,
  onSaved,
  onCancel,
}: {
  name?: string;
  onSaved: (name: string) => void;
  onCancel: () => void;
}) {
  const [examples, setExamples] = useState<Point[][]>([]);
  const [drawing, setDrawing] = useState<Point[] | null>(null);
  // The drawing as it comes in. Moves can outrun renders, so they add to
  // this, not to the state of the last render.
  const live = useRef<Point[] | null>(null);
  const [name, setName] = useState(fixed ?? "");
  const [error, setError] = useState<string | null>(null);
  // Bumped to replay the shake when a drawing is turned away.
  const [shake, setShake] = useState(0);

  const at = (e: PointerEvent<SVGSVGElement>): Point => {
    const box = e.currentTarget.getBoundingClientRect();
    return [Math.round(e.clientX - box.left), Math.round(e.clientY - box.top)];
  };

  const done = examples.length >= EXAMPLES;

  const onDown = (e: PointerEvent<SVGSVGElement>) => {
    if (done || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    live.current = [at(e)];
    setDrawing(live.current);
    setError(null);
  };
  const onMove = (e: PointerEvent<SVGSVGElement>) => {
    if (!live.current) return;
    live.current = [...live.current, at(e)];
    setDrawing(live.current);
  };
  const onUp = async () => {
    const drawn = live.current;
    live.current = null;
    setDrawing(null);
    // A click or a twitch isn't a drawing.
    if (!drawn || extent(drawn) < 30) return;
    if (examples.length > 0 && !(await alike(examples, drawn))) {
      setError("That one didn't look like the others. Try again.");
      setShake((n) => n + 1);
      return;
    }
    setExamples((all) => (all.length < EXAMPLES ? [...all, drawn] : all));
  };

  const onSave = async () => {
    try {
      onSaved(await saveShape(name, examples, fixed !== undefined));
    } catch (e) {
      setError(sentence(e));
    }
  };

  const prompt = done
    ? fixed
      ? "Got it. Save to use the new drawings."
      : "Got it. Name the shape and save it."
    : examples.length === 0
      ? "Draw the shape here with the left button, the way you'd draw it with the trigger."
      : "Again. Size, tilt and direction don't matter.";

  return (
    <div className="flex flex-col gap-2.5 rounded-lg border border-dashed border-line p-3">
      <p className="text-[12px] text-ink-2">{prompt}</p>
      <svg
        key={shake}
        className={`h-48 w-full touch-none rounded-md bg-ink/[0.04] ${done ? "" : "cursor-crosshair"} ${
          shake ? "animate-[shake_0.3s]" : ""
        }`}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={() => {
          live.current = null;
          setDrawing(null);
        }}
      >
        {examples.map((points, i) => (
          <Line key={i} points={points} className="stroke-ink-3" />
        ))}
        {drawing && <Line points={drawing} className="stroke-accent" />}
      </svg>

      <div className="flex items-center gap-3">
        <div className="flex flex-1 gap-1" role="progressbar" aria-valuemax={EXAMPLES} aria-valuenow={examples.length}>
          {Array.from({ length: EXAMPLES }, (_, i) => (
            <span
              key={i}
              className={`h-1.5 flex-1 rounded-full transition-colors duration-300 ${
                i < examples.length ? "bg-accent" : "bg-ink/10"
              }`}
            />
          ))}
        </div>
        <span className="w-10 text-right text-[12px] text-ink-2 tabular-nums">
          {examples.length}/{EXAMPLES}
        </span>
      </div>

      {error && <Problem>{error}</Problem>}

      <div className="flex flex-wrap items-center gap-2">
        <button
          className="button"
          disabled={examples.length === 0}
          onClick={(e) => {
            e.preventDefault();
            setExamples([]);
            setError(null);
          }}
        >
          Start Over
        </button>
        {fixed === undefined && (
          <input
            className="field ml-auto w-36"
            value={name}
            placeholder="Name, e.g. star"
            onChange={(e) => {
              setName(e.target.value);
              setError(null);
            }}
          />
        )}
        <button
          className={`button primary ${fixed === undefined ? "" : "ml-auto"}`}
          disabled={!done || !name.trim()}
          onClick={(e) => {
            e.preventDefault();
            onSave();
          }}
        >
          Save Shape
        </button>
        <button
          className="button plain text-ink-2"
          onClick={(e) => {
            e.preventDefault();
            onCancel();
          }}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function Line({ points, className }: { points: Point[]; className: string }) {
  return (
    <polyline
      points={points.join(" ")}
      className={`fill-none ${className}`}
      strokeWidth={3}
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  );
}

/** The longer side of the points' bounding box. */
function extent(points: Point[]) {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return Math.max(Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys));
}
