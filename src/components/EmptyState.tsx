interface EmptyStateProps {
  /** What is missing, as a plain statement. */
  missing: string;
  /** The command or screen that fixes it. */
  fix: string;
}

/** No illustrations, no mascots: what is missing and what fixes it (spec: UI). */
export function EmptyState({ missing, fix }: EmptyStateProps) {
  return (
    <div className="flex flex-col gap-1 text-14">
      <p className="text-text">{missing}</p>
      <p className="text-text-dim">
        <span className="font-medium">Fix:</span> {fix}
      </p>
    </div>
  );
}
