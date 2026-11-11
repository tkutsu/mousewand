// What's wrong with a gesture, in words, before the daemon's parser sees it.

import type { Gesture } from "./api";

/** One problem per gesture, or null; a gesture repeating an earlier one's input is one. */
export function problems(gestures: Gesture[]): (string | null)[] {
  const drawnAs = (g: Gesture) => `${g.input}:${g.value}:${g.with ?? ""}`;
  return gestures.map((g, i) => {
    if (g.input === "stroke" && !g.value) return "Type a stroke, like DR for down, then right.";
    const same = gestures.findIndex((o) => drawnAs(o) === drawnAs(g));
    if (same < i) return `Gesture ${same + 1} already uses this ${g.input}${g.with ? " with these keys" : ""}.`;
    if (g.kind === "hold" && !g.hold?.trim()) return "Choose the keys to hold down.";
    if (g.kind === "hold" && (g.steps.length > 1 || g.steps[0]?.action !== "keys")) {
      return "Holding keys works with one step that presses keys.";
    }
    const { steps } = g;
    for (const [n, step] of steps.entries()) {
      const where = steps.length > 1 ? ` in step ${n + 1}` : "";
      if (step.action === "wait") {
        if (!/^\d+$/.test(step.target.trim())) return `Type how many milliseconds to wait${where}.`;
        if (n === 0 || n === steps.length - 1) return "A wait goes between two other steps.";
      } else if (!step.target.trim()) {
        return {
          keys: `Choose the keys${where}: press Record, then the shortcut.`,
          command: `Type the command to run${where}.`,
          dispatch: `Type the compositor command${where}.`,
        }[step.action];
      }
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
