import { useState, type ReactNode } from "react";

import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { EmptyState } from "../components/EmptyState";
import { ForecastChart } from "../components/ForecastChart";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { MarkedMoney } from "../components/Untrusted";
import type { Forecast, SafeToSpend, UpcomingObligation } from "../lib/ipc";
import {
  useAccounts,
  useDebtTotals,
  useForecast,
  useSafeToSpend,
  useTrust,
  useUpcoming,
} from "../lib/queries";
import { BASELINE, useUiStore } from "../lib/store";
import { TRUST_LABEL, TRUST_TONE } from "../lib/trust";

/**
 * The cockpit. Fits 1440×900 without page scroll; every panel is a fixed grid cell. Panels whose
 * engine does not exist yet (M5–M7) say what is missing and what will fill them.
 */
export function Dashboard() {
  const safe = useSafeToSpend();
  const upcoming = useUpcoming(14);
  const trust = useTrust();
  const accounts = useAccounts();
  const forecast = useForecast(BASELINE);
  const totals = useDebtTotals();
  const firewalled = (accounts.data ?? []).filter((a) => a.firewalled && !a.archived);
  const setScreen = useUiStore((s) => s.setScreen);
  const nextIncome = safe.data?.terms.obligations.next_income ?? null;
  return (
    <div className="grid h-full grid-cols-12 grid-rows-[minmax(0,1.4fr)_minmax(0,1fr)_minmax(0,1fr)] gap-3 p-4">
      <Panel
        title="Safe to spend"
        className="col-span-8"
        aside={
          <Button
            variant="quiet"
            onClick={() => {
              setScreen("plan");
            }}
          >
            Plan…
          </Button>
        }
      >
        {safe.data && safe.data.terms.available.accounts.length > 0 ? (
          <Hero data={safe.data} />
        ) : (
          <div className="flex h-full flex-col justify-between gap-3">
            <p className="money text-28 text-text-dim" aria-label="Safe to spend not computed">
              —
            </p>
            <EmptyState
              missing="Not computed: no cash account counts towards available (firewalled and archived accounts never do)."
              fix="Add a checking, savings, cash or payment-app account (Accounts) and import its statements (Import); reconcile them (Reconcile) so the figure is trusted."
            />
          </div>
        )}
      </Panel>
      <Panel title="Next confirmed income" className="col-span-4">
        {nextIncome ? (
          <div className="flex h-full flex-col gap-1">
            <Money cents={nextIncome.expected_net_cents} size={28} sign="always" />
            <p className="text-14">
              <span className="money">{nextIncome.date}</span>
              <span className="text-text-dim">
                {" "}
                · {nextIncome.days_away === 0 ? "today" : `in ${String(nextIncome.days_away)} days`}
              </span>
            </p>
            <p className="truncate text-14 text-text-dim">{nextIncome.stream_name}</p>
            <p className="mt-auto text-12 text-text-dim">
              Net of deductions, as the stream expects it. Obligations due up to this date are
              subtracted from safe to spend.
            </p>
          </div>
        ) : (
          <EmptyState
            missing="No confirmed income stream."
            fix="Plan › Income streams: add your base pay cycle and mark it confirmed. Until then the obligation window is 30 days."
          />
        )}
      </Panel>

      <Panel title="Next 14 days" className="col-span-4">
        {upcoming.data && upcoming.data.obligations.length > 0 ? (
          <UpcomingList rows={upcoming.data.obligations} />
        ) : (
          <EmptyState
            missing="No unpaid confirmed obligation is overdue or due in the next 14 days."
            fix="Plan › Obligations: confirm detected bills or add them. Paid occurrences drop off as soon as the payment row is matched."
          />
        )}
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
      <Panel
        title="Forecast"
        className="col-span-4"
        aside={
          <Button
            variant="quiet"
            onClick={() => {
              setScreen("forecast");
            }}
          >
            Forecast…
          </Button>
        }
      >
        {forecast.data && safe.data && safe.data.terms.available.accounts.length > 0 ? (
          <ForecastSummary data={forecast.data} />
        ) : (
          <EmptyState
            missing="No forecast: it needs a cash account to start from."
            fix="Add accounts and import statements; confirm income and obligations under Plan so the 91 days have something to draw."
          />
        )}
      </Panel>

      <Panel
        title="Debt total"
        className="col-span-3"
        aside={
          <Button
            variant="quiet"
            onClick={() => {
              setScreen("debts");
            }}
          >
            Debts…
          </Button>
        }
      >
        {totals.data && totals.data.debts > 0 ? (
          <div className="flex flex-col gap-1 text-14">
            <Money cents={totals.data.total_debt_cents} size={28} tone={false} />
            <p className="text-12 text-text-dim">
              owed across {String(totals.data.debts)} debt{totals.data.debts === 1 ? "" : "s"}; each
              minimum is an obligation in the plan.
            </p>
          </div>
        ) : (
          <EmptyState
            missing="No debt recorded."
            fix="Debts: add the cards and loans you owe on, linked to their accounts or standalone."
          />
        )}
      </Panel>
      <Panel title="Informal loans" className="col-span-3">
        {totals.data &&
        (totals.data.open_informal > 0 || totals.data.informal_remaining_cents > 0) ? (
          <div className="flex flex-col gap-1 text-14">
            <Money cents={totals.data.informal_remaining_cents} size={28} tone={false} />
            <p className="text-12 text-text-dim">
              still owed on {String(totals.data.open_informal)} informal loan
              {totals.data.open_informal === 1 ? "" : "s"}; repayments are transfers, scheduled
              before any accelerated paydown.
            </p>
          </div>
        ) : (
          <EmptyState
            missing="No informal loan open."
            fix="Debts › Informal loans: record money borrowed from a person and the row it arrived on."
          />
        )}
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

type Term = "available" | "earmarks" | "obligations" | "buffer";

interface TermRow {
  term: Term;
  label: string;
  cents: number;
  subtract: boolean;
}

/**
 * The hero and its drill-down. The four rows are the four terms of §5.4 in formula order; the
 * total is the core's `safe_cents`, which the core computes from these same terms.
 */
function Hero({ data }: { data: SafeToSpend }) {
  const [open, setOpen] = useState<Term | null>(null);
  const untrustedBy = data.trust.hero.untrusted.map((u) => u.account_name);
  const { available, earmarks, obligations, buffer } = data.terms;
  const rows: TermRow[] = [
    {
      term: "available",
      label: `Available across ${String(available.accounts.length)} account${available.accounts.length === 1 ? "" : "s"}`,
      cents: available.cents,
      subtract: false,
    },
    {
      term: "earmarks",
      label: `Earmarks set aside (${String(earmarks.items.length)})`,
      cents: earmarks.cents,
      subtract: true,
    },
    {
      term: "obligations",
      label: `Obligations due by ${obligations.window_end} (${String(obligations.items.length)})`,
      cents: obligations.cents,
      subtract: true,
    },
    { term: "buffer", label: "Timing buffer", cents: buffer.cents, subtract: true },
  ];
  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <div className="flex flex-wrap items-baseline gap-3">
        <MarkedMoney cents={data.safe_cents} size={28} untrustedBy={untrustedBy} />
        <span className="money text-12 text-text-dim">as of {data.as_of}</span>
        {obligations.window_reason === "no_confirmed_income" ? (
          <Chip
            tone="warning"
            title="No confirmed income stream: obligations are counted 30 days out"
          >
            30-day window: no confirmed income
          </Chip>
        ) : null}
      </div>
      <table className="w-full text-14">
        <tbody>
          {rows.map((row) => (
            <tr key={row.term} className="border-t border-line">
              <td className="py-1">
                <button
                  type="button"
                  aria-expanded={open === row.term}
                  className="inline-flex items-center gap-2 text-left hover:text-text"
                  onClick={() => {
                    setOpen(open === row.term ? null : row.term);
                  }}
                >
                  <span className="money w-3 text-text-dim">{row.subtract ? "−" : ""}</span>
                  <span className={open === row.term ? "text-text" : "text-text-dim"}>
                    {row.label}
                  </span>
                </button>
              </td>
              <td className="py-1 text-right">
                <Money cents={row.cents} tone={false} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {open ? (
        <div className="min-h-0 flex-1 overflow-auto rounded-1 border border-line bg-bg p-2 text-12">
          {TERM_DETAIL[open](data)}
        </div>
      ) : (
        <p className="text-12 text-text-dim">Click a term to see what is behind it.</p>
      )}
    </div>
  );
}

function DetailRow({ left, right, note }: { left: ReactNode; right: ReactNode; note?: string }) {
  return (
    <li className="flex items-center gap-2 py-0.5" title={note}>
      <span className="min-w-0 flex-1 truncate">{left}</span>
      <span className="money text-right">{right}</span>
    </li>
  );
}

function Heading({ children }: { children: ReactNode }) {
  return <p className="mt-1 font-medium text-text-dim first:mt-0">{children}</p>;
}

const TERM_DETAIL: Record<Term, (data: SafeToSpend) => ReactNode> = {
  available: (data) => {
    const { accounts } = data.terms.available;
    const {
      firewalled_accounts,
      venture_accounts,
      pending_flagged_inflows,
      posted_flagged_inflows,
    } = data.excluded;
    const untrusted = new Set(data.trust.hero.untrusted.map((u) => u.account_id));
    return (
      <div className="flex flex-col gap-1">
        <Heading>Counted: posted balance + pending inflows − pending outflows</Heading>
        <ul>
          {accounts.map((a) => (
            <DetailRow
              key={a.account_id}
              left={
                <span className="inline-flex items-center gap-2">
                  {a.account_name}
                  {untrusted.has(a.account_id) ? <Chip tone="untrusted">unreconciled</Chip> : null}
                  {a.pending_in_cents !== 0 || a.pending_out_cents !== 0 ? (
                    <span className="text-text-dim">
                      pending +{formatPlain(a.pending_in_cents)} / −
                      {formatPlain(a.pending_out_cents)}
                    </span>
                  ) : null}
                </span>
              }
              right={
                <Money
                  cents={a.posted_cents + a.pending_in_cents - a.pending_out_cents}
                  tone={false}
                />
              }
            />
          ))}
        </ul>
        {firewalled_accounts.length + venture_accounts.length > 0 ? (
          <>
            <Heading>Not counted</Heading>
            <ul>
              {[...firewalled_accounts, ...venture_accounts].map((a) => (
                <DetailRow
                  key={a.account_id}
                  note={a.reason}
                  left={
                    <span className="inline-flex items-center gap-2">
                      {a.account_name}
                      <Chip tone="info">{a.reason.split(":")[0]}</Chip>
                    </span>
                  }
                  right={<Money cents={a.posted_cents} tone={false} />}
                />
              ))}
            </ul>
          </>
        ) : null}
        {pending_flagged_inflows.length > 0 ? (
          <>
            <Heading>
              Pending inflows left out: borrowing or securities sales are not cash until posted
            </Heading>
            <ul>
              {pending_flagged_inflows.map((f) => (
                <DetailRow
                  key={f.txn_id}
                  left={`${f.posted_date} ${f.account_name} · ${f.flags.join(", ")}`}
                  right={<Money cents={f.cents} tone={false} />}
                />
              ))}
            </ul>
          </>
        ) : null}
        {posted_flagged_inflows.length > 0 ? (
          <>
            <Heading>
              Posted and inside the balance, flagged: borrowing or securities sales are never income
            </Heading>
            <ul>
              {posted_flagged_inflows.map((f) => (
                <DetailRow
                  key={f.txn_id}
                  left={`${f.posted_date} ${f.account_name} · ${f.flags.join(", ")}`}
                  right={<Money cents={f.cents} tone={false} />}
                />
              ))}
            </ul>
          </>
        ) : null}
      </div>
    );
  },
  earmarks: (data) => {
    const { items } = data.terms.earmarks;
    return (
      <div className="flex flex-col gap-1">
        <Heading>
          Money already in the counted accounts that is spoken for: each earmark's entries dated up
          to today
        </Heading>
        {items.length === 0 ? (
          <p>No earmark is funded from a counted account.</p>
        ) : (
          <ul>
            {items.map((e) => (
              <DetailRow
                key={e.earmark_id}
                left={
                  <span className="inline-flex items-center gap-2">
                    {e.name}
                    <Chip>{e.kind.replace("_", " ")}</Chip>
                    {e.remaining_cents < 0 ? (
                      <Chip tone="warning" title="Released more than funded; counts as zero">
                        over-released
                      </Chip>
                    ) : null}
                  </span>
                }
                right={<Money cents={e.counted_cents} tone={false} />}
              />
            ))}
          </ul>
        )}
      </div>
    );
  },
  obligations: (data) => {
    const { items, next_income, window_end } = data.terms.obligations;
    return (
      <div className="flex flex-col gap-1">
        <Heading>
          Unpaid confirmed occurrences due by {window_end}
          {next_income
            ? ` (next confirmed income: ${next_income.stream_name} on ${next_income.date})`
            : " (no confirmed income: 30-day window)"}
          , each reduced by what its earmark already holds
        </Heading>
        {items.length === 0 ? (
          <p>Nothing unpaid is due in the window.</p>
        ) : (
          <ul>
            {items.map((o) => (
              <DetailRow
                key={`${String(o.obligation_id)}-${o.due_date}`}
                left={
                  <span className="inline-flex items-center gap-2">
                    <span className="money text-text-dim">{o.due_date}</span>
                    {o.name}
                    {o.overdue ? <Chip tone="negative">overdue</Chip> : null}
                    {o.earmark_covered_cents > 0 ? (
                      <span className="text-text-dim">
                        expected {formatPlain(o.expected_cents)}, earmark covers{" "}
                        {formatPlain(o.earmark_covered_cents)}
                      </span>
                    ) : null}
                  </span>
                }
                right={<Money cents={o.counted_cents} tone={false} />}
              />
            ))}
          </ul>
        )}
      </div>
    );
  },
  buffer: (data) => (
    <div className="flex flex-col gap-1">
      <Heading>
        Minimum buffer against timing: deposits that post late, payments that post early
      </Heading>
      <p>
        <Money cents={data.terms.buffer.cents} tone={false} /> is the setting{" "}
        <span className="money">timing_buffer_cents</span>; change it under Plan › Reserves. The
        emergency reserve is separate and enters through the earmark term.
      </p>
    </div>
  ),
};

/** The baseline's 91 days as a sparkline, its lowest point, and the first shortfall or breach. */
function ForecastSummary({ data }: { data: Forecast }) {
  const untrustedBy = data.trust.hero.untrusted.map((u) => u.account_name);
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 text-14">
      <div className="h-14 w-full">
        <ForecastChart days={data.days} lowest={data.lowest} width={360} height={56} compact />
      </div>
      <p className="flex items-baseline gap-2">
        <span className="text-text-dim">Lowest</span>
        <MarkedMoney cents={data.lowest.cents} untrustedBy={untrustedBy} />
        <span className="money text-12 text-text-dim">on {data.lowest.date}</span>
      </p>
      {data.first_shortfall ? (
        <p className="flex items-center gap-2">
          <Chip tone="negative">shortfall</Chip>
          <span className="money text-12">{data.first_shortfall.date}</span>
          <Money cents={data.first_shortfall.cents} size={14} />
        </p>
      ) : data.first_buffer_breach ? (
        <p className="flex items-center gap-2">
          <Chip tone="warning">buffer breach</Chip>
          <span className="money text-12">{data.first_buffer_breach.date}</span>
          <Money cents={data.first_buffer_breach.cents} size={14} />
        </p>
      ) : (
        <p className="text-12 text-text-dim">
          No shortfall and no buffer breach in the next {String(data.horizon_days)} days.
        </p>
      )}
    </div>
  );
}

function UpcomingList({ rows }: { rows: UpcomingObligation[] }) {
  const total = rows.reduce((sum, r) => sum + r.expected_cents, 0);
  return (
    <div className="flex h-full min-h-0 flex-col gap-1 text-14">
      <ul className="flex min-h-0 flex-1 flex-col gap-1 overflow-auto">
        {rows.map((r) => (
          <li
            key={`${String(r.obligation_id)}-${r.due_date}`}
            className="flex items-center gap-2"
            title={`${r.source_account_name}${r.variability_cents > 0 ? ` · usually ± ${formatPlain(r.variability_cents)}` : ""}`}
          >
            <span className="money text-12 text-text-dim">{r.due_date}</span>
            <span className="truncate">{r.name}</span>
            {r.overdue ? <Chip tone="negative">overdue</Chip> : null}
            {r.autopay ? <Chip>autopay</Chip> : null}
            {r.earmark_covered_cents > 0 ? <Chip tone="positive">earmarked</Chip> : null}
            <span className="ml-auto">
              <Money cents={r.expected_cents} tone={false} />
            </span>
          </li>
        ))}
      </ul>
      <p className="flex items-center justify-between border-t border-line pt-1 text-12 text-text-dim">
        <span>
          Σ expected, {String(rows.length)} occurrence{rows.length === 1 ? "" : "s"}
        </span>
        <Money cents={total} tone={false} />
      </p>
    </div>
  );
}

/** Unsigned figure for inline prose, where the sign is already in the words around it. */
function formatPlain(cents: number): string {
  return (Math.abs(cents) / 100).toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}
