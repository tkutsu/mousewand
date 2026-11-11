// The General page's section about this app: the gesture that opens it, and
// the ones that work in its window. Those are ordinary gestures in `apps/mousewand-settings.toml`
// that press the window's own shortcuts.

import { useState } from "react";
import { newGesture, type App, type Drawn, type Gesture, type Settings } from "./api";
import { problems } from "./check";
import { ShapeSelect, StrokeInput } from "./Drawn";
import ShapePad from "./ShapePad";
import { Problem, Row, Section, Segmented } from "./ui";

/** What a gesture can do in this window, by the shortcut it presses. */
export const COMMANDS = [
  { label: "Save", keys: "ctrl+s", shown: "Ctrl+S", suggested: "39" },
  { label: "Close", keys: "ctrl+w", shown: "Ctrl+W", suggested: "DR" },
  { label: "New gesture", keys: "ctrl+n", shown: "Ctrl+N", suggested: "D" },
  { label: "Next page", keys: "ctrl+Tab", shown: "Ctrl+Tab", suggested: "R" },
  { label: "Previous page", keys: "ctrl+shift+Tab", shown: "Ctrl+Shift+Tab", suggested: "L" },
] as const;

type Command = (typeof COMMANDS)[number];

const presses = (g: Gesture, command: Command) =>
  g.action === "keys" && g.target.toLowerCase() === command.keys.toLowerCase();

type Props = {
  settings: Settings;
  app: App;
  shapes: string[];
  onSettings: (settings: Settings) => void;
  onApp: (app: App) => void;
  onNewShape: (name: string) => void;
};

export default function SettingsGestures({ settings, app, shapes, onSettings, onApp, onNewShape }: Props) {
  // Which picker is drawing a new shape: "open", or a command's label.
  const [drawing, setDrawing] = useState<string | null>(null);

  const drawnFor = (command: Command): Drawn | null => {
    const g = app.gestures.find((g) => presses(g, command));
    return g && (g.input === "stroke" || g.input === "shape") ? { input: g.input, value: g.value } : null;
  };

  /** The commands' gestures in order, then any others the file has. */
  const rebuild = (changed: Command, to: Drawn | null) => {
    const others = app.gestures.filter((g) => !COMMANDS.some((c) => presses(g, c)));
    const ours = COMMANDS.flatMap((c): Gesture[] => {
      const drawn = c === changed ? to : drawnFor(c);
      return drawn ? [{ ...newGesture(), ...drawn, target: c.keys, name: c.label }] : [];
    });
    onApp({ ...app, gestures: [...ours, ...others] });
  };

  const suggest = () =>
    onApp({
      ...app,
      gestures: [
        ...COMMANDS.map((c) => ({ ...newGesture(), value: c.suggested, target: c.keys, name: c.label })),
        ...app.gestures.filter((g) => !COMMANDS.some((c) => presses(g, c))),
      ],
    });

  const found = problems(app.gestures);
  const problemFor = (command: Command) => {
    const i = app.gestures.findIndex((g) => presses(g, command));
    return i < 0 ? null : found[i];
  };

  const open = settings.openSettings;
  return (
    <Section
      title="Mousewand App"
      action={
        <button className="button plain h-6 px-2 text-accent" onClick={suggest}>
          Use Suggested
        </button>
      }
      note="Draw Open settings over any other window to open settings on that app's gestures. The rest work over this window, and press its shortcuts, which also work from the keyboard."
    >
      <div>
        <Row label="Open settings" hint="Does nothing over this window">
          <Picker
            drawn={open}
            shapes={shapes}
            drawing={drawing === "open"}
            onChange={(openSettings) => openSettings && onSettings({ ...settings, openSettings })}
            onDraw={() => setDrawing("open")}
          />
        </Row>
        {drawing === "open" && (
          <Pad
            onSaved={(name) => {
              onNewShape(name);
              onSettings({ ...settings, openSettings: { input: "shape", value: name } });
              setDrawing(null);
            }}
            onCancel={() => setDrawing(null)}
          />
        )}
      </div>
      {COMMANDS.map((command) => (
        <div key={command.label}>
          <Row label={command.label} hint={command.shown}>
            <Picker
              drawn={drawnFor(command)}
              shapes={shapes}
              drawing={drawing === command.label}
              off
              onChange={(drawn) => rebuild(command, drawn)}
              onDraw={() => setDrawing(command.label)}
            />
          </Row>
          {problemFor(command) && (
            <div className="px-3 pb-2">
              <Problem>{problemFor(command)}</Problem>
            </div>
          )}
          {drawing === command.label && (
            <Pad
              onSaved={(name) => {
                onNewShape(name);
                rebuild(command, { input: "shape", value: name });
                setDrawing(null);
              }}
              onCancel={() => setDrawing(null)}
            />
          )}
        </div>
      ))}
    </Section>
  );
}

/** Off, a stroke or a shape, and its field. */
function Picker({
  drawn,
  shapes,
  drawing,
  off,
  onChange,
  onDraw,
}: {
  drawn: Drawn | null;
  shapes: string[];
  drawing: boolean;
  off?: boolean;
  onChange: (drawn: Drawn | null) => void;
  onDraw: () => void;
}) {
  const kinds: ["off" | Drawn["input"], string][] = [
    ...(off ? [["off", "Off"] as ["off", string]] : []),
    ["stroke", "Stroke"],
    ["shape", "Shape"],
  ];
  return (
    <div className="flex items-start gap-2">
      {drawn?.input === "stroke" && (
        <StrokeInput value={drawn.value} onChange={(value) => onChange({ input: "stroke", value })} />
      )}
      {drawn?.input === "shape" && (
        <ShapeSelect
          value={drawn.value}
          shapes={shapes}
          drawing={drawing}
          onChange={(value) => onChange({ input: "shape", value })}
          onDraw={onDraw}
        />
      )}
      <Segmented
        value={drawn?.input ?? "off"}
        options={kinds}
        onChange={(kind) =>
          onChange(
            kind === "off" ? null : { input: kind, value: kind === "shape" ? (shapes[0] ?? "circle") : "" },
          )
        }
      />
    </div>
  );
}

function Pad(props: { onSaved: (name: string) => void; onCancel: () => void }) {
  return (
    <div className="px-3 pb-3">
      <ShapePad {...props} />
    </div>
  );
}
