import { useEffect, useState } from "react";
import { buttonOf } from "./keys";

const BUTTONS: [string, string][] = [
  ["middle", "Middle (wheel click)"],
  ["right", "Right"],
  ["back", "Back (mouse 4)"],
  ["forward", "Forward (mouse 5)"],
  ["mouse6", "Mouse 6"],
  ["mouse7", "Mouse 7"],
  ["mouse8", "Mouse 8"],
];

/** A mouse button picker with a "Detect" button that takes the next press. */
export default function ButtonInput({ value, onChange }: { value: string; onChange: (button: string) => void }) {
  const [detecting, setDetecting] = useState(false);
  const known = BUTTONS.some(([name]) => name === value);

  useEffect(() => {
    if (!detecting) return;
    // The whole press is ours, so a back button doesn't also go back.
    let pressed: string | null = null;
    const swallow = (e: Event) => {
      e.preventDefault();
      e.stopPropagation();
    };
    const down = (e: MouseEvent) => {
      swallow(e);
      pressed = buttonOf(e);
    };
    const up = (e: MouseEvent) => {
      swallow(e);
      // Left cancels.
      if (pressed) onChange(pressed);
      setDetecting(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") setDetecting(false);
    };
    const listeners: [string, EventListener][] = [
      ["mousedown", down as EventListener],
      ["mouseup", up as EventListener],
      ["auxclick", swallow],
      ["contextmenu", swallow],
      ["keydown", key as EventListener],
    ];
    for (const [type, listener] of listeners) window.addEventListener(type, listener, true);
    return () => {
      for (const [type, listener] of listeners) window.removeEventListener(type, listener, true);
    };
  }, [detecting, onChange]);

  return (
    <div className="flex gap-1.5">
      <select
        className="field min-w-0 flex-1"
        title="The mouse button you hold to draw"
        value={value}
        disabled={detecting}
        onChange={(e) => onChange(e.target.value)}
      >
        {!known && <option value={value}>{value}</option>}
        {BUTTONS.map(([name, label]) => (
          <option key={name} value={name}>
            {label}
          </option>
        ))}
      </select>
      <button
        className={`button ${detecting ? "recording" : ""}`}
        title="Press the button on your mouse. Esc or a left click cancels."
        onClick={(e) => {
          e.preventDefault();
          setDetecting(true);
        }}
      >
        {detecting ? "● Press it…" : "Detect"}
      </button>
    </div>
  );
}
