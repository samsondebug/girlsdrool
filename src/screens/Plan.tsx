import { useMemo, useState, type FormEvent } from "react";

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
  Category,
  Confidence,
  Cycle,
  DueRule,
  Earmark,
  EarmarkInput,
  EarmarkKind,
  EarmarkSchedule,
  EntryKind,
  IncomeInput,
  IncomeKind,
  IncomeStream,
  LedgerRow,
  Obligation,
  ObligationInput,
  ObligationKind,
  WeekendRule,
} from "../lib/ipc";
import { formatCents, parseCentsInput } from "../lib/money";
import {
  useAccounts,
  useAddEarmarkEntry,
  useCategories,
  useCreateEarmark,
  useCreateIncomeStream,
  useCreateObligation,
  useDeleteEarmarkEntry,
  useDeleteObligationCandidate,
  useDetectCandidates,
  useEarmarkEntries,
  useEarmarks,
  useIncomeStreams,
  useLedger,
  useNextOccurrences,
  useObligations,
  usePayments,
  usePolicies,
  useReceipts,
  useRecordPayment,
  useRecordReceipt,
  useRemovePayment,
  useRemoveReceipt,
  useSetObligationStatus,
  useSettings,
  useUpdateEarmark,
  useUpdateIncomeStream,
  useUpdateObligation,
  useUpdateSetting,
} from "../lib/queries";
import { useUiStore } from "../lib/store";

const CIVIL_DATE = /^\d{4}-\d{2}-\d{2}$/;
const INCOME_KINDS: IncomeKind[] = ["base", "bonus", "rsu", "deferred_comp", "other"];
const CYCLES: Cycle[] = ["weekly", "biweekly", "semimonthly", "monthly", "once"];
const CONFIDENCES: Confidence[] = ["confirmed", "expected", "rumored"];
const WEEKEND_RULES: WeekendRule[] = ["previous_business_day", "next_business_day", "none"];
const OBLIGATION_KINDS: ObligationKind[] = ["bill", "debt_minimum", "other"];
const DUE_RULES: DueRule[] = ["monthly_day", "annual", "nth_weekday", "biweekly", "once"];
const EARMARK_KINDS: EarmarkKind[] = ["obligation", "sinking_fund", "emergency_reserve"];
const SCHEDULES: EarmarkSchedule[] = ["none", "monthly", "per_paycheck", "by_date"];
const ENTRY_KINDS: EntryKind[] = ["fund", "release", "adjust"];
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function optionalCents(text: string): { cents: number | null; error: string | null } {
  if (text.trim() === "") return { cents: null, error: null };
  const cents = parseCentsInput(text);
  return cents === null ? { cents: null, error: "not an amount" } : { cents, error: null };
}

function optionalInt(text: string): number | null {
  return /^\d{1,4}$/.test(text.trim()) ? Number(text.trim()) : null;
}

function label(s: string): string {
  return s.replace(/_/g, " ");
}

function dueRuleText(o: Obligation): string {
  switch (o.due_rule) {
    case "monthly_day":
      return `monthly on day ${o.due_day ?? "?"}`;
    case "annual":
      return `annual on ${String(o.due_month ?? 0).padStart(2, "0")}-${String(o.due_day ?? 0).padStart(2, "0")}`;
    case "nth_weekday":
      return `${o.due_nth === 5 ? "last" : `${o.due_nth ?? "?"}.`} ${WEEKDAYS[o.due_weekday ?? 0] ?? "?"} of the month`;
    case "biweekly":
      return `every two weeks from ${o.anchor_date ?? "?"}`;
    case "once":
      return `once on ${o.anchor_date ?? "?"}`;
  }
}

/**
 * The plan: income streams, obligations (confirmed and candidates), earmarks with their entries,
 * the two reserves (timing buffer setting and emergency-reserve earmarks) and the policies.
 */
