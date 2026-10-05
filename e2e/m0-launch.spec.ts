import { expect, test } from "@playwright/test";

import { launchKept } from "./kept";

const PASS = "correct horse battery staple";

test.describe("M0: launch, create, lock, unlock", () => {
  test("first run creates an encrypted database; a wrong passphrase fails closed", async () => {
    const app = await launchKept();
    try {
      const { page } = app;

      // KEPT_DATA_DIR is set, so the folder is chosen and only the database is missing.
      await expect(page.getByRole("heading", { name: "Create the database" })).toBeVisible();
      await page.getByLabel("Passphrase", { exact: true }).fill(PASS);
      await page.getByLabel("Confirm passphrase").fill(PASS);
      await page.getByRole("button", { name: "Create database" }).click();

      await expect(page.getByRole("heading", { name: "Safe to spend" })).toBeVisible();

      // The dashboard never scrolls at the configured 1440×900 window.
      const main = page.getByRole("main");
      const fits = await main.evaluate((el) => el.scrollHeight <= el.clientHeight);
      expect(fits).toBe(true);

      await page.getByRole("button", { name: "Lock" }).click();
      await expect(page.getByRole("heading", { name: "Unlock Kept" })).toBeVisible();

      await page.getByLabel("Passphrase", { exact: true }).fill("not the passphrase");
      await page.getByRole("button", { name: "Unlock", exact: true }).click();
      await expect(page.getByRole("alert")).toContainText("wrong passphrase");
      await expect(page.getByRole("heading", { name: "Unlock Kept" })).toBeVisible();

      await page.getByLabel("Passphrase", { exact: true }).fill(PASS);
      await page.getByRole("button", { name: "Unlock", exact: true }).click();
      await expect(page.getByRole("heading", { name: "Safe to spend" })).toBeVisible();
    } finally {
      await app.close();
    }
  });
});
