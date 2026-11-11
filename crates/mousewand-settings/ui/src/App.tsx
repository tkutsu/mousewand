import { useEffect, useState, type ReactNode } from "react";
import * as api from "./api";
import ButtonInput from "./ButtonInput";
import GestureList from "./GestureList";

type Tab = "global" | "app" | "setup";

export default function App() {
  const [state, setState] = useState<api.State | null>(null);
  const [settings, setSettings] = useState<api.Settings | null>(null);
  const [global, setGlobal] = useState<api.Gesture[]>([]);
  const [app, setApp] = useState<api.App | null>(null);
  const [notes, setNotes] = useState<string[]>([]);
  const [shapes, setShapes] = useState<string[]>([]);
  const [tab, setTab] = useState<Tab>("global");
  const [dirty, setDirty] = useState(false);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    Promise.all([api.load(), api.notes()])
      .then(([s, n]) => {
        setState(s);
        setSettings(s.settings);
        setGlobal(s.global);
        setApp(s.app);
        setShapes(s.shapes);
        setNotes(n.notes);
        if (n.open && n.notes.length > 0) setTab("setup");
        else if (s.app) setTab("app");
      })
      .catch((e) => setStatus({ ok: false, text: String(e) }));
  }, []);

  const edit =
    <T,>(set: (value: T) => void) =>
    (value: T) => {
      set(value);
      setDirty(true);
      setStatus(null);
    };

  const onSave = async () => {
    if (!settings) return;
    try {
      await api.save(settings, global, app);
      setDirty(false);
      setStatus({ ok: true, text: "Saved. mousewand picks it up right away." });
    } catch (e) {
      setStatus({ ok: false, text: String(e) });
    }
  };

  const shapeProps = {
    shapes,
    onNewShape: (name: string) => setShapes((list) => (list.includes(name) ? list : [...list, name])),
  };

  if (!state || !settings) {
    return <div className="p-8 text-sm text-zinc-500">{status?.text ?? "Loading…"}</div>;
  }

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center gap-6 border-b border-zinc-200 px-6 pt-4 dark:border-zinc-800">
        <span className="pb-3 text-lg font-semibold tracking-tight">mousewand</span>
        <nav className="flex gap-1">
          <TabButton active={tab === "global"} onClick={() => setTab("global")}>
            Everywhere
          </TabButton>
          {app && (
            <TabButton active={tab === "app"} onClick={() => setTab("app")}>
              {app.classes[0]}
            </TabButton>
          )}
          {notes.length > 0 && (
            <TabButton active={tab === "setup"} onClick={() => setTab("setup")}>
              Setup
            </TabButton>
          )}
        </nav>
        <span className="ml-auto truncate pb-3 font-mono text-xs text-zinc-400" title={state.dir}>
          {state.dir}
        </span>
      </header>

      <main className="flex-1 overflow-y-auto px-6 py-5">
        {tab === "setup" ? (
          <SetupTab notes={notes} />
        ) : tab === "global" ? (
          <div className="flex flex-col gap-6">
            <Section title="Settings">
              <SettingsForm settings={settings} themes={state.themes} onChange={edit(setSettings)} />
            </Section>
            <Section title="Gestures for every app" hint="An app's own gestures override these.">
              <GestureList gestures={global} onChange={edit(setGlobal)} {...shapeProps} />
            </Section>
          </div>
        ) : (
          app && <AppTab app={app} presets={state.presets} onChange={edit(setApp)} shapeProps={shapeProps} />
        )}
      </main>

      <footer className="flex items-center gap-4 border-t border-zinc-200 px-6 py-3 dark:border-zinc-800">
        <p className={`flex-1 text-sm ${status?.ok === false ? "text-red-500" : "text-zinc-500"}`}>
          {status?.text ?? (dirty ? "Unsaved changes" : "Keys are physical positions (US layout), whatever your layout.")}
        </p>
        <button className="button bg-accent text-white hover:brightness-110" disabled={!dirty} onClick={onSave}>
          Save
        </button>
      </footer>
    </div>
  );
}

function SetupTab({ notes }: { notes: string[] }) {
  return (
    <Section title="On this desktop" hint="mousewand shows these once, and again when something new comes up.">
      <ul className="flex max-w-2xl flex-col gap-3">
        {notes.map((note) => (
          <li
            key={note}
            className="rounded-lg border border-zinc-200 px-4 py-3 text-sm leading-relaxed dark:border-zinc-800"
          >
            {/* `code` in backticks: odd parts are code. */}
            {note.split("`").map((part, i) =>
              i % 2 ? (
                <code key={i} className="rounded bg-zinc-100 px-1 font-mono text-[0.85em] dark:bg-zinc-800">
                  {part}
                </code>
              ) : (
                part
              ),
            )}
          </li>
        ))}
      </ul>
    </Section>
  );
}

