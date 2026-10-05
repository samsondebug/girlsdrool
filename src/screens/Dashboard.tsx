import { EmptyState } from "../components/EmptyState";
import { Panel } from "../components/Panel";

/**
 * The cockpit. Fits 1440×900 without page scroll; every panel is a fixed grid cell. Until the
 * engines behind each panel exist (M1–M7) the panel says what is missing and what will fill it.
 */
export function Dashboard() {
  return (
    <div className="grid h-full grid-cols-12 grid-rows-[minmax(0,1.4fr)_minmax(0,1fr)_minmax(0,1fr)] gap-3 p-4">
      <Panel title="Safe to spend" className="col-span-8">
        <div className="flex h-full flex-col justify-between gap-3">
          <p className="money text-28 text-text-dim" aria-label="Safe to spend not computed">
            —
          </p>
          <EmptyState
            missing="Not computed: there are no cash accounts and no reconciled balances."
            fix="Add accounts and import statements (Ledger, M1), then reconcile them (Reconcile, M3). The hero appears in M4 with its drill-down."
          />
        </div>
      </Panel>
      <Panel title="Next confirmed income" className="col-span-4">
        <EmptyState
          missing="No confirmed income stream."
          fix="Plan › Income streams (M4): add your base pay cycle and mark it confirmed."
        />
      </Panel>

      <Panel title="Next 14 days" className="col-span-4">
        <EmptyState
          missing="No confirmed obligations."
          fix="Plan › Obligations (M4): confirm detected bills or add them."
        />
      </Panel>
      <Panel title="Reconciliation health" className="col-span-4">
        <EmptyState
          missing="No account has been reconciled."
          fix="Reconcile (M3): enter a statement balance per account and period."
        />
      </Panel>
      <Panel title="Forecast" className="col-span-4">
        <EmptyState
          missing="No forecast: it needs balances, income and obligations."
          fix="Forecast (M5) draws 30 days and 13 weeks once the plan exists."
        />
      </Panel>

      <Panel title="Debt total" className="col-span-3">
        <EmptyState missing="No debts recorded." fix="Debts and loans (M6)." />
      </Panel>
      <Panel title="Informal loans" className="col-span-3">
        <EmptyState missing="No informal loans recorded." fix="Debts and loans (M6)." />
      </Panel>
      <Panel title="Venture cap" className="col-span-3">
        <EmptyState missing="No ventures recorded." fix="Ventures (M7)." />
      </Panel>
      <Panel title="Firewall" className="col-span-3">
        <EmptyState
          missing="No firewalled account."
          fix="Mark the brokerage firewalled when adding it (Ledger › Accounts, M1)."
        />
      </Panel>
    </div>
  );
}
