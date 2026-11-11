import { useEffect, useState, type ReactNode } from "react";
import { MODIFIERS, menuLabel, type Group } from "./actions";
import { newGesture, newStep, type Gesture, type ShapeInfo, type Step } from "./api";
import { problems } from "./check";
import { ShapeSelect, StrokeInput } from "./Drawn";
import { arrows, comboOf } from "./keys";
import ShapePad from "./ShapePad";
import { Problem, Section, ShapeThumb, Toggles } from "./ui";

type Props = {
  title: string;
  note?: ReactNode;
  gestures: Gesture[];
  /** The shapes a gesture can use. */
  shapes: ShapeInfo[];
  /** Ready-made actions to offer, by group. */
  readyMade: Group[];
  /** Whether mousewand can see which modifiers are held. */
  keyboard: boolean;
  onChange: (gestures: Gesture[]) => void;
  /** A shape was drawn and saved. */
  onNewShape: (name: string) => void;
};

export default function GestureList({ title, note, gestures, onChange, ...rest }: Props) {
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
          onChange={(patch) => update(i, patch)}
          onRemove={() => remove(i)}
          {...rest}
        />
      ))}
    </Section>
  );
}

type RowProps = Omit<Props, "title" | "note" | "gestures" | "onChange"> & {
  gesture: Gesture;
  problem: string | null;
  onChange: (patch: Partial<Gesture>) => void;
  onRemove: () => void;
};

