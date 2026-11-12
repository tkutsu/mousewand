// The settings app in a browser, with Tauri's backend faked: `load` answers with
// the state below, and every call is recorded so tests can check what a save
// sent. Typed against api.ts, so the fixture can't drift from what the UI expects.

import { test as base, type Page } from "@playwright/test";
import { newGesture, type Gesture, type State } from "../src/api";

const gesture = (patch: Partial<Gesture>): Gesture => ({ ...newGesture(), ...patch });
const keys = (target: string) => [{ action: "keys" as const, target }];

/** About 1 across, like the backend's thumbnails. */
const outline = (points: number, at: (t: number) => [number, number]) =>
  Array.from({ length: points + 1 }, (_, i) => at((i / points) * 2 * Math.PI));

export const STATE: State = {
  dir: "/home/someone/.config/mousewand",
  settings: {
    trigger: "middle",
    threshold: 8,
    holdMs: 300,
    cancelMs: 0,
    minSegment: 30,
    theme: "default",
    openSettings: { input: "shape", value: "circle" },
    hint: {
      enabled: true,
      next: true,
      position: "bottom center",
      font: "sans-serif",
      size: 17,
      text: "#f2f2f7",
      glyph: "#9ec4ff",
      failed: "#ff5959",
      background: "#1c1e26",
      radius: 14,
    },
  },
  global: [
    gesture({ value: "UD", name: "Say hello", steps: [{ action: "command", target: "notify-send hello" }] }),
    gesture({ value: "9", name: "Switch windows", kind: "hold", hold: "alt", steps: keys("Tab") }),
    gesture({
      input: "wheel",
      value: "up",
      name: "Volume up",
      steps: [{ action: "command", target: "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+" }],
    }),
  ],
  app: {
    file: "firefox",
    classes: ["firefox", "librewolf"],
    enabled: true,
    gestures: [
      gesture({ value: "L", name: "Back", steps: keys("alt+Left") }),
      gesture({ value: "R", name: "Forward", steps: keys("alt+Right") }),
      gesture({ value: "L", with: "shift", name: "Previous tab", steps: keys("ctrl+shift+Tab") }),
    ],
  },
  settingsApp: { file: "mousewand-settings", classes: ["mousewand-settings"], enabled: true, gestures: [] },
  themes: [
    { name: "default", colors: [[0.45, 0.75, 1]], width: 26, glow: 0.6 },
    {
      name: "ocean",
      colors: [
        [0.88, 0.98, 1],
        [0.31, 0.82, 1],
        [0.12, 0.44, 0.92],
        [0.04, 0.24, 0.57],
      ],
      width: 28,
      glow: 0.7,
    },
    { name: "ember", colors: [[1, 0.54, 0.24]], width: 34, glow: 0.9 },
  ],
  shapes: [
    { name: "circle", builtIn: true, outline: outline(32, (t) => [0.5 + 0.5 * Math.cos(t), 0.5 + 0.5 * Math.sin(t)]) },
    {
      name: "triangle",
      builtIn: true,
      outline: [
        [0.5, 0],
        [1, 0.87],
        [0, 0.87],
        [0.5, 0],
      ],
    },
    { name: "zigzag", builtIn: false, outline: [0, 1, 2, 3, 4].map((i) => [i / 4, i % 2]) },
  ],
  presets: [
    {
      name: "Firefox",
      classes: ["firefox", "librewolf"],
      gestures: [
        gesture({ value: "U", name: "New tab", steps: keys("ctrl+t") }),
        gesture({ value: "L", name: "Back", steps: keys("alt+Left") }),
      ],
    },
    { name: "Kitty", classes: ["kitty"], gestures: [gesture({ value: "D", name: "New window", steps: keys("ctrl+shift+Enter") })] },
  ],
};

export type Backend = {
  state: State;
  notes: string[];
  /** What a save fails with, if it should. */
  saveError: string | null;
};

type Call = { cmd: string; args: Record<string, unknown> };

/** Installs the fake backend before the app's scripts run. */
async function fake(page: Page, backend: Backend) {
  await page.addInitScript((backend: Backend) => {
    const calls: Call[] = [];
    Object.assign(window, {
      __calls: calls,
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
        transformCallback: () => 0,
        unregisterCallback: () => {},
        invoke: async (cmd: string, args: Record<string, unknown>) => {
          calls.push({ cmd, args });
          switch (cmd) {
            case "load":
              return backend.state;
            case "notes":
              return { notes: backend.notes, open: false };
            case "keyboard_access":
            case "alike":
              return true;
            case "save":
              if (backend.saveError) throw backend.saveError;
              return null;
            case "save_shape":
              return args.name;
            default:
              return null;
          }
        },
      },
    });
  }, backend);
}

export const test = base.extend<{ backend: Backend; calls: (cmd: string) => Promise<Record<string, unknown>[]> }>({
  backend: [{ state: STATE, notes: [], saveError: null }, { option: true }],
  page: async ({ page, backend }, use) => {
    await fake(page, backend);
    await page.goto("/");
    await page.getByRole("heading", { level: 1 }).waitFor();
    await use(page);
  },
  /** The arguments of each call the app made to `cmd`, in order. */
  calls: async ({ page }, use) => {
    await use(async (cmd) =>
      page.evaluate(
        (cmd) => (window as unknown as { __calls: Call[] }).__calls.filter((c) => c.cmd === cmd).map((c) => c.args),
        cmd,
      ),
    );
  },
});

export { expect } from "@playwright/test";
