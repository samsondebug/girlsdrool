import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Chip } from "../components/Chip";
import { Dialog } from "../components/Dialog";
import { EmptyState } from "../components/EmptyState";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { Select } from "../components/Select";
import { TextField } from "../components/TextField";
import type {
  Account,
  DebtComparison,
  DebtInput,
  DebtKind,
  DebtPayment,
  DebtView,
  InformalInput,
  InformalLoan,
  InterestMethod,
  MinimumRule,
  StrategyRun,
} from "../lib/ipc";
import { formatBps, formatCents, parseCentsInput } from "../lib/money";
import {
  useAccounts,
  useAddInformalScheduleRow,
  useCreateDebt,
  useCreateInformalLoan,
  useDebtComparison,
  useDebtPaymentCandidates,
  useDebtPayments,
  useDebts,
  useDeleteInformalScheduleRow,
  useInformalLoans,
  useRecordDebtPayment,
  useRemoveDebtPayment,
  useSetInformalNote,
  useUpdateDebt,
  useUpdateInformalLoan,
} from "../lib/queries";
import { useUiStore } from "../lib/store";

const KIND_LABEL: Record<DebtKind, string> = {
  credit_card: "credit card",
  loan: "loan",
  informal: "informal",
};

const RULE_LABEL: Record<MinimumRule, string> = {
  fixed: "fixed",
  percent_of_balance: "% of balance",
  interest_plus_percent: "interest + %",
  full_balance: "full balance",
  none: "none",
};

const METHOD_LABEL: Record<InterestMethod, string> = {
  monthly_nominal: "monthly nominal",
  actual_365: "actual/365",
};

/**
 * Debts and loans: every debt with what it owes and its next minimum, the strategy comparison
 * in interest cents for a monthly extra, and the informal loans with their schedules, the
 * repayments found in the ledger, the 12-month scenario and a note draft that never leaves.
 */
export function Debts() {
  const debts = useDebts();
  const loans = useInformalLoans();
  const accounts = useAccounts();
  const [editing, setEditing] = useState<DebtView | "new" | null>(null);
  const [editingLoan, setEditingLoan] = useState<InformalLoan | "new" | null>(null);
  const [extraText, setExtraText] = useState("");
  const extra = extraText.trim() === "" ? null : parseCentsInput(extraText);
  const comparison = useDebtComparison(extra === null || extra < 0 ? null : extra);
  const formal = (debts.data ?? []).filter((d) => d.kind !== "informal");
  const accountName = (id: number | null) =>
    (accounts.data ?? []).find((a) => a.id === id)?.name ?? "—";
  return (
    <div className="grid h-full grid-cols-12 gap-3 overflow-auto p-4">
      <Panel
        title="Debts"
        className="col-span-7 shrink-0"
        aside={
          <Button
            variant="primary"
            onClick={() => {
              setEditing("new");
            }}
          >
            New debt
          </Button>
        }
      >
        {formal.length === 0 ? (
          <EmptyState
            missing="No debt recorded."
            fix="Add the cards and loans you owe on: linked to a credit or loan account, or standalone with an opening balance."
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {formal.map((d) => (
              <DebtRow
                key={d.id}
                debt={d}
                onEdit={() => {
                  setEditing(d);
                }}
              />
            ))}
          </ul>
        )}
      </Panel>

      <Panel title="Informal loans" className="col-span-5 shrink-0">
        <div className="flex flex-col gap-2">
          {(loans.data ?? []).length === 0 ? (
            <EmptyState
              missing="No informal loan recorded."
              fix="Record money borrowed from a person: who, how much, what was promised; point at the row the money arrived on so it is flagged borrowing, never income."
            />
          ) : (
            <ul className="flex flex-col gap-2">
              {(loans.data ?? []).map((l) => (
                <LoanRow
                  key={l.debt_id}
                  loan={l}
                  accountName={accountName(l.payment_account_id)}
                  onEdit={() => {
                    setEditingLoan(l);
                  }}
                />
              ))}
            </ul>
          )}
          <Button
            variant="secondary"
            className="self-start"
            onClick={() => {
              setEditingLoan("new");
            }}
          >
            New informal loan
          </Button>
        </div>
      </Panel>

      <Panel title="Strategy comparison" className="col-span-12 shrink-0">
        <div className="flex flex-col gap-3">
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(e) => {
              e.preventDefault();
            }}
          >
            <TextField
              label="Extra per month, on top of every minimum"
              mono
              className="w-56"
              value={extraText}
              placeholder="0.00"
              onChange={(e) => {
                setExtraText(e.target.value);
              }}
              hint={
                comparison.data?.extra_source === "user"
                  ? "source: your input"
                  : "source: none yet — a review's dependable surplus will propose one (M8)"
              }
              error={
                extraText.trim() !== "" && (extra === null || extra < 0) ? "not an amount" : null
              }
            />
            {comparison.data?.informal_first ? (
              <Chip tone="info" title="policy informal_first">
                informal loans first
              </Chip>
            ) : null}
          </form>
          {comparison.data ? (
            <ComparisonView data={comparison.data} />
          ) : comparison.error ? (
            <EmptyState missing={comparison.error.message} fix="Enter a non-negative extra." />
          ) : null}
        </div>
      </Panel>

      {editing ? (
        <DebtDialog
          debt={editing === "new" ? null : editing}
          accounts={accounts.data ?? []}
          onClose={() => {
            setEditing(null);
          }}
        />
      ) : null}
      {editingLoan ? (
        <LoanDialog
          loan={editingLoan === "new" ? null : editingLoan}
          accounts={accounts.data ?? []}
          onClose={() => {
            setEditingLoan(null);
          }}
        />
      ) : null}
    </div>
  );
}

