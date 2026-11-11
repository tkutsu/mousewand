import { invoke } from "@tauri-apps/api/core";

export type Gesture = {
  input: "stroke" | "shape" | "wheel" | "click";
  value: string;
  kind: "oneshot" | "repeat" | "hold";
  step: number | null;
  hold: string | null;
  action: "keys" | "command" | "dispatch";
  target: string;
  /** What hints call it. */
  name: string | null;
};

export type Settings = {
  trigger: string;
  threshold: number;
  holdMs: number;
  minSegment: number;
  theme: string;
};

export type App = {
  file: string;
  classes: string[];
  gestures: Gesture[];
};

export type Preset = {
  name: string;
  classes: string[];
  gestures: Gesture[];
};

export type State = {
  dir: string;
  settings: Settings;
  global: Gesture[];
  app: App | null;
  themes: string[];
  /** The built-in shapes, then the drawn ones. */
  shapes: string[];
  presets: Preset[];
};

export const load = () => invoke<State>("load");

/** The daemon's setup notes for this compositor; `open` asks to show them first. */
export type Notes = { notes: string[]; open: boolean };

export const notes = () => invoke<Notes>("notes");

/** Saves a drawn shape, each example a list of [x, y]; resolves to its file name. */
export const saveShape = (name: string, examples: [number, number][][]) =>
  invoke<string>("save_shape", { name, examples });

export const save = (settings: Settings, global: Gesture[], app: App | null) =>
  invoke<void>("save", { settings, global, app });

export const newGesture = (): Gesture => ({
  input: "stroke",
  value: "",
  kind: "oneshot",
  step: null,
  hold: null,
  action: "keys",
  target: "",
  name: null,
});