export function Plan() {
  const accounts = useAccounts();
  const categories = useCategories();
  const streams = useIncomeStreams();
  const obligations = useObligations();
  const earmarks = useEarmarks();
  const policies = usePolicies();
  const settings = useSettings(true);
  const updateSetting = useUpdateSetting();
  const detect = useDetectCandidates();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [editingStream, setEditingStream] = useState<IncomeStream | "new" | null>(null);
  const [editingObligation, setEditingObligation] = useState<Obligation | "new" | null>(null);
  const [editingEarmark, setEditingEarmark] = useState<Earmark | "new" | null>(null);

  const accountList = useMemo(
    () => (accounts.data ?? []).filter((a) => !a.archived),
    [accounts.data],
  );
  const categoryList = useMemo(
    () => (categories.data ?? []).filter((c) => !c.archived),
    [categories.data],
  );
  const streamList = streams.data ?? [];
  const obligationList = obligations.data ?? [];
  const earmarkList = earmarks.data ?? [];
  const confirmed = obligationList.filter((o) => o.status === "confirmed");
  const candidates = obligationList.filter((o) => o.status === "candidate");
  const retired = obligationList.filter((o) => o.status === "retired");
  const accountName = (id: number | null) =>
    accountList.find((a) => a.id === id)?.name ?? (id === null ? "any account" : `account ${id}`);
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };

  return (
    <div className="grid h-full grid-cols-2 gap-3 overflow-auto p-4">
      <Panel
        title="Income streams"
        className="shrink-0"
        aside={
          <Button
            variant="primary"
            onClick={() => {
              setEditingStream("new");
            }}
          >
            New stream
          </Button>
        }
      >
        {streamList.length === 0 ? (
          <EmptyState
            missing="No income stream."
            fix="Add your base pay cycle and mark it confirmed; only confirmed streams enter the hero and the forecast."
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {streamList.map((s) => (
              <StreamRow
                key={s.id}
                stream={s}
                accountName={accountName(s.deposit_account_id)}
                onEdit={() => {
                  setEditingStream(s);
                }}
              />
            ))}
          </ul>
        )}
      </Panel>

      <Panel
        title="Obligations"
        className="shrink-0"
        aside={
          <span className="flex gap-2">
            <Button
              variant="secondary"
              disabled={detect.isPending}
              onClick={() => {
                detect.mutate(undefined, {
                  onSuccess: (created) => {
                    pushNotice({
                      tone: created.length > 0 ? "info" : "positive",
                      text:
                        created.length > 0
                          ? `${created.length} recurring payee${created.length === 1 ? "" : "s"} proposed as candidates; confirm or delete each.`
                          : "No new recurring payee: every one is already an obligation or a candidate.",
                    });
                  },
                  onError: fail,
                });
              }}
            >
              Detect candidates
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                setEditingObligation("new");
              }}
            >
              New obligation
            </Button>
          </span>
        }
      >
        {obligationList.length === 0 ? (
          <EmptyState
            missing="No obligation."
            fix="Add a bill, or press Detect candidates to propose the recurring payees in the ledger; a candidate counts for nothing until you confirm it."
          />
        ) : (
          <div className="flex flex-col gap-3">
            {confirmed.length > 0 ? (
              <ul className="flex flex-col gap-2">
                {confirmed.map((o) => (
                  <ObligationRow
                    key={o.id}
                    obligation={o}
                    accountName={accountName(o.source_account_id)}
                    onEdit={() => {
                      setEditingObligation(o);
                    }}
                  />
                ))}
              </ul>
            ) : null}
            {candidates.length > 0 ? (
              <div className="flex flex-col gap-2">
                <h4 className="text-12 font-medium text-text-dim">
                  Candidates · {candidates.length} · detected from recurring rows, not yet counted
                </h4>
                <ul className="flex flex-col gap-2">
                  {candidates.map((o) => (
                    <ObligationRow
                      key={o.id}
                      obligation={o}
                      accountName={accountName(o.source_account_id)}
                      onEdit={() => {
                        setEditingObligation(o);
                      }}
                    />
                  ))}
                </ul>
              </div>
            ) : null}
            {retired.length > 0 ? (
              <p className="text-12 text-text-dim">
                Retired: {retired.map((o) => o.name).join(", ")}.
              </p>
            ) : null}
          </div>
        )}
      </Panel>

      <Panel
        title="Earmarks"
        className="shrink-0"
        aside={
          <Button
            variant="primary"
            onClick={() => {
              setEditingEarmark("new");
            }}
          >
            New earmark
          </Button>
        }
      >
        {earmarkList.length === 0 ? (
          <EmptyState
            missing="No earmark."
            fix="Reserve cents against a funding account for an obligation, a sinking fund or the emergency reserve; remaining is derived from its entries and subtracted from the hero once."
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {earmarkList.map((e) => (
              <EarmarkRow
                key={e.id}
                earmark={e}
                accountName={accountName(e.funding_account_id)}
                obligationName={obligationList.find((o) => o.id === e.obligation_id)?.name ?? null}
                onEdit={() => {
                  setEditingEarmark(e);
                }}
              />
            ))}
          </ul>
        )}
      </Panel>

      <div className="flex flex-col gap-3">
        <Panel title="Reserves" className="shrink-0">
          <div className="flex flex-col gap-3 text-14">
            {settings.data ? (
              <BufferForm
                key={settings.data.timing_buffer_cents}
                initial={settings.data.timing_buffer_cents}
                pending={updateSetting.isPending}
                error={updateSetting.isError ? updateSetting.error.message : null}
                onSave={(cents) => {
                  updateSetting.mutate({ key: "timing_buffer_cents", value: cents });
                }}
              />
            ) : null}
            <div>
              <p className="text-12 text-text-dim">Emergency reserve</p>
              {earmarkList.filter((e) => e.kind === "emergency_reserve" && e.active).length ===
              0 ? (
                <p>
                  None yet: add an earmark of kind emergency reserve on the account that holds it.
                </p>
              ) : (
                earmarkList
                  .filter((e) => e.kind === "emergency_reserve" && e.active)
                  .map((e) => (
                    <p key={e.id}>
                      {e.name} on {accountName(e.funding_account_id)}, target{" "}
                      <Money cents={e.target_cents} />
                    </p>
                  ))
              )}
              <p className="mt-1 text-12 text-text-dim">
                Two reserves, stored separately, each subtracted once: the timing buffer is a
                setting; the emergency reserve is an earmark whose remaining comes from its entries.
              </p>
            </div>
          </div>
        </Panel>

        <Panel title="Policies" className="shrink-0">
          <ul className="flex flex-col gap-1 text-14">
            {(policies.data ?? []).map((p) => (
              <li key={p.id} className="flex items-center gap-2">
                <span>{p.name}</span>
                <Chip tone={p.is_system ? "info" : "dim"}>
                  {p.is_system ? "system" : label(p.kind)}
                </Chip>
              </li>
            ))}
          </ul>
          <p className="mt-2 text-12 text-text-dim">
            System policies cannot be deleted; the engines enforce them. User policies arrive with
            the review (M8) and are listed as reminders.
          </p>
        </Panel>
      </div>

      {editingStream !== null ? (
        <StreamDialog
          stream={editingStream === "new" ? null : editingStream}
          accounts={accountList}
          onClose={() => {
            setEditingStream(null);
          }}
        />
      ) : null}
      {editingObligation !== null ? (
        <ObligationDialog
          obligation={editingObligation === "new" ? null : editingObligation}
          accounts={accountList}
          categories={categoryList}
          onClose={() => {
            setEditingObligation(null);
          }}
        />
      ) : null}
      {editingEarmark !== null ? (
        <EarmarkDialog
          earmark={editingEarmark === "new" ? null : editingEarmark}
          accounts={accountList}
          obligations={confirmed}
          streams={streamList}
          onClose={() => {
            setEditingEarmark(null);
          }}
        />
      ) : null}
    </div>
  );
}

