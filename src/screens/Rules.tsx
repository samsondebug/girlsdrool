import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Chip } from "../components/Chip";
import { Dialog } from "../components/Dialog";
import { EmptyState } from "../components/EmptyState";
import { Select } from "../components/Select";
import { TextField } from "../components/TextField";
import type { Account, AutomationReport, Category, Rule, RuleInput, Venture } from "../lib/ipc";
import { formatCents, parseCentsInput } from "../lib/money";
import {
  useAccounts,
  useApplyRules,
  useCategories,
  useCreateRule,
  useDeleteRule,
  useReorderRules,
  useRules,
  useUpdateRule,
  useVentures,
} from "../lib/queries";
import { FLAG_BITS } from "../lib/query-chips";
import { useUiStore } from "../lib/store";

/** Flags a rule may set; review and heuristic-only flags are not offered. */
const RULE_FLAGS = ["borrowing", "securities_sale", "fee", "interest"];

function summarizeAutomation(r: AutomationReport): string {
  return `Considered ${r.considered} rows: rules place ${r.rule_hits}, heuristics ${r.heuristic_hits}, ${r.unclassified} stay in review; ${r.changed} changed. Linked ${r.transfers_linked} transfers and ${r.refunds_linked} refunds.`;
}

function flagNames(bits: number): string[] {
  return Object.entries(FLAG_BITS)
    .filter(([, bit]) => (bits & bit) !== 0)
    .map(([name]) => name);
}

function toInput(rule: Rule, overrides: Partial<RuleInput> = {}): RuleInput {
  return {
    name: rule.name,
    enabled: rule.enabled,
    match_payee_contains: rule.match_payee_contains,
    match_payee_regex: rule.match_payee_regex,
    match_memo_contains: rule.match_memo_contains,
    match_amount_min_cents: rule.match_amount_min_cents,
    match_amount_max_cents: rule.match_amount_max_cents,
    match_account_id: rule.match_account_id,
    action_category_id: rule.action_category_id,
    action_venture_id: rule.action_venture_id,
    action_flags: flagNames(rule.action_flags_set),
    action_tag_ids: rule.action_tag_ids,
    ...overrides,
  };
}

function matchSummary(rule: Rule, accounts: Account[]): string[] {
  const parts: string[] = [];
  if (rule.match_payee_contains !== null)
    parts.push(`payee contains "${rule.match_payee_contains}"`);
  if (rule.match_payee_regex !== null) parts.push(`payee matches /${rule.match_payee_regex}/`);
  if (rule.match_memo_contains !== null) parts.push(`memo contains "${rule.match_memo_contains}"`);
  if (rule.match_amount_min_cents !== null) {
    parts.push(`amount ≥ ${formatCents(rule.match_amount_min_cents)}`);
  }
  if (rule.match_amount_max_cents !== null) {
    parts.push(`amount ≤ ${formatCents(rule.match_amount_max_cents)}`);
  }
  if (rule.match_account_id !== null) {
    const name = accounts.find((a) => a.id === rule.match_account_id)?.name;
    parts.push(`account ${name ?? String(rule.match_account_id)}`);
  }
  return parts;
}

function actionSummary(rule: Rule, categories: Category[], ventures: Venture[]): string[] {
  const parts: string[] = [];
  if (rule.action_category_id !== null) {
    const path = categories.find((c) => c.id === rule.action_category_id)?.path;
    parts.push(path ?? `category ${String(rule.action_category_id)}`);
  }
  if (rule.action_venture_id !== null) {
    const name = ventures.find((v) => v.id === rule.action_venture_id)?.name;
    parts.push(`venture ${name ?? String(rule.action_venture_id)}`);
  }
  for (const name of flagNames(rule.action_flags_set)) parts.push(`flag ${name}`);
  return parts;
}

