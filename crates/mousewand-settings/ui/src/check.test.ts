import { describe, expect, it } from "vitest";
import { newGesture, type Gesture } from "./api";
import { cleanStroke, problems, sentence } from "./check";

const gesture = (patch: Partial<Gesture>): Gesture => ({
  ...newGesture(),
  value: "L",
  steps: [{ action: "keys", target: "alt+Left" }],
  ...patch,
});

describe("problems", () => {
  it("passes a finished gesture", () => {
    expect(problems([gesture({})])).toEqual([null]);
  });

  it("asks for a stroke", () => {
    expect(problems([gesture({ value: "" })])[0]).toMatch(/Type a stroke/);
  });

  it("names the earlier gesture drawn the same way", () => {
    expect(problems([gesture({}), gesture({})])).toEqual([null, "Gesture 1 already uses this stroke."]);
  });

  it("lets the same stroke differ by its modifiers", () => {
    expect(problems([gesture({}), gesture({ with: "shift" })])).toEqual([null, null]);
    expect(problems([gesture({ with: "shift" }), gesture({ with: "shift" })])[1]).toBe(
      "Gesture 1 already uses this stroke with these keys.",
    );
  });

  it("asks for what each kind of step needs", () => {
    const step = (action: "keys" | "command" | "dispatch") => problems([gesture({ steps: [{ action, target: " " }] })])[0];
    expect(step("keys")).toMatch(/press Record/);
    expect(step("command")).toBe("Type the command to run.");
    expect(step("dispatch")).toBe("Type the compositor command.");
  });

  it("says which step in a list is unfinished", () => {
    const steps = [
      { action: "keys" as const, target: "ctrl+l" },
      { action: "command" as const, target: "" },
    ];
    expect(problems([gesture({ steps })])[0]).toBe("Type the command to run in step 2.");
  });

  it("puts a wait between two steps, in whole milliseconds", () => {
    const keys = { action: "keys" as const, target: "a" };
    expect(problems([gesture({ steps: [{ action: "wait", target: "50" }, keys] })])[0]).toBe(
      "A wait goes between two other steps.",
    );
    expect(problems([gesture({ steps: [keys, { action: "wait", target: "soon" }, keys] })])[0]).toBe(
      "Type how many milliseconds to wait in step 2.",
    );
    expect(problems([gesture({ steps: [keys, { action: "wait", target: "50" }, keys] })])).toEqual([null]);
  });

  it("only holds keys with one step that presses them", () => {
    const hold = { kind: "hold" as const, hold: "alt" };
    expect(problems([gesture({ ...hold, steps: [{ action: "command", target: "x" }] })])[0]).toMatch(/one step/);
    expect(problems([gesture({ ...hold, steps: [{ action: "keys", target: "Tab" }] })])).toEqual([null]);
  });
});

describe("cleanStroke", () => {
  it("keeps directions only, in capitals, none twice in a row", () => {
    expect(cleanStroke("uuddlr")).toBe("UDLR");
    expect(cleanStroke("d r!x9 1")).toBe("DR91");
    expect(cleanStroke("")).toBe("");
  });
});

describe("sentence", () => {
  it("drops the Error: prefix and capitalizes", () => {
    expect(sentence(new Error("no such file"))).toBe("No such file");
    expect(sentence("  bad toml ")).toBe("Bad toml");
  });
});
