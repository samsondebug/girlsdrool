import type { ReactNode } from "react";

interface UntrustedProps {
  children: ReactNode;
  /** Account names whose reconciliation is missing, stale, or off. */
  accounts: readonly string[];
}

/**
 * Untrusted figures get a dashed underline and a label naming the accounts; never color alone
 * (spec: UI). The child is expected to carry the `untrusted` class (see `Money`).
 */
export function Untrusted({ children, accounts }: UntrustedProps) {
  return (
    <span className="inline-flex flex-wrap items-baseline gap-2">
      {children}
      <span className="text-12 text-untrusted">
        unreconciled: {accounts.length > 0 ? accounts.join(", ") : "no account reconciled"}
      </span>
    </span>
  );
}
