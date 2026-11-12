import { describe, expect, it } from "vitest";
import { arrows, buttonOf, comboOf } from "./keys";

const key = (code: string, mods: Partial<KeyboardEvent> = {}) =>
  comboOf({ code, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods } as KeyboardEvent);

describe("comboOf", () => {
  it("reads the key's position, not what the layout prints", () => {
    expect(key("KeyT", { ctrlKey: true, shiftKey: true })).toBe("ctrl+shift+t");
    expect(key("KeyQ", { altKey: true })).toBe("alt+q");
  });

  it("names modifiers in a set order", () => {
    expect(key("KeyA", { metaKey: true, altKey: true, shiftKey: true, ctrlKey: true })).toBe("ctrl+shift+alt+super+a");
  });

  it("waits while only modifiers are down", () => {
    expect(key("ControlLeft", { ctrlKey: true })).toBeNull();
    expect(key("ShiftRight", { shiftKey: true })).toBeNull();
  });

  it("names digits, function keys and the rest", () => {
    expect(key("Digit5")).toBe("5");
    expect(key("Numpad7")).toBe("7");
    expect(key("F11")).toBe("F11");
    expect(key("ArrowLeft", { altKey: true })).toBe("alt+Left");
    expect(key("BracketLeft")).toBe("leftbrace");
    expect(key("Period")).toBe("dot");
  });

  it("ignores keys mousewand can't send", () => {
    expect(key("IntlBackslash")).toBeNull();
  });
});

describe("buttonOf", () => {
  it("names every button but left", () => {
    const button = (n: number) => buttonOf({ button: n } as MouseEvent);
    expect([0, 1, 2, 3, 4].map(button)).toEqual([null, "middle", "right", "back", "forward"]);
  });
});

describe("arrows", () => {
  it("draws a stroke's directions", () => {
    expect(arrows("DR")).toBe("↓→");
    expect(arrows("u7913")).toBe("↑↖↗↙↘");
    expect(arrows("x")).toBe("·");
  });
});
