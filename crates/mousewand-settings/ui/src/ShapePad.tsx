import { useState, type PointerEvent } from "react";
import { saveShape } from "./api";

type Point = [number, number];

/** Drawings that teach a shape; more of them cover more ways of drawing it. */
const EXAMPLES = 3;

/** Draw a new shape a few times, name it, and save it to `shapes/`. */
export default function ShapePad({ onSaved, onCancel }: { onSaved: (name: string) => void; onCancel: () => void }) {
  const [examples, setExamples] = useState<Point[][]>([]);
  const [drawing, setDrawing] = useState<Point[] | null>(null);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const at = (e: PointerEvent<SVGSVGElement>): Point => {
    const box = e.currentTarget.getBoundingClientRect();
    return [e.clientX - box.left, e.clientY - box.top];
  };

  const done = examples.length >= EXAMPLES;

  const onDown = (e: PointerEvent<SVGSVGElement>) => {
    if (done || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrawing([at(e)]);
  };
  const onMove = (e: PointerEvent<SVGSVGElement>) => {
    if (drawing) setDrawing([...drawing, at(e)]);
  };
  const onUp = () => {
    // A click or a twitch isn't a drawing.
    if (drawing && extent(drawing) > 30) setExamples([...examples, drawing]);
    setDrawing(null);
  };

  const onSave = async () => {
    try {
      onSaved(await saveShape(name, examples));
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex basis-full flex-col gap-2 rounded-md border border-dashed border-zinc-300 p-3 dark:border-zinc-700">
      <p className="text-xs text-zinc-500">
        Draw the shape here {EXAMPLES} times with the left button, the way you'd draw it with the trigger. Size, tilt
        and direction don't matter.
      </p>
      <svg
        className={`h-48 w-full touch-none rounded bg-zinc-100 dark:bg-zinc-800/60 ${done ? "" : "cursor-crosshair"}`}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={() => setDrawing(null)}
      >
        {examples.map((points, i) => (
          <Line key={i} points={points} className="stroke-zinc-400 dark:stroke-zinc-500" />
        ))}
        {drawing && <Line points={drawing} className="stroke-accent" />}
      </svg>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-zinc-500">
          {examples.length} of {EXAMPLES}
        </span>
        <button
          className="button border border-zinc-300 hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
          disabled={examples.length === 0}
          onClick={(e) => {
            e.preventDefault();
            setExamples([]);
          }}
        >
          Clear
        </button>
        <input
          className="field ml-auto w-36"
          value={name}
          placeholder="Name, e.g. star"
          onChange={(e) => {
            setName(e.target.value);
            setError(null);
          }}
        />
        <button
          className="button bg-accent text-white hover:brightness-110"
          disabled={examples.length === 0 || !name.trim()}
          onClick={(e) => {
            e.preventDefault();
            onSave();
          }}
        >
          Save shape
        </button>
        <button
          className="button text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
          onClick={(e) => {
            e.preventDefault();
            onCancel();
          }}
        >
          Cancel
        </button>
      </div>
      {error && <p className="text-sm text-red-500">{error}</p>}
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