function BufferForm({
  initial,
  pending,
  error,
  onSave,
}: {
  initial: number;
  pending: boolean;
  error: string | null;
  onSave: (cents: number) => void;
}) {
  const [text, setText] = useState(formatCents(initial, { symbol: false }));
  const cents = parseCentsInput(text);
  return (
    <form
      className="flex items-end gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (cents !== null && cents >= 0) onSave(cents);
      }}
    >
      <TextField
        label="Timing buffer (minimum_buffer)"
        mono
        className="w-48"
        value={text}
        onChange={(e) => {
          setText(e.target.value);
        }}
        hint="subtracted from the hero as is"
        error={error ?? (cents === null ? "not an amount" : null)}
      />
      <Button
        type="submit"
        variant="secondary"
        disabled={pending || cents === null || cents === initial}
      >
        Save
      </Button>
    </form>
  );
}

// ---- income streams -----------------------------------------------------------------------------

function StreamRow({
  stream: s,
  accountName,
  onEdit,
}: {
  stream: IncomeStream;
  accountName: string;
  onEdit: () => void;
}) {
  const [open, setOpen] = useState(false);
  const next = useNextOccurrences("income", s.id, 3);
  return (
    <li
      className={`flex flex-col gap-1 rounded-2 border border-line p-2 text-14 ${s.active ? "" : "text-text-dim"}`}
    >
      <div className="flex items-center gap-2">
        <span className="font-medium">{s.name}</span>
        <Chip tone={s.confidence === "confirmed" ? "positive" : "warning"}>{s.confidence}</Chip>
        <Chip>{label(s.kind)}</Chip>
        {s.active ? null : <Chip>inactive</Chip>}
        <span className="ml-auto whitespace-nowrap">
          <Money cents={s.expected_net_cents} />
          {s.variability_cents > 0 ? (
            <span className="text-12 text-text-dim"> ± {formatCents(s.variability_cents)}</span>
          ) : null}
        </span>
        <Button variant="quiet" onClick={onEdit}>
          Edit
        </Button>
        <Button
          variant="quiet"
          onClick={() => {
            setOpen(!open);
          }}
        >
          {open ? "Hide receipts" : "Receipts"}
        </Button>
      </div>
      <p className="text-12 text-text-dim">
        {s.cycle} from {s.anchor_date}
        {s.cycle === "semimonthly"
          ? ` (days ${s.semimonthly_day_1 ?? "?"} and ${s.semimonthly_day_2 ?? "?"})`
          : ""}
        {" · "}
        {label(s.weekend_rule)} · deposit {accountName}
        {s.match_payee_contains ? ` · payee contains "${s.match_payee_contains}"` : ""}
        {next.data && next.data.length > 0 ? ` · next ${next.data.join(", ")}` : ""}
      </p>
      {open ? <Receipts stream={s} /> : null}
    </li>
  );
}

function Receipts({ stream }: { stream: IncomeStream }) {
  const receipts = useReceipts(stream.id);
  const remove = useRemoveReceipt();
  const record = useRecordReceipt();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [picking, setPicking] = useState<string | null>(null);
  const [dueDate, setDueDate] = useState("");
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  return (
    <div className="mt-1 flex flex-col gap-1 border-t border-line pt-2 text-12">
      {(receipts.data ?? []).length === 0 ? (
        <p className="text-text-dim">No receipt recorded yet.</p>
      ) : null}
      {(receipts.data ?? []).map((r) => (
        <p key={r.due_date} className="flex items-center gap-2">
          <span className="money">due {r.due_date}</span>
          <span className="text-text-dim">row {r.txn_id}</span>
          <Chip>{r.matched_by}</Chip>
          <Button
            variant="quiet"
            onClick={() => {
              remove.mutate({ streamId: stream.id, dueDate: r.due_date }, { onError: fail });
            }}
          >
            Unmatch
          </Button>
        </p>
      ))}
      <form
        className="flex items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (CIVIL_DATE.test(dueDate.trim())) setPicking(dueDate.trim());
        }}
      >
        <TextField
          label="Record a receipt for the occurrence due"
          mono
          className="w-44"
          value={dueDate}
          onChange={(e) => {
            setDueDate(e.target.value);
          }}
          placeholder="YYYY-MM-DD"
        />
        <Button type="submit" variant="secondary" disabled={!CIVIL_DATE.test(dueDate.trim())}>
          Pick the row…
        </Button>
      </form>
      {picking !== null ? (
        <RowPicker
          title={`Receipt of ${stream.name} due ${picking}`}
          accountId={stream.deposit_account_id}
          around={picking}
          inflows
          onPick={(row) => {
            record.mutate(
              { streamId: stream.id, dueDate: picking, txnId: row.id },
              {
                onSuccess: () => {
                  pushNotice({ tone: "positive", text: `Receipt recorded for ${picking}.` });
                  setPicking(null);
                  setDueDate("");
                },
                onError: fail,
              },
            );
          }}
          onClose={() => {
            setPicking(null);
          }}
        />
      ) : null}
    </div>
  );
}

