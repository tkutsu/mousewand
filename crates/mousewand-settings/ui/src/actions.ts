// Ready-made actions to pick instead of typing keys. Each one is just a step
// with the shortcut most apps use, filled in where it can be seen and
// changed. An app's preset knows its own shortcuts, so those come first.

import type { Preset, Step } from "./api";

export type ReadyMade = { name: string; step: Step };

export type Group = { title: string; actions: ReadyMade[] };

const keys = (name: string, combo: string): ReadyMade => ({ name, step: { action: "keys", target: combo } });
const command = (name: string, line: string): ReadyMade => ({ name, step: { action: "command", target: line } });

export const COMMON: Group[] = [
  {
    title: "Navigate",
    actions: [
      keys("Back", "alt+Left"),
      keys("Forward", "alt+Right"),
      keys("Reload", "F5"),
      keys("Scroll to top", "ctrl+Home"),
      keys("Scroll to bottom", "ctrl+End"),
      keys("Find", "ctrl+f"),
    ],
  },
  {
    title: "Tabs",
    actions: [
      keys("New tab", "ctrl+t"),
      keys("Close tab", "ctrl+w"),
      keys("Reopen closed tab", "ctrl+shift+t"),
      keys("Next tab", "ctrl+Tab"),
      keys("Previous tab", "ctrl+shift+Tab"),
    ],
  },
  {
    title: "Edit",
    actions: [
      keys("Copy", "ctrl+c"),
      keys("Cut", "ctrl+x"),
      keys("Paste", "ctrl+v"),
      keys("Undo", "ctrl+z"),
      keys("Redo", "ctrl+shift+z"),
      keys("Select all", "ctrl+a"),
      keys("Save", "ctrl+s"),
    ],
  },
  {
    title: "View",
    actions: [
      keys("Zoom in", "ctrl+equal"),
      keys("Zoom out", "ctrl+minus"),
      keys("Actual size", "ctrl+0"),
      keys("Full screen", "F11"),
    ],
  },
  {
    title: "Media",
    actions: [
      keys("Play or pause", "playpause"),
      keys("Next track", "next"),
      keys("Previous track", "prev"),
      keys("Volume up", "volumeup"),
      keys("Volume down", "volumedown"),
      keys("Mute", "mute"),
    ],
  },
  {
    title: "Selected Text",
    actions: [
      command("Copy selection", "wl-paste --primary | wl-copy"),
      command(
        "Search the web for it",
        'xdg-open "https://duckduckgo.com/?q=$(wl-paste --primary --no-newline | jq -sRr @uri)"',
      ),
    ],
  },
];

/** The actions of the presets made for this app, then the common ones. */
export function groupsFor(classes: string[], presets: Preset[]): Group[] {
  const lower = new Set(classes.map((c) => c.toLowerCase()));
  const own = presets
    .filter((p) => p.classes.some((c) => lower.has(c.toLowerCase())))
    .map((p) => ({
      title: `For ${p.name}`,
      actions: p.gestures
        .filter((g) => g.name && g.steps.length === 1)
        .map((g) => ({ name: g.name!, step: g.steps[0] })),
    }));
  return [...own, ...COMMON];
}

/** How a ready-made action reads in a menu: its name, then what it does. */
export function menuLabel(action: ReadyMade): string {
  if (action.step.action !== "keys") return action.name;
  return `${action.name}  ·  ${shown(action.step.target)}`;
}

/** A combo the way menus write it: "ctrl+shift+t" → "Ctrl+Shift+T". */
export function shown(combo: string): string {
  return combo
    .split("+")
    .map((part) => {
      const arrow = { left: "←", right: "→", up: "↑", down: "↓" }[part.toLowerCase()];
      if (arrow) return arrow;
      return part.length === 1 ? part.toUpperCase() : part.charAt(0).toUpperCase() + part.slice(1);
    })
    .join("+");
}

/** The modifiers a gesture can be drawn with, in the order combos name them. */
export const MODIFIERS = [
  { key: "ctrl", label: "Ctrl", glyph: "⌃" },
  { key: "shift", label: "Shift", glyph: "⇧" },
  { key: "alt", label: "Alt", glyph: "⌥" },
  { key: "super", label: "Super", glyph: "❖" },
] as const;