function AppTab({
  app,
  presets,
  onChange,
  shapeProps,
}: {
  app: api.App;
  presets: api.Preset[];
  onChange: (app: api.App) => void;
  shapeProps: { shapes: string[]; onNewShape: (name: string) => void };
}) {
  const matching = presets.filter((p) => p.classes.some((c) => app.classes.some((a) => a.toLowerCase() === c.toLowerCase())));
  const others = presets.filter((p) => !matching.includes(p));
  const [preset, setPreset] = useState(matching[0]?.name ?? "");

  const addPreset = () => {
    const chosen = presets.find((p) => p.name === preset);
    if (!chosen) return;
    // A preset's gesture replaces one with the same input.
    const key = (g: api.Gesture) => `${g.input}:${g.value}`;
    const incoming = new Set(chosen.gestures.map(key));
    onChange({ ...app, gestures: [...app.gestures.filter((g) => !incoming.has(key(g))), ...chosen.gestures] });
  };

  return (
    <div className="flex flex-col gap-6">
      <Section title="Window classes" hint={`Saved in apps/${app.file}.toml`}>
        <input
          className="field w-full max-w-md font-mono"
          value={app.classes.join(", ")}
          onChange={(e) =>
            onChange({
              ...app,
              classes: e.target.value
                .split(",")
                .map((c) => c.trim())
                .filter(Boolean),
            })
          }
        />
      </Section>
      <Section title="Presets">
        <div className="flex gap-2">
          <select className="field" value={preset} onChange={(e) => setPreset(e.target.value)}>
            <option value="" disabled>
              Choose a preset…
            </option>
            {matching.length > 0 && (
              <optgroup label="For this app">
                {matching.map((p) => (
                  <option key={p.name}>{p.name}</option>
                ))}
              </optgroup>
            )}
            <optgroup label="Others">
              {others.map((p) => (
                <option key={p.name}>{p.name}</option>
              ))}
            </optgroup>
          </select>
          <button
            className="button border border-zinc-300 hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
            disabled={!preset}
            onClick={addPreset}
          >
            Add its gestures
          </button>
        </div>
      </Section>
      <Section title={`Gestures for ${app.classes[0] ?? app.file}`}>
        <GestureList gestures={app.gestures} onChange={(gestures) => onChange({ ...app, gestures })} {...shapeProps} />
      </Section>
    </div>
  );
}

function SettingsForm({
  settings,
  themes,
  onChange,
}: {
  settings: api.Settings;
  themes: string[];
  onChange: (settings: api.Settings) => void;
}) {
  const set = (patch: Partial<api.Settings>) => onChange({ ...settings, ...patch });
  const number = (key: "threshold" | "holdMs" | "minSegment") => (
    <input
      className="field w-24"
      type="number"
      min={1}
      value={settings[key]}
      onChange={(e) => set({ [key]: Number(e.target.value) })}
    />
  );
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-4">
      <div className="col-span-2">
        <Labeled label="Trigger button">
          <ButtonInput value={settings.trigger} onChange={(trigger) => set({ trigger })} />
        </Labeled>
      </div>
      <Labeled label="Trail theme">
        <select className="field" value={settings.theme} onChange={(e) => set({ theme: e.target.value })}>
          {themes.map((t) => (
            <option key={t}>{t}</option>
          ))}
        </select>
      </Labeled>
      <Labeled label="Start after (px)">{number("threshold")}</Labeled>
      <Labeled label="Hold still for (ms)">{number("holdMs")}</Labeled>
      <Labeled label="Ignore strokes under (px)">{number("minSegment")}</Labeled>
    </div>
  );
}

function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-baseline gap-3">
        <h2 className="text-sm font-semibold">{title}</h2>
        {hint && <span className="text-xs text-zinc-500">{hint}</span>}
      </div>
      {children}
    </section>
  );
}

function Labeled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="label">{label}</span>
      {children}
    </label>
  );
}

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      className={`-mb-px border-b-2 px-3 pb-3 text-sm transition ${
        active
          ? "border-accent font-medium text-zinc-900 dark:text-zinc-100"
          : "border-transparent text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200"
      }`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
