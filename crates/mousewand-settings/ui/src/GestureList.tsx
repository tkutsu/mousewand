import { useEffect, useState, type ReactNode } from "react";
import { MODIFIERS, menuLabel, type Group } from "./actions";
import { newGesture, newStep, type Gesture, type ShapeInfo, type Step } from "./api";
import { problems } from "./check";
import { ShapeSelect, StrokeInput } from "./Drawn";
import { arrows, comboOf } from "./keys";
import ShapePad from "./ShapePad";
import { Problem, Section, Segmented, ShapeThumb, Toggles } from "./ui";

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
  // A repeating gesture that presses keys can keep modifiers down until
  // release, as alt-tab needs. It's `hold` in the action.
  const canKeepDown = continuous && g.steps.length === 1 && g.steps[0].action === "keys";
  const kept = g.hold ? g.hold.split("+") : [];

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

      <Line label="When" nowrap>
        <select
          className="field"
          title={INPUT_TIPS[g.input]}
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
        {canHoldKeys && (
          <Toggles
            values={held}
            options={MODIFIERS.map((m) => ({ key: m.key, label: m.label, title: `Only while ${m.label} is held` }))}
            onChange={(keys) => {
              // Combos name them in a set order: ctrl, shift, alt, super.
              const ordered = MODIFIERS.map((m) => m.key).filter((k) => keys.includes(k));
              onChange({ with: ordered.length ? ordered.join("+") : null });
            }}
          />
        )}
        {g.input === "wheel" && (
          <select
            className="field"
            title="Which way the wheel turns"
            value={g.value}
            onChange={(e) => onChange({ value: e.target.value })}
          >
            <option value="up">Up</option>
            <option value="down">Down</option>
            <option value="left">Left</option>
            <option value="right">Right</option>
          </select>
        )}
        {g.input === "click" && (
          <Segmented
            value={g.value}
            options={[
              ["left", "Left", "A left click while holding the trigger"],
              ["right", "Right", "A right click while holding the trigger"],
            ]}
            onChange={(value) => onChange({ value })}
          />
        )}
        {continuous && (
          <span className="flex items-center gap-1.5 text-ink-2" title="Fires again each time you move this far">
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
        {g.input === "stroke" && (
          // Last and at the far end, so it stays put as what's before it changes.
          <span className="ml-auto pl-6">
            <Segmented
              value={continuous ? "repeat" : "oneshot"}
              options={[
                ["oneshot", "On Release", "Fires once, when you let go"],
                ["repeat", "Repeats", "Fires as you go, and again as you keep moving"],
              ]}
              onChange={(kind) => {
                if (kind === (continuous ? "repeat" : "oneshot")) return;
                onChange(kind === "repeat" ? { kind: "repeat", with: null } : { kind: "oneshot", hold: null });
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

      {g.steps.map((step, i) => (
        <Line key={i} label={i === 0 ? "Do" : "Then"}>
          <StepEditor
            step={step}
            readyMade={readyMade}
            keysOnly={g.kind === "hold"}
            onChange={(next) => setStep(i, next)}
            onReadyMade={(group, index) => pickReadyMade(i, group, index)}
          />
          {canKeepDown && (
            <span className="flex items-center gap-2">
              <Toggles
                values={kept}
                options={MODIFIERS.map((m) => ({
                  key: m.key,
                  label: m.label,
                  title: `Keeps ${m.label} down until you let go, like alt-tab`,
                }))}
                onChange={(keys) => {
                  const ordered = MODIFIERS.map((m) => m.key).filter((k) => keys.includes(k));
                  // Keys other than modifiers, from a hand-written config, stay.
                  const hold = [...ordered, ...kept.filter((k) => !MODIFIERS.some((m) => m.key === k))];
                  onChange(hold.length ? { kind: "hold", hold: hold.join("+") } : { kind: "repeat", hold: null });
                }}
              />
              <span className="text-[12px] text-ink-3">held</span>
            </span>
          )}
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
        // In the labels' column, under the last step.
        <button
          className="button plain -my-1 -ml-1.5 h-6 w-fit justify-start px-1.5 text-[12px] text-accent"
          title="Do something else after this, in turn"
          onClick={() => onChange({ steps: [...g.steps, newStep()] })}
        >
          + Add Step
        </button>
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
        title={STEP_TIPS[step.action]}
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
            title={step.action === "command" ? "Runs in a shell" : "Sent to your compositor as is"}
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
              title="A pause before the next step"
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

const INPUT_TIPS: Record<Gesture["input"], string> = {
  stroke: "Directions you draw",
  shape: "A shape you draw",
  wheel: "The wheel, while holding the trigger",
  click: "A click, while holding the trigger",
};

const STEP_TIPS: Record<Step["action"], string> = {
  keys: "Presses a keyboard shortcut",
  command: "Runs a shell command",
  dispatch: "Sends a command to your compositor",
  wait: "Pauses before the next step",
};

/**
 * One line of a gesture: a quiet word, then its controls, wrapping unless
 * `nowrap`. The word stays level with the first line of controls.
 */
function Line({ label, nowrap, children }: { label: string; nowrap?: boolean; children: ReactNode }) {
  return (
    <div className="flex items-start gap-2">
      <span className="flex h-7 w-16 shrink-0 items-center text-[12px] text-ink-2">{label}</span>
      <div className={`flex min-w-0 flex-1 items-center gap-2 ${nowrap ? "" : "flex-wrap"}`}>{children}</div>
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
        title="Type a combo, or record one"
        value={recording ? "" : value}
        placeholder={recording ? "Press a shortcut… (Esc cancels)" : "ctrl+shift+t"}
        readOnly={recording}
        onChange={(e) => onChange(e.target.value)}
      />
      <button
        className={`button ${recording ? "recording" : ""}`}
        title="Press the shortcut to fill it in"
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
