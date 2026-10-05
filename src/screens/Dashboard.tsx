import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { EmptyState } from "../components/EmptyState";
import { Panel } from "../components/Panel";
import { useAccounts, useTrust } from "../lib/queries";
import { useUiStore } from "../lib/store";
import { TRUST_LABEL, TRUST_TONE } from "../lib/trust";

/**
 * The cockpit. Fits 1440×900 without page scroll; every panel is a fixed grid cell. Until the
 * engines behind each panel exist (M1–M7) the panel says what is missing and what will fill it.
 */
export function Dashboard() {
  const trust = useTrust();
  const accounts = useAccounts();
  const firewalled = (accounts.data ?? []).filter((a) => a.firewalled && !a.archived);
  const setScreen = useUiStore((s) => s.setScreen);
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
        {trust.data && trust.data.accounts.length > 0 ? (
          <div className="flex h-full min-h-0 flex-col gap-2 text-14">
            <p className={trust.data.hero.trusted ? "text-positive" : "text-untrusted"}>
              {trust.data.hero.trusted
                ? "Every cash account is reconciled."
                : `Untrusted: ${trust.data.hero.untrusted.map((u) => u.account_name).join(", ")}.`}
            </p>
            <ul className="flex min-h-0 flex-col gap-1 overflow-auto">
              {trust.data.accounts.map((a) => (
                <li key={a.account_id} className="flex items-center gap-2" title={a.reason}>
                  <span className="truncate">{a.account_name}</span>
                  <Chip tone={TRUST_TONE[a.status]}>{TRUST_LABEL[a.status]}</Chip>
                  <span className="money ml-auto text-12 text-text-dim">
                    {a.latest_period_end ?? "—"}
                  </span>
                </li>
              ))}
            </ul>
            <Button
              variant="quiet"
              className="self-start"
              onClick={() => {
                setScreen("reconcile");
              }}
            >
              Reconcile…
            </Button>
          </div>
        ) : (
          <EmptyState
            missing="No account to reconcile."
            fix="Add accounts (Accounts), import statements (Import), then enter each statement's closing balance (Reconcile)."
          />
        )}
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
        {firewalled.length > 0 ? (
          <div className="flex flex-col gap-1 text-14">
            {firewalled.map((a) => (
              <p key={a.id} className="flex items-center gap-2">
                <span className="truncate">{a.name}</span>
                <Chip tone="info">firewalled</Chip>
              </p>
            ))}
            <p className="text-12 text-text-dim">
              Not available cash. An outflow stays in the review queue until it is acknowledged
              (policy firewall_exclusion).
            </p>
          </div>
        ) : (
          <EmptyState
            missing="No firewalled account."
            fix="Mark the brokerage firewalled when adding it (Accounts)."
          />
        )}
      </Panel>
    </div>
  );
}
