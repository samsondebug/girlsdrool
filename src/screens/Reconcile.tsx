import { useMemo, useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { EmptyState } from "../components/EmptyState";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { TextField } from "../components/TextField";
import type { AccountTrust, LedgerRow, Reconciliation, StatementSource } from "../lib/ipc";
import { formatCents, parseCentsInput } from "../lib/money";
import {
  useDeleteReconciliation,
  useExplorer,
  useReconcile,
  useReconciliations,
  useTrust,
  useUpdateAccount,
} from "../lib/queries";
import { useUiStore } from "../lib/store";
import { TRUST_LABEL, TRUST_TONE } from "../lib/trust";

const CIVIL_DATE = /^\d{4}-\d{2}-\d{2}$/;

/**
 * Reconcile: one statement balance per account and period, roll-forward from the last balanced
 * period, zero tolerance, and the difference explorer for a period that is off.
 */
export function Reconcile() {
  const trust = useTrust();
  const draft = useUiStore((s) => s.reconcileDraft);
  const setDraft = useUiStore((s) => s.setReconcileDraft);
  const [chosenId, setChosenId] = useState<number | null>(draft?.accountId ?? null);
  const [periodId, setPeriodId] = useState<number | null>(null);

  const accounts = useMemo(() => trust.data?.accounts ?? [], [trust.data]);
  // the first cash account until one is picked
  const defaultId = (accounts.find((a) => a.contributes) ?? accounts[0])?.account_id ?? null;
  const selectedId = chosenId ?? defaultId;
  const selected = accounts.find((a) => a.account_id === selectedId) ?? null;
  const periods = useReconciliations(selectedId);
  const list = useMemo(() => periods.data ?? [], [periods.data]);
  const latest = list.at(-1) ?? null;
  const shown = list.find((p) => p.id === periodId) ?? latest;

  return (
    <div className="grid h-full grid-cols-[340px_1fr] gap-3 overflow-hidden p-4">
      <div className="flex min-h-0 flex-col gap-3 overflow-auto">
        <Panel title="Accounts" className="shrink-0">
          {trust.isPending ? <p className="text-12 text-text-dim">Loading…</p> : null}
          {trust.isError ? <p className="text-12 text-negative">{trust.error.message}</p> : null}
          {trust.data ? (
            <>
              <p
                className={`mb-2 text-14 ${trust.data.hero.trusted ? "text-positive" : "text-untrusted"}`}
              >
                {trust.data.hero.trusted
                  ? "Every cash account is reconciled; the hero can be trusted."
                  : `Hero untrusted: ${trust.data.hero.untrusted.map((u) => u.account_name).join(", ")}.`}
              </p>
              <ul className="flex flex-col gap-1">
                {accounts.map((a) => (
                  <li key={a.account_id}>
                    <button
                      type="button"
                      aria-current={a.account_id === selectedId ? "true" : undefined}
                      onClick={() => {
                        setChosenId(a.account_id);
                        setPeriodId(null);
                      }}
                      className={`flex w-full flex-col gap-1 rounded-2 border px-2 py-1 text-left text-14 ${
                        a.account_id === selectedId
                          ? "border-accent bg-bg-inset"
                          : "border-line hover:border-text-dim"
                      }`}
                    >
                      <span className="flex items-center gap-2">
                        <span className="truncate">{a.account_name}</span>
                        <Chip tone={TRUST_TONE[a.status]} title={a.reason}>
                          {TRUST_LABEL[a.status]}
                        </Chip>
                        {a.contributes ? null : (
                          <Chip title="not in the hero's cash set">outside hero</Chip>
                        )}
                      </span>
                      <span className="text-12 text-text-dim">{a.reason}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </>
          ) : null}
        </Panel>
      </div>

      <div className="flex min-h-0 flex-col gap-3 overflow-auto">
        {selected === null ? (
          <EmptyState
            missing="No account selected."
            fix="Add accounts (Accounts) and import their statements (Import); then pick one here."
          />
        ) : (
          <>
            <Panel
              title={`Periods — ${selected.account_name}`}
              className="shrink-0"
              aside={
                <StaleWindow
                  key={`${selected.account_id}-${selected.stale_after_days}`}
                  account={selected}
                />
              }
            >
              {periods.isError ? (
                <p className="text-12 text-negative">{periods.error.message}</p>
              ) : null}
              {list.length === 0 ? (
                <p className="text-14 text-text-dim">
                  No period yet. Enter the first statement balance below; the period starts at the
                  account's opening date.
                </p>
              ) : (
                <table className="w-full border-collapse text-14">
                  <thead className="text-12 text-text-dim">
                    <tr>
                      {[
                        "Period",
                        "Opening",
                        "Statement",
                        "Computed",
                        "Difference",
                        "Status",
                        "",
                      ].map((h) => (
                        <th
                          key={h}
                          className={`border-b border-line px-2 py-1 text-left font-medium ${
                            ["Opening", "Statement", "Computed", "Difference"].includes(h)
                              ? "text-right"
                              : ""
                          }`}
                        >
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {list.map((p) => (
                      <PeriodRow
                        key={p.id}
                        period={p}
                        selected={shown?.id === p.id}
                        deletable={latest?.id === p.id}
                        onSelect={() => {
                          setPeriodId(p.id);
                        }}
                      />
                    ))}
                  </tbody>
                </table>
              )}
            </Panel>

            <Panel title="Enter a statement balance" className="shrink-0">
              <StatementForm
                key={`${selected.account_id}-${latest?.id ?? 0}-${draft?.periodEnd ?? ""}`}
                account={selected}
                latest={latest}
                draft={
                  draft?.accountId === selected.account_id
                    ? { periodEnd: draft.periodEnd, cents: draft.statementClosingCents }
                    : null
                }
                onDone={() => {
                  setDraft(null);
                  setPeriodId(null);
                }}
              />
            </Panel>

            {shown ? <Explorer period={shown} /> : null}
          </>
        )}
      </div>
    </div>
  );
}

interface PeriodRowProps {
  period: Reconciliation;
  selected: boolean;
  deletable: boolean;
  onSelect: () => void;
}

function PeriodRow({ period: p, selected, deletable, onSelect }: PeriodRowProps) {
  const remove = useDeleteReconciliation();
  const reconcile = useReconcile();
  const pushNotice = useUiStore((s) => s.pushNotice);
  return (
    <tr className={`border-b border-line ${selected ? "bg-bg-inset" : ""}`}>
      <td className="px-2 py-1">
        <button type="button" className="money text-left hover:text-accent" onClick={onSelect}>
          {p.period_start} … {p.period_end}
        </button>
      </td>
      <td className="px-2 py-1 text-right">
        <Money cents={p.opening_cents} />
      </td>
      <td className="px-2 py-1 text-right">
        <Money cents={p.statement_closing_cents} />
        <span className="ml-1 text-12 text-text-dim">{p.statement_source}</span>
      </td>
      <td className="px-2 py-1 text-right">
        <Money cents={p.computed_closing_cents} />
      </td>
      <td className="px-2 py-1 text-right">
        <Money cents={p.difference_cents} sign="always" />
      </td>
      <td className="px-2 py-1">
        <Chip tone={p.status === "balanced" ? "positive" : "negative"}>{p.status}</Chip>
      </td>
      <td className="px-2 py-1 text-right">
        {deletable ? (
          <Button
            variant="quiet"
            disabled={remove.isPending}
            onClick={() => {
              remove.mutate(p.id, {
                onSuccess: () => {
                  pushNotice({
                    tone: "info",
                    text: `Removed the period ending ${p.period_end}.`,
                    undo: {
                      label: "Undo",
                      run: () => {
                        reconcile.mutate(
                          {
                            account_id: p.account_id,
                            period_end: p.period_end,
                            statement_closing_cents: p.statement_closing_cents,
                            statement_source: p.statement_source,
                          },
                          {
                            onError: (error) => {
                              pushNotice({ tone: "negative", text: error.message });
                            },
                          },
                        );
                      },
                    },
                  });
                },
                onError: (error) => {
                  pushNotice({ tone: "negative", text: error.message });
                },
              });
            }}
          >
            Delete
          </Button>
        ) : null}
      </td>
    </tr>
  );
}

interface StatementFormProps {
  account: AccountTrust;
  latest: Reconciliation | null;
  draft: { periodEnd: string; cents: number } | null;
  onDone: () => void;
}

/** Period end + statement closing. A draft from an import report arrives with source `file`. */
function StatementForm({ account, latest, draft, onDone }: StatementFormProps) {
  const reconcile = useReconcile();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [periodEnd, setPeriodEnd] = useState(draft?.periodEnd ?? "");
  const [closing, setClosing] = useState(draft ? formatCents(draft.cents, { symbol: false }) : "");
  const source: StatementSource =
    draft !== null && draft.periodEnd === periodEnd.trim() ? "file" : "user";
  const cents = parseCentsInput(closing);
  const validDate = CIVIL_DATE.test(periodEnd.trim());
  const error = reconcile.isError ? reconcile.error : null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!validDate || cents === null) return;
    reconcile.mutate(
      {
        account_id: account.account_id,
        period_end: periodEnd.trim(),
        statement_closing_cents: cents,
        statement_source: source,
      },
      {
        onSuccess: (r) => {
          pushNotice({
            tone: r.status === "balanced" ? "positive" : "warning",
            text:
              r.status === "balanced"
                ? `${account.account_name} balanced through ${r.period_end}: ${formatCents(r.computed_closing_cents)}.`
                : `${account.account_name} is off by ${formatCents(r.difference_cents, { sign: "always" })} for the period ending ${r.period_end}; the explorer lists what could explain it.`,
          });
          setPeriodEnd("");
          setClosing("");
          onDone();
        },
      },
    );
  };

  return (
    <form onSubmit={submit} className="flex items-end gap-3">
      <TextField
        label="Period end"
        mono
        className="w-40"
        value={periodEnd}
        onChange={(e) => {
          setPeriodEnd(e.target.value);
        }}
        placeholder="YYYY-MM-DD"
        hint={
          latest
            ? `next period starts ${latest.status === "balanced" ? "after " + latest.period_end : "after the last balanced period"}`
            : "the first period starts at the opening date"
        }
        error={error?.field === "period_end" ? error.message : null}
      />
      <TextField
        label="Statement closing balance"
        mono
        className="w-48"
        value={closing}
        onChange={(e) => {
          setClosing(e.target.value);
        }}
        placeholder="0.00"
        hint={source === "file" ? "from the file's running balance" : "from the statement"}
        error={closing.trim() !== "" && cents === null ? "not an amount" : null}
      />
      <Button
        type="submit"
        variant="primary"
        disabled={!validDate || cents === null || reconcile.isPending}
      >
        Reconcile
      </Button>
      {error !== null && error.field !== "period_end" ? (
        <span role="alert" className="text-12 text-negative">
          {error.message}
        </span>
      ) : null}
    </form>
  );
}

/** The per-account override of the stale window (the setting is the default). */
function StaleWindow({ account }: { account: AccountTrust }) {
  const update = useUpdateAccount();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [days, setDays] = useState(String(account.stale_after_days));
  const parsed = Number(days);
  const valid = /^\d{1,4}$/.test(days.trim()) && parsed >= 1;
  return (
    <form
      className="flex items-end gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid) return;
        update.mutate(
          { id: account.account_id, patch: { recon_stale_after_days: parsed } },
          {
            onSuccess: () => {
              pushNotice({
                tone: "info",
                text: `${account.account_name}: stale after ${parsed} days.`,
              });
            },
            onError: (error) => {
              pushNotice({ tone: "negative", text: error.message });
            },
          },
        );
      }}
    >
      <TextField
        label="Stale after (days)"
        mono
        className="w-32"
        value={days}
        onChange={(e) => {
          setDays(e.target.value);
        }}
        error={valid ? null : "1 or more days"}
      />
      <Button
        type="submit"
        variant="secondary"
        disabled={!valid || update.isPending || parsed === account.stale_after_days}
      >
        Save
      </Button>
    </form>
  );
}

function RowList({ rows, title }: { rows: LedgerRow[]; title: string }) {
  return (
    <div className="flex flex-col gap-1">
      <h4 className="text-12 font-medium text-text-dim">
        {title} · {rows.length}
      </h4>
      {rows.length === 0 ? (
        <p className="text-12 text-text-dim">none</p>
      ) : (
        <ul className="flex flex-col gap-1 text-14">
          {rows.map((r) => (
            <li key={r.id} className="flex items-center gap-2">
              <span className="money">{r.posted_date}</span>
              <span className="truncate" title={r.payee_raw}>
                {r.payee_norm}
              </span>
              {r.status === "pending" ? <Chip tone="info">pending</Chip> : null}
              <span className="ml-auto">
                <Money cents={r.amount_cents} />
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** What could explain a difference: the period's rows with a running balance, the neighbours,
 * pending rows, and rows still held in quarantine. Rows equal to ±difference are pointed out. */
function Explorer({ period }: { period: Reconciliation }) {
  const explorer = useExplorer(period.id);
  const d = period.difference_cents;
  const ex = explorer.data;
  return (
    <Panel title={`Difference explorer — period ending ${period.period_end}`} className="shrink-0">
      <p className="text-14">
        Opening <Money cents={period.opening_cents} /> + Σ posted rows ={" "}
        <Money cents={period.computed_closing_cents} /> vs statement{" "}
        <Money cents={period.statement_closing_cents} /> → difference{" "}
        <Money cents={d} sign="always" />{" "}
        {d === 0 ? (
          <Chip tone="positive">balanced</Chip>
        ) : (
          <Chip tone="negative">
            {d > 0
              ? "the ledger has more than the statement"
              : "the ledger has less than the statement"}
          </Chip>
        )}
      </p>
      {d !== 0 ? (
        <p className="mt-1 text-12 text-text-dim">
          A single wrong row shows as a row of {formatCents(d)} too many or {formatCents(-d)} too
          few; a missing row sits among the neighbours, pending or quarantined rows below.
        </p>
      ) : null}
      {explorer.isPending ? <p className="mt-2 text-12 text-text-dim">Loading…</p> : null}
      {explorer.isError ? (
        <p className="mt-2 text-12 text-negative">{explorer.error.message}</p>
      ) : null}
      {ex ? (
        <div className="mt-3 grid grid-cols-[1fr_320px] gap-4">
          <div className="flex flex-col gap-1">
            <h4 className="text-12 font-medium text-text-dim">
              Rows in the period · {ex.in_period.length}
            </h4>
            <table className="w-full border-collapse text-14">
              <tbody>
                {ex.in_period.map((r) => {
                  const suspect = d !== 0 && (r.amount_cents === d || r.amount_cents === -d);
                  return (
                    <tr key={r.id} className="border-b border-line">
                      <td className="money px-1 py-0.5">{r.posted_date}</td>
                      <td className="max-w-80 truncate px-1 py-0.5" title={r.payee_raw}>
                        {r.payee_norm}
                      </td>
                      <td className="px-1 py-0.5">
                        {suspect ? <Chip tone="warning">equals the difference</Chip> : null}
                        {r.status === "pending" ? <Chip tone="info">pending</Chip> : null}
                      </td>
                      <td className="px-1 py-0.5 text-right">
                        <Money cents={r.amount_cents} />
                      </td>
                      <td className="px-1 py-0.5 text-right">
                        <Money cents={r.running_cents} tone={false} />
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <div className="flex flex-col gap-3">
            <RowList rows={ex.before} title={`Posted within ${ex.neighbour_days} days before`} />
            <RowList rows={ex.after} title={`Posted within ${ex.neighbour_days} days after`} />
            <RowList rows={ex.pending} title="Pending on this account" />
            <div className="flex flex-col gap-1">
              <h4 className="text-12 font-medium text-text-dim">
                Held in quarantine · {ex.quarantine.length}
              </h4>
              {ex.quarantine.length === 0 ? (
                <p className="text-12 text-text-dim">none</p>
              ) : (
                <ul className="flex flex-col gap-1 text-14">
                  {ex.quarantine.map((q) => {
                    const row = JSON.parse(q.row_json) as {
                      posted_date: string;
                      payee_raw: string;
                      amount_cents: number;
                    };
                    return (
                      <li key={q.id} className="flex items-center gap-2">
                        <span className="money">{row.posted_date}</span>
                        <span className="truncate">{row.payee_raw}</span>
                        <span className="ml-auto">
                          <Money cents={row.amount_cents} />
                        </span>
                      </li>
                    );
                  })}
                </ul>
              )}
              {ex.quarantine.length > 0 ? (
                <p className="text-12 text-text-dim">
                  Resolve them in Import › Suspected duplicates.
                </p>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </Panel>
  );
}
