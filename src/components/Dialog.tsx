import * as RadixDialog from "@radix-ui/react-dialog";
import type { ReactNode } from "react";

import { Button } from "./Button";

interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  width?: "md" | "lg";
}

/** Radix handles focus trapping, escape and aria wiring; the skin is the app's tokens. */
export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  width = "md",
}: DialogProps) {
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="fixed inset-0 bg-bg-inset/80" />
        <RadixDialog.Content
          className={`fixed top-1/2 left-1/2 flex max-h-[90vh] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 overflow-auto rounded-2 border border-line bg-bg-raised p-5 text-text ${
            width === "lg" ? "w-[840px]" : "w-[560px]"
          }`}
        >
          <div className="flex items-start justify-between gap-4">
            <div>
              <RadixDialog.Title className="text-16 font-semibold">{title}</RadixDialog.Title>
              {description ? (
                <RadixDialog.Description className="mt-1 text-14 text-text-dim">
                  {description}
                </RadixDialog.Description>
              ) : (
                <RadixDialog.Description className="sr-only">{title}</RadixDialog.Description>
              )}
            </div>
            <RadixDialog.Close asChild>
              <Button variant="quiet" aria-label="Close">
                ×
              </Button>
            </RadixDialog.Close>
          </div>
          {children}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}
