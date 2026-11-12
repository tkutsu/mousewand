// Ready-made actions to pick instead of typing keys. Each one is just a step
// with the shortcut most apps use, filled in where it can be seen and
// changed. An app's own shortcuts (shortcuts.toml) come first.

import type { AppShortcuts, Step } from "./api";

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
      command("Copy selection", 'printf %s "$SELECTION" | wl-copy'),
      command(
        "Search the web for it",
        'xdg-open "https://duckduckgo.com/?q=$(printf %s "$SELECTION" | jq -sRr @uri)"',
      ),
      command("Open it as a link", 'xdg-open "$SELECTION"'),
    ],
  },
];

/** This app's own shortcuts, then the common ones it doesn't replace by name. */
export function groupsFor(classes: string[], apps: AppShortcuts[]): Group[] {
  const lower = new Set(classes.map((c) => c.toLowerCase()));
  const own = apps
    .filter((a) => a.classes.some((c) => lower.has(c.toLowerCase())))
    .map((a) => ({ title: `For ${a.name}`, actions: a.shortcuts.map((s) => keys(s.name, s.keys)) }));
  const taken = new Set(own.flatMap((g) => g.actions.map((a) => a.name)));
  const common = COMMON.map((g) => ({ ...g, actions: g.actions.filter((a) => !taken.has(a.name)) }));
  return [...own, ...common.filter((g) => g.actions.length > 0)];
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
