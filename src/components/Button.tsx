import type { ButtonHTMLAttributes } from "react";

export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
}

const variantClasses: Record<ButtonVariant, string> = {
  primary: "bg-accent text-bg border-accent hover:brightness-110",
  secondary: "bg-bg-raised text-text border-line hover:border-text-dim",
  quiet: "bg-transparent text-text-dim border-transparent hover:text-text",
  danger: "bg-transparent text-negative border-negative hover:bg-negative hover:text-bg",
};

export function Button({
  variant = "secondary",
  type = "button",
  className = "",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={`inline-flex h-8 items-center justify-center gap-2 rounded-2 border px-3 text-14 font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${variantClasses[variant]} ${className}`}
      {...rest}
    />
  );
}
