import { useEffect, useState, type ReactNode } from "react";
import { newGesture, type Gesture, type ShapeInfo } from "./api";
import { problems } from "./check";
import { ShapeSelect, StrokeInput } from "./Drawn";
import { arrows, comboOf } from "./keys";
import ShapePad from "./ShapePad";
import { Problem, Section, ShapeThumb } from "./ui";

type Props = {
  title: string;
  note?: ReactNode;
  gestures: Gesture[];
  /** The shapes a gesture can use. */
  shapes: ShapeInfo[];
  onChange: (gestures: Gesture[]) => void;
  /** A shape was drawn and saved. */
  onNewShape: (name: string) => void;
};

export default function GestureList({ title, note, gestures, shapes, onChange, onNewShape }: Props) {
  const update = (i: number, patch: Partial<Gesture>) =>
    onChange(gestures.map((g, j) => (j === i ? { ...g, ...patch } : g)));
  const remove = (i: number) => onChange(gestures.filter((_, j) => j !== i));
  const found = problems(gestures);

  return (
    <Section
      title={title}
      note={note}
      action={
        <button className="button plain h-6 px-2 text-accent" onClick={() => onChange([...gestures, newGesture()])}>
          <span className="text-[15px] leading-none">+</span> Add Gesture
        </button>
      }
    >
      {gestures.length === 0 && <p className="px-3 py-5 text-center text-ink-2">No gestures yet.</p>}
      {gestures.map((g, i) => (
        <Row
          key={i}
          gesture={g}
          problem={found[i]}
          shapes={shapes}
          onChange={(patch) => update(i, patch)}
          onNewShape={onNewShape}
          onRemove={() => remove(i)}
        />
      ))}
    </Section>
  );
}

function Row({
  gesture: g,
  problem,
  shapes,
  onChange,
  onNewShape,
  onRemove,
}: {
  gesture: Gesture;
  problem: string | null;
  shapes: ShapeInfo[];
  onChange: (patch: Partial<Gesture>) => void;
  onNewShape: (name: string) => void;
  onRemove: () => void;
}) {
  const [drawing, setDrawing] = useState(false);
  const continuous = g.input === "stroke" && g.kind !== "oneshot";
  return (
    <div className="group flex flex-col gap-2 px-4 pt-2.5 pb-3">
      <div className="flex items-center gap-3">
        <input
          className="-ml-1.5 h-8 min-w-0 flex-1 rounded-md border border-transparent bg-transparent px-1.5 text-[15px] font-semibold outline-none transition placeholder:font-normal placeholder:text-ink-3 hover:border-line focus:border-accent focus:bg-control focus:ring-[3px] focus:ring-accent/25"
          value={g.name ?? ""}
          placeholder="Name this gesture"
          title="What hints show for this gesture"
          onChange={(e) => onChange({ name: e.target.value || null })}
        />
        <Preview gesture={g} shapes={shapes} />
        <button
          className="button plain w-7 px-0 text-ink-3 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 hover:text-danger"
          onClick={onRemove}
          title="Remove this gesture"
          aria-label="Remove this gesture"
        >
          ✕
        </button>
      </div>

      <Line label="When">
        <select
          className="field"
          value={g.input}
          onChange={(e) => {
            const input = e.target.value as Gesture["input"];
            const value = { stroke: "", shape: shapes[0]?.name ?? "circle", wheel: "up", click: "right" }[input];
            onChange({ input, value, kind: "oneshot", step: null, hold: null });
            setDrawing(false);
          }}
        >
          <option value="stroke">Stroke</option>
          <option value="shape">Shape</option>
          <option value="wheel">Wheel</option>
          <option value="click">Click</option>
        </select>
        {g.input === "stroke" && (
          <StrokeInput value={g.value} arrows={false} onChange={(value) => onChange({ value })} />
        )}
        {g.input === "shape" && (
          <ShapeSelect
            value={g.value}
            shapes={shapes.map((s) => s.name)}
            drawing={drawing}
            onChange={(value) => onChange({ value })}
            onDraw={() => setDrawing(true)}
          />
        )}
        {g.input === "wheel" && (
          <select className="field" value={g.value} onChange={(e) => onChange({ value: e.target.value })}>
            <option value="up">Up</option>
            <option value="down">Down</option>
            <option value="left">Left</option>
            <option value="right">Right</option>
          </select>
        )}
        {g.input === "click" && (
          <select className="field" value={g.value} onChange={(e) => onChange({ value: e.target.value })}>
            <option value="left">Left</option>
            <option value="right">Right</option>
          </select>
        )}
        {g.input === "stroke" && (
          <select
            className="field"
            value={g.kind}
            onChange={(e) => {
              const kind = e.target.value as Gesture["kind"];
              onChange({ kind, hold: kind === "hold" ? (g.hold ?? "alt") : null });
            }}
          >
            <option value="oneshot">Fires on Release</option>
            <option value="repeat">Repeats</option>
            <option value="hold">Repeats, Holding Keys</option>
          </select>
        )}
        {continuous && (
          <span className="flex items-center gap-1.5 text-ink-2">
            every
            <input
              className="field w-14 text-right"
              type="number"
              min={10}
              value={g.step ?? 80}
              onChange={(e) => onChange({ step: Number(e.target.value) || null })}
            />
            px
          </span>
        )}
      </Line>

      {g.input === "stroke" && g.kind === "hold" && (
        <Line label="Hold">
          <div className="w-56">
            <KeysInput value={g.hold ?? ""} onChange={(hold) => onChange({ hold })} />
          </div>
        </Line>
      )}

      <Line label="Do">
        <select
          className="field"
          value={g.action}
          onChange={(e) => onChange({ action: e.target.value as Gesture["action"] })}
        >
          <option value="keys">Press Keys</option>
          <option value="command">Run Command</option>
          <option value="dispatch">Hyprland Dispatch</option>
        </select>
        <div className="min-w-44 flex-1">
          {g.action === "keys" ? (
            <KeysInput value={g.target} onChange={(target) => onChange({ target })} />
          ) : (
            <input
              className="field w-full font-mono"
              value={g.target}
              placeholder={g.action === "command" ? "notify-send hi" : 'hl.dsp.focus({ workspace = "e+1" })'}
              onChange={(e) => onChange({ target: e.target.value })}
            />
          )}
        </div>
      </Line>

      {problem && (
        <div className="pl-14">
          <Problem>{problem}</Problem>
        </div>
      )}
      {g.input === "shape" && drawing && (
        <ShapePad
          onSaved={(name) => {
            onNewShape(name);
            onChange({ value: name });
            setDrawing(false);
          }}
          onCancel={() => setDrawing(false)}
        />
      )}
    </div>
  );
}

