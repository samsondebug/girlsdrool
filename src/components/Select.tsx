import { useId, type SelectHTMLAttributes } from "react";

interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "id"> {
  label?: string;
  /** Hide the label visually (keeps it for assistive tech). */
  compact?: boolean;
}

export function Select({ label, compact = false, className = "", children, ...rest }: SelectProps) {
  const id = useId();
  return (
    <div className={`flex ${compact ? "items-center gap-2" : "flex-col gap-1"} ${className}`}>
      {label ? (
        <label htmlFor={id} className={compact ? "sr-only" : "text-12 font-medium text-text-dim"}>
          {label}
        </label>
      ) : null}
      <select
        id={id}
        className="h-8 min-w-0 rounded-2 border border-line bg-bg-inset px-2 text-14 text-text focus:border-accent"
        {...rest}
      >
        {children}
      </select>
    </div>
  );
}
