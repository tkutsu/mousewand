import { defineConfig, devices } from "@playwright/test";

// The settings UI in Chromium, against vite, with the Tauri backend faked
// (e2e/fixture.ts). The window's size is tauri.conf.json's.
const chrome = { ...devices["Desktop Chrome"], viewport: { width: 920, height: 720 } };

export default defineConfig({
  testDir: "e2e",
  snapshotPathTemplate: "e2e/look/{arg}-{projectName}.png",
  fullyParallel: true,
  reporter: [["list"]],
  use: { baseURL: "http://localhost:5174" },
  expect: { toHaveScreenshot: { maxDiffPixelRatio: 0.002, animations: "disabled" } },
  projects: [
    { name: "flows", testMatch: "flows.spec.ts", use: chrome },
    { name: "light", testMatch: "look.spec.ts", use: { ...chrome, colorScheme: "light" } },
    { name: "dark", testMatch: "look.spec.ts", use: { ...chrome, colorScheme: "dark" } },
  ],
  webServer: {
    // Not 5173, so it runs beside `pnpm dev`.
    command: "pnpm exec vite --port 5174 --strictPort",
    url: "http://localhost:5174",
    reuseExistingServer: true,
  },
});
