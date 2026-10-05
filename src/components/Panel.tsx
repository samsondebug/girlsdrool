import type { ReactNode } from "react";

interface PanelProps {
  title: string;
  children: ReactNode;
  className?: string;
  /** Right-aligned content in the title row (a status chip, a link). */
  aside?: ReactNode;
}

/** A raised surface with a quiet uppercase title. Dashboard panels never scroll. */
export function Panel({ title, children, className = "", aside }: PanelProps) {
  return (
    <section
      aria-label={title}
      className={`flex min-h-0 flex-col gap-2 rounded-2 border border-line bg-bg-raised p-3 ${className}`}
    >
      <header className="flex items-baseline justify-between gap-2">
        <h2 className="text-12 font-medium uppercase tracking-wide text-text-dim">{title}</h2>
        {aside}
      </header>
      <div className="min-h-0 flex-1">{children}</div>
    </section>
  );
}
