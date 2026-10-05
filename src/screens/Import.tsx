import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useState } from "react";

import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { EmptyState } from "../components/EmptyState";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { Select } from "../components/Select";
import { PreviewView } from "../components/PreviewTable";
import { toAppError, type ImportReport, type ImportSource, type Preview } from "../lib/ipc";
import { formatBps } from "../lib/money";
import {
  useAccounts,
  useBatches,
  useImportCommit,
  useImportPreview,
  useProfiles,
  useQuarantine,
  useResolveQuarantine,
  useUndoBatch,
} from "../lib/queries";
import { reportError } from "../lib/report";
import { useUiStore } from "../lib/store";

/** Import: pick or drop a file (or paste CSV), check the mapping, commit, read the report, undo. */
export function Import() {
  const accounts = useAccounts();
  const profiles = useProfiles();
  const batches = useBatches();
  const quarantine = useQuarantine();
  const preview = useImportPreview();
  const commit = useImportCommit();
  const undo = useUndoBatch();
  const resolve = useResolveQuarantine();
  const pushNotice = useUiStore((s) => s.pushNotice);

  const [accountId, setAccountId] = useState("");
  const [profileId, setProfileId] = useState("");
  const [source, setSource] = useState<ImportSource | null>(null);
  const [pasted, setPasted] = useState("");
  const [pickError, setPickError] = useState<string | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [dragging, setDragging] = useState(false);

  const activeAccounts = (accounts.data ?? []).filter((a) => !a.archived);
  const accountNum = accountId === "" ? null : Number(accountId);
  const profileNum = profileId === "" ? null : Number(profileId);

  const runPreview = (src: ImportSource, acct: number | null, prof: number | null) => {
    setReport(null);
    if (acct === null) return;
    preview.mutate({ accountId: acct, profileId: prof, source: src });
  };

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") setDragging(true);
        else if (event.payload.type === "leave") setDragging(false);
        else {
          setDragging(false);
          const path = event.payload.paths[0];
          if (path) {
            const src: ImportSource = { kind: "path", path };
            setSource(src);
            runPreview(src, accountNum, profileNum);
          }
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((error: unknown) => {
        reportError(error, "listening for dropped files");
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
    // the handler reads the current account/profile through the latest closure on each mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountNum, profileNum]);

  const pickFile = async () => {
    setPickError(null);
    try {
      const selected = await open({
        multiple: false,
        title: "Choose a statement export",
        filters: [{ name: "Statement exports", extensions: ["csv", "txt", "ofx", "qfx"] }],
      });
      if (typeof selected === "string") {
        const src: ImportSource = { kind: "path", path: selected };
        setSource(src);
        runPreview(src, accountNum, profileNum);
      }
    } catch (error: unknown) {
      setPickError(toAppError(error).message);
    }
  };

  const usePasted = () => {
    if (pasted.trim() === "") return;
    const src: ImportSource = { kind: "text", name: "pasted.csv", text: pasted };
    setSource(src);
    runPreview(src, accountNum, profileNum);
  };

  const doCommit = () => {
    if (source === null || accountNum === null) return;
    commit.mutate(
      { accountId: accountNum, profileId: profileNum, source },
      {
        onSuccess: (r) => {
          setReport(r);
          const batchId = r.batch_id;
          if (r.reason) {
            pushNotice({ tone: "info", text: r.summary });
            return;
          }
          pushNotice({
            tone: "positive",
            text: r.summary,
            undo: {
              label: `Undo batch ${batchId}`,
              run: () => {
                undo.mutate(batchId, {
                  onSuccess: (u) => {
                    pushNotice({
                      tone: "info",
                      text: `Undid batch ${u.batch_id}: ${u.deleted} rows removed, ${u.restored} restored${u.unlinked > 0 ? `, ${u.unlinked} links removed` : ""}.`,
                    });
                    setReport(null);
                  },
                  onError: (error) => {
                    pushNotice({ tone: "negative", text: error.message });
                  },
                });
              },
            },
          });
        },
      },
    );
  };

  const p: Preview | undefined = preview.data;
  const previewError = preview.isError ? preview.error.message : null;
  const commitError = commit.isError ? commit.error.message : null;

  return (
    <div className="grid h-full grid-cols-[1fr_380px] gap-3 overflow-hidden p-4">
      <div className="flex min-h-0 flex-col gap-3 overflow-auto">
        <Panel title="Source" className="shrink-0">
          <div className="grid grid-cols-2 gap-3">
            <Select
              label="Account"
              value={accountId}
              onChange={(e) => {
                setAccountId(e.target.value);
                const acct = e.target.value === "" ? null : Number(e.target.value);
                if (source) runPreview(source, acct, profileNum);
              }}
            >
              <option value="">Choose the account this export belongs to…</option>
              {activeAccounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </Select>
            <Select
              label="Profile"
              value={profileId}
              onChange={(e) => {
                setProfileId(e.target.value);
                const prof = e.target.value === "" ? null : Number(e.target.value);
                if (source) runPreview(source, accountNum, prof);
              }}
            >
              <option value="">Auto-detect from the header</option>
              {(profiles.data ?? []).map((pr) => (
                <option key={pr.id} value={pr.id}>
                  {pr.name}
                  {pr.institution ? ` — ${pr.institution}` : ""}
                </option>
              ))}
            </Select>
          </div>
          <div
            className={`mt-3 flex min-h-[88px] flex-col items-center justify-center gap-2 rounded-2 border border-dashed p-3 text-14 ${
              dragging ? "border-accent text-text" : "border-line text-text-dim"
            }`}
          >
            <span>Drop a CSV or OFX/QFX export here</span>
            <span className="flex items-center gap-2">
              <Button
                variant="secondary"
                onClick={() => {
                  void pickFile();
                }}
              >
                Choose file…
              </Button>
              {source?.kind === "path" ? (
                <span className="money text-12">{source.path}</span>
              ) : null}
              {source?.kind === "text" ? <span className="text-12">pasted text</span> : null}
            </span>
          </div>
          <div className="mt-3 flex items-end gap-2">
            <div className="flex flex-1 flex-col gap-1">
              <label htmlFor="paste" className="text-12 font-medium text-text-dim">
                Or paste CSV text
              </label>
              <textarea
                id="paste"
                rows={3}
                className="money rounded-2 border border-line bg-bg-inset p-2 text-12"
                value={pasted}
                onChange={(e) => {
                  setPasted(e.target.value);
                }}
                placeholder={"Date,Description,Amount\n2026-09-01,RENT,-2400.00"}
              />
            </div>
            <Button variant="secondary" onClick={usePasted} disabled={pasted.trim() === ""}>
              Use pasted text
            </Button>
          </div>
          {pickError ? (
            <p role="alert" className="mt-2 text-14 text-negative">
              {pickError}
            </p>
          ) : null}
          {accountNum === null && source ? (
            <p className="mt-2 text-14 text-warning">
              Choose the account first; the preview needs it.
            </p>
          ) : null}
        </Panel>

        <Panel title="Mapping preview" className="shrink-0">
          {preview.isPending ? (
            <p className="text-14 text-text-dim">Reading…</p>
          ) : previewError ? (
            <p role="alert" className="text-14 text-negative">
              {previewError}
            </p>
          ) : p === undefined ? (
            <EmptyState missing="No file yet." fix="Drop, choose or paste an export above." />
          ) : (
            <PreviewView preview={p} />
          )}
        </Panel>

        <Panel title="Commit" className="shrink-0">
          <div className="flex items-center gap-3">
            <Button
              variant="primary"
              onClick={doCommit}
              disabled={
                p?.profile == null || p.problem !== null || commit.isPending || accountNum === null
              }
            >
              {commit.isPending ? "Importing…" : "Import this file"}
            </Button>
            {typeof p?.already_imported_batch === "number" ? (
              <span className="text-14 text-warning">
                This exact file was already imported as batch {p.already_imported_batch}; importing
                again records a no-op.
              </span>
            ) : null}
            {commitError ? (
              <span role="alert" className="text-14 text-negative">
                {commitError}
              </span>
            ) : null}
          </div>
          {report ? <ReportView report={report} accountId={accountNum} /> : null}
        </Panel>
      </div>

      <div className="flex min-h-0 flex-col gap-3 overflow-auto">
        <Panel title="Suspected duplicates" className="shrink-0">
          {quarantine.data === undefined ? (
            <p className="text-14 text-text-dim">Loading…</p>
          ) : quarantine.data.length === 0 ? (
            <EmptyState
              missing="Nothing held for review."
              fix="Rows that look like duplicates of existing rows land here."
            />
          ) : (
            <ul className="flex flex-col gap-2">
              {quarantine.data.map((q) => {
                const row = JSON.parse(q.row_json) as {
                  posted_date: string;
                  payee_raw: string;
                  amount_cents: number;
                };
                return (
                  <li
                    key={q.id}
                    className="flex flex-col gap-1 rounded-2 border border-line p-2 text-14"
                  >
                    <span>
                      <span className="money">{row.posted_date}</span> {row.payee_raw}{" "}
                      <Money cents={row.amount_cents} />
                    </span>
                    <span className="text-12 text-text-dim">{q.reason}</span>
                    <span className="flex gap-2">
                      <Button
                        variant="secondary"
                        disabled={resolve.isPending}
                        onClick={() => {
                          resolve.mutate(
                            { id: q.id, action: "insert" },
                            {
                              onSuccess: () => {
                                pushNotice({
                                  tone: "positive",
                                  text: "Inserted as a separate row.",
                                });
                              },
                            },
                          );
                        }}
                      >
                        It is separate: insert
                      </Button>
                      <Button
                        variant="danger"
                        disabled={resolve.isPending}
                        onClick={() => {
                          resolve.mutate(
                            { id: q.id, action: "discard" },
                            {
                              onSuccess: () => {
                                pushNotice({ tone: "info", text: "Discarded as a duplicate." });
                              },
                            },
                          );
                        }}
                      >
                        Duplicate: discard
                      </Button>
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </Panel>

        <Panel title="Batches" className="shrink-0">
          {batches.data === undefined ? (
            <p className="text-14 text-text-dim">Loading…</p>
          ) : batches.data.length === 0 ? (
            <EmptyState
              missing="No imports yet."
              fix="Each committed file becomes a batch you can undo."
            />
          ) : (
            <ul className="flex flex-col gap-2">
              {batches.data.map((b) => (
                <li
                  key={b.id}
                  className={`flex flex-col gap-1 rounded-2 border border-line p-2 text-12 ${b.undone_at ? "text-text-dim" : ""}`}
                >
                  <span className="flex items-center gap-2">
                    <span className="font-medium text-text">#{b.id}</span>
                    <span className="truncate">{b.file_name}</span>
                    {b.undone_at ? <Chip>undone</Chip> : null}
                  </span>
                  <span className="money">
                    {b.date_from ?? "—"} … {b.date_to ?? "—"} · +{b.inserted} ~{b.updated} =
                    {b.skipped} ?{b.quarantined}
                  </span>
                  {b.undone_at === null ? (
                    <Button
                      variant="quiet"
                      className="self-start"
                      disabled={undo.isPending}
                      onClick={() => {
                        undo.mutate(b.id, {
                          onSuccess: (u) => {
                            pushNotice({
                              tone: "info",
                              text: `Undid batch ${u.batch_id}: ${u.deleted} rows removed, ${u.restored} restored${u.unlinked > 0 ? `, ${u.unlinked} links removed` : ""}.`,
                            });
                          },
                          onError: (error) => {
                            pushNotice({ tone: "negative", text: error.message });
                          },
                        });
                      }}
                    >
                      Undo this batch
                    </Button>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>
    </div>
  );
}

function ReportView({ report, accountId }: { report: ImportReport; accountId: number | null }) {
  const setScreen = useUiStore((s) => s.setScreen);
  const setReconcileDraft = useUiStore((s) => s.setReconcileDraft);
  return (
    <div className="mt-3 flex flex-col gap-2 text-14">
      <p className="text-text">{report.summary}</p>
      {report.file_closing_cents !== null &&
      report.file_closing_date !== null &&
      accountId !== null ? (
        <p className="flex items-center gap-2 text-12 text-text-dim">
          The file's running balance ends at <Money cents={report.file_closing_cents} /> on{" "}
          <span className="money">{report.file_closing_date}</span>.
          <Button
            variant="secondary"
            onClick={() => {
              setReconcileDraft({
                accountId,
                periodEnd: report.file_closing_date ?? "",
                statementClosingCents: report.file_closing_cents ?? 0,
              });
              setScreen("reconcile");
            }}
          >
            Reconcile with it
          </Button>
        </p>
      ) : null}
      <p className="text-12 text-text-dim">
        batch #{report.batch_id} · profile {report.profile_name} · {report.date_from ?? "—"} …{" "}
        {report.date_to ?? "—"} · threshold {formatBps(report.threshold_bps)}
      </p>
      {report.updated.length > 0 ? (
        <ul className="text-12 text-text-dim">
          {report.updated.map((u) => (
            <li key={u.txn_id}>
              row {u.row} updated ledger row {u.txn_id} ({u.fields.join(", ")}), similarity{" "}
              {formatBps(u.similarity_bps)}
            </li>
          ))}
        </ul>
      ) : null}
      {report.skipped.filter((s) => s.by === "profile_rule").length > 0 ? (
        <p className="text-12 text-text-dim">
          Rows skipped by the profile rule:{" "}
          {report.skipped
            .filter((s) => s.by === "profile_rule")
            .map((s) => s.row)
            .join(", ")}
        </p>
      ) : null}
      {report.quarantined.length > 0 ? (
        <p className="text-12 text-warning">
          Held for review: rows {report.quarantined.map((q) => q.row).join(", ")} — resolve them in
          the panel on the right.
        </p>
      ) : null}
    </div>
  );
}
