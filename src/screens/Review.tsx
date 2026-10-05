import { useState, type ReactNode } from "react";

import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Chip } from "../components/Chip";
import { EmptyState } from "../components/EmptyState";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { TextField } from "../components/TextField";
import type { Review as ReviewData, ReviewRowRef, Surplus, TrendPoint } from "../lib/ipc";
import { formatBps, formatCents } from "../lib/money";
import {
  useAbandonReview,
  useCompleteReview,
  useCurrentReview,
  useRefreshReview,
  useReviews,
  useSetReviewActionDone,
  useSetReviewActions,
  useStartReview,
  useTakeSnapshot,
  useTrends,
} from "../lib/queries";
import { useUiStore } from "../lib/store";
import { TRUST_LABEL, TRUST_TONE } from "../lib/trust";

/**
 * The weekly review: a mode, not a notification. It walks the steps, states the dependable
 * surplus with its terms, and completes only with exactly three actions. History keeps what each
 * review showed; trends read snapshots only.
 */
export function Review() {
  const current = useCurrentReview();
  const history = useReviews();
  const start = useStartReview();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  const open = current.data ?? null;
  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-4">
      {open ? (
        <Walk review={open} />
      ) : (
        <Panel
          title="Weekly review"
          className="shrink-0"
          aside={
            <Button
              variant="primary"
              disabled={start.isPending || current.isPending}
              onClick={() => {
                start.mutate(undefined, { onError: fail });
              }}
            >
              Start review
            </Button>
          }
        >
          <EmptyState
            missing="No review in progress."
            fix="Start one: it walks balances, unreviewed rows, the next 14 days, plan variance, debts, ventures and flags, states the dependable surplus, and ends with exactly three actions."
          />
        </Panel>
      )}
      <History reviews={(history.data ?? []).filter((r) => r.status !== "in_progress")} />
      <Trends />
    </div>
  );
}

// ---- the walk ---------------------------------------------------------------------------------

