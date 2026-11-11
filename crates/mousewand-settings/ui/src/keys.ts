// Turns a key press into a mousewand combo like "ctrl+shift+t". mousewand sends
// physical keys, so this reads `event.code` (the key's position), not
// `event.key` (what the layout prints on it).

const named: Record<string, string> = {
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ArrowUp: "Up",
  ArrowDown: "Down",
  Enter: "Enter",
  Escape: "Esc",
  Tab: "Tab",
  Space: "space",
  Backspace: "backspace",
  Delete: "delete",
  Insert: "insert",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  Minus: "minus",
  Equal: "equal",
  BracketLeft: "leftbrace",
  BracketRight: "rightbrace",
  Semicolon: "semicolon",
  Quote: "apostrophe",
  Backquote: "grave",
  Backslash: "backslash",
  Comma: "comma",
  Period: "dot",
  Slash: "slash",
  PrintScreen: "print",
  Pause: "pause",
  ContextMenu: "menu",
};

const modifiers = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
]);

function keyName(code: string): string | null {
  if (code in named) return named[code];
  const letter = code.match(/^Key([A-Z])$/);
  if (letter) return letter[1].toLowerCase();
  const digit = code.match(/^(?:Digit|Numpad)(\d)$/);
  if (digit) return digit[1];
  if (/^F\d{1,2}$/.test(code)) return code;
  return null;
}

/** The combo for a keydown, or null while only modifiers are down. */
export function comboOf(event: KeyboardEvent): string | null {
  if (modifiers.has(event.code)) return null;
  const key = keyName(event.code);
  if (!key) return null;
  const parts = [];
  if (event.ctrlKey) parts.push("ctrl");
  if (event.shiftKey) parts.push("shift");
  if (event.altKey) parts.push("alt");
  if (event.metaKey) parts.push("super");
  return [...parts, key].join("+");
}

/**
 * The trigger name for a mouse button press, or null for left and right.
 * Browsers report the buttons up to mouse 5: mouse 4 is back, 5 forward.
 */
export function buttonOf(event: MouseEvent): string | null {
  const names: Record<number, string> = { 1: "middle", 3: "back", 4: "forward" };
  return names[event.button] ?? null;
}

/** Arrows for a stroke: "DR" → "↓→". */
export function arrows(stroke: string): string {
  const map: Record<string, string> = {
    U: "↑",
    D: "↓",
    L: "←",
    R: "→",
    "7": "↖",
    "9": "↗",
    "1": "↙",
    "3": "↘",
  };
  return [...stroke.toUpperCase()].map((c) => map[c] ?? "·").join("");
}
