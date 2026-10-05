import { Component, useEffect, useMemo, useState, type ReactNode } from "react";

import { Button } from "../components/Button";
import { Palette, type PaletteCommand } from "../components/Palette";
import { Shortcuts } from "../components/Shortcuts";
import { api, type AppStatus } from "../lib/ipc";
import {
  useBackupNow,
  useExportAuditPack,
  useStartReview,
  useStatusMutation,
  useTakeSnapshot,
} from "../lib/queries";
import { useUiStore, type Screen } from "../lib/store";
import { Accounts } from "./Accounts";
import { Dashboard } from "./Dashboard";
import { Debts } from "./Debts";
import { Forecast } from "./Forecast";
import { Import } from "./Import";
import { Ledger } from "./Ledger";
import { Plan } from "./Plan";
import { Queue } from "./Queue";
import { Reconcile } from "./Reconcile";
import { Review } from "./Review";
import { Rules } from "./Rules";
import { Settings } from "./Settings";
import { Ventures } from "./Ventures";

interface ShellProps {
  status: AppStatus;
}

const navItems: { screen: Screen; label: string }[] = [
  { screen: "dashboard", label: "Dashboard" },
  { screen: "ledger", label: "Ledger" },
  { screen: "queue", label: "Queue" },
  { screen: "review", label: "Review" },
  { screen: "rules", label: "Rules" },
  { screen: "reconcile", label: "Reconcile" },
  { screen: "plan", label: "Plan" },
  { screen: "forecast", label: "Forecast" },
  { screen: "debts", label: "Debts" },
  { screen: "ventures", label: "Ventures" },
  { screen: "import", label: "Import" },
  { screen: "accounts", label: "Accounts" },
  { screen: "settings", label: "Settings" },
];

interface BoundaryProps {
  children: ReactNode;
  onReset: () => void;
}

interface BoundaryState {
  message: string | null;
}

/**
 * A render error on one screen must not blank the whole window: the boundary names the error
 * and offers the dashboard. The key on the boundary resets it when the screen changes.
 */
class ScreenBoundary extends Component<BoundaryProps, BoundaryState> {
  override state: BoundaryState = { message: null };

  static getDerivedStateFromError(error: unknown): BoundaryState {
    return { message: error instanceof Error ? error.message : String(error) };
  }

  override render(): ReactNode {
    if (this.state.message === null) return this.props.children;
    return (
      <div role="alert" className="flex flex-col gap-2 p-6 text-14">
        <p className="font-medium">This screen hit an error and stopped rendering.</p>
        <p className="text-text-dim">{this.state.message}</p>
        <p className="text-text-dim">
          Nothing was written: every write is one audited command, and this error happened while
          drawing the screen. Back to the dashboard, then try again.
        </p>
        <div>
          <Button variant="primary" onClick={this.props.onReset}>
            Back to dashboard
          </Button>
        </div>
      </div>
    );
  }
}

function ActiveScreen({ screen, status }: { screen: Screen; status: AppStatus }) {
  switch (screen) {
    case "dashboard":
      return <Dashboard />;
    case "ledger":
      return <Ledger />;
    case "queue":
      return <Queue />;
    case "review":
      return <Review />;
    case "rules":
      return <Rules />;
    case "reconcile":
      return <Reconcile />;
    case "plan":
      return <Plan />;
    case "forecast":
      return <Forecast />;
    case "debts":
      return <Debts />;
    case "ventures":
      return <Ventures />;
    case "import":
      return <Import />;
    case "accounts":
      return <Accounts />;
    case "settings":
      return <Settings status={status} />;
  }
}

/** True while the key event comes from a place where typing is expected. */
function typingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

