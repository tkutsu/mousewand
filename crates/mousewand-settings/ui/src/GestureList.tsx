import { useEffect, useState, type ReactNode } from "react";
import { newGesture, type Gesture } from "./api";
import { arrows, comboOf } from "./keys";
import ShapePad from "./ShapePad";

type Props = {
  gestures: Gesture[];
  /** The shapes a gesture can use. */
  shapes: string[];
  onChange: (gestures: Gesture[]) => void;
  /** A shape was drawn and saved. */
  onNewShape: (name: string) => void;
};

export default function GestureList({ gestures, shapes, onChange, onNewShape }: Props) {
  const update = (i: number, patch: Partial<Gesture>) =>
    onChange(gestures.map((g, j) => (j === i ? { ...g, ...patch } : g)));
  const remove = (i: number) => onChange(gestures.filter((_, j) => j !== i));

  return (
    <div className="flex flex-col gap-2">
      {gestures.length === 0 && (
        <p className="py-6 text-center text-sm text-zinc-500">No gestures yet.</p>
      )}
      {gestures.map((g, i) => (
        <Row
          key={i}
          gesture={g}
          shapes={shapes}
          onChange={(patch) => update(i, patch)}
          onNewShape={onNewShape}
          onRemove={() => remove(i)}
        />
      ))}
      <button
        className="button self-start text-accent hover:bg-accent/10"
        onClick={() => onChange([...gestures, newGesture()])}
      >
        + Add gesture
      </button>
    </div>
  );
}

function Row({
  gesture: g,
  shapes,
  onChange,
  onNewShape,
  onRemove,
}: {
  gesture: Gesture;
  shapes: string[];
  onChange: (patch: Partial<Gesture>) => void;
  onNewShape: (name: string) => void;
  onRemove: () => void;
}) {
  const [drawing, setDrawing] = useState(false);
  const continuous = g.input === "stroke" && g.kind !== "oneshot";
  return (
    <div className="flex flex-wrap items-end gap-2 rounded-lg border border-zinc-200 bg-white p-3 dark:border-zinc-800 dark:bg-zinc-900/60">
      <Field label="When">
        <select
          className="field"
          value={g.input}
          onChange={(e) => {
            const input = e.target.value as Gesture["input"];
            const value = { stroke: "", shape: shapes[0] ?? "circle", wheel: "up", click: "right" }[input];
            onChange({ input, value, kind: "oneshot", step: null, hold: null });
            setDrawing(false);
          }}
        >
          <option value="stroke">Stroke</option>
          <option value="shape">Shape</option>
          <option value="wheel">Wheel</option>
          <option value="click">Click</option>
        </select>
      </Field>

      {g.input === "stroke" && (
        <Field label="Stroke">
          <div className="flex items-center gap-2">
            <input
              className="field w-24 font-mono uppercase"
              value={g.value}
              placeholder="DR"
              onChange={(e) => onChange({ value: e.target.value.toUpperCase().replace(/[^UDLR7913]/g, "") })}
            />
            <span className="min-w-12 text-lg tracking-widest text-accent" title="U D L R, diagonals 7 9 1 3">
              {arrows(g.value)}
            </span>
          </div>
        </Field>
      )}
      {g.input === "shape" && (
        <Field label="Shape">
          <select
            className="field"
            value={drawing ? "" : g.value}
            onChange={(e) => (e.target.value ? onChange({ value: e.target.value }) : setDrawing(true))}
          >
            {shapes.map((s) => (
              <option key={s}>{s}</option>
            ))}
            <option value="">Draw a new one…</option>
          </select>
        </Field>
      )}
      {g.input === "wheel" && (
        <Field label="Direction">
          <select className="field" value={g.value} onChange={(e) => onChange({ value: e.target.value })}>
            <option value="up">Up</option>
            <option value="down">Down</option>
            <option value="left">Left</option>
            <option value="right">Right</option>
          </select>
        </Field>
      )}
      {g.input === "click" && (
        <Field label="Button">
          <select className="field" value={g.value} onChange={(e) => onChange({ value: e.target.value })}>
            <option value="left">Left</option>
            <option value="right">Right</option>
          </select>
        </Field>
      )}

      {g.input === "stroke" && (
        <Field label="Fires">
          <select
            className="field"
            value={g.kind}
            onChange={(e) => {
              const kind = e.target.value as Gesture["kind"];
              onChange({ kind, hold: kind === "hold" ? (g.hold ?? "alt") : null });
            }}
          >
            <option value="oneshot">On release</option>
            <option value="repeat">Repeatedly</option>
            <option value="hold">Repeatedly, holding keys</option>
          </select>
        </Field>
      )}
      {continuous && (
        <Field label="Every (px)">
          <input
            className="field w-20"
            type="number"
            min={10}
            value={g.step ?? 80}
            onChange={(e) => onChange({ step: Number(e.target.value) || null })}
          />
        </Field>
      )}
      {g.input === "stroke" && g.kind === "hold" && (
        <Field label="Hold down">
          <KeysInput value={g.hold ?? ""} onChange={(hold) => onChange({ hold })} />
        </Field>
      )}

      <Field label="Name">
        <input
          className="field w-32"
          value={g.name ?? ""}
          placeholder="Back"
          title="What hints show for this gesture"
          onChange={(e) => onChange({ name: e.target.value || null })}
        />
      </Field>
      <Field label="Do">
        <select
          className="field"
          value={g.action}
          onChange={(e) => onChange({ action: e.target.value as Gesture["action"] })}
        >
          <option value="keys">Press keys</option>
          <option value="command">Run command</option>
          <option value="dispatch">Hyprland dispatch</option>
        </select>
      </Field>
      <Field label={g.action === "keys" ? "Keys" : g.action === "command" ? "Command" : "Dispatcher"} grow>
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
      </Field>

      <button
        className="button ml-auto text-zinc-400 hover:bg-red-500/10 hover:text-red-500"
        onClick={onRemove}
        title="Remove"
      >
        ✕
      </button>

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

function Field({ label, grow, children }: { label: string; grow?: boolean; children: ReactNode }) {
  return (
    <label className={`flex flex-col gap-1 ${grow ? "min-w-48 flex-1" : ""}`}>
      <span className="label">{label}</span>
      {children}
    </label>
  );
}

/** A key combo field with a "Record" button that captures the next shortcut. */
function KeysInput({ value, onChange }: { value: string; onChange: (combo: string) => void }) {
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
    <div className="flex gap-1">
      <input
        className="field w-full min-w-32 font-mono"
        value={recording ? "" : value}
        placeholder={recording ? "Press a shortcut… (Esc cancels)" : "ctrl+shift+t"}
        readOnly={recording}
        onChange={(e) => onChange(e.target.value)}
      />
      <button
        className={`button whitespace-nowrap ${
          recording
            ? "bg-red-500 text-white"
            : "border border-zinc-300 hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
        }`}
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
