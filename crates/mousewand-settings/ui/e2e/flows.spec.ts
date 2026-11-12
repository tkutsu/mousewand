import type { Page } from "@playwright/test";
import type { App, Gesture, Settings } from "../src/api";
import { STATE, expect, test } from "./fixture";

type Saved = { settings: Settings; global: Gesture[]; apps: App[] };

const heading = (page: Page) => page.getByRole("heading", { level: 1 });
const go = (page: Page, name: string) => page.getByRole("navigation").getByRole("button", { name }).click();
const save = (page: Page) => page.getByRole("button", { name: "Save" }).click();

test("opens on the app it was drawn over", async ({ page }) => {
  await expect(heading(page)).toHaveText("Firefox");
  await expect(page.getByRole("switch", { name: "Gestures in Firefox" })).toHaveAttribute("aria-checked", "true");
});

test.describe("without an app", () => {
  test.use({ backend: { state: { ...STATE, app: null }, notes: [], saveError: null } });

  test("opens on Every App, with no app page", async ({ page }) => {
    await expect(heading(page)).toHaveText("Every App");
    await expect(page.getByRole("navigation").getByRole("button")).toHaveText(["General", "Every App", "Shapes"]);
  });
});

test("turns pages from the sidebar and with Ctrl+Tab", async ({ page }) => {
  await go(page, "Shapes");
  await expect(heading(page)).toHaveText("Shapes");
  await page.keyboard.press("Control+Tab");
  await expect(heading(page)).toHaveText("General");
  await page.keyboard.press("Control+Shift+Tab");
  await expect(heading(page)).toHaveText("Shapes");
});

test("adds a gesture with a ready-made action and saves it", async ({ page, calls }) => {
  await go(page, "Every App");
  await expect(page.getByRole("button", { name: "Save" })).toBeDisabled();
  await page.getByRole("button", { name: "Add Gesture" }).click();
  await page.getByPlaceholder("DR").last().fill("lu");
  // The Tabs group's first action: New tab.
  await page.locator("select", { has: page.locator('option[value="keys"]') }).last().selectOption("ready:1:0");
  await expect(page.getByPlaceholder("Name this gesture").last()).toHaveValue("New tab");
  await save(page);

  await expect(page.getByText("Saved. mousewand uses it right away.")).toBeVisible();
  const [saved] = (await calls("save")) as Saved[];
  expect(saved.global.at(-1)).toMatchObject({
    input: "stroke",
    value: "LU",
    name: "New tab",
    steps: [{ action: "keys", target: "ctrl+t" }],
  });
  expect(saved.global).toHaveLength(STATE.global.length + 1);
});

test("won't save an unfinished gesture, and says why", async ({ page, calls }) => {
  await go(page, "Every App");
  await page.getByRole("button", { name: "Add Gesture" }).click();
  await save(page);
  await expect(page.getByText("Some gestures aren't finished yet")).toBeVisible();
  await expect(page.getByText("Type a stroke, like DR for down, then right.")).toBeVisible();
  expect(await calls("save")).toEqual([]);
});

test("turns an app's gestures off", async ({ page, calls }) => {
  await page.getByRole("switch", { name: "Gestures in Firefox" }).click();
  await expect(page.getByText("The trigger button is an ordinary button over Firefox")).toBeVisible();
  await save(page);
  const [saved] = (await calls("save")) as Saved[];
  expect(saved.apps.map((a) => [a.file, a.enabled])).toEqual([
    ["firefox", false],
    ["mousewand-settings", true],
  ]);
});

test("adds a preset, replacing gestures drawn the same way", async ({ page, calls }) => {
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await save(page);
  const [saved] = (await calls("save")) as Saved[];
  // Back is replaced by the preset's, which comes after; Previous tab is
  // drawn with Shift, so it stays.
  expect(saved.apps[0].gestures.map((g) => g.name)).toEqual(["Forward", "Previous tab", "New tab", "Back"]);
});

test("saves the theme picked under General", async ({ page, calls }) => {
  await go(page, "General");
  await page.getByTitle("How the trail looks as you draw").selectOption("ember");
  await page.keyboard.press("Control+s");
  const [saved] = (await calls("save")) as Saved[];
  expect(saved.settings.theme).toBe("ember");
});

test("asks before closing with unsaved changes", async ({ page, calls }) => {
  await page.getByRole("switch", { name: "Gestures in Firefox" }).click();
  await page.keyboard.press("Control+w");
  await expect(page.getByText("You have unsaved changes.")).toBeVisible();
  expect(await calls("plugin:window|close")).toEqual([]);
  await page.keyboard.press("Control+w");
  await expect.poll(() => calls("plugin:window|close")).toHaveLength(1);
});

test.describe("when saving fails", () => {
  test.use({ backend: { state: STATE, notes: [], saveError: "permission denied: config.toml" } });

  test("says why", async ({ page }) => {
    await page.getByRole("switch", { name: "Gestures in Firefox" }).click();
    await save(page);
    await expect(page.getByText("Couldn't save: Permission denied: config.toml")).toBeVisible();
  });
});
