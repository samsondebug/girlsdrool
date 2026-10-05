import { useId, type InputHTMLAttributes } from "react";

interface TextFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "id"> {
  label: string;
  hint?: string;
  error?: string | null;
  /** Money and codes render in the mono face with tabular numerals. */
  mono?: boolean;
}

export function TextField({
  label,
  hint,
  error = null,
  mono = false,
  className = "",
  ...rest
}: TextFieldProps) {
  const id = useId();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedBy = [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ");
  return (
    <div className={`flex flex-col gap-1 ${className}`}>
      <label htmlFor={id} className="text-12 font-medium text-text-dim">
        {label}
      </label>
      <input
        id={id}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy || undefined}
        className={`h-8 rounded-2 border bg-bg-inset px-2 text-14 text-text placeholder:text-text-dim ${
          error ? "border-negative" : "border-line focus:border-accent"
        } ${mono ? "money" : ""}`}
        {...rest}
      />
      {hint && !error ? (
        <p id={hintId} className="text-12 text-text-dim">
          {hint}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} role="alert" className="text-12 text-negative">
          {error}
        </p>
      ) : null}
    </div>
  );
}
