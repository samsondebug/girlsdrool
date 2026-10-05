import { useMemo, useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { Dialog } from "../components/Dialog";
import { EmptyState } from "../components/EmptyState";
import { LinkDialog } from "../components/LinkDialog";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { Select } from "../components/Select";
import { TextField } from "../components/TextField";
import { MarkedMoney } from "../components/Untrusted";
import { useCorrectionProposal } from "../lib/corrections";
import type { LedgerRow } from "../lib/ipc";
import { formatCents } from "../lib/money";
import {
  useAccounts,
  useAcknowledgeFirewall,
  useCashView,
  useCategories,
  useReviewQueue,
  useSpendingView,
  useTrust,
  useUpdateTxn,
} from "../lib/queries";
import { FLAG_BITS } from "../lib/query-chips";
import { useUiStore } from "../lib/store";
import { untrustedAmong, untrustedCash } from "../lib/trust";

const NEEDS_REVIEW = 1;
const CIVIL_DATE = /^\d{4}-\d{2}-\d{2}$/;

interface Reason {
  label: string;
  title: string;
  tone: "warning" | "info" | "dim";
}

/** What each heuristic code asks of the person. */
const HEURISTIC_REASONS: Record<string, Reason> = {
  atm_withdrawal: {
    label: "cash withdrawal",
    title: "cash leaves the ledger here; say what it was for",
    tone: "info",
  },
  payment_app_row: {
    label: "payment app",
    title: "a payment-app row: who was it, and what for?",
    tone: "info",
  },
  refund_candidate: {
    label: "possible refund",
    title: "an earlier purchase of this size has a similar payee; Link… to confirm",
    tone: "info",
  },
  transfer_ambiguous: {
    label: "ambiguous transfer",
    title: "more than one row could be the other leg; Link… to choose",
    tone: "warning",
  },
};

/** Why a row is in the queue, in the order the person should think about it. */
function reasons(row: LedgerRow, firewalled: boolean): Reason[] {
  const out: Reason[] = [];
  if (firewalled && row.amount_cents < 0 && (row.flags & NEEDS_REVIEW) !== 0) {
    out.push({
      label: "firewall touch",
      title: "money left a firewalled account; acknowledge it so the touch is on record",
      tone: "warning",
    });
  }
  const heuristic = row.heuristic_code === null ? undefined : HEURISTIC_REASONS[row.heuristic_code];
  if (heuristic !== undefined) out.push(heuristic);
  if (row.classification === "unclassified") {
    out.push({
      label: "no rule matched",
      title: "neither a rule nor a heuristic placed this row",
      tone: "dim",
    });
  }
  for (const [name, bit] of Object.entries(FLAG_BITS)) {
    if (name === "needs_review" || name === "cash_withdrawal" || name === "payment_app_unknown") {
      continue;
    }
    if ((row.flags & bit) !== 0) {
      out.push({ label: name.replace(/_/g, " "), title: `flag ${name}`, tone: "dim" });
    }
  }
  return out;
}

function monthRange(): { from: string; to: string } {
  const now = new Date();
  const year = now.getFullYear();
  const month = now.getMonth();
  const pad = (n: number) => String(n).padStart(2, "0");
  const last = new Date(year, month + 1, 0).getDate();
  return { from: `${year}-${pad(month + 1)}-01`, to: `${year}-${pad(month + 1)}-${pad(last)}` };
}

/** The review queue, largest amount first, with the spending and cash views above it. */
export function Review() {
  const queue = useReviewQueue();
  const categories = useCategories();
  const accounts = useAccounts();
  const updateTxn = useUpdateTxn();
  const propose = useCorrectionProposal();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [linking, setLinking] = useState<LedgerRow | null>(null);
  const [acking, setAcking] = useState<LedgerRow | null>(null);

  const categoryOptions = useMemo(
    () => (categories.data ?? []).filter((c) => !c.archived),
    [categories.data],
  );
  const firewalled = useMemo(
    () => new Set((accounts.data ?? []).filter((a) => a.firewalled).map((a) => a.id)),
    [accounts.data],
  );
  const rows = queue.data ?? [];

  return (
    <div className="flex h-full flex-col gap-3 overflow-hidden p-4">
      <ViewsStrip />

      <div className="flex items-baseline gap-3">
        <h2 className="text-16 font-semibold">Review queue</h2>
        <span className="text-12 text-text-dim">
          {rows.length} {rows.length === 1 ? "row needs" : "rows need"} a decision · largest first ·
          a category here proposes a rule
        </span>
        {queue.isError ? (
          <span className="text-12 text-negative">{queue.error.message}</span>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-auto rounded-2 border border-line bg-bg-raised">
        {queue.isPending ? (
          <p className="p-4 text-12 text-text-dim">Loading…</p>
        ) : rows.length === 0 ? (
          <div className="p-4">
            <EmptyState
              missing="Nothing is waiting for review."
              fix="Import a statement (Import); rows no rule or heuristic can place land here, largest first."
            />
          </div>
        ) : (
          <table className="w-full border-collapse text-14">
            <thead className="sticky top-0 z-10 bg-bg-raised text-12 text-text-dim">
              <tr>
                {["Posted", "Account", "Payee", "Amount", "Why", "Category", ""].map((h) => (
                  <th
                    key={h}
                    className={`border-b border-line px-2 py-1 text-left font-medium ${h === "Amount" ? "text-right" : ""}`}
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.id} className="border-b border-line">
                  <td className="money px-2 py-1 whitespace-nowrap">{row.posted_date}</td>
                  <td className="px-2 py-1 whitespace-nowrap">{row.account_name}</td>
                  <td className="max-w-80 truncate px-2 py-1" title={row.payee_raw}>
                    {row.payee_norm}
                    {row.memo !== "" ? (
                      <span className="ml-2 text-12 text-text-dim">{row.memo}</span>
                    ) : null}
                  </td>
                  <td className="px-2 py-1 text-right whitespace-nowrap">
                    <Money cents={row.amount_cents} />
                  </td>
                  <td className="px-2 py-1">
                    <span className="flex flex-wrap gap-1">
                      {reasons(row, firewalled.has(row.account_id)).map((r) => (
                        <Chip key={r.label} tone={r.tone} title={r.title}>
                          {r.label}
                        </Chip>
                      ))}
                    </span>
                  </td>
                  <td className="w-56 px-2 py-1">
                    <Select
                      label="Category"
                      compact
                      className="w-full"
                      value={row.category_id ?? ""}
                      onChange={(e) => {
                        const value = e.target.value;
                        const categoryId = value === "" ? null : Number(value);
                        updateTxn.mutate(
                          { id: row.id, patch: { category_id: categoryId } },
                          {
                            onSuccess: () => {
                              const path = categoryOptions.find((c) => c.id === categoryId)?.path;
                              if (categoryId !== null && path !== undefined) propose(row.id, path);
                            },
                            onError: (error) => {
                              pushNotice({ tone: "negative", text: error.message });
                            },
                          },
                        );
                      }}
                    >
                      <option value="">— choose —</option>
                      {categoryOptions.map((c) => (
                        <option key={c.id} value={c.id}>
                          {c.path}
                        </option>
                      ))}
                    </Select>
                  </td>
                  <td className="px-2 py-1">
                    <span className="flex justify-end gap-1">
                      <Button
                        variant="quiet"
                        onClick={() => {
                          setLinking(row);
                        }}
                      >
                        Link…
                      </Button>
                      {firewalled.has(row.account_id) &&
                      row.amount_cents < 0 &&
                      (row.flags & NEEDS_REVIEW) !== 0 ? (
                        <Button
                          variant="secondary"
                          onClick={() => {
                            setAcking(row);
                          }}
                        >
                          Acknowledge
                        </Button>
                      ) : null}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {linking ? (
        <LinkDialog
          row={linking}
          onClose={() => {
            setLinking(null);
          }}
        />
      ) : null}
      {acking ? (
        <AcknowledgeDialog
          row={acking}
          onClose={() => {
            setAcking(null);
          }}
        />
      ) : null}
    </div>
  );
}

/** Spending view and cash view for a date range, and the difference between them. */
function ViewsStrip() {
  const [range, setRange] = useState(monthRange);
  const [draft, setDraft] = useState(range);
  const valid = CIVIL_DATE.test(range.from) && CIVIL_DATE.test(range.to);
  const spending = useSpendingView(range.from, range.to, valid);
  const cash = useCashView(range.from, range.to, valid);
  const s = spending.data;
  const c = cash.data;
  const error = spending.error ?? cash.error;
  const trust = useTrust();
  const allUntrusted = untrustedAmong(trust.data, null);
  const cashUntrusted = untrustedCash(trust.data);

  const apply = (event: FormEvent) => {
    event.preventDefault();
    setRange({ from: draft.from.trim(), to: draft.to.trim() });
  };

  return (
    <Panel
      title="Spending view vs cash view"
      className="shrink-0"
      aside={
        <form onSubmit={apply} className="flex items-end gap-2">
          <TextField
            label="From"
            mono
            className="w-32"
            value={draft.from}
            onChange={(e) => {
              setDraft({ ...draft, from: e.target.value });
            }}
          />
          <TextField
            label="To"
            mono
            className="w-32"
            value={draft.to}
            onChange={(e) => {
              setDraft({ ...draft, to: e.target.value });
            }}
          />
          <Button type="submit" variant="secondary">
            Show
          </Button>
        </form>
      }
    >
      {error ? <p className="text-12 text-negative">{error.message}</p> : null}
      <div className="grid grid-cols-3 gap-6 text-14">
        <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1">
          <dt className="col-span-2 text-12 text-text-dim">Spending: what was consumed</dt>
          <dt>Gross outflows</dt>
          <dd className="text-right">
            {s ? <MarkedMoney untrustedBy={allUntrusted} cents={s.gross_outflows_cents} /> : "—"}
          </dd>
          <dt>Linked refunds</dt>
          <dd className="text-right">
            {s ? <MarkedMoney untrustedBy={allUntrusted} cents={-s.linked_refunds_cents} /> : "—"}
          </dd>
          <dt>Same-category reimbursements</dt>
          <dd className="text-right">
            {s ? <MarkedMoney untrustedBy={allUntrusted} cents={-s.reimbursements_cents} /> : "—"}
          </dd>
          <dt className="font-medium">Net spending</dt>
          <dd className="text-right font-medium">
            {s ? <MarkedMoney untrustedBy={allUntrusted} cents={s.net_spending_cents} /> : "—"}
          </dd>
          <dt className="text-text-dim">Positive rows still in review</dt>
          <dd className="text-right text-text-dim">
            {s ? <MarkedMoney untrustedBy={allUntrusted} cents={s.positive_review_cents} /> : "—"}
          </dd>
        </dl>
        <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1">
          <dt className="col-span-2 text-12 text-text-dim">Cash: what left the cash accounts</dt>
          <dt>Outflows</dt>
          <dd className="text-right">
            {c ? <MarkedMoney untrustedBy={cashUntrusted} cents={-c.outflows_cents} /> : "—"}
          </dd>
          <dt>Inflows</dt>
          <dd className="text-right">
            {c ? <MarkedMoney untrustedBy={cashUntrusted} cents={c.inflows_cents} /> : "—"}
          </dd>
          <dt className="font-medium">Net change</dt>
          <dd className="text-right font-medium">
            {c ? <MarkedMoney untrustedBy={cashUntrusted} cents={c.net_cents} /> : "—"}
          </dd>
          <dt className="text-text-dim">Accounts</dt>
          <dd className="text-right text-text-dim">{c ? c.by_account.length : "—"}</dd>
        </dl>
        <div className="flex flex-col gap-1">
          <p className="text-12 text-text-dim">Gross spending − cash outflows</p>
          <p className="text-20">
            {s && c ? (
              <MarkedMoney
                cents={s.gross_outflows_cents - c.outflows_cents}
                size={20}
                tone={false}
                untrustedBy={allUntrusted}
              />
            ) : (
              "—"
            )}
          </p>
          <p className="text-12 text-text-dim">
            A card purchase is spending on its posted date on the card; the card payment is cash
            leaving the bank later. A transfer between two cash accounts is in neither view.
          </p>
        </div>
      </div>
    </Panel>
  );
}

interface AcknowledgeDialogProps {
  row: LedgerRow;
  onClose: () => void;
}

/** Policy `firewall_exclusion`: an outflow from a firewalled account is acknowledged in-app. */
function AcknowledgeDialog({ row, onClose }: AcknowledgeDialogProps) {
  const ack = useAcknowledgeFirewall();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [note, setNote] = useState("");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    ack.mutate(
      { txnId: row.id, note },
      {
        onSuccess: () => {
          pushNotice({
            tone: "positive",
            text: "Acknowledged. The row leaves the queue; the acknowledgment is in the audit log.",
          });
          onClose();
        },
        onError: (error) => {
          pushNotice({ tone: "negative", text: error.message });
        },
      },
    );
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title="Acknowledge a firewall touch"
      description={`${row.account_name} · ${row.posted_date} · ${row.payee_norm} · ${formatCents(row.amount_cents)}`}
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        <p className="text-14 text-text-dim">
          Money left a firewalled account. Say why, so the touch is on record; the row then leaves
          the review queue.
        </p>
        <TextField
          label="Note"
          value={note}
          onChange={(e) => {
            setNote(e.target.value);
          }}
          placeholder="e.g. moved to checking for the insurance premium"
          autoFocus
        />
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={ack.isPending}>
            Acknowledge
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
