import { defineConfig } from "@playwright/test";

/**
 * The critical path runs against the real app: Kept is launched with WebView2's remote
 * debugging port and Playwright attaches over CDP (ADR-0009). Windows only.
 */
export default defineConfig({
  testDir: "./e2e",
  timeout: 180_000,
  expect: { timeout: 15_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: { trace: "retain-on-failure" },
});
