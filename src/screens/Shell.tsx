import { Button } from "../components/Button";
import { api, type AppStatus } from "../lib/ipc";
import { useStatusMutation } from "../lib/queries";
import { useUiStore, type Screen } from "../lib/store";
import { Accounts } from "./Accounts";
import { Dashboard } from "./Dashboard";
import { Import } from "./Import";
import { Ledger } from "./Ledger";
import { Reconcile } from "./Reconcile";
import { Review } from "./Review";
import { Rules } from "./Rules";
import { Settings } from "./Settings";

interface ShellProps {
  status: AppStatus;
}

const navItems: { screen: Screen; label: string }[] = [
  { screen: "dashboard", label: "Dashboard" },
  { screen: "ledger", label: "Ledger" },
  { screen: "review", label: "Review" },
  { screen: "rules", label: "Rules" },
  { screen: "reconcile", label: "Reconcile" },
  { screen: "import", label: "Import" },
  { screen: "accounts", label: "Accounts" },
  { screen: "settings", label: "Settings" },
];

function ActiveScreen({ screen, status }: { screen: Screen; status: AppStatus }) {
  switch (screen) {
    case "dashboard":
      return <Dashboard />;
    case "ledger":
      return <Ledger />;
    case "review":
      return <Review />;
    case "rules":
      return <Rules />;
    case "reconcile":
      return <Reconcile />;
    case "import":
      return <Import />;
    case "accounts":
      return <Accounts />;
    case "settings":
      return <Settings status={status} />;
  }
}

/** The unlocked app: header, navigation, and the active screen. Screens own their scrolling. */
export function Shell({ status }: ShellProps) {
  const screen = useUiStore((s) => s.screen);
  const setScreen = useUiStore((s) => s.setScreen);
  const lock = useStatusMutation(() => api.lock());

  return (
    <div className="grid h-full grid-cols-[192px_1fr] grid-rows-[48px_1fr]">
      <header className="col-span-2 flex items-center gap-4 border-b border-line bg-bg-raised px-4">
        <span className="text-16 font-semibold tracking-tight">Kept</span>
        <span className="money truncate text-12 text-text-dim" title={status.data_dir ?? ""}>
          {status.data_dir}
        </span>
        <span className="ml-auto text-12 text-text-dim">v{status.version}</span>
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
        <ActiveScreen screen={screen} status={status} />
      </main>
    </div>
  );
}
