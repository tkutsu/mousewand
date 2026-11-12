// Every page as it looks, in light and dark, against the shots in e2e/look/.
// After a deliberate change: pnpm test:look --update-snapshots.

import { STATE, expect, test } from "./fixture";

const pages = ["Firefox", "Every App", "General", "Shapes"];

for (const name of pages) {
  test(name, async ({ page }) => {
    await page.getByRole("navigation").getByRole("button", { name }).click();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(name);
    await expect(page).toHaveScreenshot(`${name.toLowerCase().replace(" ", "-")}.png`);
  });
}

test.describe("with setup notes", () => {
  test.use({
    backend: {
      state: STATE,
      notes: ["labwc doesn't say which window is under the cursor. Turn on `followMouse` in its `rc.xml`."],
      saveError: null,
    },
  });

  test("Setup", async ({ page }) => {
    await page.getByRole("navigation").getByRole("button", { name: "Setup" }).click();
    await expect(page).toHaveScreenshot("setup.png");
  });
});
