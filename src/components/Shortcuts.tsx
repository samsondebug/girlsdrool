import { SHORTCUTS } from "../lib/shortcuts";
import { Dialog } from "./Dialog";

/** F1: every shortcut Kept answers to, by scope. */
export function Shortcuts({ onClose }: { onClose: () => void }) {
  return (
    <Dialog
      open
      title="Keyboard shortcuts"
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
    >
      <div className="flex flex-col gap-4">
        {SHORTCUTS.map((group) => (
          <section key={group.scope} aria-label={group.scope}>
            <h3 className="mb-1 text-12 font-medium tracking-wide text-text-dim uppercase">
              {group.scope}
            </h3>
            <table className="w-full border-collapse text-14">
              <tbody>
                {group.rows.map((r) => (
                  <tr key={r.keys} className="border-t border-line">
                    <td className="money w-40 py-1 pr-3 text-12">{r.keys}</td>
                    <td className="py-1">{r.does}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        ))}
      </div>
    </Dialog>
  );
}
