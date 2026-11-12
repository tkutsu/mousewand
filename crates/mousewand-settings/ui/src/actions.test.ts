import { describe, expect, it } from "vitest";
import { COMMON, groupsFor, menuLabel, shown } from "./actions";
import { appName, newGesture, type App, type Preset } from "./api";

const firefox: Preset = {
  name: "Firefox",
  classes: ["firefox", "librewolf"],
  gestures: [
    { ...newGesture(), value: "L", name: "Back", steps: [{ action: "keys", target: "alt+Left" }] },
    // Unnamed, or more than one step: not offered as a ready-made action.
    { ...newGesture(), value: "R", steps: [{ action: "keys", target: "alt+Right" }] },
    {
      ...newGesture(),
      value: "UR",
      name: "Duplicate",
      steps: [
        { action: "keys", target: "ctrl+l" },
        { action: "keys", target: "alt+Enter" },
      ],
    },
  ],
};

describe("groupsFor", () => {
  it("puts the app's own preset first, matched by class in any case", () => {
    const groups = groupsFor(["LibreWolf"], [firefox]);
    expect(groups[0]).toEqual({
      title: "For Firefox",
      actions: [{ name: "Back", step: { action: "keys", target: "alt+Left" } }],
    });
    expect(groups.slice(1)).toEqual(COMMON);
  });

  it("offers only the common actions to other apps", () => {
    expect(groupsFor(["kitty"], [firefox])).toEqual(COMMON);
  });
});

describe("shown", () => {
  it("writes combos the way menus do", () => {
    expect(shown("ctrl+shift+t")).toBe("Ctrl+Shift+T");
    expect(shown("alt+Left")).toBe("Alt+←");
    expect(shown("F5")).toBe("F5");
  });
});

describe("menuLabel", () => {
  it("adds the keys to a keys action, and nothing to a command", () => {
    expect(menuLabel({ name: "Back", step: { action: "keys", target: "alt+Left" } })).toBe("Back  ·  Alt+←");
    expect(menuLabel({ name: "Open", step: { action: "command", target: "xdg-open x" } })).toBe("Open");
  });
});

describe("appName", () => {
  const app = (classes: string[], file = "file"): App => ({ file, classes, enabled: true, gestures: [] });

  it("turns a window class into a name for people", () => {
    expect(appName(app(["org.gnome.Nautilus"]))).toBe("Nautilus");
    expect(appName(app(["google-chrome"]))).toBe("Google Chrome");
    expect(appName(app([], "my_app"))).toBe("My App");
  });
});
