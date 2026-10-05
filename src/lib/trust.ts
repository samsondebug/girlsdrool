/**
 * Trust helpers for the untrusted marking (spec: unreconciled figures are visibly marked
 * everywhere they appear). The core decides each account's status; this only picks the names
 * a figure must carry.
 */
import type { TrustReport, TrustStatus } from "./ipc";

/** Names of the accounts among `ids` (every account when null) whose figures cannot be trusted. */
export function untrustedAmong(
  trust: TrustReport | undefined,
  ids: readonly number[] | null,
): string[] {
  if (trust === undefined) return [];
  return trust.accounts
    .filter((a) => (ids === null || ids.includes(a.account_id)) && a.status !== "reconciled")
    .map((a) => a.account_name);
}

/** Names of the contributing (cash) accounts that are not reconciled. */
export function untrustedCash(trust: TrustReport | undefined): string[] {
  if (trust === undefined) return [];
  return trust.accounts
    .filter((a) => a.contributes && a.status !== "reconciled")
    .map((a) => a.account_name);
}

export const TRUST_TONE: Record<TrustStatus, "positive" | "warning" | "negative" | "dim"> = {
  reconciled: "positive",
  stale: "warning",
  off: "negative",
  never_reconciled: "dim",
};

export const TRUST_LABEL: Record<TrustStatus, string> = {
  reconciled: "reconciled",
  stale: "stale",
  off: "off",
  never_reconciled: "never reconciled",
};