/** The unlocked app: header, navigation, the active screen, and the keyboard layer (ADR-0047). */
export function Shell({ status }: ShellProps) {
  const screen = useUiStore((s) => s.screen);
  const setScreen = useUiStore((s) => s.setScreen);
  const pushNotice = useUiStore((s) => s.pushNotice);
  const lock = useStatusMutation(() => api.lock());
  const backupNow = useBackupNow();
  const startReview = useStartReview();
  const snapshot = useTakeSnapshot();
  const auditPack = useExportAuditPack();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  const commands = useMemo<PaletteCommand[]>(() => {
    const fail = (error: { message: string }) => {
      pushNotice({ tone: "negative", text: error.message });
    };
    const go: PaletteCommand[] = navItems.map((item) => ({
      id: `go-${item.screen}`,
      label: item.label,
      group: "Go to",
      run: () => {
        setScreen(item.screen);
      },
    }));
    const actions: PaletteCommand[] = [
      {
        id: "do-lock",
        label: "Lock Kept",
        group: "Do",
        hint: "Ctrl Shift L",
        run: () => {
          lock.mutate(undefined);
        },
      },
      {
        id: "do-backup",
        label: "Back up now",
        group: "Do",
        hint: "verified copy in backups/",
        run: () => {
          backupNow.mutate(undefined, {
            onSuccess: (entry) => {
              pushNotice({ tone: "positive", text: `Backup written and verified: ${entry.path}` });
            },
            onError: fail,
          });
        },
      },
      {
        id: "do-review",
        label: "Start the weekly review",
        group: "Do",
        run: () => {
          startReview.mutate(undefined, {
            onSuccess: () => {
              setScreen("review");
            },
            onError: (error) => {
              setScreen("review");
              fail(error);
            },
          });
        },
      },
      {
        id: "do-snapshot",
        label: "Take a snapshot now",
        group: "Do",
        hint: "a trend point",
        run: () => {
          snapshot.mutate(undefined, {
            onSuccess: () => {
              pushNotice({ tone: "positive", text: "Snapshot taken; Trends has a new point." });
            },
            onError: fail,
          });
        },
      },
      {
        id: "do-import",
        label: "Import a statement",
        group: "Do",
        run: () => {
          setScreen("import");
        },
      },
      {
        id: "do-audit-pack",
        label: "Write the audit pack",
        group: "Do",
        hint: "exports/ in the data folder",
        run: () => {
          auditPack.mutate(null, {
            onSuccess: (r) => {
              pushNotice({ tone: "positive", text: `${r.files.length} files written to ${r.dir}` });
            },
            onError: fail,
          });
        },
      },
      {
        id: "do-shortcuts",
        label: "Keyboard shortcuts",
        group: "Do",
        hint: "F1",
        run: () => {
          setShortcutsOpen(true);
        },
      },
    ];
    return [...go, ...actions];
  }, [auditPack, backupNow, lock, pushNotice, setScreen, snapshot, startReview]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const typing = typingTarget(event.target);
      if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setShortcutsOpen(false);
        setPaletteOpen((open) => !open);
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === "l") {
        event.preventDefault();
        lock.mutate(undefined);
        return;
      }
      if (event.key === "F1" || (event.key === "?" && !typing)) {
        event.preventDefault();
        setPaletteOpen(false);
        setShortcutsOpen((open) => !open);
        return;
      }
      if (event.key === "/" && !typing && !event.ctrlKey && !event.metaKey && !event.altKey) {
        const box = document.querySelector<HTMLInputElement>('input[data-shortcut="query"]');
        if (box) {
          event.preventDefault();
          box.focus();
          box.select();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [lock]);

  return (
    <div className="grid h-full grid-cols-[192px_1fr] grid-rows-[48px_1fr]">
      <header className="col-span-2 flex items-center gap-4 border-b border-line bg-bg-raised px-4">
        <span className="text-16 font-semibold tracking-tight">Kept</span>
        <span className="money truncate text-12 text-text-dim" title={status.data_dir ?? ""}>
          {status.data_dir}
        </span>
        <span className="ml-auto text-12 text-text-dim">v{status.version}</span>
        <Button
          variant="quiet"
          title="Command palette (Ctrl K)"
          onClick={() => {
            setPaletteOpen(true);
          }}
        >
          <span className="money text-12">Ctrl K</span>
        </Button>
        <Button
          variant="quiet"
          aria-label="Keyboard shortcuts (F1)"
          title="Keyboard shortcuts (F1)"
          onClick={() => {
            setShortcutsOpen(true);
          }}
        >
          ?
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            lock.mutate(undefined);
          }}
          disabled={lock.isPending}
        >
          Lock
        </Button>
      </header>
      <nav
        aria-label="Screens"
        className="flex flex-col gap-1 border-r border-line bg-bg-raised p-2"
      >
        {navItems.map((item) => {
          const current = item.screen === screen;
          return (
            <button
              key={item.screen}
              type="button"
              aria-current={current ? "page" : undefined}
              onClick={() => {
                setScreen(item.screen);
              }}
              className={`h-8 rounded-2 px-3 text-left text-14 ${
                current ? "bg-bg-inset text-text" : "text-text-dim hover:text-text"
              }`}
            >
              {item.label}
            </button>
          );
        })}
      </nav>
      <main className="min-h-0 overflow-hidden">
        <ScreenBoundary
          key={screen}
          onReset={() => {
            setScreen("dashboard");
          }}
        >
          <ActiveScreen screen={screen} status={status} />
        </ScreenBoundary>
      </main>
      {paletteOpen ? (
        <Palette
          commands={commands}
          onClose={() => {
            setPaletteOpen(false);
          }}
        />
      ) : null}
      {shortcutsOpen ? (
        <Shortcuts
          onClose={() => {
            setShortcutsOpen(false);
          }}
        />
      ) : null}
    </div>
  );
}