// ---- debts ----------------------------------------------------------------------------------

function DebtRow({ debt: d, onEdit }: { debt: DebtView; onEdit: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <li
      className={`flex flex-col gap-1 rounded-2 border border-line p-2 text-14 ${d.active ? "" : "text-text-dim"}`}
    >
      <div className="flex items-center gap-2">
        <span className="font-medium">{d.name}</span>
        <Chip>{KIND_LABEL[d.kind]}</Chip>
        {d.account_name ? (
          <Chip tone="info">linked: {d.account_name}</Chip>
        ) : (
          <Chip>standalone</Chip>
        )}
        {d.active ? null : <Chip>inactive</Chip>}
        {d.strategy_participation ? null : <Chip tone="warning">out of strategy</Chip>}
        <span className="ml-auto whitespace-nowrap">
          <Money cents={d.owed_cents} tone={false} size={16} />
          <span className="text-12 text-text-dim"> owed</span>
        </span>
        <Button variant="quiet" onClick={onEdit}>
          Edit
        </Button>
        <Button
          variant="quiet"
          aria-expanded={open}
          onClick={() => {
            setOpen((o) => !o);
          }}
        >
          Payments
        </Button>
      </div>
      <p className="text-12 text-text-dim">
        {formatBps(d.effective_apr_bps)} APR
        {d.promo_apr_bps !== null && d.promo_end
          ? ` (promo ${formatBps(d.promo_apr_bps)} through ${d.promo_end})`
          : ""}{" "}
        · {METHOD_LABEL[d.interest_method]} · minimum {RULE_LABEL[d.minimum_rule]}
        {d.due_day !== null ? ` on day ${String(d.due_day)}` : ""} · next period{" "}
        {d.next_period_start}..{d.next_period_end}: interest {formatCents(d.next_interest_cents)},
        minimum {formatCents(d.next_minimum_cents)}
        {d.obligation_id !== null ? " (an obligation in the plan)" : ""}
        {d.payment_account_name ? ` · paid from ${d.payment_account_name}` : ""}
      </p>
      {open ? <PaymentsPanel debt={d} /> : null}
    </li>
  );
}

