import { useEffect, useMemo, useRef, useState } from "react";

import { Dialog } from "./Dialog";

export interface PaletteCommand {
  id: string;
  /** What the person sees; matched case-insensitively against what they type. */
  label: string;
  /** `Go to` for screens, `Do` for actions; shown as a quiet group label. */
  group: "Go to" | "Do";
  hint?: string;
  run: () => void;
}

interface PaletteProps {
  commands: PaletteCommand[];
  onClose: () => void;
}

function matches(command: PaletteCommand, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  const hay = `${command.group} ${command.label} ${command.hint ?? ""}`.toLowerCase();
  return q.split(/\s+/).every((word) => hay.includes(word));
}

/** Ctrl+K: every screen and the main commands, filtered as you type, run with Enter. */
export function Palette({ commands, onClose }: PaletteProps) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const shown = useMemo(() => commands.filter((c) => matches(c, query)), [commands, query]);
  const current = shown[Math.min(active, Math.max(shown.length - 1, 0))];

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]');
    el?.scrollIntoView({ block: "nearest" });
  }, [active, shown]);

  const run = (command: PaletteCommand | undefined) => {
    if (!command) return;
    onClose();
    command.run();
  };

  return (
    <Dialog
      open
      title="Command palette"
      description="Type to filter; Enter runs the highlighted entry."
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
    >
      <input
        autoFocus
        aria-label="Command"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={current ? `palette-${current.id}` : undefined}
        className="h-9 w-full rounded-2 border border-line bg-bg-inset px-3 text-14"
        placeholder="Go to a screen or run a command…"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((i) => (shown.length === 0 ? 0 : (i + 1) % shown.length));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((i) => (shown.length === 0 ? 0 : (i - 1 + shown.length) % shown.length));
          } else if (e.key === "Enter") {
            e.preventDefault();
            run(current);
          }
        }}
      />
      <ul
        id="palette-list"
        ref={listRef}
        role="listbox"
        aria-label="Commands"
        className="flex max-h-80 flex-col overflow-auto text-14"
      >
        {shown.length === 0 ? (
          <li className="px-3 py-2 text-text-dim">Nothing matches "{query.trim()}".</li>
        ) : (
          shown.map((c) => {
            const selected = c.id === current?.id;
            return (
              <li
                key={c.id}
                id={`palette-${c.id}`}
                role="option"
                aria-selected={selected}
                className={`flex cursor-pointer items-center gap-3 rounded-2 px-3 py-1.5 ${
                  selected ? "bg-bg-inset text-text" : "text-text-dim"
                }`}
                onMouseEnter={() => {
                  setActive(shown.indexOf(c));
                }}
                onClick={() => {
                  run(c);
                }}
              >
                <span className="w-12 shrink-0 text-12 text-text-dim">{c.group}</span>
                <span className="flex-1">{c.label}</span>
                {c.hint ? <span className="money text-12 text-text-dim">{c.hint}</span> : null}
              </li>
            );
          })
        )}
      </ul>
    </Dialog>
  );
}
