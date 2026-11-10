import { invoke } from "@tauri-apps/api/core";

export type Gesture = {
  input: "stroke" | "wheel" | "rocker";
  value: string;
  kind: "oneshot" | "repeat" | "hold";
  step: number | null;
  hold: string | null;
  action: "keys" | "command" | "dispatch";
  target: string;
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
  presets: Preset[];
};

export const load = () => invoke<State>("load");

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
});
