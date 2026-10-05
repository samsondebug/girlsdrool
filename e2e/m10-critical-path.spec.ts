import { readFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

import { launchKept } from "./kept";

const PASS = "correct horse battery staple";

/**
 * The critical path on the real app (ADR-0009, M10): create the database, add an account, import
 * a statement by pasting it, reconcile the month with the fixture's closing, read the hero.
 * Every figure typed here comes from fixtures/EXPECTED.md (Northbank Checking, 2026-07).
 */
test.describe("M10: import → reconcile → safe to spend", () => {
  test("a pasted statement reconciles and the dashboard reports a trusted hero", async () => {
    const app = await launchKept();
    try {
      const { page } = app;
      await expect(page.getByRole("heading", { name: "Create the database" })).toBeVisible();
      await page.getByLabel("Passphrase", { exact: true }).fill(PASS);
      await page.getByLabel("Confirm passphrase").fill(PASS);
      await page.getByRole("button", { name: "Create database" }).click();
      await expect(page.getByRole("heading", { name: "Safe to spend" })).toBeVisible();

      // the account exactly as the fixture lists it
      await page.getByRole("button", { name: "Accounts" }).click();
      await page.getByLabel("Name", { exact: true }).fill("Northbank Checking");
      await page.getByLabel("Institution").fill("Northbank");
      await page.getByLabel("Kind").selectOption("checking");
      await page.getByLabel("Opening balance").fill("3214.55");
      await page.getByLabel("Opening date").fill("2026-07-01");
      await page.getByRole("button", { name: "Add account" }).click();
      await expect(page.getByText("Northbank Checking").first()).toBeVisible();

      // import July by pasting the fixture file; the header auto-detects the profile
      await page.getByRole("button", { name: "Import" }).click();
      await page.getByLabel("Account").selectOption({ label: "Northbank Checking" });
      const csv = readFileSync("fixtures/northbank/northbank_checking_2026-07.csv", "utf8");
      await page.getByLabel("Or paste CSV text").fill(csv);
      await page.getByRole("button", { name: "Use pasted text" }).click();
      await expect(page.getByText("profile: northbank_csv")).toBeVisible();
      await page.getByRole("button", { name: "Import this file" }).click();
      await expect(page.getByText(/Read \d+ rows\. Inserted \d+\./)).toBeVisible();

      // reconcile the month with the closing the fixture states (EXPECTED.md: 5,647.48)
      await page.getByRole("button", { name: "Reconcile" }).click();
      await page.getByRole("button", { name: "Northbank Checking" }).click();
      await page.getByLabel("Period end").fill("2026-07-31");
      await page.getByLabel("Statement closing balance").fill("5647.48");
      await page.getByRole("button", { name: "Reconcile", exact: true }).last().click();
      await expect(page.getByText("balanced").first()).toBeVisible();

      // the hero is a number, trusted, and the dashboard fits the window
      await page.getByRole("button", { name: "Dashboard" }).click();
      await expect(page.getByRole("heading", { name: "Safe to spend" })).toBeVisible();
      await expect(page.getByText("Every cash account is reconciled.")).toBeVisible();
      await expect(page.getByLabel("Safe to spend not computed")).toHaveCount(0);
      const main = page.getByRole("main");
      const fits = await main.evaluate((el) => el.scrollHeight <= el.clientHeight);
      expect(fits).toBe(true);
    } finally {
      await app.close();
    }
  });

  test("the palette navigates and the shortcut overlay opens on F1", async () => {
    const app = await launchKept();
    try {
      const { page } = app;
      await page.getByLabel("Passphrase", { exact: true }).fill(PASS);
      await page.getByLabel("Confirm passphrase").fill(PASS);
      await page.getByRole("button", { name: "Create database" }).click();
      await expect(page.getByRole("heading", { name: "Safe to spend" })).toBeVisible();

      await page.keyboard.press("Control+k");
      await expect(page.getByRole("heading", { name: "Command palette" })).toBeVisible();
      await page.getByLabel("Command").fill("settings");
      await page.keyboard.press("Enter");
      await expect(page.getByRole("heading", { name: "Data folder" })).toBeVisible();

      await page.keyboard.press("F1");
      await expect(page.getByRole("heading", { name: "Keyboard shortcuts" })).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(page.getByRole("heading", { name: "Keyboard shortcuts" })).toHaveCount(0);

      await page.keyboard.press("Control+k");
      await page.getByLabel("Command").fill("lock");
      await page.keyboard.press("Enter");
      await expect(page.getByRole("heading", { name: "Unlock Kept" })).toBeVisible();
    } finally {
      await app.close();
    }
  });
});
