import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { COMMON, groupsFor } from "./actions";
import * as api from "./api";
import ButtonInput from "./ButtonInput";
import { problems, sentence } from "./check";
import GestureList from "./GestureList";
import SettingsGestures from "./SettingsGestures";
import Shapes from "./Shapes";
import { Logo, Problem, Row, Section, Switch } from "./ui";

type Page = "general" | "every" | "app" | "shapes" | "setup";

type Status = { kind: "ok" | "error" | "warn"; text: string } | null;

export default function App() {
  const [state, setState] = useState<api.State | null>(null);
  const [settings, setSettings] = useState<api.Settings | null>(null);
  const [global, setGlobal] = useState<api.Gesture[]>([]);
  const [app, setApp] = useState<api.App | null>(null);
  const [settingsApp, setSettingsApp] = useState<api.App | null>(null);
  const [shapes, setShapes] = useState<api.ShapeInfo[]>([]);
  const [notes, setNotes] = useState<string[]>([]);
  const [keyboard, setKeyboard] = useState(true);
  const [page, setPage] = useState<Page>("every");
  const [dirty, setDirty] = useState(false);
  const [status, setStatus] = useState<Status>(null);
  // A close asked for with unsaved changes; asking again discards them.
  const [closing, setClosing] = useState(false);
  const main = useRef<HTMLElement>(null);

  useEffect(() => {
    api.keyboardAccess().then(setKeyboard, () => setKeyboard(false));
    Promise.all([api.load(), api.notes()])
      .then(([s, n]) => {
        setState(s);
        setSettings(s.settings);
        setGlobal(s.global);
        setApp(s.app);
        setSettingsApp(s.settingsApp);
        setShapes(s.shapes);
        setNotes(n.notes);
        if (n.open && n.notes.length > 0) setPage("setup");
        else setPage(s.app ? "app" : "every");
      })
      .catch((e) => setStatus({ kind: "error", text: sentence(e) }));
  }, []);

  const edit =
    <T,>(set: (value: T) => void) =>
    (value: T) => {
      set(value);
      setDirty(true);
      setStatus(null);
      setClosing(false);
    };

  const pages: Page[] = [
    "general",
    "every",
    ...(app ? (["app"] as const) : []),
    "shapes",
    ...(notes.length > 0 ? (["setup"] as const) : []),
  ];

  /** The first page with a gesture that won't save. */
  const unfinished = (): Page | null => {
    if (problems(global).some(Boolean)) return "every";
    if (app?.enabled && problems(app.gestures).some(Boolean)) return "app";
    const open = settings?.openSettings;
    if (open?.input === "stroke" && !open.value) return "general";
    if (settingsApp && problems(settingsApp.gestures).some(Boolean)) return "general";
    return null;
  };

  const onSave = async () => {
    if (!settings || !settingsApp || !dirty) return;
    const where = unfinished();
    if (where) {
      setPage(where);
      setStatus({ kind: "error", text: "Some gestures aren't finished yet: fix the ones marked in red, then save." });
      return;
    }
    try {
      await api.save(settings, global, app ? [app, settingsApp] : [settingsApp]);
      setDirty(false);
      setStatus({ kind: "ok", text: "Saved. mousewand uses it right away." });
    } catch (e) {
      setStatus({ kind: "error", text: `Couldn't save: ${sentence(e)}` });
    }
  };

  const addGesture = () => {
    if (page === "every") edit(setGlobal)([...global, api.newGesture()]);
    else if (page === "app" && app) edit(setApp)({ ...app, gestures: [...app.gestures, api.newGesture()] });
    else return;
    requestAnimationFrame(() => main.current?.scrollTo({ top: main.current.scrollHeight, behavior: "smooth" }));
  };

  const close = () => {
    if (dirty && !closing) {
      setClosing(true);
      setStatus({ kind: "warn", text: "You have unsaved changes. Save them, or close again to discard them." });
      return;
    }
    getCurrentWindow().close();
  };

  // The shortcuts the settings window's gestures press.
  const shortcuts = useRef<(e: KeyboardEvent) => void>(() => {});
  shortcuts.current = (e: KeyboardEvent) => {
    if (!e.ctrlKey || e.altKey || e.metaKey) return;
    const turn = (by: number) => {
      const i = pages.indexOf(page);
      setPage(pages[(i + by + pages.length) % pages.length]);
    };
    const run: Record<string, () => void> = {
      KeyS: onSave,
      KeyW: close,
      KeyN: addGesture,
      Tab: () => turn(e.shiftKey ? -1 : 1),
    };
    const action = (!e.shiftKey || e.code === "Tab") && run[e.code];
    if (action) {
      e.preventDefault();
      action();
    }
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => shortcuts.current(e);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // After a shape is drawn, redrawn or removed: its file changed on disk.
  const reloadShapes = () =>
    api
      .load()
      .then((s) => setShapes(s.shapes))
      .catch((e) => setStatus({ kind: "error", text: sentence(e) }));

  const shapeProps = { shapes, onNewShape: reloadShapes, keyboard };

  /** Where the unsaved gestures use a shape, if they do. */
  const usedWhere = (name: string): string | null => {
    const uses = (gestures: api.Gesture[]) => gestures.some((g) => g.input === "shape" && g.value === name);
    if (settings?.openSettings.input === "shape" && settings.openSettings.value === name) return "General";
    if (uses(global)) return "Every App";
    if (app && uses(app.gestures)) return api.appName(app);
    if (settingsApp && uses(settingsApp.gestures)) return "General";
    return null;
  };

  if (!state || !settings || !settingsApp) {
    return (
      <div className="grid h-screen place-items-center p-8">
        {status ? <Problem>{status.text}</Problem> : <span className="text-ink-2">Loading…</span>}
      </div>
    );
  }

  const titles: Record<Page, string> = {
    app: app ? api.appName(app) : "",
    every: "Every App",
    general: "General",
    shapes: "Shapes",
    setup: "Setup",
  };

  return (
    <div className="flex h-screen">
      <aside className="flex w-52 shrink-0 flex-col border-r border-line bg-sidebar px-2.5 pt-4 pb-3">
        <div className="flex items-center gap-2 px-2 pb-4">
          <Logo className="size-5" />
          <span className="text-[15px] font-semibold tracking-tight">mousewand</span>
        </div>
        <nav className="flex flex-col gap-0.5">
          {pages.map((p) => (
            <NavItem key={p} page={p} current={page} onSelect={setPage}>
              {titles[p]}
            </NavItem>
          ))}
        </nav>
        <span className="mt-auto truncate px-2 font-mono text-[10px] text-ink-3" title={state.dir}>
          {state.dir.replace(/^\/home\/[^/]+/, "~")}
        </span>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <main ref={main} className="flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-4xl flex-col gap-6 px-8 pt-6 pb-10">
            <h1 className="text-[20px] font-semibold tracking-tight">{titles[page]}</h1>
            {page === "general" && (
              <>
                <GeneralPage settings={settings} themes={state.themes} onChange={edit(setSettings)} />
                <SettingsGestures
                  settings={settings}
                  app={settingsApp}
                  onSettings={edit(setSettings)}
                  onApp={edit(setSettingsApp)}
                  shapes={shapes.map((s) => s.name)}
                  onNewShape={reloadShapes}
                />
              </>
            )}
            {page === "every" && (
              <GestureList
                title="Gestures"
                note="Every app gets these, unless it has its own gesture for the same input. Ready-made actions press the shortcut most apps use; if an app uses another, change the keys."
                gestures={global}
                onChange={edit(setGlobal)}
                readyMade={COMMON}
                {...shapeProps}
              />
            )}
            {page === "shapes" && <Shapes shapes={shapes} usedWhere={usedWhere} onChanged={reloadShapes} />}
            {page === "app" && app && (
              <AppPage app={app} presets={state.presets} onChange={edit(setApp)} shapeProps={shapeProps} />
            )}
            {page === "setup" && <SetupPage notes={notes} />}
          </div>
        </main>

        <footer className="flex items-center gap-4 border-t border-line bg-window/80 px-8 py-3 backdrop-blur">
          <div className="min-w-0 flex-1">
            {status?.kind === "error" || status?.kind === "warn" ? (
              <Problem>{status.text}</Problem>
            ) : (
              <p className="truncate text-[12px] text-ink-2">
                {status?.text ??
                  (dirty ? "Unsaved changes" : "Keys are physical positions on a US layout, whatever your layout.")}
              </p>
            )}
          </div>
          <button className="button primary" disabled={!dirty} onClick={onSave} title="Ctrl+S">
            Save
          </button>
        </footer>
      </div>
    </div>
  );
}

function NavItem({
  page,
  current,
  onSelect,
  children,
}: {
  page: Page;
  current: Page;
  onSelect: (page: Page) => void;
  children: ReactNode;
}) {
  const active = page === current;
  return (
    <button
      className={`flex h-7 items-center gap-2 rounded-md px-2 text-left text-[13px] ${
        active ? "bg-accent text-white" : "hover:bg-ink/5"
      }`}
      onClick={() => onSelect(page)}
    >
      <span className="truncate">{children}</span>
    </button>
  );
}

function GeneralPage({
  settings,
  themes,
  onChange,
}: {
  settings: api.Settings;
  themes: string[];
  onChange: (settings: api.Settings) => void;
}) {
  const set = (patch: Partial<api.Settings>) => onChange({ ...settings, ...patch });
  const number = (key: "threshold" | "holdMs" | "minSegment" | "cancelMs", unit: string) => (
    <>
      <input
        className="field w-20 text-right"
        type="number"
        min={1}
        value={settings[key]}
        onChange={(e) => set({ [key]: Number(e.target.value) })}
      />
      <span className="w-5 text-ink-2">{unit}</span>
    </>
  );
  return (
    <>
      <Section note="Hold it and draw. A quick click is still a click.">
        <Row label="Trigger button">
          <div className="w-72">
            <ButtonInput value={settings.trigger} onChange={(trigger) => set({ trigger })} />
          </div>
        </Row>
      </Section>
      <Section title="Feel">
        <Row label="Start after" hint="How far to move before a press becomes a gesture">
          {number("threshold", "px")}
        </Row>
        <Row label="Hold still for" hint="Holding still this long makes it a normal press, for dragging">
          {number("holdMs", "ms")}
        </Row>
        <Row label="Ignore strokes under" hint="Shorter wiggles don't count as a direction">
          {number("minSegment", "px")}
        </Row>
        <Row
          label="Cancel by holding still"
          hint="Changed your mind mid-stroke? Stop moving, and nothing fires"
        >
          {settings.cancelMs > 0 && number("cancelMs", "ms")}
          <Switch
            on={settings.cancelMs > 0}
            label="Cancel by holding still"
            onChange={(on) => set({ cancelMs: on ? 700 : 0 })}
          />
        </Row>
      </Section>
      <Section
        title="Pausing"
        note={
          <>
            For games and anything else that wants the button for itself. An app can also turn its gestures off
            for good, on its own page.
          </>
        }
      >
        <Row label="Pause or resume" hint="Bind it to a keyboard shortcut in your desktop's settings">
          <code className="rounded-md bg-ink/[0.06] px-2 py-1 font-mono text-[12px] select-text">mousewand toggle</code>
        </Row>
      </Section>
      <Section title="Trail">
        <Row label="Theme" hint="Themes are files in themes/">
          <select className="field w-40" value={settings.theme} onChange={(e) => set({ theme: e.target.value })}>
            {themes.map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
        </Row>
      </Section>
    </>
  );
}

function AppPage({
  app,
  presets,
  onChange,
  shapeProps,
}: {
  app: api.App;
  presets: api.Preset[];
  onChange: (app: api.App) => void;
  shapeProps: { shapes: api.ShapeInfo[]; onNewShape: (name: string) => void; keyboard: boolean };

}) {
  const lower = (s: string) => s.toLowerCase();
  const matching = presets.filter((p) => p.classes.some((c) => app.classes.map(lower).includes(lower(c))));
  const others = presets.filter((p) => !matching.includes(p));
  const [preset, setPreset] = useState(matching[0]?.name ?? "");
  const name = api.appName(app);

  const addPreset = () => {
    const chosen = presets.find((p) => p.name === preset);
    if (!chosen) return;
    // A preset's gesture replaces one drawn the same way.
    const key = (g: api.Gesture) => `${g.input}:${g.value}:${g.with ?? ""}`;
    const incoming = new Set(chosen.gestures.map(key));
    onChange({ ...app, gestures: [...app.gestures.filter((g) => !incoming.has(key(g))), ...chosen.gestures] });
  };

  return (
    <>
      <Section
        note={
          app.enabled
            ? `Saved in apps/${app.file}.toml`
            : `The trigger button is an ordinary button over ${name}: a press, a drag or a click, as if mousewand weren't there.`
        }
      >
        <Row label={`Gestures in ${name}`} hint={app.enabled ? "On" : "Off"}>
          <Switch on={app.enabled} label={`Gestures in ${name}`} onChange={(enabled) => onChange({ ...app, enabled })} />
        </Row>
        <Row label="Window classes" hint="Separate several with commas">
          <input
            className="field w-72 font-mono"
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
        </Row>
        {app.enabled && (
          <Row label="Preset" hint="Adds its gestures, replacing yours for the same input">
            <select className="field w-48" value={preset} onChange={(e) => setPreset(e.target.value)}>
              <option value="" disabled>
                Choose…
              </option>
              {matching.length > 0 && (
                <optgroup label="For This App">
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
            <button className="button" disabled={!preset} onClick={addPreset}>
              Add
            </button>
          </Row>
        )}
      </Section>
      {app.enabled && (
        <GestureList
          title="Gestures"
          note={`These replace the ones for every app that are drawn the same way. Ready-made actions press ${name}'s usual shortcuts; if you've changed one, change the keys too.`}
          gestures={app.gestures}
          onChange={(gestures) => onChange({ ...app, gestures })}
          readyMade={groupsFor(app.classes, presets)}
          {...shapeProps}
        />
      )}
    </>
  );
}

function SetupPage({ notes }: { notes: string[] }) {
  return (
    <Section title="On This Desktop" note="mousewand shows these once, and again when something new comes up.">
      {notes.map((note) => (
        <p key={note} className="px-3 py-3 leading-relaxed">
          {/* `code` in backticks: odd parts are code. */}
          {note.split("`").map((part, i) =>
            i % 2 ? (
              <code key={i} className="rounded bg-ink/[0.06] px-1 font-mono text-[0.92em]">
                {part}
              </code>
            ) : (
              part
            ),
          )}
        </p>
      ))}
    </Section>
  );
}
