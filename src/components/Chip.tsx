import type { ReactNode } from "react";

interface ChipProps {
  children: ReactNode;
  tone?: "dim" | "warning" | "negative" | "info" | "positive" | "untrusted";
  title?: string;
}

const toneClass: Record<NonNullable<ChipProps["tone"]>, string> = {
  dim: "border-line text-text-dim",
  warning: "border-warning text-warning",
  negative: "border-negative text-negative",
  info: "border-info text-info",
  positive: "border-positive text-positive",
  untrusted: "border-untrusted text-untrusted",
};

/** A small bordered label. Text carries the meaning; the tone only reinforces it. */
export function Chip({ children, tone = "dim", title }: ChipProps) {
  return (
    <span
      title={title}
      className={`inline-flex h-5 items-center rounded-1 border px-1.5 text-12 leading-none whitespace-nowrap ${toneClass[tone]}`}
    >
      {children}
    </span>
  );
}
