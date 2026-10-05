import { useUiStore, type Notice } from "../lib/store";
import { Button } from "./Button";

const toneClass: Record<Notice["tone"], string> = {
  info: "border-l-info",
  warning: "border-l-warning",
  negative: "border-l-negative",
  positive: "border-l-positive",
};

/** Bottom-right stack of transient notices. Each names what happened and its undo, if any. */
export function Notices() {
  const notices = useUiStore((s) => s.notices);
  const dismiss = useUiStore((s) => s.dismissNotice);
  if (notices.length === 0) return null;
  return (
    <div
      className="fixed right-4 bottom-4 flex w-96 flex-col gap-2"
      role="status"
      aria-live="polite"
    >
      {notices.map((n) => (
        <div
          key={n.id}
          className={`flex items-start gap-3 rounded-2 border border-line border-l-4 bg-bg-raised p-3 text-14 ${toneClass[n.tone]}`}
        >
          <p className="flex-1 break-words">{n.text}</p>
          {n.undo ? (
            <Button
              variant="secondary"
              onClick={() => {
                n.undo?.run();
                dismiss(n.id);
              }}
            >
              {n.undo.label}
            </Button>
          ) : null}
          <Button
            variant="quiet"
            aria-label="Dismiss notice"
            onClick={() => {
              dismiss(n.id);
            }}
          >
            ×
          </Button>
        </div>
      ))}
    </div>
  );
}
