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
};

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

export type Preset = {
  name: string;
  classes: string[];
  gestures: Gesture[];
};

export type ShapeInfo = {
  name: string;
  /** Built in, not drawn: nothing to redraw or remove. */
  builtIn: boolean;
  /** Its first example, about 1 across, for a thumbnail. */
  outline: [number, number][];
};

export type State = {
  dir: string;
  settings: Settings;
  global: Gesture[];
  app: App | null;
  /** The gestures over this app's own window. */
  settingsApp: App;
  themes: string[];
  /** The built-in shapes, then the drawn ones. */
  shapes: ShapeInfo[];
  presets: Preset[];
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

