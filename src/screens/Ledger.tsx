import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
  type RowSelectionState,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { Dialog } from "../components/Dialog";
import { EmptyState } from "../components/EmptyState";
import { LinkDialog } from "../components/LinkDialog";
import { Money } from "../components/Money";
import { Select } from "../components/Select";
import { TextField } from "../components/TextField";
import { MarkedMoney } from "../components/Untrusted";
import { useCorrectionProposal } from "../lib/corrections";
import type { Category, LedgerRow, SplitPart } from "../lib/ipc";
import { formatCents, parseCentsInput } from "../lib/money";
import {
  useAccounts,
  useCategories,
  useDeleteView,
  useLedger,
  useRecategorize,
  useSaveView,
  useSavedViews,
  useSplit,
  useTrust,
  useUnsplit,
  useUpdateTxn,
} from "../lib/queries";
import { compileQuery, FLAG_BITS } from "../lib/query-chips";
import { useUiStore } from "../lib/store";
import { untrustedAmong } from "../lib/trust";

const ROW_HEIGHT = 32;

/** Short chip labels; the full flag name is the chip's title. */
const FLAG_LABELS: Record<string, string> = {
  needs_review: "review",
  cash_withdrawal: "cash",
  payment_app_unknown: "pay app",
  borrowing: "borrowing",
  securities_sale: "sec. sale",
  fee: "fee",
  interest: "interest",
};

function flagChips(bits: number): { name: string; label: string }[] {
  return Object.entries(FLAG_BITS)
    .filter(([, bit]) => (bits & bit) !== 0)
    .map(([name]) => ({ name, label: FLAG_LABELS[name] ?? name }));
}