/** Ledger rows on an account around a date, to pick a receipt or a payment by hand. */
function RowPicker({
  title,
  accountId,
  around,
  inflows,
  onPick,
  onClose,
}: {
  title: string;
  accountId: number | null;
  around: string;
  inflows: boolean;
  onPick: (row: LedgerRow) => void;
  onClose: () => void;
}) {
  const from = shiftDate(around, -30);
  const to = shiftDate(around, 30);
  const ledger = useLedger({
    account_ids: accountId === null ? [] : [accountId],
    category_ids: [],
    tag_names: [],
    venture_ids: [],
    needs_review: false,
    unclassified: false,
    flags_any: 0,
    date_from: from,
    date_to: to,
  });
  const rows = (ledger.data?.pages.flatMap((p) => p.rows) ?? []).filter((r) =>
    inflows ? r.amount_cents > 0 : r.amount_cents < 0,
  );
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={title}
      description={`${inflows ? "Inflows" : "Outflows"} posted ${from} … ${to}`}
      width="lg"
    >
      {rows.length === 0 ? (
        <p className="text-14 text-text-dim">No row in that window.</p>
      ) : (
        <ul className="flex max-h-96 flex-col gap-1 overflow-auto text-14">
          {rows.map((r) => (
            <li
              key={r.id}
              className="flex items-center gap-3 rounded-2 border border-line px-2 py-1"
            >
              <span className="money">{r.posted_date}</span>
              <span className="truncate">{r.account_name}</span>
              <span className="truncate text-text-dim" title={r.payee_raw}>
                {r.payee_norm}
              </span>
              <span className="ml-auto">
                <Money cents={r.amount_cents} />
              </span>
              <Button
                variant="primary"
                onClick={() => {
                  onPick(r);
                }}
              >
                This one
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}

/** Civil-date arithmetic for a picker window; the core validates every date it stores. */
function shiftDate(iso: string, days: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  const date = new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1));
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

function StreamDialog({
  stream,
  accounts,
  onClose,
}: {
  stream: IncomeStream | null;
  accounts: Account[];
  onClose: () => void;
}) {
  const create = useCreateIncomeStream();
  const update = useUpdateIncomeStream();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [name, setName] = useState(stream?.name ?? "");
  const [kind, setKind] = useState<IncomeKind>(stream?.kind ?? "base");
  const [cycle, setCycle] = useState<Cycle>(stream?.cycle ?? "biweekly");
  const [anchor, setAnchor] = useState(stream?.anchor_date ?? "");
  const [day1, setDay1] = useState(String(stream?.semimonthly_day_1 ?? 15));
  const [day2, setDay2] = useState(String(stream?.semimonthly_day_2 ?? 31));
  const [expected, setExpected] = useState(
    stream ? formatCents(stream.expected_net_cents, { symbol: false }) : "",
  );
  const [variability, setVariability] = useState(
    stream && stream.variability_cents > 0
      ? formatCents(stream.variability_cents, { symbol: false })
      : "",
  );
  const [confidence, setConfidence] = useState<Confidence>(stream?.confidence ?? "confirmed");
  const [weekend, setWeekend] = useState<WeekendRule>(
    stream?.weekend_rule ?? "previous_business_day",
  );
  const [account, setAccount] = useState(
    stream?.deposit_account_id == null ? "" : String(stream.deposit_account_id),
  );
  const [payee, setPayee] = useState(stream?.match_payee_contains ?? "");
  const [active, setActive] = useState(stream?.active ?? true);
  const [error, setError] = useState<string | null>(null);
  const expectedCents = parseCentsInput(expected);
  const varParsed = optionalCents(variability);
  const busy = create.isPending || update.isPending;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (expectedCents === null || varParsed.error !== null || !CIVIL_DATE.test(anchor.trim()))
      return;
    const input: IncomeInput = {
      name: name.trim(),
      kind,
      cycle,
      anchor_date: anchor.trim(),
      semimonthly_day_1: cycle === "semimonthly" ? optionalInt(day1) : null,
      semimonthly_day_2: cycle === "semimonthly" ? optionalInt(day2) : null,
      expected_net_cents: expectedCents,
      variability_cents: varParsed.cents ?? 0,
      confidence,
      weekend_rule: weekend,
      deposit_account_id: account === "" ? null : Number(account),
      match_payee_contains: payee.trim() === "" ? null : payee.trim().toLowerCase(),
      active,
    };
    const onError = (err: Error) => {
      setError(err.message);
    };
    const onSuccess = (saved: IncomeStream) => {
      pushNotice({ tone: "positive", text: `Income stream "${saved.name}" saved.` });
      onClose();
    };
    if (stream) update.mutate({ id: stream.id, input }, { onSuccess, onError });
    else create.mutate(input, { onSuccess, onError });
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={stream ? `Edit ${stream.name}` : "New income stream"}
      description="Only a confirmed stream enters the hero and the forecast; expected and rumored streams never do."
      width="lg"
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        <div className="grid grid-cols-3 gap-3">
          <TextField
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            autoFocus
          />
          <Select
            label="Kind"
            value={kind}
            onChange={(e) => {
              setKind(INCOME_KINDS.find((k) => k === e.target.value) ?? "base");
            }}
          >
            {INCOME_KINDS.map((k) => (
              <option key={k} value={k}>
                {label(k)}
              </option>
            ))}
          </Select>
          <Select
            label="Confidence"
            value={confidence}
            onChange={(e) => {
              setConfidence(CONFIDENCES.find((c) => c === e.target.value) ?? "confirmed");
            }}
          >
            {CONFIDENCES.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </Select>
          <Select
            label="Cycle"
            value={cycle}
            onChange={(e) => {
              setCycle(CYCLES.find((c) => c === e.target.value) ?? "biweekly");
            }}
          >
            {CYCLES.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </Select>
          <TextField
            label="Anchor date"
            mono
            value={anchor}
            onChange={(e) => {
              setAnchor(e.target.value);
            }}
            placeholder="YYYY-MM-DD"
            hint="a pay date the cycle counts from"
            error={anchor.trim() !== "" && !CIVIL_DATE.test(anchor.trim()) ? "YYYY-MM-DD" : null}
          />
          <Select
            label="Weekend rule"
            value={weekend}
            onChange={(e) => {
              setWeekend(
                WEEKEND_RULES.find((w) => w === e.target.value) ?? "previous_business_day",
              );
            }}
          >
            {WEEKEND_RULES.map((w) => (
              <option key={w} value={w}>
                {label(w)}
              </option>
            ))}
          </Select>
          {cycle === "semimonthly" ? (
            <>
              <TextField
                label="First day"
                mono
                value={day1}
                onChange={(e) => {
                  setDay1(e.target.value);
                }}
                hint="31 = last day"
              />
              <TextField
                label="Second day"
                mono
                value={day2}
                onChange={(e) => {
                  setDay2(e.target.value);
                }}
              />
              <span />
            </>
          ) : null}
          <TextField
            label="Expected net"
            mono
            value={expected}
            onChange={(e) => {
              setExpected(e.target.value);
            }}
            error={expected.trim() !== "" && expectedCents === null ? "not an amount" : null}
          />
          <TextField
            label="Variability (±)"
            mono
            value={variability}
            onChange={(e) => {
              setVariability(e.target.value);
            }}
            error={varParsed.error}
          />
          <Select
            label="Deposit account"
            value={account}
            onChange={(e) => {
              setAccount(e.target.value);
            }}
          >
            <option value="">any account</option>
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </Select>
          <TextField
            label="Payee contains"
            mono
            value={payee}
            onChange={(e) => {
              setPayee(e.target.value);
            }}
            hint="matches receipts by the normalised payee"
          />
          <Checkbox
            label="Active"
            checked={active}
            onChange={(e) => {
              setActive(e.target.checked);
            }}
          />
        </div>
        {error !== null ? <p className="text-12 text-negative">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            disabled={
              busy ||
              name.trim() === "" ||
              expectedCents === null ||
              !CIVIL_DATE.test(anchor.trim())
            }
          >
            {stream ? "Save" : "Create"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

// ---- obligations --------------------------------------------------------------------------------

function ObligationRow({
  obligation: o,
  accountName,
  onEdit,
}: {
  obligation: Obligation;
  accountName: string;
  onEdit: () => void;
}) {
  const [open, setOpen] = useState(false);
  const setStatus = useSetObligationStatus();
  const remove = useDeleteObligationCandidate();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const next = useNextOccurrences("obligation", o.id, 2);
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  const candidate = o.status === "candidate";
  return (
    <li
      className={`flex flex-col gap-1 rounded-2 border p-2 text-14 ${candidate ? "border-dashed border-line" : "border-line"}`}
    >
      <div className="flex items-center gap-2">
        <span className="font-medium">{o.name}</span>
        <Chip tone={candidate ? "warning" : "positive"}>{o.status}</Chip>
        <Chip>{label(o.kind)}</Chip>
        {o.autopay ? <Chip tone="info">autopay</Chip> : null}
        <span className="ml-auto whitespace-nowrap">
          <Money cents={-o.expected_cents} />
          {o.variability_cents > 0 ? (
            <span className="text-12 text-text-dim"> ± {formatCents(o.variability_cents)}</span>
          ) : null}
        </span>
        {candidate ? (
          <>
            <Button
              variant="primary"
              disabled={setStatus.isPending}
              onClick={() => {
                setStatus.mutate(
                  { id: o.id, status: "confirmed" },
                  {
                    onSuccess: () => {
                      pushNotice({
                        tone: "positive",
                        text: `${o.name} confirmed; its payments are matched now.`,
                      });
                    },
                    onError: fail,
                  },
                );
              }}
            >
              Confirm
            </Button>
            <Button
              variant="quiet"
              disabled={remove.isPending}
              onClick={() => {
                remove.mutate(o.id, { onError: fail });
              }}
            >
              Delete
            </Button>
          </>
        ) : (
          <Button
            variant="quiet"
            disabled={setStatus.isPending}
            onClick={() => {
              setStatus.mutate({ id: o.id, status: "retired" }, { onError: fail });
            }}
          >
            Retire
          </Button>
        )}
        <Button variant="quiet" onClick={onEdit}>
          Edit
        </Button>
        {candidate ? null : (
          <Button
            variant="quiet"
            onClick={() => {
              setOpen(!open);
            }}
          >
            {open ? "Hide payments" : "Payments"}
          </Button>
        )}
      </div>
      <p className="text-12 text-text-dim">
        {dueRuleText(o)} · from {accountName}
        {o.match_payee_contains ? ` · payee contains "${o.match_payee_contains}"` : ""}
        {next.data && next.data.length > 0 ? ` · next ${next.data.join(", ")}` : ""}
        {candidate && o.detected_from_json ? " · from recurring rows" : ""}
      </p>
      {open ? <Payments obligation={o} /> : null}
    </li>
  );
}

function Payments({ obligation }: { obligation: Obligation }) {
  const payments = usePayments(obligation.id);
  const remove = useRemovePayment();
  const record = useRecordPayment();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [picking, setPicking] = useState<string | null>(null);
  const [dueDate, setDueDate] = useState("");
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  return (
    <div className="mt-1 flex flex-col gap-1 border-t border-line pt-2 text-12">
      {(payments.data ?? []).length === 0 ? (
        <p className="text-text-dim">No payment recorded yet.</p>
      ) : null}
      {(payments.data ?? []).map((p) => (
        <p key={p.due_date} className="flex items-center gap-2">
          <span className="money">due {p.due_date}</span>
          <span className="text-text-dim">row {p.txn_id}</span>
          <Chip>{p.matched_by}</Chip>
          <Button
            variant="quiet"
            onClick={() => {
              remove.mutate(
                { obligationId: obligation.id, dueDate: p.due_date },
                { onError: fail },
              );
            }}
          >
            Unmatch
          </Button>
        </p>
      ))}
      <form
        className="flex items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (CIVIL_DATE.test(dueDate.trim())) setPicking(dueDate.trim());
        }}
      >
        <TextField
          label="Record a payment for the occurrence due"
          mono
          className="w-44"
          value={dueDate}
          onChange={(e) => {
            setDueDate(e.target.value);
          }}
          placeholder="YYYY-MM-DD"
        />
        <Button type="submit" variant="secondary" disabled={!CIVIL_DATE.test(dueDate.trim())}>
          Pick the row…
        </Button>
      </form>
      {picking !== null ? (
        <RowPicker
          title={`Payment of ${obligation.name} due ${picking}`}
          accountId={obligation.source_account_id}
          around={picking}
          inflows={false}
          onPick={(row) => {
            record.mutate(
              { obligationId: obligation.id, dueDate: picking, txnId: row.id },
              {
                onSuccess: () => {
                  pushNotice({ tone: "positive", text: `Payment recorded for ${picking}.` });
                  setPicking(null);
                  setDueDate("");
                },
                onError: fail,
              },
            );
          }}
          onClose={() => {
            setPicking(null);
          }}
        />
      ) : null}
    </div>
  );
}

function ObligationDialog({
  obligation,
  accounts,
  categories,
  onClose,
}: {
  obligation: Obligation | null;
  accounts: Account[];
  categories: Category[];
  onClose: () => void;
}) {
  const create = useCreateObligation();
  const update = useUpdateObligation();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [name, setName] = useState(obligation?.name ?? "");
  const [kind, setKind] = useState<ObligationKind>(obligation?.kind ?? "bill");
  const [rule, setRule] = useState<DueRule>(obligation?.due_rule ?? "monthly_day");
  const [day, setDay] = useState(String(obligation?.due_day ?? 1));
  const [month, setMonth] = useState(String(obligation?.due_month ?? 1));
  const [weekday, setWeekday] = useState(String(obligation?.due_weekday ?? 0));
  const [nth, setNth] = useState(String(obligation?.due_nth ?? 1));
  const [anchor, setAnchor] = useState(obligation?.anchor_date ?? "");
  const [expected, setExpected] = useState(
    obligation ? formatCents(obligation.expected_cents, { symbol: false }) : "",
  );
  const [variability, setVariability] = useState(
    obligation && obligation.variability_cents > 0
      ? formatCents(obligation.variability_cents, { symbol: false })
      : "",
  );
  const [account, setAccount] = useState(obligation ? String(obligation.source_account_id) : "");
  const [autopay, setAutopay] = useState(obligation?.autopay ?? false);
  const [category, setCategory] = useState(
    obligation?.category_id == null ? "" : String(obligation.category_id),
  );
  const [payee, setPayee] = useState(obligation?.match_payee_contains ?? "");
  const [error, setError] = useState<string | null>(null);
  const expectedCents = parseCentsInput(expected);
  const varParsed = optionalCents(variability);
  const needsAnchor = rule === "biweekly" || rule === "once";
  const busy = create.isPending || update.isPending;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (expectedCents === null || varParsed.error !== null || account === "") return;
    const input: ObligationInput = {
      name: name.trim(),
      kind,
      status: obligation?.status ?? "confirmed",
      due_rule: rule,
      due_day: rule === "monthly_day" || rule === "annual" ? optionalInt(day) : null,
      due_month: rule === "annual" ? optionalInt(month) : null,
      due_weekday: rule === "nth_weekday" ? optionalInt(weekday) : null,
      due_nth: rule === "nth_weekday" ? optionalInt(nth) : null,
      anchor_date: needsAnchor ? anchor.trim() : null,
      expected_cents: expectedCents,
      variability_cents: varParsed.cents ?? 0,
      source_account_id: Number(account),
      autopay,
      category_id: category === "" ? null : Number(category),
      debt_id: obligation?.debt_id ?? null,
      match_payee_contains: payee.trim() === "" ? null : payee.trim().toLowerCase(),
    };
    const onError = (err: Error) => {
      setError(err.message);
    };
    const onSuccess = (saved: Obligation) => {
      pushNotice({ tone: "positive", text: `Obligation "${saved.name}" saved.` });
      onClose();
    };
    if (obligation) update.mutate({ id: obligation.id, input }, { onSuccess, onError });
    else create.mutate(input, { onSuccess, onError });
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={obligation ? `Edit ${obligation.name}` : "New obligation"}
      description="A confirmed obligation's unpaid occurrences due before the next confirmed income are subtracted from the hero; payments are matched from the ledger."
      width="lg"
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        <div className="grid grid-cols-3 gap-3">
          <TextField
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            autoFocus
          />
          <Select
            label="Kind"
            value={kind}
            onChange={(e) => {
              setKind(OBLIGATION_KINDS.find((k) => k === e.target.value) ?? "bill");
            }}
          >
            {OBLIGATION_KINDS.map((k) => (
              <option key={k} value={k}>
                {label(k)}
              </option>
            ))}
          </Select>
          <Select
            label="Due rule"
            value={rule}
            onChange={(e) => {
              setRule(DUE_RULES.find((r) => r === e.target.value) ?? "monthly_day");
            }}
          >
            {DUE_RULES.map((r) => (
              <option key={r} value={r}>
                {label(r)}
              </option>
            ))}
          </Select>
          {rule === "monthly_day" || rule === "annual" ? (
            <TextField
              label="Day of month"
              mono
              value={day}
              onChange={(e) => {
                setDay(e.target.value);
              }}
              hint="31 = last day"
            />
          ) : null}
          {rule === "annual" ? (
            <TextField
              label="Month (1–12)"
              mono
              value={month}
              onChange={(e) => {
                setMonth(e.target.value);
              }}
            />
          ) : null}
          {rule === "nth_weekday" ? (
            <>
              <Select
                label="Weekday"
                value={weekday}
                onChange={(e) => {
                  setWeekday(e.target.value);
                }}
              >
                {WEEKDAYS.map((w, i) => (
                  <option key={w} value={i}>
                    {w}
                  </option>
                ))}
              </Select>
              <TextField
                label="Nth (5 = last)"
                mono
                value={nth}
                onChange={(e) => {
                  setNth(e.target.value);
                }}
              />
            </>
          ) : null}
          {needsAnchor ? (
            <TextField
              label={rule === "once" ? "Date" : "Anchor date"}
              mono
              value={anchor}
              onChange={(e) => {
                setAnchor(e.target.value);
              }}
              placeholder="YYYY-MM-DD"
            />
          ) : null}
          <TextField
            label="Expected"
            mono
            value={expected}
            onChange={(e) => {
              setExpected(e.target.value);
            }}
            error={expected.trim() !== "" && expectedCents === null ? "not an amount" : null}
          />
          <TextField
            label="Variability (±)"
            mono
            value={variability}
            onChange={(e) => {
              setVariability(e.target.value);
            }}
            error={varParsed.error}
          />
          <Select
            label="Paid from"
            value={account}
            onChange={(e) => {
              setAccount(e.target.value);
            }}
          >
            <option value="">choose an account</option>
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </Select>
          <Select
            label="Category"
            value={category}
            onChange={(e) => {
              setCategory(e.target.value);
            }}
          >
            <option value="">none</option>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.path}
              </option>
            ))}
          </Select>
          <TextField
            label="Payee contains"
            mono
            value={payee}
            onChange={(e) => {
              setPayee(e.target.value);
            }}
            hint="matches payments by the normalised payee"
          />
          <Checkbox
            label="Autopay"
            checked={autopay}
            onChange={(e) => {
              setAutopay(e.target.checked);
            }}
          />
        </div>
        {error !== null ? <p className="text-12 text-negative">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            disabled={busy || name.trim() === "" || expectedCents === null || account === ""}
          >
            {obligation ? "Save" : "Create"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

// ---- earmarks -----------------------------------------------------------------------------------

function EarmarkRow({
  earmark: e,
  accountName,
  obligationName,
  onEdit,
}: {
  earmark: Earmark;
  accountName: string;
  obligationName: string | null;
  onEdit: () => void;
}) {
  const [open, setOpen] = useState(false);
  const entries = useEarmarkEntries(open ? e.id : null);
  const update = useUpdateEarmark();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const remaining = (entries.data ?? []).reduce((acc, en) => acc + en.amount_cents, 0);
  return (
    <li
      className={`flex flex-col gap-1 rounded-2 border border-line p-2 text-14 ${e.active ? "" : "text-text-dim"}`}
    >
      <div className="flex items-center gap-2">
        <span className="font-medium">{e.name}</span>
        <Chip tone="info">{label(e.kind)}</Chip>
        {e.active ? null : <Chip>inactive</Chip>}
        <span className="ml-auto text-12 text-text-dim">target</span>
        <Money cents={e.target_cents} />
        <Button variant="quiet" onClick={onEdit}>
          Edit
        </Button>
        <Button
          variant="quiet"
          onClick={() => {
            update.mutate(
              { id: e.id, input: toEarmarkInput(e, { active: !e.active }) },
              {
                onError: (error) => {
                  pushNotice({ tone: "negative", text: error.message });
                },
              },
            );
          }}
        >
          {e.active ? "Deactivate" : "Activate"}
        </Button>
        <Button
          variant="quiet"
          onClick={() => {
            setOpen(!open);
          }}
        >
          {open ? "Hide entries" : "Entries"}
        </Button>
      </div>
      <p className="text-12 text-text-dim">
        funded from {accountName}
        {obligationName ? ` · for ${obligationName}` : ""}
        {e.schedule === "none"
          ? ""
          : ` · ${label(e.schedule)}${e.schedule_amount_cents !== null ? ` ${formatCents(e.schedule_amount_cents)}` : ""}`}
        {e.target_date ? ` · by ${e.target_date}` : ""}
        {open && entries.data ? ` · remaining ${formatCents(remaining)}` : ""}
      </p>
      {open ? <Entries earmark={e} /> : null}
    </li>
  );
}

function toEarmarkInput(e: Earmark, overrides: Partial<EarmarkInput> = {}): EarmarkInput {
  return {
    name: e.name,
    kind: e.kind,
    funding_account_id: e.funding_account_id,
    obligation_id: e.obligation_id,
    target_cents: e.target_cents,
    target_date: e.target_date,
    schedule: e.schedule,
    schedule_amount_cents: e.schedule_amount_cents,
    schedule_day: e.schedule_day,
    schedule_income_stream_id: e.schedule_income_stream_id,
    active: e.active,
    ...overrides,
  };
}

function Entries({ earmark }: { earmark: Earmark }) {
  const entries = useEarmarkEntries(earmark.id);
  const add = useAddEarmarkEntry();
  const remove = useDeleteEarmarkEntry();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [date, setDate] = useState("");
  const [kind, setKind] = useState<EntryKind>("fund");
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const cents = parseCentsInput(amount);
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  return (
    <div className="mt-1 flex flex-col gap-1 border-t border-line pt-2 text-12">
      {(entries.data ?? []).length === 0 ? (
        <p className="text-text-dim">No entry: remaining is 0.00.</p>
      ) : null}
      {(entries.data ?? []).map((en) => (
        <p key={en.id} className="flex items-center gap-2">
          <span className="money">{en.entry_date}</span>
          <Chip>{en.kind}</Chip>
          <Money cents={en.amount_cents} />
          <span className="truncate text-text-dim">{en.note}</span>
          {en.txn_id !== null ? <span className="text-text-dim">row {en.txn_id}</span> : null}
          <Button
            variant="quiet"
            className="ml-auto"
            onClick={() => {
              remove.mutate(en.id, { onError: fail });
            }}
          >
            Remove
          </Button>
        </p>
      ))}
      <form
        className="flex items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (cents === null || !CIVIL_DATE.test(date.trim())) return;
          const signed =
            kind === "release" ? -Math.abs(cents) : kind === "fund" ? Math.abs(cents) : cents;
          add.mutate(
            {
              earmarkId: earmark.id,
              input: { entry_date: date.trim(), kind, amount_cents: signed, note },
            },
            {
              onSuccess: () => {
                setAmount("");
                setNote("");
              },
              onError: fail,
            },
          );
        }}
      >
        <TextField
          label="Date"
          mono
          className="w-36"
          value={date}
          onChange={(e) => {
            setDate(e.target.value);
          }}
          placeholder="YYYY-MM-DD"
        />
        <Select
          label="Kind"
          value={kind}
          onChange={(e) => {
            setKind(ENTRY_KINDS.find((k) => k === e.target.value) ?? "fund");
          }}
        >
          {ENTRY_KINDS.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </Select>
        <TextField
          label="Amount"
          mono
          className="w-32"
          value={amount}
          onChange={(e) => {
            setAmount(e.target.value);
          }}
          error={amount.trim() !== "" && cents === null ? "not an amount" : null}
        />
        <TextField
          label="Note"
          className="flex-1"
          value={note}
          onChange={(e) => {
            setNote(e.target.value);
          }}
        />
        <Button
          type="submit"
          variant="secondary"
          disabled={add.isPending || cents === null || !CIVIL_DATE.test(date.trim())}
        >
          Add
        </Button>
      </form>
    </div>
  );
}

function EarmarkDialog({
  earmark,
  accounts,
  obligations,
  streams,
  onClose,
}: {
  earmark: Earmark | null;
  accounts: Account[];
  obligations: Obligation[];
  streams: IncomeStream[];
  onClose: () => void;
}) {
  const create = useCreateEarmark();
  const update = useUpdateEarmark();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [name, setName] = useState(earmark?.name ?? "");
  const [kind, setKind] = useState<EarmarkKind>(earmark?.kind ?? "sinking_fund");
  const [account, setAccount] = useState(earmark ? String(earmark.funding_account_id) : "");
  const [obligation, setObligation] = useState(
    earmark?.obligation_id == null ? "" : String(earmark.obligation_id),
  );
  const [target, setTarget] = useState(
    earmark ? formatCents(earmark.target_cents, { symbol: false }) : "",
  );
  const [targetDate, setTargetDate] = useState(earmark?.target_date ?? "");
  const [schedule, setSchedule] = useState<EarmarkSchedule>(earmark?.schedule ?? "none");
  const [scheduleAmount, setScheduleAmount] = useState(
    earmark?.schedule_amount_cents == null
      ? ""
      : formatCents(earmark.schedule_amount_cents, { symbol: false }),
  );
  const [scheduleDay, setScheduleDay] = useState(String(earmark?.schedule_day ?? 1));
  const [stream, setStream] = useState(
    earmark?.schedule_income_stream_id == null ? "" : String(earmark.schedule_income_stream_id),
  );
  const [error, setError] = useState<string | null>(null);
  const targetCents = parseCentsInput(target);
  const scheduleParsed = optionalCents(scheduleAmount);
  const busy = create.isPending || update.isPending;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (targetCents === null || scheduleParsed.error !== null || account === "") return;
    const input: EarmarkInput = {
      name: name.trim(),
      kind,
      funding_account_id: Number(account),
      obligation_id: kind === "obligation" && obligation !== "" ? Number(obligation) : null,
      target_cents: targetCents,
      target_date: targetDate.trim() === "" ? null : targetDate.trim(),
      schedule,
      schedule_amount_cents: schedule === "none" ? null : scheduleParsed.cents,
      schedule_day: schedule === "monthly" ? optionalInt(scheduleDay) : null,
      schedule_income_stream_id:
        schedule === "per_paycheck" && stream !== "" ? Number(stream) : null,
      active: earmark?.active ?? true,
    };
    const onError = (err: Error) => {
      setError(err.message);
    };
    const onSuccess = (saved: Earmark) => {
      pushNotice({
        tone: "positive",
        text: `Earmark "${saved.name}" saved. Add entries to fund it.`,
      });
      onClose();
    };
    if (earmark) update.mutate({ id: earmark.id, input }, { onSuccess, onError });
    else create.mutate(input, { onSuccess, onError });
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={earmark ? `Edit ${earmark.name}` : "New earmark"}
      description="Remaining is the sum of its entries; the hero subtracts it once when the funding account is one of the cash accounts."
      width="lg"
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        <div className="grid grid-cols-3 gap-3">
          <TextField
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            autoFocus
          />
          <Select
            label="Kind"
            value={kind}
            onChange={(e) => {
              setKind(EARMARK_KINDS.find((k) => k === e.target.value) ?? "sinking_fund");
            }}
          >
            {EARMARK_KINDS.map((k) => (
              <option key={k} value={k}>
                {label(k)}
              </option>
            ))}
          </Select>
          <Select
            label="Funding account"
            value={account}
            onChange={(e) => {
              setAccount(e.target.value);
            }}
          >
            <option value="">choose an account</option>
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </Select>
          {kind === "obligation" ? (
            <Select
              label="Obligation"
              value={obligation}
              onChange={(e) => {
                setObligation(e.target.value);
              }}
            >
              <option value="">choose an obligation</option>
              {obligations.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </Select>
          ) : null}
          <TextField
            label="Target"
            mono
            value={target}
            onChange={(e) => {
              setTarget(e.target.value);
            }}
            error={target.trim() !== "" && targetCents === null ? "not an amount" : null}
          />
          <TextField
            label="Target date"
            mono
            value={targetDate}
            onChange={(e) => {
              setTargetDate(e.target.value);
            }}
            placeholder="YYYY-MM-DD"
          />
          <Select
            label="Schedule"
            value={schedule}
            onChange={(e) => {
              setSchedule(SCHEDULES.find((s) => s === e.target.value) ?? "none");
            }}
          >
            {SCHEDULES.map((s) => (
              <option key={s} value={s}>
                {label(s)}
              </option>
            ))}
          </Select>
          {schedule !== "none" ? (
            <TextField
              label="Schedule amount"
              mono
              value={scheduleAmount}
              onChange={(e) => {
                setScheduleAmount(e.target.value);
              }}
              error={scheduleParsed.error}
            />
          ) : null}
          {schedule === "monthly" ? (
            <TextField
              label="Day of month"
              mono
              value={scheduleDay}
              onChange={(e) => {
                setScheduleDay(e.target.value);
              }}
            />
          ) : null}
          {schedule === "per_paycheck" ? (
            <Select
              label="Income stream"
              value={stream}
              onChange={(e) => {
                setStream(e.target.value);
              }}
            >
              <option value="">choose a stream</option>
              {streams.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </Select>
          ) : null}
        </div>
        {error !== null ? <p className="text-12 text-negative">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            disabled={busy || name.trim() === "" || targetCents === null || account === ""}
          >
            {earmark ? "Save" : "Create"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
