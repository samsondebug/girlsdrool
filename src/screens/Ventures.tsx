import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { Dialog } from "../components/Dialog";
import { EmptyState } from "../components/EmptyState";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { Select } from "../components/Select";
import { TextField } from "../components/TextField";
import type { VentureInput, VentureRollup, VentureStatus } from "../lib/ipc";
import { formatBps, formatCents, parseCentsInput } from "../lib/money";
import { useCreateVenture, useUpdateVenture, useVentureSummary } from "../lib/queries";
import { useUiStore } from "../lib/store";

const STATUS_TONE: Record<string, "positive" | "warning" | "negative"> = {
  fund: "positive",
  freeze: "warning",
  kill: "negative",
};

/**
 * Ventures: one card per venture with its cap gauge, the five buckets over the trailing twelve
 * months, operating cash flow, the milestone countdown and the stop-condition alert. The verdict
 * is the person's; every figure is the core's.
 */
export function Ventures() {
  const summary = useVentureSummary();
  const [editing, setEditing] = useState<VentureRollup | "new" | null>(null);
  const data = summary.data;
  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-4">
      <Panel
        title="Ventures"
        className="shrink-0"
        aside={
          <Button
            variant="primary"
            onClick={() => {
              setEditing("new");
            }}
          >
            New venture
          </Button>
        }
      >
        {data ? (
          data.ventures.length === 0 ? (
            <EmptyState
              missing="No venture recorded."
              fix="Name it, set the cash cap you are willing to lose, a milestone with a date and the stop condition; then tag its rows and mark its accounts as venture-owned (Accounts)."
            />
          ) : (
            <div className="flex flex-col gap-3">
              <p className="text-12 text-text-dim">
                Trailing twelve months ({data.window_start} .. {data.as_of}). Cap used across
                ventures {formatCents(data.total_cap_used_cents)} of{" "}
                {formatCents(data.total_cap_cents)}; operating spend{" "}
                {formatCents(data.total_operating_expense_cents)} ={" "}
                {formatBps(data.spend_share_bps)} of confirmed base pay received (
                {formatCents(data.take_home_cents)}). Sunk cost is not an input.
              </p>
              <div className="grid grid-cols-2 gap-3">
                {data.ventures.map((v) => (
                  <VentureCard
                    key={v.id}
                    venture={v}
                    onEdit={() => {
                      setEditing(v);
                    }}
                  />
                ))}
              </div>
            </div>
          )
        ) : null}
      </Panel>
      {editing ? (
        <VentureDialog
          venture={editing === "new" ? null : editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      ) : null}
    </div>
  );
}

function VentureCard({ venture: v, onEdit }: { venture: VentureRollup; onEdit: () => void }) {
  const pct = Math.min(100, Math.max(0, v.cap_utilization_bps / 100));
  const tone = v.alerts.length > 0 ? "bg-negative" : pct >= 80 ? "bg-warning" : "bg-accent";
  const buckets: { label: string; bucket: VentureRollup["customer_revenue"]; sign: 1 | -1 }[] = [
    { label: "Customer revenue", bucket: v.customer_revenue, sign: 1 },
    { label: "Operating expense", bucket: v.operating_expense, sign: -1 },
    { label: "Owner contribution", bucket: v.owner_contribution, sign: 1 },
    { label: "Financing", bucket: v.financing, sign: 1 },
    { label: "Withdrawal", bucket: v.withdrawal, sign: -1 },
  ];
  return (
    <section className="flex flex-col gap-2 rounded-2 border border-line p-3 text-14">
      <header className="flex items-center gap-2">
        <span className="text-16 font-medium">{v.name}</span>
        <Chip tone={STATUS_TONE[v.status] ?? "dim"}>{v.status}</Chip>
        {v.alerts.map((a) => (
          <Chip key={a} tone="negative">
            {a}
          </Chip>
        ))}
        <Button variant="quiet" className="ml-auto" onClick={onEdit}>
          Edit
        </Button>
      </header>
      <div className="flex flex-col gap-1">
        <p className="flex items-baseline justify-between">
          <span className="text-text-dim">Cap used</span>
          <span className="money">
            {formatCents(v.cap_used_cents)} / {formatCents(v.cash_cap_cents)} ·{" "}
            {formatBps(v.cap_utilization_bps)}
          </span>
        </p>
        <div
          className="h-3 w-full rounded-1 bg-bg-inset"
          role="meter"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct}
          aria-label={`${v.name} cap used ${formatBps(v.cap_utilization_bps)}`}
        >
          <div className={`h-3 rounded-1 ${tone}`} style={{ width: `${String(pct)}%` }} />
        </div>
        <p className="text-12 text-text-dim">
          contributions + operating expense paid from personal accounts (
          {formatCents(v.operating_expense_from_personal_cents)}) − withdrawals; remaining{" "}
          {formatCents(v.cap_remaining_cents)}
        </p>
      </div>
      <table className="w-full">
        <tbody>
          {buckets.map((b) => (
            <tr key={b.label} className="border-t border-line">
              <td className="py-1 pr-2">{b.label}</td>
              <td className="py-1 pr-2 text-right text-12 text-text-dim">
                {String(b.bucket.rows)} row{b.bucket.rows === 1 ? "" : "s"}
              </td>
              <td className="py-1 text-right">
                <Money cents={b.sign * b.bucket.cents} />
              </td>
            </tr>
          ))}
          <tr className="border-t border-line font-medium">
            <td className="py-1 pr-2">Operating cash flow</td>
            <td />
            <td className="py-1 text-right">
              <Money cents={v.operating_cash_flow_cents} />
            </td>
          </tr>
        </tbody>
      </table>
      <p className="text-12 text-text-dim">
        {v.milestone ? `Milestone: ${v.milestone}` : "No milestone"}
        {v.milestone_date
          ? ` by ${v.milestone_date} (${v.milestone_days !== null && v.milestone_days >= 0 ? `${String(v.milestone_days)} days left` : "passed"})`
          : ""}
        {v.time_budget_hours !== null ? ` · time budget ${String(v.time_budget_hours)} h` : ""}
      </p>
      <p className="text-12">
        <span className="text-text-dim">Stop condition: </span>
        {v.stop_condition || "none written down"}
      </p>
      {v.accounts.length > 0 ? (
        <p className="text-12 text-text-dim">
          Venture-owned:{" "}
          {v.accounts.map((a) => `${a.name} ${formatCents(a.balance_cents)}`).join(", ")} (never
          personal cash).
        </p>
      ) : (
        <p className="text-12 text-text-dim">
          No venture-owned account: mark one under Accounts so transfers into it count as
          contributions.
        </p>
      )}
    </section>
  );
}

