import { formatCents, type Cents } from "../lib/money";

interface MoneyProps {
  cents: Cents;
  /** Size token; the hero uses 28. */
  size?: 14 | 16 | 20 | 28;
  /** Color negatives with the negative token (sign is always shown regardless). */
  tone?: boolean;
  /** Mark the figure untrusted: dashed underline, the label is rendered by the caller. */
  untrusted?: boolean;
  sign?: "auto" | "always";
}

const sizeClass: Record<NonNullable<MoneyProps["size"]>, string> = {
  14: "text-14",
  16: "text-16",
  20: "text-20",
  28: "text-28",
};

/** Integer cents from the core, rendered in the mono face with tabular numerals. */
export function Money({
  cents,
  size = 14,
  tone = true,
  untrusted = false,
  sign = "auto",
}: MoneyProps) {
  const negative = tone && cents < 0;
  return (
    <span
      className={`money ${sizeClass[size]} ${negative ? "text-negative" : ""} ${untrusted ? "untrusted" : ""}`}
    >
      {formatCents(cents, { sign })}
    </span>
  );
}