/** One line of a gesture: a quiet word, then its controls. */
function Line({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-12 shrink-0 text-[12px] text-ink-2">{label}</span>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">{children}</div>
    </div>
  );
}

/** What to draw or press, at the end of the name. */
function Preview({ gesture: g, shapes }: { gesture: Gesture; shapes: ShapeInfo[] }) {
  if (g.input === "stroke") {
    return <span className="text-[17px] tracking-[0.15em] text-accent">{arrows(g.value)}</span>;
  }
  if (g.input === "shape") {
    const shape = shapes.find((s) => s.name === g.value);
    return shape ? <ShapeThumb outline={shape.outline} className="size-6 text-accent" /> : null;
  }
  const wheel: Record<string, string> = { up: "↑", down: "↓", left: "←", right: "→" };
  return (
    <span className="text-[12px] font-medium text-accent">
      {g.input === "wheel" ? `Wheel ${wheel[g.value]}` : `${g.value === "left" ? "Left" : "Right"} click`}
    </span>
  );
}

/** A key combo field with a "Record" button that captures the next shortcut. */
export function KeysInput({ value, onChange }: { value: string; onChange: (combo: string) => void }) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape" && !e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey) {
        setRecording(false);
        return;
      }
      const combo = comboOf(e);
      if (combo) {
        onChange(combo);
        setRecording(false);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onChange]);

  return (
    <div className="flex gap-1.5">
      <input
        className="field w-full min-w-28 font-mono"
        value={recording ? "" : value}
        placeholder={recording ? "Press a shortcut… (Esc cancels)" : "ctrl+shift+t"}
        readOnly={recording}
        onChange={(e) => onChange(e.target.value)}
      />
      <button
        className={`button ${recording ? "recording" : ""}`}
        onClick={(e) => {
          e.preventDefault();
          setRecording(!recording);
        }}
      >
        {recording ? "● Recording" : "Record"}
      </button>
    </div>
  );
}
