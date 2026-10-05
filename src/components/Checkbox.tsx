import { useId, type InputHTMLAttributes } from "react";

interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "id" | "type"> {
  label: string;
}

export function Checkbox({ label, className = "", ...rest }: CheckboxProps) {
  const id = useId();
  return (
    <div className={`flex items-center gap-2 ${className}`}>
      <input id={id} type="checkbox" className="h-4 w-4 accent-accent" {...rest} />
      <label htmlFor={id} className="text-14 text-text">
        {label}
      </label>
    </div>
  );
}