function Row({ gesture: g, problem, shapes, readyMade, keyboard, onChange, onNewShape, onRemove }: RowProps) {
  const [drawing, setDrawing] = useState(false);
  const continuous = g.input === "stroke" && g.kind !== "oneshot";
  // Held modifiers would mix into keys typed while they're down, so only
  // gestures that fire on release can use them.
  const canHoldKeys = (g.input === "stroke" || g.input === "shape") && !continuous;
  const held = g.with ? g.with.split("+") : [];

  const setStep = (i: number, step: Step) => onChange({ steps: g.steps.map((s, j) => (j === i ? step : s)) });
  const pickReadyMade = (i: number, group: number, index: number) => {
    const action = readyMade[group].actions[index];
    const steps = g.steps.map((s, j) => (j === i ? { ...action.step } : s));
    // A fresh gesture takes the action's name; a named one keeps its own.
    onChange(!g.name && g.steps.length === 1 ? { steps, name: action.name } : { steps });
  };

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
            const keepWith = input === "stroke" || input === "shape";
            onChange({ input, value, kind: "oneshot", step: null, hold: null, with: keepWith ? g.with : null });
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
              onChange({
                kind,
                hold: kind === "hold" ? (g.hold ?? "alt") : null,
                with: kind === "oneshot" ? g.with : null,
                steps: kind === "hold" ? [g.steps.find((s) => s.action === "keys") ?? newStep()] : g.steps,
              });
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
        {canHoldKeys && (
          <span className="ml-auto flex items-center gap-2" title="Only while these keys are held on the keyboard">
            <span className="text-[12px] text-ink-3">holding</span>
            <Toggles
              values={held}
              options={MODIFIERS.map((m) => ({ key: m.key, label: m.label }))}
              onChange={(keys) => {
                // Combos name them in a set order: ctrl, shift, alt, super.
                const ordered = MODIFIERS.map((m) => m.key).filter((k) => keys.includes(k));
                onChange({ with: ordered.length ? ordered.join("+") : null });
              }}
            />
          </span>
        )}
      </Line>
      {g.with && !keyboard && (
        <div className="pl-[4.5rem]">
          <Problem>
            mousewand can't read the keyboard yet, so this won't fire. The README's Modifiers section shows how to
            allow it.
          </Problem>
        </div>
      )}

      {g.input === "stroke" && g.kind === "hold" && (
        <Line label="Hold">
          <div className="w-56">
            <KeysInput value={g.hold ?? ""} onChange={(hold) => onChange({ hold })} />
          </div>
        </Line>
      )}

      {g.steps.map((step, i) => (
        <Line key={i} label={i === 0 ? "Do" : "Then"}>
          <StepEditor
            step={step}
            readyMade={readyMade}
            keysOnly={g.kind === "hold"}
            onChange={(next) => setStep(i, next)}
            onReadyMade={(group, index) => pickReadyMade(i, group, index)}
          />
          {g.steps.length > 1 && (
            <button
              className="button plain w-7 px-0 text-ink-3 hover:text-danger"
              onClick={() => onChange({ steps: g.steps.filter((_, j) => j !== i) })}
              title="Remove this step"
              aria-label="Remove this step"
            >
              −
            </button>
          )}
        </Line>
      ))}
      {g.kind !== "hold" && (
        <div
          className={`pl-[4.5rem] ${
            g.steps.length > 1 ? "" : "-my-1 hidden group-focus-within:block group-hover:block"
          }`}
        >
          <button
            className="button plain -ml-2 h-6 px-2 text-[12px] text-accent"
            title="Do something else after this, in turn"
            onClick={() => onChange({ steps: [...g.steps, newStep()] })}
          >
            + Add Step
          </button>
        </div>
      )}

      {problem && (
        <div className="pl-[4.5rem]">
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

/** One step: what kind, and its keys, command or wait. Ready-made actions live in the same menu. */
function StepEditor({
  step,
  readyMade,
  keysOnly,
  onChange,
  onReadyMade,
}: {
  step: Step;
  readyMade: Group[];
  keysOnly: boolean;
  onChange: (step: Step) => void;
  onReadyMade: (group: number, index: number) => void;
}) {
  return (
    <>
      <select
        className="field w-36"
        value={step.action}
        onChange={(e) => {
          const [kind, group, index] = e.target.value.split(":");
          if (kind === "ready") return onReadyMade(Number(group), Number(index));
          const action = kind as Step["action"];
          onChange({ action, target: action === "wait" ? "200" : action === step.action ? step.target : "" });
        }}
      >
        <option value="keys">Press Keys</option>
        {!keysOnly && (
          <>
            <option value="command">Run Command</option>
            <option value="dispatch">Compositor</option>
            <option value="wait">Wait</option>
          </>
        )}
        {readyMade.map((group, g) => (
          <optgroup key={group.title} label={group.title}>
            {group.actions
              .map((action, i) => ({ action, i }))
              .filter(({ action }) => !keysOnly || action.step.action === "keys")
              .map(({ action, i }) => (
                <option key={action.name} value={`ready:${g}:${i}`}>
                  {menuLabel(action)}
                </option>
              ))}
          </optgroup>
        ))}
      </select>
      <div className="flex min-w-44 flex-1 items-center gap-1.5">
        {step.action === "keys" && <KeysInput value={step.target} onChange={(target) => onChange({ ...step, target })} />}
        {(step.action === "command" || step.action === "dispatch") && (
          <input
            className="field w-full font-mono"
            value={step.target}
            placeholder={step.action === "command" ? "notify-send hi" : "workspace e+1"}
            onChange={(e) => onChange({ ...step, target: e.target.value })}
          />
        )}
        {step.action === "wait" && (
          <>
            <input
              className="field w-20 text-right"
              type="number"
              min={10}
              value={step.target}
              onChange={(e) => onChange({ ...step, target: e.target.value })}
            />
            <span className="text-ink-2">ms</span>
          </>
        )}
      </div>
    </>
  );
}

/** One line of a gesture: a quiet word, then its controls. */
function Line({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-16 shrink-0 text-[12px] text-ink-2">{label}</span>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">{children}</div>
    </div>
  );
}

/** What to draw or press, at the end of the name. */
function Preview({ gesture: g, shapes }: { gesture: Gesture; shapes: ShapeInfo[] }) {
  const held = MODIFIERS.filter((m) => g.with?.split("+").includes(m.key));
  const keys = held.length > 0 && (
    <span className="text-[13px] text-ink-2" title={`Holding ${held.map((m) => m.label).join(" and ")}`}>
      {held.map((m) => m.glyph).join("")}
    </span>
  );
  let drawn: ReactNode = null;
  if (g.input === "stroke") {
    drawn = <span className="text-[17px] tracking-[0.15em] text-accent">{arrows(g.value)}</span>;
  } else if (g.input === "shape") {
    const shape = shapes.find((s) => s.name === g.value);
    drawn = shape ? <ShapeThumb outline={shape.outline} className="size-6 text-accent" /> : null;
  } else {
    const wheel: Record<string, string> = { up: "↑", down: "↓", left: "←", right: "→" };
    drawn = (
      <span className="text-[12px] font-medium text-accent">
        {g.input === "wheel" ? `Wheel ${wheel[g.value]}` : `${g.value === "left" ? "Left" : "Right"} click`}
      </span>
    );
  }
  return (
    <span className="flex items-center gap-1.5">
      {keys}
      {drawn}
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
    <div className="flex w-full gap-1.5">
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