/** The ledger: virtualized, inline edits, multi-select recategorize, splits, saved views. */
export function Ledger() {
  const accounts = useAccounts();
  const categories = useCategories();
  const savedViews = useSavedViews();
  const trust = useTrust();
  const queryText = useUiStore((s) => s.ledgerQuery);
  const setQueryText = useUiStore((s) => s.setLedgerQuery);
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [draft, setDraft] = useState(queryText);
  const [selection, setSelection] = useState<RowSelectionState>({});
  const [splitting, setSplitting] = useState<LedgerRow | null>(null);
  const [linking, setLinking] = useState<LedgerRow | null>(null);
  const [savingView, setSavingView] = useState(false);
  const propose = useCorrectionProposal();

  const lookups = useMemo(
    () => ({
      accounts: (accounts.data ?? []).map((a) => ({ id: a.id, name: a.name })),
      categories: (categories.data ?? []).map((c) => ({ id: c.id, name: c.name, path: c.path })),
      ventures: [],
    }),
    [accounts.data, categories.data],
  );
  const compiled = useMemo(() => compileQuery(queryText, lookups), [queryText, lookups]);
  const ledger = useLedger(compiled.filter);
  const rows = useMemo(() => ledger.data?.pages.flatMap((p) => p.rows) ?? [], [ledger.data]);
  const first = ledger.data?.pages[0];

  const updateTxn = useUpdateTxn();
  const recategorize = useRecategorize();
  const unsplit = useUnsplit();
  const saveView = useSaveView();
  const deleteView = useDeleteView();

  const applyQuery = (event: FormEvent) => {
    event.preventDefault();
    setQueryText(draft.trim());
    setSelection({});
  };

  const categoryOptions = useMemo(
    () => (categories.data ?? []).filter((c) => !c.archived),
    [categories.data],
  );

  const columns = useMemo<ColumnDef<LedgerRow>[]>(
    () => [
      {
        id: "select",
        header: ({ table }) => (
          <input
            type="checkbox"
            aria-label="Select all loaded rows"
            className="accent-accent"
            checked={table.getIsAllRowsSelected()}
            onChange={table.getToggleAllRowsSelectedHandler()}
          />
        ),
        cell: ({ row }) => (
          <input
            type="checkbox"
            aria-label={`Select row ${row.original.id}`}
            className="accent-accent"
            checked={row.getIsSelected()}
            onChange={row.getToggleSelectedHandler()}
          />
        ),
        size: 32,
      },
      {
        id: "date",
        header: "Posted",
        cell: ({ row }) => (
          <span className="money" title={`effective ${row.original.effective_date}`}>
            {row.original.posted_date}
          </span>
        ),
        size: 104,
      },
      {
        id: "account",
        header: "Account",
        cell: ({ row }) => <span className="truncate">{row.original.account_name}</span>,
        size: 160,
      },
      {
        id: "payee",
        header: "Payee",
        cell: ({ row }) => (
          <InlineText
            value={row.original.payee_norm}
            title={row.original.payee_raw}
            edited={(row.original.user_edited & 1) !== 0}
            onSave={(payee_norm) => {
              updateTxn.mutate({ id: row.original.id, patch: { payee_norm } });
            }}
          >
            {row.original.parent_id !== null ? (
              <span
                className="mr-1 text-text-dim"
                title={`part of split row ${row.original.parent_id}`}
              >
                ↳
              </span>
            ) : null}
          </InlineText>
        ),
        size: 260,
      },
      {
        id: "memo",
        header: "Memo",
        cell: ({ row }) => (
          <InlineText
            value={row.original.memo}
            edited={(row.original.user_edited & 2) !== 0}
            placeholder="—"
            onSave={(memo) => {
              updateTxn.mutate({ id: row.original.id, patch: { memo } });
            }}
          />
        ),
        size: 200,
      },
      {
        id: "category",
        header: "Category",
        cell: ({ row }) => (
          <Select
            label="Category"
            compact
            value={row.original.category_id ?? ""}
            className="w-full"
            onChange={(e) => {
              const value = e.target.value;
              const categoryId = value === "" ? null : Number(value);
              updateTxn.mutate(
                { id: row.original.id, patch: { category_id: categoryId } },
                {
                  onSuccess: () => {
                    const path = categoryOptions.find((c) => c.id === categoryId)?.path;
                    if (categoryId !== null && path !== undefined) {
                      propose(row.original.id, path);
                    }
                  },
                  onError: (error) => {
                    pushNotice({ tone: "negative", text: error.message });
                  },
                },
              );
            }}
          >
            <option value="">— unclassified —</option>
            {categoryOptions.map((c) => (
              <option key={c.id} value={c.id}>
                {c.path}
              </option>
            ))}
          </Select>
        ),
        size: 220,
      },
      {
        id: "amount",
        header: () => <span className="block text-right">Amount</span>,
        cell: ({ row }) => (
          <span className="block text-right">
            <Money cents={row.original.amount_cents} />
          </span>
        ),
        size: 120,
      },
      {
        id: "state",
        header: "State",
        cell: ({ row }) => (
          <span className="flex flex-wrap gap-1">
            {row.original.status === "pending" ? <Chip tone="info">pending</Chip> : null}
            {flagChips(row.original.flags).map((f) => (
              <Chip
                key={f.name}
                tone={f.name === "needs_review" ? "warning" : "dim"}
                title={f.name.replace(/_/g, " ")}
              >
                {f.label}
              </Chip>
            ))}
            {row.original.transfer_link_id !== null ? <Chip tone="positive">transfer</Chip> : null}
            {row.original.refund_link_id !== null ? <Chip tone="positive">refund</Chip> : null}
            {row.original.tags.map((t) => (
              <Chip key={t}>#{t}</Chip>
            ))}
          </span>
        ),
        size: 220,
      },
      {
        id: "actions",
        header: "",
        cell: ({ row }) =>
          row.original.parent_id === null ? (
            <span className="flex gap-1">
              <Button
                variant="quiet"
                onClick={() => {
                  setLinking(row.original);
                }}
              >
                Link
              </Button>
              <Button
                variant="quiet"
                onClick={() => {
                  setSplitting(row.original);
                }}
              >
                Split
              </Button>
            </span>
          ) : (
            <Button
              variant="quiet"
              onClick={() => {
                const parentId = row.original.parent_id ?? 0;
                unsplit.mutate(parentId, {
                  onSuccess: (n) => {
                    pushNotice({
                      tone: "info",
                      text: `Unsplit row ${parentId}: ${n} parts removed.`,
                    });
                  },
                  onError: (error) => {
                    pushNotice({ tone: "negative", text: error.message });
                  },
                });
              }}
            >
              Unsplit
            </Button>
          ),
        size: 130,
      },
    ],
    [categoryOptions, propose, pushNotice, unsplit, updateTxn],
  );

  const table = useReactTable({
    data: rows,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (r) => String(r.id),
    state: { rowSelection: selection },
    onRowSelectionChange: setSelection,
    enableRowSelection: true,
  });

  const scrollRef = useRef<HTMLDivElement>(null);
  const tableRows = table.getRowModel().rows;
  const virtualizer = useVirtualizer({
    count: tableRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const lastIndex = virtualItems.at(-1)?.index ?? -1;
  useEffect(() => {
    if (lastIndex >= tableRows.length - 40 && ledger.hasNextPage && !ledger.isFetchingNextPage) {
      void ledger.fetchNextPage();
    }
  }, [lastIndex, tableRows.length, ledger]);

  const selectedIds = Object.keys(selection)
    .filter((k) => selection[k])
    .map(Number);
  const [bulkCategory, setBulkCategory] = useState("");

  const applyBulk = () => {
    const categoryId = bulkCategory === "" ? null : Number(bulkCategory);
    const ids = selectedIds;
    recategorize.mutate(
      { ids, categoryId },
      {
        onSuccess: (n) => {
          pushNotice({ tone: "positive", text: `Recategorized ${n} rows.` });
          setSelection({});
        },
        onError: (error) => {
          pushNotice({ tone: "negative", text: error.message });
        },
      },
    );
  };

  return (
    <div className="flex h-full flex-col gap-2 overflow-hidden p-4">
      <form onSubmit={applyQuery} className="flex items-end gap-2">
        <TextField
          label="Query"
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
          }}
          placeholder='account:"Northbank Checking" cat:groceries >100 needs:review flag:borrowing date:2026-09'
          className="flex-1"
          mono
          error={compiled.errors.length > 0 ? compiled.errors.join("; ") : null}
        />
        <Button type="submit" variant="primary">
          Apply
        </Button>
        <Select
          label="Saved views"
          compact
          value=""
          onChange={(e) => {
            const view = savedViews.data?.find((v) => String(v.id) === e.target.value);
            if (view) {
              setDraft(view.query_text);
              setQueryText(view.query_text);
            }
          }}
        >
          <option value="">Saved views…</option>
          {(savedViews.data ?? []).map((v) => (
            <option key={v.id} value={v.id}>
              {v.name}
            </option>
          ))}
        </Select>
        <Button
          variant="secondary"
          onClick={() => {
            setSavingView(true);
          }}
          disabled={queryText.trim() === ""}
        >
          Save view
        </Button>
      </form>

      <div className="flex items-center gap-3 text-12 text-text-dim">
        {first ? (
          <>
            <span>
              {first.total_rows.toLocaleString("en-US")} rows · Σ{" "}
              <MarkedMoney
                cents={first.total_cents}
                tone={false}
                untrustedBy={untrustedAmong(
                  trust.data,
                  compiled.filter.account_ids.length > 0 ? compiled.filter.account_ids : null,
                )}
              />
            </span>
            <span>{rows.length < first.total_rows ? `${rows.length} loaded` : "all loaded"}</span>
          </>
        ) : ledger.isPending ? (
          <span>Loading…</span>
        ) : null}
        {ledger.isError ? <span className="text-negative">{ledger.error.message}</span> : null}
        {selectedIds.length > 0 ? (
          <span className="ml-auto flex items-center gap-2">
            <span>{selectedIds.length} selected →</span>
            <Select
              label="Recategorize to"
              compact
              value={bulkCategory}
              onChange={(e) => {
                setBulkCategory(e.target.value);
              }}
            >
              <option value="">— unclassified —</option>
              {categoryOptions.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.path}
                </option>
              ))}
            </Select>
            <Button variant="primary" onClick={applyBulk} disabled={recategorize.isPending}>
              Recategorize
            </Button>
          </span>
        ) : null}
      </div>

      <div
        ref={scrollRef}
        className="min-h-0 flex-1 overflow-auto rounded-2 border border-line bg-bg-raised"
      >
        {first?.total_rows === 0 ? (
          <div className="p-4">
            <EmptyState
              missing={queryText ? "No rows match this query." : "The ledger is empty."}
              fix={
                queryText
                  ? "Change or clear the query chips above."
                  : "Import a statement (Import) after adding its account (Accounts)."
              }
            />
          </div>
        ) : (
          <table className="w-full border-collapse text-14" style={{ tableLayout: "fixed" }}>
            <thead className="sticky top-0 z-10 bg-bg-raised text-12 text-text-dim">
              {table.getHeaderGroups().map((hg) => (
                <tr key={hg.id}>
                  {hg.headers.map((h) => (
                    <th
                      key={h.id}
                      className="border-b border-line px-2 py-1 text-left font-medium"
                      style={{ width: h.getSize() }}
                    >
                      {h.isPlaceholder
                        ? null
                        : flexRender(h.column.columnDef.header, h.getContext())}
                    </th>
                  ))}
                </tr>
              ))}
            </thead>
            <tbody
              style={{ height: virtualizer.getTotalSize(), position: "relative", display: "block" }}
            >
              {virtualItems.map((item) => {
                const row = tableRows[item.index];
                if (!row) return null;
                return (
                  <tr
                    key={row.id}
                    data-index={item.index}
                    className={`absolute left-0 flex w-full items-center border-b border-line ${row.getIsSelected() ? "bg-bg-inset" : ""}`}
                    style={{ transform: `translateY(${item.start}px)`, height: ROW_HEIGHT }}
                  >
                    {row.getVisibleCells().map((cell) => (
                      <td
                        key={cell.id}
                        className="min-w-0 truncate px-2"
                        style={{ width: cell.column.getSize(), flex: "0 0 auto" }}
                      >
                        {flexRender(cell.column.columnDef.cell, cell.getContext())}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {splitting ? (
        <SplitDialog
          row={splitting}
          categories={categoryOptions}
          onClose={() => {
            setSplitting(null);
          }}
        />
      ) : null}
      {linking ? (
        <LinkDialog
          row={linking}
          onClose={() => {
            setLinking(null);
          }}
        />
      ) : null}

      <Dialog
        open={savingView}
        onOpenChange={setSavingView}
        title="Save this view"
        description={`Query: ${queryText}`}
      >
        <SaveViewForm
          onSave={(name) => {
            saveView.mutate(
              { name, queryText },
              {
                onSuccess: (view) => {
                  pushNotice({
                    tone: "positive",
                    text: `Saved view "${view.name}".`,
                    undo: {
                      label: "Undo",
                      run: () => {
                        deleteView.mutate(view.id);
                      },
                    },
                  });
                  setSavingView(false);
                },
                onError: (error) => {
                  pushNotice({ tone: "negative", text: error.message });
                },
              },
            );
          }}
        />
      </Dialog>
    </div>
  );
}

interface InlineTextProps {
  value: string;
  title?: string;
  placeholder?: string;
  edited: boolean;
  onSave: (value: string) => void;
  children?: React.ReactNode;
}

/** Click to edit; Enter saves, Escape cancels. A dot marks a user-edited field. */
function InlineText({ value, title, placeholder = "", edited, onSave, children }: InlineTextProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  if (editing) {
    return (
      <input
        autoFocus
        aria-label="Edit"
        className="h-6 w-full rounded-1 border border-accent bg-bg-inset px-1 text-14"
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onBlur={() => {
          setEditing(false);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            if (draft.trim() !== value) onSave(draft.trim());
            setEditing(false);
          } else if (e.key === "Escape") {
            setEditing(false);
          }
        }}
      />
    );
  }
  return (
    <button
      type="button"
      title={title ?? "Click to edit"}
      className="block w-full truncate text-left hover:text-accent"
      onClick={() => {
        setDraft(value);
        setEditing(true);
      }}
    >
      {children}
      {value === "" ? <span className="text-text-dim">{placeholder}</span> : value}
      {edited ? (
        <span className="ml-1 text-accent" title="edited by you; imports will not change it">
          •
        </span>
      ) : null}
    </button>
  );
}

function SaveViewForm({ onSave }: { onSave: (name: string) => void }) {
  const [name, setName] = useState("");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim() !== "") onSave(name.trim());
      }}
      className="flex items-end gap-2"
    >
      <TextField
        label="Name"
        value={name}
        onChange={(e) => {
          setName(e.target.value);
        }}
        className="flex-1"
        autoFocus
      />
      <Button type="submit" variant="primary">
        Save
      </Button>
    </form>
  );
}

interface SplitDialogProps {
  row: LedgerRow;
  categories: Category[];
  onClose: () => void;
}

/** Parts must sum to the row exactly; the core enforces it and the dialog shows the remainder. */
function SplitDialog({ row, categories, onClose }: SplitDialogProps) {
  const split = useSplit();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [parts, setParts] = useState<{ amount: string; category: string; memo: string }[]>([
    { amount: formatCents(row.amount_cents, { symbol: false }), category: "", memo: "" },
    { amount: "0.00", category: "", memo: "" },
  ]);
  const cents = parts.map((p) => parseCentsInput(p.amount));
  const valid = cents.every((c) => c !== null);
  const sum = cents.reduce<number>((acc, c) => acc + (c ?? 0), 0);
  const remainder = row.amount_cents - sum;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!valid || remainder !== 0) return;
    const payload: SplitPart[] = parts.map((p, i) => ({
      amount_cents: cents[i] ?? 0,
      category_id: p.category === "" ? null : Number(p.category),
      memo: p.memo,
    }));
    split.mutate(
      { parentId: row.id, parts: payload },
      {
        onSuccess: (children) => {
          pushNotice({
            tone: "positive",
            text: `Split row ${row.id} into ${children.length} parts.`,
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
      title={`Split ${row.payee_norm}`}
      description={`${row.posted_date} · ${formatCents(row.amount_cents)} · parts must sum to the row`}
      width="lg"
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        {parts.map((p, i) => (
          <div key={i} className="grid grid-cols-[140px_1fr_1fr_auto] items-end gap-2">
            <TextField
              label="Amount"
              value={p.amount}
              mono
              onChange={(e) => {
                setParts(parts.map((q, j) => (j === i ? { ...q, amount: e.target.value } : q)));
              }}
              error={cents[i] === null ? "not an amount" : null}
            />
            <Select
              label="Category"
              value={p.category}
              onChange={(e) => {
                setParts(parts.map((q, j) => (j === i ? { ...q, category: e.target.value } : q)));
              }}
            >
              <option value="">— unclassified —</option>
              {categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.path}
                </option>
              ))}
            </Select>
            <TextField
              label="Memo"
              value={p.memo}
              onChange={(e) => {
                setParts(parts.map((q, j) => (j === i ? { ...q, memo: e.target.value } : q)));
              }}
            />
            <Button
              variant="quiet"
              disabled={parts.length <= 2}
              onClick={() => {
                setParts(parts.filter((_, j) => j !== i));
              }}
              aria-label="Remove part"
            >
              ×
            </Button>
          </div>
        ))}
        <div className="flex items-center gap-3">
          <Button
            variant="secondary"
            onClick={() => {
              setParts([...parts, { amount: "0.00", category: "", memo: "" }]);
            }}
          >
            Add part
          </Button>
          <span className={`money text-14 ${remainder === 0 ? "text-positive" : "text-negative"}`}>
            remainder {formatCents(remainder)}
          </span>
          <Button
            type="submit"
            variant="primary"
            className="ml-auto"
            disabled={!valid || remainder !== 0 || split.isPending}
          >
            Split
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
