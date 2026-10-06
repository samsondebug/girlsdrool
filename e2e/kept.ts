/**
 * Launch the built Kept binary with a scratch data folder and attach Playwright over CDP.
 * The app opens WebView2's remote-debugging port when KEPT_E2E_CDP_PORT is set (lib.rs builds
 * the main window with the switch); WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS is set as well for a
 * binary built without that hook.
 */
import { chromium, type Browser, type Page } from "@playwright/test";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

export interface KeptApp {
  page: Page;
  browser: Browser;
  process: ChildProcess;
  dataDir: string;
  close: () => Promise<void>;
}

const candidates = [
  "src-tauri/target/debug/Kept.exe",
  "src-tauri/target/debug/kept.exe",
  "src-tauri/target/release/Kept.exe",
  "src-tauri/target/release/kept.exe",
];

function exePath(): string {
  const fromEnv = process.env.KEPT_E2E_EXE;
  if (fromEnv) return fromEnv;
  const found = candidates.find((c) => existsSync(c));
  if (!found) {
    throw new Error(
      `Kept binary not found. Run \`just build-debug\` or set KEPT_E2E_EXE. Looked at: ${candidates.join(", ")}`,
    );
  }
  return found;
}

/** The app's own log files, so a failed attach shows what Kept did before the deadline. */
function readLogTail(dataDir: string): string {
  const dir = path.join(dataDir, "logs");
  if (!existsSync(dir)) return "(no logs directory)";
  return readdirSync(dir)
    .map((name) => `${name}:\n${readFileSync(path.join(dir, name), "utf8").slice(-4000)}`)
    .join("\n");
}

async function connectWithRetry(endpoint: string, deadlineMs: number): Promise<Browser> {
  const deadline = Date.now() + deadlineMs;
  let lastError: unknown = null;
  while (Date.now() < deadline) {
    try {
      return await chromium.connectOverCDP(endpoint, { timeout: 5_000 });
    } catch (error: unknown) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }
  throw new Error(`could not attach to WebView2 at ${endpoint}: ${String(lastError)}`);
}

export async function launchKept(): Promise<KeptApp> {
  if (process.platform !== "win32") {
    throw new Error("E2E requires Windows: WebView2 exposes CDP; WebKitGTK does not (ADR-0009).");
  }
  const port = 9300 + Math.floor(Math.random() * 400);
  const dataDir = mkdtempSync(path.join(tmpdir(), "kept-e2e-"));
  const child = spawn(exePath(), [], {
    env: {
      ...process.env,
      KEPT_DATA_DIR: dataDir,
      KEPT_E2E_CDP_PORT: String(port),
      WEBVIEW2_USER_DATA_FOLDER: path.join(dataDir, "webview"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  const state: { exited: string | null } = { exited: null };
  const exited = new Promise<void>((resolve) => {
    child.on("exit", (code, signal) => {
      state.exited = `Kept exited early (code ${String(code)}, signal ${String(signal)})`;
      resolve();
    });
  });
  let browser: Browser;
  try {
    browser = await connectWithRetry(`http://127.0.0.1:${port}`, 60_000);
  } catch (error: unknown) {
    const status = state.exited ?? "Kept is still running";
    throw new Error(`${String(error)}\n${status}\n--- kept logs ---\n${readLogTail(dataDir)}`, {
      cause: error,
    });
  }
  const context = browser.contexts()[0] ?? (await browser.newContext());
  const page = context.pages()[0] ?? (await context.waitForEvent("page"));
  await page.waitForLoadState("domcontentloaded");
  return {
    page,
    browser,
    process: child,
    dataDir,
    close: async () => {
      try {
        await browser.close();
      } finally {
        // The whole process tree: WebView2's helper processes hold the data folder open until
        // they are gone, and the database file until the app is.
        spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
        await Promise.race([exited, new Promise((resolve) => setTimeout(resolve, 10_000))]);
        rmSync(dataDir, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
      }
    },
  };
}