/** Ordered rules: first match wins. Edit, reorder, enable, delete, and run them over the ledger. */
export function Rules() {
  const rules = useRules();
  const categories = useCategories();
  const accounts = useAccounts();
  const ventures = useVentures();
  const reorder = useReorderRules();
  const remove = useDeleteRule();
  const create = useCreateRule();
  const apply = useApplyRules();
  const update = useUpdateRule();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [editing, setEditing] = useState<Rule | "new" | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<number | null>(null);

  const list = rules.data ?? [];
  const accountList = accounts.data ?? [];
  const categoryList = categories.data ?? [];
  const ventureList = ventures.data ?? [];
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };

  const swap = (index: number, delta: number) => {
    const ids = list.map((r) => r.id);
    const other = index + delta;
    const a = ids[index];
    const b = ids[other];
    if (a === undefined || b === undefined) return;
    ids[index] = b;
    ids[other] = a;
    reorder.mutate(ids, { onError: fail });
  };

  return (
    <div className="flex h-full flex-col gap-3 overflow-hidden p-4">
      <div className="flex items-center gap-3">
        <div>
          <h2 className="text-16 font-semibold">Rules</h2>
          <p className="text-12 text-text-dim">
            {list.length} {list.length === 1 ? "rule" : "rules"} · first match wins, top to bottom ·
            heuristics take the rows no rule matches · a correction in the ledger proposes a rule
            here
          </p>
        </div>
        <Button
          variant="secondary"
          className="ml-auto"
          disabled={apply.isPending}
          onClick={() => {
            apply.mutate(undefined, {
              onSuccess: (report) => {
                pushNotice({ tone: "positive", text: summarizeAutomation(report) });
              },
              onError: fail,
            });
          }}
        >
          Apply rules now
        </Button>
        <Button
          variant="primary"
          onClick={() => {
            setEditing("new");
          }}
        >
          New rule
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto rounded-2 border border-line bg-bg-raised">
        {rules.isPending ? (
          <p className="p-4 text-12 text-text-dim">Loading…</p>
        ) : list.length === 0 ? (
          <div className="p-4">
            <EmptyState
              missing="No rules yet."
              fix="Change a row's category in the Ledger or Review and accept the proposed rule, or add one with New rule."
            />
          </div>
        ) : (
          <table className="w-full border-collapse text-14">
            <thead className="sticky top-0 z-10 bg-bg-raised text-12 text-text-dim">
              <tr>
                {["#", "On", "Name", "When", "Then", "Hits", ""].map((h) => (
                  <th
                    key={h}
                    className={`border-b border-line px-2 py-1 text-left font-medium ${h === "Hits" ? "text-right" : ""}`}
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {list.map((rule, index) => (
                <tr
                  key={rule.id}
                  className={`border-b border-line ${rule.enabled ? "" : "text-text-dim"}`}
                >
                  <td className="money px-2 py-1">{index + 1}</td>
                  <td className="px-2 py-1">
                    <input
                      type="checkbox"
                      aria-label={`Enable ${rule.name}`}
                      className="accent-accent"
                      checked={rule.enabled}
                      onChange={(e) => {
                        update.mutate(
                          { id: rule.id, input: toInput(rule, { enabled: e.target.checked }) },
                          { onError: fail },
                        );
                      }}
                    />
                  </td>
                  <td className="px-2 py-1 font-medium whitespace-nowrap">{rule.name}</td>
                  <td className="px-2 py-1">
                    <span className="flex flex-wrap gap-1">
                      {matchSummary(rule, accountList).map((m) => (
                        <Chip key={m}>{m}</Chip>
                      ))}
                    </span>
                  </td>
                  <td className="px-2 py-1">
                    <span className="flex flex-wrap gap-1">
                      {actionSummary(rule, categoryList, ventureList).map((m) => (
                        <Chip key={m} tone="info">
                          {m}
                        </Chip>
                      ))}
                    </span>
                  </td>
                  <td className="money px-2 py-1 text-right">{rule.hit_count}</td>
                  <td className="px-2 py-1">
                    <span className="flex justify-end gap-1">
                      <Button
                        variant="quiet"
                        aria-label={`Move ${rule.name} up`}
                        disabled={index === 0 || reorder.isPending}
                        onClick={() => {
                          swap(index, -1);
                        }}
                      >
                        ↑
                      </Button>
                      <Button
                        variant="quiet"
                        aria-label={`Move ${rule.name} down`}
                        disabled={index === list.length - 1 || reorder.isPending}
                        onClick={() => {
                          swap(index, 1);
                        }}
                      >
                        ↓
                      </Button>
                      <Button
                        variant="quiet"
                        onClick={() => {
                          setEditing(rule);
                        }}
                      >
                        Edit
                      </Button>
                      {confirmDelete === rule.id ? (
                        <>
                          <Button
                            variant="danger"
                            disabled={remove.isPending}
                            onClick={() => {
                              remove.mutate(rule.id, {
                                onSuccess: () => {
                                  pushNotice({
                                    tone: "info",
                                    text: `Deleted rule "${rule.name}". Rows it placed keep their category until rules run again.`,
                                    undo: {
                                      label: "Undo",
                                      run: () => {
                                        create.mutate(toInput(rule), { onError: fail });
                                      },
                                    },
                                  });
                                  setConfirmDelete(null);
                                },
                                onError: fail,
                              });
                            }}
                          >
                            Delete
                          </Button>
                          <Button
                            variant="quiet"
                            onClick={() => {
                              setConfirmDelete(null);
                            }}
                          >
                            Keep
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="quiet"
                          onClick={() => {
                            setConfirmDelete(rule.id);
                          }}
                        >
                          Delete…
                        </Button>
                      )}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {editing !== null ? (
        <RuleDialog
          rule={editing === "new" ? null : editing}
          accounts={accountList}
          categories={categoryList.filter((c) => !c.archived)}
          ventures={ventureList.filter((v) => !v.archived)}
          onClose={() => {
            setEditing(null);
          }}
        />
      ) : null}
    </div>
  );
}

function optionalCents(text: string): { cents: number | null; error: string | null } {
  if (text.trim() === "") return { cents: null, error: null };
  const cents = parseCentsInput(text);
  return cents === null ? { cents: null, error: "not an amount" } : { cents, error: null };
}

function optionalId(text: string): number | null {
  return text === "" ? null : Number(text);
}

interface RuleDialogProps {
  rule: Rule | null;
  accounts: Account[];
  categories: Category[];
  ventures: Venture[];
  onClose: () => void;
}

/** New or existing rule. The core requires at least one condition and one action. */
function RuleDialog({ rule, accounts, categories, ventures, onClose }: RuleDialogProps) {
  const create = useCreateRule();
  const update = useUpdateRule();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [name, setName] = useState(rule?.name ?? "");
  const [enabled, setEnabled] = useState(rule?.enabled ?? true);
  const [payee, setPayee] = useState(rule?.match_payee_contains ?? "");
  const [regex, setRegex] = useState(rule?.match_payee_regex ?? "");
  const [memo, setMemo] = useState(rule?.match_memo_contains ?? "");
  const [min, setMin] = useState(
    rule?.match_amount_min_cents == null
      ? ""
      : formatCents(rule.match_amount_min_cents, { symbol: false }),
  );
  const [max, setMax] = useState(
    rule?.match_amount_max_cents == null
      ? ""
      : formatCents(rule.match_amount_max_cents, { symbol: false }),
  );
  const [account, setAccount] = useState(
    rule?.match_account_id == null ? "" : String(rule.match_account_id),
  );
  const [category, setCategory] = useState(
    rule?.action_category_id == null ? "" : String(rule.action_category_id),
  );
  const [venture, setVenture] = useState(
    rule?.action_venture_id == null ? "" : String(rule.action_venture_id),
  );
  const [flags, setFlags] = useState<string[]>(rule ? flagNames(rule.action_flags_set) : []);
  const [error, setError] = useState<string | null>(null);

  const minParsed = optionalCents(min);
  const maxParsed = optionalCents(max);
  const busy = create.isPending || update.isPending;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (minParsed.error !== null || maxParsed.error !== null) return;
    const input: RuleInput = {
      name: name.trim(),
      enabled,
      match_payee_contains: payee.trim() === "" ? null : payee.trim().toLowerCase(),
      match_payee_regex: regex.trim() === "" ? null : regex.trim(),
      match_memo_contains: memo.trim() === "" ? null : memo.trim().toLowerCase(),
      match_amount_min_cents: minParsed.cents,
      match_amount_max_cents: maxParsed.cents,
      match_account_id: optionalId(account),
      action_category_id: optionalId(category),
      action_venture_id: optionalId(venture),
      action_flags: flags,
      action_tag_ids: rule?.action_tag_ids ?? [],
    };
    const onError = (err: Error) => {
      setError(err.message);
    };
    if (rule) {
      update.mutate(
        { id: rule.id, input },
        {
          onSuccess: (saved) => {
            pushNotice({ tone: "positive", text: `Rule "${saved.name}" saved.` });
            onClose();
          },
          onError,
        },
      );
    } else {
      create.mutate(input, {
        onSuccess: (saved) => {
          pushNotice({
            tone: "positive",
            text: `Rule "${saved.name}" created at position ${saved.position}. Apply rules now runs it over the ledger.`,
          });
          onClose();
        },
        onError,
      });
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={rule ? `Edit rule: ${rule.name}` : "New rule"}
      description="Conditions are matched against the normalised payee and memo (lowercase); every condition given must hold."
      width="lg"
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <div className="grid grid-cols-[1fr_auto] items-end gap-3">
          <TextField
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            autoFocus
          />
          <Checkbox
            label="Enabled"
            checked={enabled}
            onChange={(e) => {
              setEnabled(e.target.checked);
            }}
          />
        </div>
        <fieldset className="grid grid-cols-3 gap-3 rounded-2 border border-line p-3">
          <legend className="px-1 text-12 text-text-dim">When</legend>
          <TextField
            label="Payee contains"
            value={payee}
            mono
            onChange={(e) => {
              setPayee(e.target.value);
            }}
          />
          <TextField
            label="Payee matches regex"
            value={regex}
            mono
            onChange={(e) => {
              setRegex(e.target.value);
            }}
          />
          <TextField
            label="Memo contains"
            value={memo}
            mono
            onChange={(e) => {
              setMemo(e.target.value);
            }}
          />
          <TextField
            label="Amount at least"
            hint="signed: outflows are negative"
            value={min}
            mono
            onChange={(e) => {
              setMin(e.target.value);
            }}
            error={minParsed.error}
          />
          <TextField
            label="Amount at most"
            value={max}
            mono
            onChange={(e) => {
              setMax(e.target.value);
            }}
            error={maxParsed.error}
          />
          <Select
            label="Account"
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
        </fieldset>
        <fieldset className="grid grid-cols-3 gap-3 rounded-2 border border-line p-3">
          <legend className="px-1 text-12 text-text-dim">Then</legend>
          <Select
            label="Category"
            value={category}
            onChange={(e) => {
              setCategory(e.target.value);
            }}
          >
            <option value="">leave the category</option>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.path}
              </option>
            ))}
          </Select>
          <Select
            label="Venture"
            value={venture}
            onChange={(e) => {
              setVenture(e.target.value);
            }}
          >
            <option value="">no venture</option>
            {ventures.map((v) => (
              <option key={v.id} value={v.id}>
                {v.name}
              </option>
            ))}
          </Select>
          <div className="flex flex-col gap-1">
            <span className="text-12 text-text-dim">Flags to set</span>
            {RULE_FLAGS.map((flag) => (
              <Checkbox
                key={flag}
                label={flag.replace(/_/g, " ")}
                checked={flags.includes(flag)}
                onChange={(e) => {
                  setFlags(e.target.checked ? [...flags, flag] : flags.filter((f) => f !== flag));
                }}
              />
            ))}
          </div>
        </fieldset>
        {error !== null ? <p className="text-12 text-negative">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={busy || name.trim() === ""}>
            {rule ? "Save" : "Create"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