function Walk({ review: r }: { review: ReviewData }) {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const refresh = useRefreshReview();
  const saveActions = useSetReviewActions();
  const complete = useCompleteReview();
  const abandon = useAbandonReview();
  const [actions, setActions] = useState<[string, string, string]>([
    r.actions[0]?.text ?? "",
    r.actions[1]?.text ?? "",
    r.actions[2]?.text ?? "",
  ]);
  const [notes, setNotes] = useState(r.notes);
  const filled = actions.filter((a) => a.trim() !== "").length;
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  const s = r.steps;
  const untrusted = s.balances.accounts.filter((a) => a.trust !== "reconciled").map((a) => a.name);
  return (
    <>
      <Panel
        title={`Review in progress · ${r.period_start} .. ${r.period_end}`}
        className="shrink-0"
        aside={
          <div className="flex gap-1">
            <Button
              variant="quiet"
              disabled={refresh.isPending}
              onClick={() => {
                refresh.mutate(r.id, { onError: fail });
              }}
            >
              Recompute
            </Button>
            <Button
              variant="quiet"
              disabled={abandon.isPending}
              onClick={() => {
                abandon.mutate(r.id, {
                  onSuccess: () => {
                    pushNotice({ tone: "info", text: "Review abandoned; nothing was committed." });
                  },
                  onError: fail,
                });
              }}
            >
              Abandon
            </Button>
          </div>
        }
      >
        <div className="grid grid-cols-2 gap-3 text-14">
          <Step n={1} title="Balances">
            <p className="mb-1 flex items-baseline gap-2">
              <span className="text-text-dim">Available</span>
              <Money cents={s.balances.available_cents} size={16} untrusted={!s.balances.trusted} />
              {s.balances.trusted ? (
                <Chip tone="positive">every cash account reconciled</Chip>
              ) : (
                <Chip tone="untrusted">unreconciled: {untrusted.join(", ")}</Chip>
              )}
            </p>
            <ul className="flex flex-col gap-0.5">
              {s.balances.accounts.map((a) => (
                <li key={a.account_id} className="flex items-center gap-2">
                  <span className="truncate">{a.name}</span>
                  <Chip tone={TRUST_TONE[a.trust]}>{TRUST_LABEL[a.trust]}</Chip>
                  <span className="ml-auto">
                    <Money cents={a.balance_cents} />
                  </span>
                </li>
              ))}
            </ul>
          </Step>
          <Step n={2} title={`Unreviewed rows: ${String(s.unreviewed.count)}`}>
            <p className="mb-1 text-text-dim">
              Σ|amount| {formatCents(s.unreviewed.total_abs_cents)}, largest first. Classify them
              under Queue; the review never guesses.
            </p>
            <RowList rows={s.unreviewed.rows} />
          </Step>
          <Step n={3} title={`Obligations in the next 14 days: ${String(s.obligations_14.count)}`}>
            <p className="mb-1 text-text-dim">
              Σ expected {formatCents(s.obligations_14.expected_cents)}.
            </p>
            <ul className="flex flex-col gap-0.5">
              {s.obligations_14.items.map((i) => (
                <li
                  key={`${String(i.obligation_id)}-${i.due_date}`}
                  className="flex items-center gap-2"
                >
                  <span className="money text-12 text-text-dim">{i.due_date}</span>
                  <span className="truncate">{i.name}</span>
                  {i.overdue ? <Chip tone="negative">overdue</Chip> : null}
                  <span className="ml-auto">
                    <Money cents={i.expected_cents} tone={false} />
                  </span>
                </li>
              ))}
            </ul>
          </Step>
          <Step n={4} title="Plan variance">
            {s.plan_variance.variance_cents === null ? (
              <p className="text-text-dim">
                {s.plan_variance.plan_snapshot_id === null
                  ? "No plan snapshot yet; completing this review stores one."
                  : `The plan saved ${s.plan_variance.plan_date ?? ""} has no closing for today.`}
              </p>
            ) : (
              <p className="flex items-baseline gap-2">
                <span className="text-text-dim">Actual</span>
                <Money cents={s.plan_variance.actual_cents} />
                <span className="text-text-dim">vs plan</span>
                <Money cents={s.plan_variance.plan_cents ?? 0} />
                <span className="text-text-dim">=</span>
                <Money cents={s.plan_variance.variance_cents} sign="always" />
              </p>
            )}
          </Step>
          <Step n={5} title="Debt and informal-loan progress">
            <p className="flex items-baseline gap-2">
              <span className="text-text-dim">Total debt</span>
              <Money cents={s.debts.total_debt_cents} tone={false} />
              <Delta now={s.debts.total_debt_cents} before={s.debts.previous_total_debt_cents} />
            </p>
            <p className="flex items-baseline gap-2">
              <span className="text-text-dim">Informal remaining</span>
              <Money cents={s.debts.informal_remaining_cents} tone={false} />
              <Delta
                now={s.debts.informal_remaining_cents}
                before={s.debts.previous_informal_cents}
              />
            </p>
            {s.debts.previous_review_id === null ? (
              <p className="text-12 text-text-dim">No earlier review to compare against.</p>
            ) : null}
          </Step>
          <Step n={6} title="Venture cap">
            {s.ventures.ventures.length === 0 ? (
              <p className="text-text-dim">No venture.</p>
            ) : (
              <ul className="flex flex-col gap-0.5">
                {s.ventures.ventures.map((v) => (
                  <li key={v.venture_id} className="flex items-center gap-2">
                    <span className="truncate">{v.name}</span>
                    <Chip>{v.status}</Chip>
                    {v.alerts.map((a) => (
                      <Chip key={a} tone="negative">
                        {a}
                      </Chip>
                    ))}
                    <span className="money ml-auto text-12">
                      {formatCents(v.cap_used_cents)} / {formatCents(v.cap_cents)} ·{" "}
                      {formatBps(v.utilization_bps)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Step>
          <Step n={7} title={`Flags since ${s.flags.since}`}>
            <FlagList label="Borrowing" rows={s.flags.borrowing} tone="warning" />
            <FlagList label="Securities sale" rows={s.flags.securities_sale} tone="warning" />
            <FlagList
              label="Firewall touches awaiting acknowledgment"
              rows={s.flags.firewall_unacknowledged}
              tone="negative"
            />
            <FlagList label="Acknowledged" rows={s.flags.firewall_acknowledged} tone="dim" />
          </Step>
          {r.surplus ? <SurplusPanel surplus={r.surplus} /> : null}
        </div>
      </Panel>

      <Panel title="Exactly three actions" className="shrink-0">
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            complete.mutate(
              { id: r.id, actions: [...actions], notes },
              {
                onSuccess: (done) => {
                  pushNotice({
                    tone: "positive",
                    text: `Review committed with its three actions; surplus ${formatCents(done.surplus_cents ?? 0)} and a plan snapshot stored.`,
                  });
                },
                onError: fail,
              },
            );
          }}
        >
          <div className="grid grid-cols-3 gap-3">
            {actions.map((a, i) => (
              <TextField
                key={i}
                label={`Action ${String(i + 1)}`}
                value={a}
                onChange={(e) => {
                  const next: [string, string, string] = [...actions];
                  next[i] = e.target.value;
                  setActions(next);
                }}
              />
            ))}
          </div>
          <TextField
            label="Notes (optional)"
            value={notes}
            onChange={(e) => {
              setNotes(e.target.value);
            }}
          />
          <div className="flex items-center gap-2">
            <Button
              variant="quiet"
              disabled={saveActions.isPending}
              onClick={() => {
                saveActions.mutate(
                  { id: r.id, actions: [...actions] },
                  {
                    onSuccess: () => {
                      pushNotice({ tone: "info", text: "Draft actions saved." });
                    },
                    onError: fail,
                  },
                );
              }}
            >
              Save draft
            </Button>
            <span className="text-12 text-text-dim">
              {String(filled)} of 3 written; the review commits only with exactly three.
            </span>
            <Button
              type="submit"
              variant="primary"
              className="ml-auto"
              disabled={complete.isPending || filled !== 3}
            >
              Commit review
            </Button>
          </div>
        </form>
      </Panel>
    </>
  );
}

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-1 rounded-2 border border-line p-2">
      <h3 className="text-12 font-medium text-text-dim">
        {String(n)}. {title}
      </h3>
      {children}
    </section>
  );
}

function Delta({ now, before }: { now: number; before: number | null }) {
  if (before === null) return null;
  const d = now - before;
  return (
    <span className="text-12 text-text-dim">
      ({d === 0 ? "unchanged" : `${formatCents(d, { sign: "always" })} since last review`})
    </span>
  );
}

function RowList({ rows }: { rows: ReviewRowRef[] }) {
  if (rows.length === 0) return <p className="text-text-dim">None.</p>;
  return (
    <ul className="flex max-h-40 flex-col gap-0.5 overflow-auto">
      {rows.map((x) => (
        <li key={x.txn_id} className="flex items-center gap-2 text-14" title={x.why}>
          <span className="money text-12 text-text-dim">{x.posted_date}</span>
          <span className="truncate">{x.account_name}</span>
          <span className="truncate text-text-dim">{x.payee}</span>
          <span className="ml-auto">
            <Money cents={x.amount_cents} />
          </span>
        </li>
      ))}
    </ul>
  );
}

function FlagList({
  label,
  rows,
  tone,
}: {
  label: string;
  rows: ReviewRowRef[];
  tone: "warning" | "negative" | "dim";
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <p className="flex items-center gap-2">
        <Chip tone={tone}>{label}</Chip>
        <span className="text-12 text-text-dim">{String(rows.length)}</span>
      </p>
      {rows.length > 0 ? <RowList rows={rows} /> : null}
    </div>
  );
}

function SurplusPanel({ surplus: s }: { surplus: Surplus }) {
  const rows: {
    label: string;
    cents: number;
    items?: { name: string; monthly_cents: number }[];
    subtract: boolean;
  }[] = [
    {
      label: `Income: ${String(s.income_receipts)} confirmed receipts in ${String(s.income_window_days)} days (${formatCents(s.income_window_cents)}) × 30/${String(s.income_window_days)}`,
      cents: s.income_cents,
      subtract: false,
    },
    {
      label: "Fixed obligations, monthly equivalent",
      cents: s.fixed_cents,
      items: s.fixed_items,
      subtract: true,
    },
    {
      label: `Debt service: minimums + informal schedule 12 months ÷ 12 (${formatCents(s.informal_schedule_12m_cents)})`,
      cents: s.debt_service_cents,
      items: s.debt_items,
      subtract: true,
    },
    {
      label: "Irregular: annuals ÷ 12 + sinking funds",
      cents: s.irregular_cents,
      items: s.irregular_items,
      subtract: true,
    },
    { label: "Variable spend model", cents: s.variable_cents, subtract: true },
  ];
  return (
    <section className="col-span-2 flex flex-col gap-1 rounded-2 border border-accent p-2">
      <h3 className="flex items-baseline gap-2 text-12 font-medium text-text-dim">
        Dependable {s.surplus_cents >= 0 ? "surplus" : "deficit"}, monthly equivalent
        <Money cents={s.surplus_cents} size={20} />
        <span>— from rows only; borrowing and asset sales cannot enter</span>
      </h3>
      <table className="w-full text-14">
        <tbody>
          {rows.map((row) => (
            <tr key={row.label} className="border-t border-line align-top">
              <td className="py-1 pr-2">
                <span className="money w-3 text-text-dim">{row.subtract ? "−" : ""}</span>{" "}
                {row.label}
                {row.items && row.items.length > 0 ? (
                  <span className="block text-12 text-text-dim">
                    {row.items.map((i) => `${i.name} ${formatCents(i.monthly_cents)}`).join(" · ")}
                  </span>
                ) : null}
              </td>
              <td className="py-1 text-right">
                <Money cents={row.cents} tone={false} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

// ---- history and trends ----------------------------------------------------------------------

function History({ reviews }: { reviews: ReviewData[] }) {
  const tick = useSetReviewActionDone();
  const pushNotice = useUiStore((s) => s.pushNotice);
  return (
    <Panel title="History" className="shrink-0">
      {reviews.length === 0 ? (
        <EmptyState
          missing="No review yet."
          fix="Complete one; every review and its actions stay here."
        />
      ) : (
        <ul className="flex flex-col gap-2 text-14">
          {reviews.map((r) => (
            <li key={r.id} className="flex flex-col gap-1 rounded-2 border border-line p-2">
              <p className="flex items-center gap-2">
                <span className="money">{r.period_start}</span>
                <span className="text-text-dim">..</span>
                <span className="money">{r.period_end}</span>
                <Chip tone={r.status === "completed" ? "positive" : "dim"}>{r.status}</Chip>
                {r.surplus_cents !== null ? (
                  <span className="flex items-baseline gap-1">
                    <span className="text-text-dim">surplus</span>
                    <Money cents={r.surplus_cents} />
                  </span>
                ) : null}
                {r.notes ? <span className="text-12 text-text-dim">{r.notes}</span> : null}
              </p>
              {r.actions.length > 0 ? (
                <ul className="flex flex-col gap-0.5">
                  {r.actions.map((a) => (
                    <li key={a.id}>
                      <Checkbox
                        label={`${String(a.position)}. ${a.text}`}
                        checked={a.done}
                        onChange={(e) => {
                          tick.mutate(
                            { actionId: a.id, done: e.target.checked },
                            {
                              onError: (error) => {
                                pushNotice({ tone: "negative", text: error.message });
                              },
                            },
                          );
                        }}
                      />
                    </li>
                  ))}
                </ul>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

function Trends() {
  const trends = useTrends();
  const take = useTakeSnapshot();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const points = trends.data ?? [];
  return (
    <Panel
      title="Trends"
      className="shrink-0"
      aside={
        <Button
          variant="quiet"
          disabled={take.isPending}
          onClick={() => {
            take.mutate(undefined, {
              onSuccess: (s) => {
                pushNotice({ tone: "info", text: `Snapshot taken for ${s.civil_date}.` });
              },
              onError: (error) => {
                pushNotice({ tone: "negative", text: error.message });
              },
            });
          }}
        >
          Snapshot now
        </Button>
      }
    >
      {points.length === 0 ? (
        <EmptyState
          missing="No snapshot yet."
          fix="One is taken on each day's first unlock; completing a review stores one; Snapshot now takes one on demand. Trends read snapshots only, never live figures."
        />
      ) : (
        <TrendTable points={points} />
      )}
    </Panel>
  );
}

function TrendTable({ points }: { points: TrendPoint[] }) {
  return (
    <table className="w-full text-14">
      <thead className="text-12 text-text-dim">
        <tr className="border-b border-line text-left">
          <th className="py-1 pr-2 font-medium">Day</th>
          <th className="py-1 pr-2 font-medium">Kind</th>
          <th className="py-1 pr-2 text-right font-medium">Safe to spend</th>
          <th className="py-1 pr-2 text-right font-medium">Available</th>
          <th className="py-1 pr-2 text-right font-medium">Total debt</th>
          <th className="py-1 pr-2 text-right font-medium">Informal</th>
          <th className="py-1 text-right font-medium">Venture cap used</th>
        </tr>
      </thead>
      <tbody>
        {points.map((p) => (
          <tr key={p.civil_date} className="border-b border-line">
            <td className="money py-1 pr-2">{p.civil_date}</td>
            <td className="py-1 pr-2">
              <Chip>{p.kind}</Chip>
            </td>
            <td className="py-1 pr-2 text-right">
              <Money cents={p.safe_cents} untrusted={!p.trusted} />
            </td>
            <td className="py-1 pr-2 text-right">
              <Money cents={p.available_cents} tone={false} />
            </td>
            <td className="py-1 pr-2 text-right">
              <Money cents={p.total_debt_cents} tone={false} />
            </td>
            <td className="py-1 pr-2 text-right">
              <Money cents={p.informal_remaining_cents} tone={false} />
            </td>
            <td className="py-1 text-right">
              <Money cents={p.venture_cap_used_cents} tone={false} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