function VentureDialog({
  venture,
  onClose,
}: {
  venture: VentureRollup | null;
  onClose: () => void;
}) {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const create = useCreateVenture();
  const update = useUpdateVenture();
  const [name, setName] = useState(venture?.name ?? "");
  const [status, setStatus] = useState<VentureStatus>(venture?.status ?? "fund");
  const [cap, setCap] = useState(
    venture ? formatCents(venture.cash_cap_cents, { symbol: false }) : "",
  );
  const [hours, setHours] = useState(
    venture?.time_budget_hours === null || !venture ? "" : String(venture.time_budget_hours),
  );
  const [milestone, setMilestone] = useState(venture?.milestone ?? "");
  const [milestoneDate, setMilestoneDate] = useState(venture?.milestone_date ?? "");
  const [stop, setStop] = useState(venture?.stop_condition ?? "");
  const [error, setError] = useState<string | null>(null);
  const capCents = parseCentsInput(cap);
  const pending = create.isPending || update.isPending;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (capCents === null) return;
    const input: VentureInput = {
      name,
      status,
      cash_cap_cents: capCents,
      time_budget_hours: hours.trim() === "" ? null : Number(hours),
      milestone,
      milestone_date: milestoneDate || null,
      stop_condition: stop,
    };
    const done = () => {
      pushNotice({ tone: "positive", text: `${name} saved.` });
      onClose();
    };
    const fail = (err: Error) => {
      setError(err.message);
    };
    if (venture) update.mutate({ id: venture.id, input }, { onSuccess: done, onError: fail });
    else create.mutate(input, { onSuccess: done, onError: fail });
  };
  return (
    <Dialog
      open
      title={venture ? "Edit venture" : "New venture"}
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
    >
      <form className="flex w-[560px] max-w-full flex-col gap-3" onSubmit={submit}>
        <div className="grid grid-cols-2 gap-3">
          <TextField
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
          <Select
            label="Verdict"
            value={status}
            onChange={(e) => {
              setStatus(e.target.value as VentureStatus);
            }}
          >
            <option value="fund">fund</option>
            <option value="freeze">freeze</option>
            <option value="kill">kill</option>
          </Select>
          <TextField
            label="Cash cap (what you are willing to lose)"
            mono
            value={cap}
            onChange={(e) => {
              setCap(e.target.value);
            }}
            error={cap !== "" && capCents === null ? "not an amount" : null}
          />
          <TextField
            label="Time budget, hours (optional)"
            mono
            value={hours}
            onChange={(e) => {
              setHours(e.target.value);
            }}
          />
          <TextField
            label="Milestone"
            value={milestone}
            onChange={(e) => {
              setMilestone(e.target.value);
            }}
          />
          <TextField
            label="Milestone date"
            type="date"
            mono
            value={milestoneDate}
            onChange={(e) => {
              setMilestoneDate(e.target.value);
            }}
          />
          <TextField
            label="Stop condition"
            className="col-span-2"
            value={stop}
            onChange={(e) => {
              setStop(e.target.value);
            }}
            hint="written down now, so the verdict later is a comparison, not a mood"
          />
        </div>
        {error ? <p className="text-14 text-negative">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button variant="quiet" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            disabled={pending || name.trim() === "" || capCents === null}
          >
            Save
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