function PaymentsPanel({ debt: d }: { debt: DebtView }) {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const remove = useRemoveDebtPayment();
  const record = useRecordDebtPayment();
  const candidates = useDebtPaymentCandidates(d.id, d.account_id === null);
  const [date, setDate] = useState(d.next_period_start);
  const [text, setText] = useState("");
  const cents = parseCentsInput(text);
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  if (d.account_id !== null) {
    return (
      <p className="text-12 text-text-dim">
        A linked debt is paid through its account's rows: the card payments on the ledger are the
        log; the balance owed is the account's.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-2 rounded-1 border border-line bg-bg p-2 text-12">
      <LinkedPayments
        debtId={d.id}
        onRemove={(p) => {
          remove.mutate(p.id, {
            onSuccess: () => {
              pushNotice({
                tone: "info",
                text: `Removed the payment of ${p.paid_date}.`,
                undo: {
                  label: "Undo",
                  run: () => {
                    record.mutate(
                      {
                        debtId: d.id,
                        input: {
                          paid_date: p.paid_date,
                          amount_cents: p.amount_cents,
                          txn_id: p.txn_id,
                          note: p.note,
                        },
                      },
                      { onError: fail },
                    );
                  },
                },
              });
            },
            onError: fail,
          });
        }}
      />
      <form
        className="flex flex-wrap items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (cents === null || cents <= 0) return;
          record.mutate(
            { debtId: d.id, input: { paid_date: date, amount_cents: cents, note: "by hand" } },
            {
              onSuccess: () => {
                setText("");
                pushNotice({
                  tone: "positive",
                  text: `Payment of ${formatCents(cents)} recorded.`,
                });
              },
              onError: fail,
            },
          );
        }}
      >
        <TextField
          label="Paid on"
          type="date"
          mono
          className="w-40"
          value={date}
          onChange={(e) => {
            setDate(e.target.value);
          }}
        />
        <TextField
          label="Amount"
          mono
          className="w-32"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
          }}
          error={text !== "" && (cents === null || cents <= 0) ? "a positive amount" : null}
        />
        <Button
          type="submit"
          variant="secondary"
          disabled={record.isPending || cents === null || cents <= 0}
        >
          Record payment
        </Button>
      </form>
      {candidates.data && candidates.data.length > 0 ? (
        <div className="flex flex-col gap-1">
          <p className="text-text-dim">Or pick the ledger row that paid it:</p>
          <ul className="flex max-h-40 flex-col gap-1 overflow-auto">
            {candidates.data.slice(0, 12).map(([id, posted, payee, amount]) => (
              <li key={id} className="flex items-center gap-2">
                <span className="money">{posted}</span>
                <span className="truncate">{payee}</span>
                <span className="ml-auto">
                  <Money cents={amount} />
                </span>
                <Button
                  variant="quiet"
                  onClick={() => {
                    record.mutate(
                      {
                        debtId: d.id,
                        input: { paid_date: posted, amount_cents: -amount, txn_id: id },
                      },
                      { onError: fail },
                    );
                  }}
                >
                  Use
                </Button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

function LinkedPayments({
  debtId,
  onRemove,
}: {
  debtId: number;
  onRemove: (payment: DebtPayment) => void;
}) {
  const payments = useDebtPayments(debtId);
  if (!payments.data || payments.data.length === 0) {
    return <p className="text-text-dim">No payment recorded yet.</p>;
  }
  return (
    <ul className="flex flex-col gap-1">
      {payments.data.map((p) => (
        <li key={p.id} className="flex items-center gap-2">
          <span className="money">{p.paid_date}</span>
          <Money cents={p.amount_cents} tone={false} />
          {p.txn_id !== null ? <Chip tone="info">ledger row</Chip> : <Chip>by hand</Chip>}
          {p.note ? <span className="text-text-dim">{p.note}</span> : null}
          <Button
            variant="quiet"
            className="ml-auto"
            onClick={() => {
              onRemove(p);
            }}
          >
            Remove
          </Button>
        </li>
      ))}
    </ul>
  );
}

function DebtDialog({
  debt,
  accounts,
  onClose,
}: {
  debt: DebtView | null;
  accounts: Account[];
  onClose: () => void;
}) {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const create = useCreateDebt();
  const update = useUpdateDebt();
  const liability = accounts.filter((a) => a.kind === "credit" || a.kind === "loan");
  const cash = accounts.filter((a) =>
    ["checking", "savings", "cash", "payment_app"].includes(a.kind),
  );
  const [name, setName] = useState(debt?.name ?? "");
  const [kind, setKind] = useState<DebtKind>(debt?.kind ?? "credit_card");
  const [linked, setLinked] = useState(debt ? debt.account_id !== null : liability.length > 0);
  const [accountId, setAccountId] = useState<string>(
    String(debt?.account_id ?? liability[0]?.id ?? ""),
  );
  const [opening, setOpening] = useState(
    debt?.standalone_opening_cents !== null && debt
      ? formatCents(debt.standalone_opening_cents, { symbol: false })
      : "",
  );
  const [openingDate, setOpeningDate] = useState(debt?.standalone_opening_date ?? "");
  const [apr, setApr] = useState(debt ? (debt.apr_bps / 100).toFixed(2) : "");
  const [promo, setPromo] = useState(
    debt?.promo_apr_bps !== null && debt ? (debt.promo_apr_bps / 100).toFixed(2) : "",
  );
  const [promoEnd, setPromoEnd] = useState(debt?.promo_end ?? "");
  const [method, setMethod] = useState<InterestMethod>(debt?.interest_method ?? "monthly_nominal");
  const [rule, setRule] = useState<MinimumRule>(debt?.minimum_rule ?? "percent_of_balance");
  const [fixed, setFixed] = useState(
    debt ? formatCents(debt.minimum_fixed_cents, { symbol: false }) : "",
  );
  const [bps, setBps] = useState(debt ? (debt.minimum_bps / 100).toFixed(2) : "");
  const [floor, setFloor] = useState(
    debt ? formatCents(debt.minimum_floor_cents, { symbol: false }) : "",
  );
  const [dueDay, setDueDay] = useState(debt?.due_day === null ? "" : String(debt?.due_day ?? ""));
  const [participation, setParticipation] = useState(debt?.strategy_participation ?? true);
  const [order, setOrder] = useState(
    debt?.custom_order === null ? "" : String(debt?.custom_order ?? ""),
  );
  const [paymentAccount, setPaymentAccount] = useState<string>(
    String(debt?.payment_account_id ?? cash[0]?.id ?? ""),
  );
  const [needle, setNeedle] = useState(debt?.match_payee_contains ?? "");
  const [active, setActive] = useState(debt?.active ?? true);
  const [error, setError] = useState<string | null>(null);
  const pending = create.isPending || update.isPending;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const input: DebtInput = {
      name,
      kind,
      account_id: linked ? Number(accountId) : null,
      standalone_opening_cents: linked ? null : parseCentsInput(opening),
      standalone_opening_date: linked ? null : openingDate || null,
      apr_bps: percentToBps(apr),
      promo_apr_bps: promo.trim() === "" ? null : percentToBps(promo),
      promo_end: promoEnd || null,
      interest_method: method,
      minimum_rule: rule,
      minimum_fixed_cents: parseCentsInput(fixed) ?? 0,
      minimum_bps: percentToBps(bps),
      minimum_floor_cents: parseCentsInput(floor) ?? 0,
      due_day: dueDay.trim() === "" ? null : Number(dueDay),
      strategy_participation: participation,
      custom_order: order.trim() === "" ? null : Number(order),
      active,
      payment_account_id: paymentAccount === "" ? null : Number(paymentAccount),
      match_payee_contains: needle.trim() === "" ? null : needle.trim(),
    };
    const done = () => {
      pushNotice({ tone: "positive", text: `${name} saved; its minimum is in the plan.` });
      onClose();
    };
    const fail = (err: Error) => {
      setError(err.message);
    };
    if (debt) update.mutate({ id: debt.id, input }, { onSuccess: done, onError: fail });
    else create.mutate(input, { onSuccess: done, onError: fail });
  };

  return (
    <Dialog
      open
      title={debt ? "Edit debt" : "New debt"}
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
    >
      <form className="flex w-[640px] max-w-full flex-col gap-3" onSubmit={submit}>
        <div className="grid grid-cols-2 gap-3">
          <TextField
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
          <Select
            label="Kind"
            value={kind}
            onChange={(e) => {
              setKind(e.target.value as DebtKind);
            }}
          >
            <option value="credit_card">credit card</option>
            <option value="loan">loan</option>
          </Select>
          <Checkbox
            label="Linked to an account (its balance is what is owed)"
            checked={linked}
            onChange={(e) => {
              setLinked(e.target.checked);
            }}
          />
          {linked ? (
            <Select
              label="Account"
              value={accountId}
              onChange={(e) => {
                setAccountId(e.target.value);
              }}
            >
              {liability.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </Select>
          ) : (
            <>
              <TextField
                label="Opening balance owed"
                mono
                value={opening}
                onChange={(e) => {
                  setOpening(e.target.value);
                }}
              />
              <TextField
                label="Owed as of"
                type="date"
                mono
                value={openingDate}
                onChange={(e) => {
                  setOpeningDate(e.target.value);
                }}
              />
            </>
          )}
          <TextField
            label="APR %"
            mono
            value={apr}
            onChange={(e) => {
              setApr(e.target.value);
            }}
          />
          <Select
            label="Interest method"
            value={method}
            onChange={(e) => {
              setMethod(e.target.value as InterestMethod);
            }}
          >
            <option value="monthly_nominal">monthly nominal (APR / 12)</option>
            <option value="actual_365">actual/365</option>
          </Select>
          <TextField
            label="Promo APR % (optional)"
            mono
            value={promo}
            onChange={(e) => {
              setPromo(e.target.value);
            }}
          />
          <TextField
            label="Promo ends"
            type="date"
            mono
            value={promoEnd}
            onChange={(e) => {
              setPromoEnd(e.target.value);
            }}
          />
          <Select
            label="Minimum rule"
            value={rule}
            onChange={(e) => {
              setRule(e.target.value as MinimumRule);
            }}
          >
            <option value="fixed">fixed amount</option>
            <option value="percent_of_balance">% of balance, with a floor</option>
            <option value="interest_plus_percent">interest + % of balance, with a floor</option>
            <option value="full_balance">full balance</option>
          </Select>
          <TextField
            label="Due day of month"
            mono
            value={dueDay}
            onChange={(e) => {
              setDueDay(e.target.value);
            }}
          />
          <TextField
            label="Fixed minimum"
            mono
            value={fixed}
            onChange={(e) => {
              setFixed(e.target.value);
            }}
          />
          <TextField
            label="Minimum % of balance"
            mono
            value={bps}
            onChange={(e) => {
              setBps(e.target.value);
            }}
          />
          <TextField
            label="Minimum floor"
            mono
            value={floor}
            onChange={(e) => {
              setFloor(e.target.value);
            }}
          />
          <Select
            label="Paid from"
            value={paymentAccount}
            onChange={(e) => {
              setPaymentAccount(e.target.value);
            }}
          >
            <option value="">—</option>
            {cash.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </Select>
          <TextField
            label="Payment rows: payee contains"
            value={needle}
            onChange={(e) => {
              setNeedle(e.target.value);
            }}
          />
          <TextField
            label="Custom order (custom strategy)"
            mono
            value={order}
            onChange={(e) => {
              setOrder(e.target.value);
            }}
          />
          <Checkbox
            label="Takes part in the strategy"
            checked={participation}
            onChange={(e) => {
              setParticipation(e.target.checked);
            }}
          />
          <Checkbox
            label="Active"
            checked={active}
            onChange={(e) => {
              setActive(e.target.checked);
            }}
          />
        </div>
        {error ? <p className="text-14 text-negative">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button variant="quiet" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={pending || name.trim() === ""}>
            Save
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

/** "6.49" → 649 basis points; the core validates the rest. */
function percentToBps(text: string): number {
  return parseCentsInput(text) ?? 0;
}

// ---- informal loans ------------------------------------------------------------------------

function LoanRow({
  loan: l,
  accountName,
  onEdit,
}: {
  loan: InformalLoan;
  accountName: string;
  onEdit: () => void;
}) {
  const [open, setOpen] = useState(false);
  const repaid = l.remaining_cents === 0;
  return (
    <li className="flex flex-col gap-1 rounded-2 border border-line p-2 text-14">
      <div className="flex items-center gap-2">
        <span className="font-medium">{l.counterparty}</span>
        {repaid ? <Chip tone="positive">repaid</Chip> : <Chip tone="warning">open</Chip>}
        {l.proceeds_txn_id !== null ? <Chip tone="info">proceeds flagged</Chip> : null}
        <span className="ml-auto whitespace-nowrap">
          <Money cents={l.remaining_cents} tone={false} size={16} />
          <span className="text-12 text-text-dim"> of {formatCents(l.original_cents)}</span>
        </span>
        <Button variant="quiet" onClick={onEdit}>
          Edit
        </Button>
        <Button
          variant="quiet"
          aria-expanded={open}
          onClick={() => {
            setOpen((o) => !o);
          }}
        >
          Details
        </Button>
      </div>
      <p className="text-12 text-text-dim">
        borrowed {l.borrowed_date}
        {l.promised_date ? ` · promised by ${l.promised_date}` : ""}
        {l.promised_terms ? ` · "${l.promised_terms}"` : ""} · repaid from {accountName}
      </p>
      {open ? <LoanDetails loan={l} /> : null}
    </li>
  );
}

function LoanDetails({ loan: l }: { loan: InformalLoan }) {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const addRow = useAddInformalScheduleRow();
  const deleteRow = useDeleteInformalScheduleRow();
  const setNote = useSetInformalNote();
  const [due, setDue] = useState("");
  const [amount, setAmount] = useState("");
  const [note, setNoteText] = useState(l.note_draft);
  const cents = parseCentsInput(amount);
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  return (
    <div className="flex flex-col gap-2 rounded-1 border border-line bg-bg p-2 text-12">
      <p className="font-medium text-text-dim">Schedule</p>
      {l.schedule.length === 0 ? (
        <p className="text-text-dim">No scheduled repayment; the strategy's extra pays it down.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {l.schedule.map((r) => (
            <li key={r.id} className="flex items-center gap-2">
              <span className="money">{r.due_date}</span>
              <Money cents={r.amount_cents} tone={false} />
              {r.unpaid_cents === 0 ? (
                <Chip tone="positive">paid</Chip>
              ) : (
                <Chip tone="warning">unpaid {formatCents(r.unpaid_cents)}</Chip>
              )}
              <Button
                variant="quiet"
                className="ml-auto"
                onClick={() => {
                  deleteRow.mutate(r.id, {
                    onSuccess: () => {
                      pushNotice({
                        tone: "info",
                        text: `Removed the schedule row due ${r.due_date}.`,
                        undo: {
                          label: "Undo",
                          run: () => {
                            addRow.mutate(
                              {
                                debtId: l.debt_id,
                                dueDate: r.due_date,
                                amountCents: r.amount_cents,
                              },
                              { onError: fail },
                            );
                          },
                        },
                      });
                    },
                    onError: fail,
                  });
                }}
              >
                Remove
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="flex items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (cents === null || cents <= 0 || due.length !== 10) return;
          addRow.mutate(
            { debtId: l.debt_id, dueDate: due, amountCents: cents },
            {
              onSuccess: () => {
                setDue("");
                setAmount("");
              },
              onError: fail,
            },
          );
        }}
      >
        <TextField
          label="Due"
          type="date"
          mono
          className="w-40"
          value={due}
          onChange={(e) => {
            setDue(e.target.value);
          }}
        />
        <TextField
          label="Amount"
          mono
          className="w-28"
          value={amount}
          onChange={(e) => {
            setAmount(e.target.value);
          }}
        />
        <Button
          type="submit"
          variant="secondary"
          disabled={addRow.isPending || cents === null || cents <= 0}
        >
          Add
        </Button>
      </form>
      <p className="font-medium text-text-dim">Repayments found in the ledger</p>
      {l.repayments.length === 0 ? (
        <p className="text-text-dim">
          None yet: a repayment is a transfer to this loan, never an expense.
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {l.repayments.map((p) => (
            <li key={p.id} className="flex items-center gap-2">
              <span className="money">{p.paid_date}</span>
              <Money cents={p.amount_cents} tone={false} />
              {p.txn_id !== null ? <Chip tone="info">ledger row</Chip> : <Chip>by hand</Chip>}
            </li>
          ))}
        </ul>
      )}
      <p className="font-medium text-text-dim">Repayment note (drafted here, never sent by Kept)</p>
      <textarea
        className="min-h-16 rounded-2 border border-line bg-bg-inset p-2 text-14 text-text"
        value={note}
        onChange={(e) => {
          setNoteText(e.target.value);
        }}
      />
      <Button
        variant="secondary"
        className="self-start"
        disabled={setNote.isPending || note === l.note_draft}
        onClick={() => {
          setNote.mutate(
            { debtId: l.debt_id, note },
            {
              onSuccess: () => {
                pushNotice({ tone: "info", text: "Note draft saved locally." });
              },
              onError: fail,
            },
          );
        }}
      >
        Save draft
      </Button>
    </div>
  );
}

function LoanDialog({
  loan,
  accounts,
  onClose,
}: {
  loan: InformalLoan | null;
  accounts: Account[];
  onClose: () => void;
}) {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const create = useCreateInformalLoan();
  const update = useUpdateInformalLoan();
  const cash = accounts.filter((a) =>
    ["checking", "savings", "cash", "payment_app"].includes(a.kind),
  );
  const [counterparty, setCounterparty] = useState(loan?.counterparty ?? "");
  const [original, setOriginal] = useState(
    loan ? formatCents(loan.original_cents, { symbol: false }) : "",
  );
  const [borrowed, setBorrowed] = useState(loan?.borrowed_date ?? "");
  const [terms, setTerms] = useState(loan?.promised_terms ?? "");
  const [promised, setPromised] = useState(loan?.promised_date ?? "");
  const [proceeds, setProceeds] = useState(String(loan?.proceeds_txn_id ?? ""));
  const [paymentAccount, setPaymentAccount] = useState<string>(
    String(loan?.payment_account_id ?? cash[0]?.id ?? ""),
  );
  const [needle, setNeedle] = useState(loan?.match_payee_contains ?? "");
  const [participation, setParticipation] = useState(loan?.strategy_participation ?? true);
  const [active, setActive] = useState(loan?.active ?? true);
  const [error, setError] = useState<string | null>(null);
  const cents = parseCentsInput(original);
  const pending = create.isPending || update.isPending;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (cents === null) return;
    const input: InformalInput = {
      counterparty,
      original_cents: cents,
      borrowed_date: borrowed,
      promised_terms: terms,
      promised_date: promised || null,
      proceeds_txn_id: proceeds.trim() === "" ? null : Number(proceeds),
      payment_account_id: paymentAccount === "" ? null : Number(paymentAccount),
      match_payee_contains: needle.trim() === "" ? null : needle.trim(),
      strategy_participation: participation,
      active,
    };
    const done = () => {
      pushNotice({
        tone: "positive",
        text: `Loan from ${counterparty} saved${proceeds.trim() === "" ? "" : "; the proceeds row is flagged borrowing"}.`,
      });
      onClose();
    };
    const fail = (err: Error) => {
      setError(err.message);
    };
    if (loan) update.mutate({ debtId: loan.debt_id, input }, { onSuccess: done, onError: fail });
    else create.mutate(input, { onSuccess: done, onError: fail });
  };
  return (
    <Dialog
      open
      title={loan ? "Edit informal loan" : "New informal loan"}
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
    >
      <form className="flex w-[560px] max-w-full flex-col gap-3" onSubmit={submit}>
        <div className="grid grid-cols-2 gap-3">
          <TextField
            label="Lent by"
            value={counterparty}
            onChange={(e) => {
              setCounterparty(e.target.value);
            }}
          />
          <TextField
            label="Amount borrowed"
            mono
            value={original}
            onChange={(e) => {
              setOriginal(e.target.value);
            }}
            error={original !== "" && cents === null ? "not an amount" : null}
          />
          <TextField
            label="Borrowed on"
            type="date"
            mono
            value={borrowed}
            onChange={(e) => {
              setBorrowed(e.target.value);
            }}
          />
          <TextField
            label="Promised by"
            type="date"
            mono
            value={promised}
            onChange={(e) => {
              setPromised(e.target.value);
            }}
          />
          <TextField
            label="Promised terms"
            className="col-span-2"
            value={terms}
            onChange={(e) => {
              setTerms(e.target.value);
            }}
          />
          <TextField
            label="Proceeds row id (the inflow; flagged borrowing, never income)"
            mono
            value={proceeds}
            onChange={(e) => {
              setProceeds(e.target.value);
            }}
            hint="find the row in the Ledger; leave empty when the money arrived before the ledger starts"
          />
          <Select
            label="Repaid from"
            value={paymentAccount}
            onChange={(e) => {
              setPaymentAccount(e.target.value);
            }}
          >
            <option value="">—</option>
            {cash.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </Select>
          <TextField
            label="Repayment rows: payee contains"
            className="col-span-2"
            value={needle}
            onChange={(e) => {
              setNeedle(e.target.value);
            }}
          />
          <Checkbox
            label="Takes part in the strategy"
            checked={participation}
            onChange={(e) => {
              setParticipation(e.target.checked);
            }}
          />
          <Checkbox
            label="Active"
            checked={active}
            onChange={(e) => {
              setActive(e.target.checked);
            }}
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
            disabled={
              pending || counterparty.trim() === "" || cents === null || borrowed.length !== 10
            }
          >
            Save
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

// ---- strategies -----------------------------------------------------------------------------

function ComparisonView({ data }: { data: DebtComparison }) {
  const [selected, setSelected] = useState<string>("avalanche");
  const run = data.strategies.find((s) => s.strategy === selected) ?? data.strategies[0];
  if (!run) return null;
  let best = run;
  for (const s of data.strategies) {
    if (s.total_interest_cents < best.total_interest_cents) best = s;
  }
  return (
    <div className="flex flex-col gap-3 text-14">
      <p className="text-12 text-text-dim">
        Budget {formatCents(run.budget_cents)} per month = extra {formatCents(data.extra_cents)} +
        the first period's minimums, constant: a paid-off debt's minimum rolls to the next target.
        Interest in cents; a payment never exceeds opening + interest.
      </p>
      <table className="w-full">
        <thead className="text-12 text-text-dim">
          <tr className="border-b border-line text-left">
            <th className="py-1 pr-2 font-medium">Strategy</th>
            <th className="py-1 pr-2 text-right font-medium">Total interest</th>
            <th className="py-1 pr-2 font-medium">Last payoff</th>
            {run.debts.map((d) => (
              <th key={d.debt_id} className="py-1 pr-2 text-right font-medium">
                {d.name}
              </th>
            ))}
            <th className="py-1 font-medium" />
          </tr>
        </thead>
        <tbody>
          {data.strategies.map((s) => (
            <tr key={s.strategy} className="border-b border-line">
              <td className="py-1 pr-2">
                <span className="inline-flex items-center gap-2">
                  {s.strategy}
                  {s.strategy === best.strategy ? (
                    <Chip tone="positive">least interest</Chip>
                  ) : null}
                  {s.unfinished ? <Chip tone="warning">not amortizing in 120 months</Chip> : null}
                </span>
              </td>
              <td className="py-1 pr-2 text-right">
                <Money cents={s.total_interest_cents} tone={false} />
              </td>
              <td className="money py-1 pr-2">{s.payoff_date ?? "—"}</td>
              {s.debts.map((d) => (
                <td key={d.debt_id} className="py-1 pr-2 text-right">
                  <span className="money">
                    {formatCents(d.total_interest_cents)}
                    <span className="text-12 text-text-dim"> by {d.payoff_date ?? "—"}</span>
                  </span>
                </td>
              ))}
              <td className="py-1 text-right">
                <Button
                  variant={s.strategy === run.strategy ? "secondary" : "quiet"}
                  aria-pressed={s.strategy === run.strategy}
                  onClick={() => {
                    setSelected(s.strategy);
                  }}
                >
                  Schedule
                </Button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <ScenarioLine data={data} />
      <ScheduleTables run={run} />
    </div>
  );
}

function ScenarioLine({ data }: { data: DebtComparison }) {
  const s = data.scenario;
  if (s.remaining_cents === 0) {
    return <p className="text-12 text-text-dim">No informal loan is open.</p>;
  }
  return (
    <p className="flex flex-wrap items-center gap-2">
      <span className="text-text-dim">
        Informal loans repaid within {String(s.periods)} months with this extra:
      </span>
      {s.achievable ? (
        <Chip tone="positive">achievable, by {s.payoff_date ?? "—"}</Chip>
      ) : (
        <>
          <Chip tone="warning">not achievable</Chip>
          <span>
            gap after {String(s.periods)} months <Money cents={s.gap_cents} tone={false} />
          </span>
          <span className="text-text-dim">
            {s.payoff_date
              ? `; this budget gets there by ${s.payoff_date}`
              : "; this budget never gets there"}
          </span>
        </>
      )}
    </p>
  );
}

function ScheduleTables({ run }: { run: StrategyRun }) {
  const [debtId, setDebtId] = useState<number | null>(null);
  const shown = run.debts.find((d) => d.debt_id === debtId) ?? run.debts[0];
  if (!shown) return <p className="text-12 text-text-dim">Nothing is owed.</p>;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-1">
        <span className="text-12 text-text-dim">{run.strategy} schedule:</span>
        {run.debts.map((d) => (
          <Button
            key={d.debt_id}
            variant={d.debt_id === shown.debt_id ? "secondary" : "quiet"}
            aria-pressed={d.debt_id === shown.debt_id}
            onClick={() => {
              setDebtId(d.debt_id);
            }}
          >
            {d.name}
          </Button>
        ))}
      </div>
      <div className="max-h-72 overflow-auto">
        <table className="w-full text-14">
          <thead className="text-12 text-text-dim">
            <tr className="border-b border-line text-left">
              <th className="py-1 pr-2 font-medium">Period</th>
              <th className="py-1 pr-2 font-medium">From</th>
              <th className="py-1 pr-2 font-medium">To</th>
              <th className="py-1 pr-2 text-right font-medium">Opening</th>
              <th className="py-1 pr-2 text-right font-medium">Interest</th>
              <th className="py-1 pr-2 text-right font-medium">Minimum</th>
              <th className="py-1 pr-2 text-right font-medium">Payment</th>
              <th className="py-1 text-right font-medium">Closing</th>
            </tr>
          </thead>
          <tbody>
            {shown.periods.map((r) => (
              <tr key={r.period} className="border-b border-line">
                <td className="money py-1 pr-2 text-text-dim">{r.period}</td>
                <td className="money py-1 pr-2">{r.start}</td>
                <td className="money py-1 pr-2">{r.end}</td>
                <td className="py-1 pr-2 text-right">
                  <Money cents={r.opening_cents} tone={false} />
                </td>
                <td className="py-1 pr-2 text-right">
                  <Money cents={r.interest_cents} tone={false} />
                </td>
                <td className="py-1 pr-2 text-right">
                  <Money cents={r.minimum_cents} tone={false} />
                </td>
                <td className="py-1 pr-2 text-right">
                  <Money cents={r.payment_cents} tone={false} />
                </td>
                <td className="py-1 text-right">
                  <Money cents={r.closing_cents} tone={false} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="text-12 text-text-dim">
        {shown.name}: owed {formatCents(shown.owed_cents)}, interest{" "}
        {formatCents(shown.total_interest_cents)}, paid {formatCents(shown.total_paid_cents)}, off
        by {shown.payoff_date ?? "—"}.
      </p>
    </div>
  );
}
