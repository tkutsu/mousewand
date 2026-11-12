import { invoke } from "@tauri-apps/api/core";

export type Gesture = {
  input: "stroke" | "shape" | "wheel" | "click";
  value: string;
  /** Modifiers held on the keyboard, like "ctrl+shift". */
  with: string | null;
  kind: "oneshot" | "repeat" | "hold";
  step: number | null;
  hold: string | null;
  /** What it does, one step after another. */
  steps: Step[];
  /** What hints call it. */
  name: string | null;
};

/** One step of what a gesture does; a wait's target is milliseconds. */
export type Step = { action: "keys" | "command" | "dispatch" | "wait"; target: string };

/** A stroke or a shape. */
export type Drawn = { input: "stroke" | "shape"; value: string };

export type Settings = {
  trigger: string;
  threshold: number;
  holdMs: number;
  /** Holding still this long mid-stroke cancels it; 0 never does. */
  cancelMs: number;
  minSegment: number;
  theme: string;
  /** What opens this app. */
  openSettings: Drawn;
  hint: Hint;
  off: Off;
};

/** Where gestures are off: games, and windows whose class fits a pattern. */
export type Off = { games: boolean; classes: string[] };

/** Whether `cls` fits `pattern`, ignoring case: `*` is any run of characters, `?` one. */
const fits = (pattern: string, cls: string) => {
  const part = (c: string) => (c === "*" ? ".*" : c === "?" ? "." : c.replace(/[\\^$.|+()[\]{}]/, "\\$&"));
  return new RegExp("^" + [...pattern].map(part).join("") + "$", "i").test(cls);
};

/** The pattern that turns gestures off over any of `classes`, as the daemon checks it. */
export const offBy = (off: Off, games: string[], classes: string[]) =>
  [...(off.games ? games : []), ...off.classes].find((p) => classes.some((c) => fits(p, c))) ?? null;

/** The `[hint]` the form edits. Colors are "#rrggbb" or "#rrggbbaa". */
export type Hint = {
  enabled: boolean;
  /** List what the stroke could still become. */
  next: boolean;
  /** Like "bottom center", or "center". */
  position: string;
  font: string;
  size: number;
  text: string;
  glyph: string;
  /** The arrows and text once the stroke matches nothing. */
  failed: string;
  background: string;
  radius: number;
};

export type App = {
  file: string;
  classes: string[];
  /** Off, the trigger is an ordinary button over this app. */
  enabled: boolean;
  gestures: Gesture[];
};

/** An app's name for people: "org.gnome.Nautilus" is Nautilus, "google-chrome" Google Chrome. */
export const appName = (app: App) =>
  (app.classes[0] ?? app.file)
    .split(".")
    .pop()!
    .split(/[-_]/)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");

/** An app's own shortcuts, offered first in its Do menu. */
export type AppShortcuts = {
  name: string;
  classes: string[];
  shortcuts: { name: string; keys: string }[];
};

export type ShapeInfo = {
  name: string;
  /** Built in, not drawn: nothing to redraw or remove. */
  builtIn: boolean;
  /** Its first example, about 1 across, for a thumbnail. */
  outline: [number, number][];
};

/** A trail theme, enough to preview it. */
export type ThemeInfo = {
  name: string;
  /** sRGB, 0–1, head to tail. Empty if its file has an error. */
  colors: [number, number, number][];
  width: number;
  glow: number;
};

export type State = {
  dir: string;
  settings: Settings;
  global: Gesture[];
  app: App | null;
  /** The gestures over this app's own window. */
  settingsApp: App;
  themes: ThemeInfo[];
  /** The built-in shapes, then the drawn ones. */
  shapes: ShapeInfo[];
  appShortcuts: AppShortcuts[];
  /** The patterns `off.games` stands for. */
  gameClasses: string[];
};

export const load = () => invoke<State>("load");

/** The daemon's setup notes for this compositor; `open` asks to show them first. */
export type Notes = { notes: string[]; open: boolean };

export const notes = () => invoke<Notes>("notes");

/** Whether mousewand can see which modifiers are held. */
export const keyboardAccess = () => invoke<boolean>("keyboard_access");

/** Saves a drawn shape, each example a list of [x, y]; resolves to its file name. Only a redraw may replace one. */
export const saveShape = (name: string, examples: [number, number][][], redraw: boolean) =>
  invoke<string>("save_shape", { name, examples, redraw });

/** Removes a drawn shape, unless a saved gesture uses it. */
export const deleteShape = (name: string) => invoke<void>("delete_shape", { name });

/** Whether a new drawing looks like the ones before it. */
export const alike = (examples: [number, number][][], drawing: [number, number][]) =>
  invoke<boolean>("alike", { examples, drawing });

/** Saves the settings, the gestures for every app, and these apps' files. */
export const save = (settings: Settings, global: Gesture[], apps: App[]) =>
  invoke<void>("save", { settings, global, apps });

export const newStep = (): Step => ({ action: "keys", target: "" });

export const newGesture = (): Gesture => ({
  input: "stroke",
  value: "",
  with: null,
  kind: "oneshot",
  step: null,
  hold: null,
  steps: [newStep()],
  name: null,
});

