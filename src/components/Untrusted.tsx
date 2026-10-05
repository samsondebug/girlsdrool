import type { ReactNode } from "react";

import type { Cents } from "../lib/money";
import { Money } from "./Money";

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

interface MarkedMoneyProps {
  cents: Cents;
  /** The accounts behind the figure that are not reconciled; empty means the figure is trusted. */
  untrustedBy: readonly string[];
  size?: 14 | 16 | 20 | 28;
  tone?: boolean;
  sign?: "auto" | "always";
}

/** A money figure that carries its untrusted marking whenever any account behind it is not reconciled. */
export function MarkedMoney({
  cents,
  untrustedBy,
  size = 14,
  tone = true,
  sign = "auto",
}: MarkedMoneyProps) {
  const money = (
    <Money cents={cents} size={size} tone={tone} sign={sign} untrusted={untrustedBy.length > 0} />
  );
  if (untrustedBy.length === 0) return money;
  return <Untrusted accounts={untrustedBy}>{money}</Untrusted>;
}
