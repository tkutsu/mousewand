// What's wrong with a gesture, in words, before the daemon's parser sees it.

import type { Gesture } from "./api";

/** One problem per gesture, or null; a gesture repeating an earlier one's input is one. */
export function problems(gestures: Gesture[]): (string | null)[] {
  return gestures.map((g, i) => {
    if (g.input === "stroke" && !g.value) return "Type a stroke, like DR for down, then right.";
    const same = gestures.findIndex((o) => o.input === g.input && o.value === g.value);
    if (same < i) return `Gesture ${same + 1} already uses this ${g.input}.`;
    if (g.kind === "hold" && !g.hold?.trim()) return "Choose the keys to hold down.";
    if (!g.target.trim()) {
      return {
        keys: "Choose the keys: press Record, then the shortcut.",
        command: "Type the command to run.",
        dispatch: "Type the dispatcher to run.",
      }[g.action];
    }
    return null;
  });
}

/** A stroke as typed: directions only, in capitals, none twice in a row (UUUU is just U). */
export function cleanStroke(text: string): string {
  return text
    .toUpperCase()
    .replace(/[^UDLR7913]/g, "")
    .replace(/(.)\1+/g, "$1");
}

/** An error from the backend, as a sentence. */
export function sentence(error: unknown): string {
  const text = String(error).replace(/^Error:\s*/, "").trim();
  return text.charAt(0).toUpperCase() + text.slice(1);
}
