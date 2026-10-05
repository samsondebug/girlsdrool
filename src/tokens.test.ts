import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

/**
 * The light theme is a token swap, not a second design (ARCHITECTURE §10): the light block may
 * only redefine custom properties the dark root declares, plus the `color-scheme` hint the
 * browser needs, and nothing else.
 */

function block(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`no block for ${selector}`);
  const end = css.indexOf("}", start);
  return css.slice(start + selector.length + 2, end);
}

function tokenNames(body: string): string[] {
  return [...body.matchAll(/--([a-z0-9-]+)\s*:/g)].map((m) => m[1] ?? "");
}

describe("tokens.css", () => {
  const dark = tokenNames(block(":root"));
  const lightBody = block(':root[data-theme="light"]');
  const light = tokenNames(lightBody);

  it("the light theme redefines only tokens the dark root declares", () => {
    expect(light.length).toBeGreaterThan(0);
    for (const name of light) expect(dark).toContain(name);
  });

  it("the light block holds token values and the color-scheme hint, nothing else", () => {
    const lines = lightBody
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l !== "" && !l.startsWith("/*"));
    expect(lines.length).toBe(light.length + 1);
    for (const line of lines) {
      expect(line).toMatch(/^(--[a-z0-9-]+:\s*[^;{}]+|color-scheme:\s*(light|dark));$/);
    }
    expect(lines).toContain("color-scheme: light;");
  });

  it("sizes, spacing, radii and fonts are shared: light swaps colors only", () => {
    const swapped = new Set(light);
    for (const name of dark) {
      if (/^(text-\d+|space-\d+|radius-\d+|font-)/.test(name)) {
        expect(swapped.has(name)).toBe(false);
      }
    }
  });
});
