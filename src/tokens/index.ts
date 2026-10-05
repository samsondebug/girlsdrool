/**
 * Read design tokens from CSS so charts use the same colors as everything else. Values are the
 * computed custom properties on `:root`, so the light swap applies automatically.
 */
export const colorTokens = [
  "bg",
  "bg-raised",
  "bg-inset",
  "line",
  "text",
  "text-dim",
  "accent",
  "positive",
  "negative",
  "warning",
  "untrusted",
  "info",
] as const;

export type ColorToken = (typeof colorTokens)[number];

export function readToken(name: ColorToken): string {
  return getComputedStyle(document.documentElement).getPropertyValue(`--${name}`).trim();
}
