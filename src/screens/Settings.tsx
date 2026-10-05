import { open } from "@tauri-apps/plugin-dialog";
import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Chip } from "../components/Chip";
import { Dialog } from "../components/Dialog";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { PreviewView } from "../components/PreviewTable";
import { Select } from "../components/Select";
import { TextField } from "../components/TextField";
import {
  api,
  isCsvSpec,
  toAppError,
  type AmountSpec,
  type AppError,
  type AppStatus,
  type BackupEntry,
  type ExportReport,
  type ImportSource,
  type Profile,
  type ProfileDraft,
  type ProfileSpec,
  type RestoreComparison,
  type Theme,
} from "../lib/ipc";
import {
  useBackupNow,
  useBackups,
  useCreateProfile,
  useDeleteProfile,
  useDraftProfile,
  useExportAuditPack,
  useExportFull,
  useProfiles,
  useRestoreDiscard,
  useRestoreStage,
  useSettings,
  useStatusMutation,
  useTestProfile,
  useUpdateProfile,
  useUpdateSetting,
} from "../lib/queries";
import { useUiStore } from "../lib/store";

interface SettingsProps {
  status: AppStatus;
}

export function Settings({ status }: SettingsProps) {
  const settings = useSettings(true);
  const update = useUpdateSetting();
  const remember = useStatusMutation(() => api.rememberPassphrase());
  const forget = useStatusMutation(() => api.forgetRemembered());
  const lock = useStatusMutation(() => api.lock());

  const updateError = update.isError ? update.error : null;

  return (
    <div className="h-full overflow-auto p-4">
      <div className="grid max-w-[1180px] grid-cols-2 gap-3">
        <Panel title="Data folder">
          <p className="money text-14 break-all">{status.data_dir}</p>
          <p className="mt-2 text-12 text-text-dim">
            Holds kept.db, logs/, backups/ and exports/. To move it, lock Kept and choose a folder
            on the unlock screen.
          </p>
        </Panel>

        <Panel title="Passphrase">
          <p className="text-14">
            {status.remembered
              ? "Remembered in Windows Credential Manager for this data folder."
              : "Not remembered: Kept asks for it at every launch."}
          </p>
          <div className="mt-3 flex gap-2">
            {status.remembered ? (
              <Button
                variant="danger"
                disabled={forget.isPending}
                onClick={() => {
                  forget.mutate(undefined);
                }}
              >
                Forget remembered passphrase
              </Button>
            ) : (
              <Button
                variant="secondary"
                disabled={remember.isPending}
                onClick={() => {
                  remember.mutate(undefined);
                }}
              >
                Remember on this computer
              </Button>
            )}
            <Button
              variant="secondary"
              disabled={lock.isPending}
              onClick={() => {
                lock.mutate(undefined);
              }}
            >
              Lock now
            </Button>
          </div>
          {remember.isError ? (
            <p role="alert" className="mt-2 text-12 text-negative">
              {remember.error.message}
            </p>
          ) : null}
          {forget.isError ? (
            <p role="alert" className="mt-2 text-12 text-negative">
              {forget.error.message}
            </p>
          ) : null}
          <ChangePassphraseForm />
        </Panel>

        <Panel title="Appearance">
          {settings.data ? (
            <div className="flex items-center gap-3">
              <label htmlFor="theme" className="text-14">
                Theme
              </label>
              <select
                id="theme"
                className="h-8 rounded-2 border border-line bg-bg-inset px-2 text-14"
                value={settings.data.theme}
                onChange={(e) => {
                  update.mutate({ key: "theme", value: e.target.value as Theme });
                }}
              >
                <option value="dark">Dark</option>
                <option value="light">Light</option>
              </select>
            </div>
          ) : (
            <p className="text-14 text-text-dim">Loading…</p>
          )}
          <p className="mt-2 text-12 text-text-dim">
            Light is the same design with swapped tokens, not a second design.
          </p>
          {updateError?.field === "theme" ? (
            <p role="alert" className="mt-2 text-12 text-negative">
              {updateError.message}
            </p>
          ) : null}
        </Panel>

        <Panel title="Time zone">
          {settings.data ? (
            <ZoneForm
              key={settings.data.zone}
              initial={settings.data.zone}
              pending={update.isPending}
              error={updateError}
              onSave={(zone) => {
                update.mutate({ key: "zone", value: zone });
              }}
            />
          ) : (
            <p className="text-14 text-text-dim">Loading…</p>
          )}
        </Panel>

        <Panel title="Reconciliation">
          {settings.data ? (
            <StaleForm
              key={settings.data.recon_stale_after_days}
              initial={settings.data.recon_stale_after_days}
              pending={update.isPending}
              error={updateError}
              onSave={(days) => {
                update.mutate({ key: "recon_stale_after_days", value: days });
              }}
            />
          ) : (
            <p className="text-14 text-text-dim">Loading…</p>
          )}
        </Panel>

        <Panel title="About">
          <p className="text-14">Kept v{status.version}</p>
          <p className="mt-1 text-12 text-text-dim">
            Local-only. No cloud, no account, no telemetry. The only network use is a file you
            import yourself.
          </p>
        </Panel>

        <BackupsPanel keepDaily={settings.data?.backup_keep_daily ?? null} />
        <ProfilesPanel />
        <ExportsPanel />
      </div>
    </div>
  );
}

interface ZoneFormProps {
  initial: string;
  pending: boolean;
  error: AppError | null;
  onSave: (zone: string) => void;
}

/** Keyed on the saved zone by the parent, so the field resets whenever the setting changes. */
function ZoneForm({ initial, pending, error, onSave }: ZoneFormProps) {
  const [zone, setZone] = useState(initial);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSave(zone.trim());
  };
  return (
    <form onSubmit={submit} className="flex items-end gap-2">
      <TextField
        label="IANA zone"
        value={zone}
        onChange={(e) => {
          setZone(e.target.value);
        }}
        hint="Civil dates and pay cycles use this zone. Default America/Chicago."
        error={error?.field === "zone" ? error.message : null}
        className="flex-1"
        mono
      />
      <Button type="submit" variant="secondary" disabled={pending || zone.trim() === initial}>
        Save
      </Button>
    </form>
  );
}

interface StaleFormProps {
  initial: number;
  pending: boolean;
  error: AppError | null;
  onSave: (days: number) => void;
}

/** The default stale window (ADR-0021); an account can override it on the Reconcile screen. */
function StaleForm({ initial, pending, error, onSave }: StaleFormProps) {
  const [days, setDays] = useState(String(initial));
  const parsed = Number(days);
  const valid = /^\d{1,4}$/.test(days.trim()) && parsed >= 1;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (valid) onSave(parsed);
  };
  return (
    <form onSubmit={submit} className="flex items-end gap-2">
      <TextField
        label="Reconciled is stale after (days)"
        value={days}
        onChange={(e) => {
          setDays(e.target.value);
        }}
        hint="An account whose last balanced period ended longer ago than this is stale, and the hero is untrusted. Default 45."
        error={
          error?.field === "recon_stale_after_days"
            ? error.message
            : valid
              ? null
              : "1 or more days"
        }
        className="flex-1"
        mono
      />
      <Button type="submit" variant="secondary" disabled={pending || !valid || parsed === initial}>
        Save
      </Button>
    </form>
  );
}

/** Fresh backup under the current passphrase, then rekey in place (ADR-0012). */
function ChangePassphraseForm() {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const change = useStatusMutation((v: { passphrase: string; confirm: string }) =>
    api.changePassphrase(v.passphrase, v.confirm),
  );
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const error = change.isError ? change.error : null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    change.mutate(
      { passphrase, confirm },
      {
        onSuccess: () => {
          setPassphrase("");
          setConfirm("");
          pushNotice({
            tone: "positive",
            text: "Passphrase changed. A fresh backup under the previous passphrase was taken first.",
          });
        },
      },
    );
  };
  return (
    <form onSubmit={submit} className="mt-4 flex flex-col gap-2 border-t border-line pt-3">
      <p className="text-12 text-text-dim">
        Change the passphrase: a verified backup under the current one is written to backups/ first,
        then the database is rekeyed in place. Earlier backups keep the passphrase they were taken
        with.
      </p>
      <div className="grid grid-cols-2 gap-2">
        <TextField
          label="New passphrase"
          type="password"
          autoComplete="new-password"
          value={passphrase}
          onChange={(e) => {
            setPassphrase(e.target.value);
          }}
          error={error?.field === "passphrase" ? error.message : null}
        />
        <TextField
          label="Confirm"
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => {
            setConfirm(e.target.value);
          }}
          error={error?.field === "confirm" ? error.message : null}
        />
      </div>
      {error && error.field !== "passphrase" && error.field !== "confirm" ? (
        <p role="alert" className="text-12 text-negative">
          {error.message}
        </p>
      ) : null}
      <div>
        <Button
          type="submit"
          variant="secondary"
          disabled={change.isPending || passphrase === "" || confirm === ""}
        >
          Change passphrase
        </Button>
      </div>
    </form>
  );
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.ceil(bytes / 1024)} KB`;
}

function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

const KIND_LABEL: Record<BackupEntry["kind"], string> = {
  daily: "daily",
  manual: "manual",
  pre_migration: "pre-migration",
  pre_restore: "pre-restore",
};

/** Every copy taken, newest first, with the file's presence checked on disk (ADR-0046). */
function BackupsPanel({ keepDaily }: { keepDaily: number | null }) {
  const backups = useBackups();
  const backupNow = useBackupNow();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [restoreOpen, setRestoreOpen] = useState(false);
  return (
    <Panel
      title="Backups"
      className="col-span-2"
      aside={
        <span className="flex gap-2">
          <Button
            variant="secondary"
            disabled={backupNow.isPending}
            onClick={() => {
              backupNow.mutate(undefined, {
                onSuccess: (entry) => {
                  pushNotice({
                    tone: "positive",
                    text: `Backup written and verified: ${baseName(entry.path)}`,
                  });
                },
                onError: (error) => {
                  pushNotice({ tone: "negative", text: error.message });
                },
              });
            }}
          >
            Back up now
          </Button>
          <Button
            variant="secondary"
            onClick={() => {
              setRestoreOpen(true);
            }}
          >
            Restore from backup…
          </Button>
        </span>
      }
    >
      <p className="text-12 text-text-dim">
        A daily copy is taken on unlock{keepDaily === null ? "" : ` (the newest ${keepDaily} kept)`}
        ; manual, pre-migration and pre-restore copies stay until you remove them. Every copy is
        encrypted with the passphrase in use when it was taken and verified table by table.
      </p>
      {backups.data === undefined ? (
        <p className="text-14 text-text-dim">Loading…</p>
      ) : backups.data.length === 0 ? (
        <p className="text-14 text-text-dim">
          No backup yet. The first unlock of each day writes one; Back up now writes one this
          moment.
        </p>
      ) : (
        <table className="w-full border-collapse text-12">
          <thead className="text-text-dim">
            <tr>
              <th className="py-1 pr-2 text-left font-medium">Taken (UTC)</th>
              <th className="py-1 pr-2 text-left font-medium">Kind</th>
              <th className="py-1 pr-2 text-left font-medium">File</th>
              <th className="py-1 pr-2 text-right font-medium">Size</th>
              <th className="py-1 pr-2 text-left font-medium">State</th>
            </tr>
          </thead>
          <tbody>
            {backups.data.map((b) => (
              <tr key={b.id} className="border-t border-line">
                <td className="money py-1 pr-2">{b.created_at}</td>
                <td className="py-1 pr-2">
                  <Chip>{KIND_LABEL[b.kind]}</Chip>
                </td>
                <td className="money py-1 pr-2" title={b.path}>
                  {baseName(b.path)}
                </td>
                <td className="money py-1 pr-2 text-right">{formatBytes(b.bytes)}</td>
                <td className="py-1 pr-2">
                  <span className="flex gap-1">
                    {b.verified ? (
                      <Chip tone="positive">verified</Chip>
                    ) : (
                      <Chip tone="warning">not verified</Chip>
                    )}
                    {b.exists ? null : <Chip tone="dim">file gone</Chip>}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {restoreOpen ? (
        <RestoreDialog
          onClose={() => {
            setRestoreOpen(false);
          }}
        />
      ) : null}
    </Panel>
  );
}

/** Open a backup, compare it with the live database, confirm the swap (ARCHITECTURE §6.6). */
function RestoreDialog({ onClose }: { onClose: () => void }) {
  const stage = useRestoreStage();
  const discard = useRestoreDiscard();
  const confirm = useStatusMutation(() => api.restoreConfirm());
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [path, setPath] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [pickError, setPickError] = useState<string | null>(null);
  const comparison: RestoreComparison | undefined = stage.data;

  const pick = async () => {
    setPickError(null);
    try {
      const selected = await open({
        multiple: false,
        title: "Choose a Kept backup",
        filters: [{ name: "Kept backups", extensions: ["db"] }],
      });
      if (typeof selected === "string") setPath(selected);
    } catch (error: unknown) {
      setPickError(toAppError(error).message);
    }
  };
  const close = () => {
    if (comparison) discard.mutate(undefined);
    onClose();
  };
  const stageError = stage.isError ? stage.error : null;

  return (
    <Dialog
      open
      width="lg"
      title="Restore from backup"
      description="Nothing changes until you confirm. The backup is opened with its own passphrase, copied beside the live database under the current passphrase, migrated, and compared table by table."
      onOpenChange={(o) => {
        if (!o) close();
      }}
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-end gap-2">
          <TextField
            label="Backup file"
            value={path}
            onChange={(e) => {
              setPath(e.target.value);
            }}
            className="flex-1"
            error={pickError ?? (stageError?.field === "backup" ? stageError.message : null)}
            mono
          />
          <Button
            variant="secondary"
            onClick={() => {
              void pick();
            }}
          >
            Choose…
          </Button>
        </div>
        <TextField
          label="The backup's passphrase"
          type="password"
          autoComplete="off"
          value={passphrase}
          onChange={(e) => {
            setPassphrase(e.target.value);
          }}
          hint="The passphrase in use when that copy was taken."
          error={stageError && stageError.field !== "backup" ? stageError.message : null}
        />
        <div>
          <Button
            variant="primary"
            disabled={path.trim() === "" || passphrase === "" || stage.isPending}
            onClick={() => {
              stage.mutate({ path: path.trim(), passphrase });
            }}
          >
            {stage.isPending ? "Comparing…" : "Compare with live data"}
          </Button>
        </div>
        {comparison ? <ComparisonView comparison={comparison} /> : null}
        {comparison ? (
          <div className="flex items-center gap-2 border-t border-line pt-3">
            <Button
              variant="danger"
              disabled={confirm.isPending}
              onClick={() => {
                confirm.mutate(undefined, {
                  onSuccess: () => {
                    pushNotice({
                      tone: "positive",
                      text: "Restored from the backup. The previous database is in backups/ as a pre-restore copy.",
                    });
                    onClose();
                  },
                  onError: (error) => {
                    pushNotice({ tone: "negative", text: error.message });
                  },
                });
              }}
            >
              {confirm.isPending ? "Restoring…" : "Replace live data with this backup"}
            </Button>
            <Button variant="secondary" onClick={close}>
              Cancel
            </Button>
            <span className="text-12 text-text-dim">
              A verified pre-restore copy of the live database is written first.
            </span>
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}

function ComparisonView({ comparison }: { comparison: RestoreComparison }) {
  const [showAll, setShowAll] = useState(false);
  const differing = comparison.tables.filter((t) => t.live_rows !== t.backup_rows);
  const shown = showAll ? comparison.tables : differing;
  return (
    <div className="flex flex-col gap-2 rounded-2 border border-line bg-bg-inset p-3 text-14">
      <div className="flex flex-wrap items-center gap-3">
        <span>
          Safe to spend as of <span className="money">{comparison.as_of}</span>: live{" "}
          <Money cents={comparison.hero_live_cents} /> · backup{" "}
          <Money cents={comparison.hero_backup_cents} />
        </span>
        {comparison.hero_same ? (
          <Chip tone="positive">same hero</Chip>
        ) : (
          <Chip tone="warning">hero differs</Chip>
        )}
        {comparison.tables_differ === 0 ? (
          <Chip tone="positive">every table has the same row count</Chip>
        ) : (
          <Chip tone="warning">
            {comparison.tables_differ} of {comparison.tables.length} tables differ
          </Chip>
        )}
        <Chip>
          schema v{comparison.schema_before}
          {comparison.schema_after !== comparison.schema_before
            ? ` → v${comparison.schema_after}`
            : ""}
        </Chip>
      </div>
      <p className="money text-12 text-text-dim break-all">{comparison.backup_path}</p>
      {shown.length > 0 ? (
        <table className="w-full border-collapse text-12">
          <thead className="text-text-dim">
            <tr>
              <th className="py-1 pr-2 text-left font-medium">Table</th>
              <th className="py-1 pr-2 text-right font-medium">Live rows</th>
              <th className="py-1 pr-2 text-right font-medium">Backup rows</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((t) => (
              <tr
                key={t.table}
                className={`border-t border-line ${t.live_rows !== t.backup_rows ? "text-warning" : ""}`}
              >
                <td className="money py-1 pr-2">{t.table}</td>
                <td className="money py-1 pr-2 text-right">{t.live_rows}</td>
                <td className="money py-1 pr-2 text-right">{t.backup_rows}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
      <div>
        <Button
          variant="quiet"
          onClick={() => {
            setShowAll((v) => !v);
          }}
        >
          {showAll ? "Show differing tables only" : `Show all ${comparison.tables.length} tables`}
        </Button>
      </div>
    </div>
  );
}

/** Plaintext exports, written only where asked (ARCHITECTURE §6.6). */
function ExportsPanel() {
  const full = useExportFull();
  const pack = useExportAuditPack();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [report, setReport] = useState<ExportReport | null>(null);
  const run = (mutation: typeof full, dir: string | null) => {
    mutation.mutate(dir, {
      onSuccess: (r) => {
        setReport(r);
        pushNotice({ tone: "positive", text: `${r.files.length} files written to ${r.dir}` });
      },
      onError: (error) => {
        pushNotice({ tone: "negative", text: error.message });
      },
    });
  };
  const chooseAndRun = async (mutation: typeof full) => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choose an empty folder",
      });
      if (typeof selected === "string") run(mutation, selected);
    } catch (error: unknown) {
      pushNotice({ tone: "negative", text: toAppError(error).message });
    }
  };
  const busy = full.isPending || pack.isPending;
  return (
    <Panel title="Exports" className="col-span-2">
      <p className="text-12 text-text-dim">
        Exports are plaintext: anyone with the folder can read the financial history in it. They are
        never written unless you ask, and never over an existing file. The default location is a
        stamped folder under exports/ in the data folder.
      </p>
      <div className="grid grid-cols-2 gap-3">
        <div className="flex flex-col gap-1">
          <p className="text-14">Full export: one CSV per table plus kept.json.</p>
          <div className="flex gap-2">
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => {
                run(full, null);
              }}
            >
              Write full export
            </Button>
            <Button
              variant="quiet"
              disabled={busy}
              onClick={() => {
                void chooseAndRun(full);
              }}
            >
              …to a folder of my choice
            </Button>
          </div>
        </div>
        <div className="flex flex-col gap-1">
          <p className="text-14">
            Audit pack: ledger, reconciliation, safe-to-spend terms, forecast, debt schedule,
            venture rollup and a README.
          </p>
          <div className="flex gap-2">
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => {
                run(pack, null);
              }}
            >
              Write audit pack
            </Button>
            <Button
              variant="quiet"
              disabled={busy}
              onClick={() => {
                void chooseAndRun(pack);
              }}
            >
              …to a folder of my choice
            </Button>
          </div>
        </div>
      </div>
      {report ? (
        <div className="rounded-2 border border-line bg-bg-inset p-3 text-12">
          <p className="money break-all">{report.dir}</p>
          <ul className="mt-1 grid grid-cols-3 gap-x-4">
            {report.files.map((f) => (
              <li key={f.name} className="flex justify-between gap-2">
                <span className="money">{f.name}</span>
                <span className="money text-text-dim">{f.rows} rows</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </Panel>
  );
}

interface EditorState {
  profile: Profile | null;
  draft: ProfileDraft | null;
  sample: ImportSource | null;
}

/** The institution profiles: built-in ones read-only, the person's own editable (ADR-0046). */
function ProfilesPanel() {
  const profiles = useProfiles();
  const draft = useDraftProfile();
  const remove = useDeleteProfile();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [pickError, setPickError] = useState<string | null>(null);

  const newFromSample = async () => {
    setPickError(null);
    try {
      const selected = await open({
        multiple: false,
        title: "Choose a sample CSV export",
        filters: [{ name: "CSV exports", extensions: ["csv", "txt"] }],
      });
      if (typeof selected !== "string") return;
      const source: ImportSource = { kind: "path", path: selected };
      draft.mutate(source, {
        onSuccess: (d) => {
          setEditor({ profile: null, draft: d, sample: source });
        },
        onError: (error) => {
          setPickError(error.message);
        },
      });
    } catch (error: unknown) {
      setPickError(toAppError(error).message);
    }
  };

  return (
    <Panel
      title="Institution profiles"
      className="col-span-2"
      aside={
        <Button
          variant="secondary"
          disabled={draft.isPending}
          onClick={() => {
            void newFromSample();
          }}
        >
          New from sample file…
        </Button>
      }
    >
      <p className="text-12 text-text-dim">
        A profile maps one export layout onto ledger rows: which column is the date, the amount, the
        payee. Built-in profiles are read-only; OFX/QFX files need none, the standard fixes their
        fields.
      </p>
      {pickError ? (
        <p role="alert" className="text-12 text-negative">
          {pickError}
        </p>
      ) : null}
      {profiles.data === undefined ? (
        <p className="text-14 text-text-dim">Loading…</p>
      ) : (
        <table className="w-full border-collapse text-12">
          <thead className="text-text-dim">
            <tr>
              <th className="py-1 pr-2 text-left font-medium">Name</th>
              <th className="py-1 pr-2 text-left font-medium">Institution</th>
              <th className="py-1 pr-2 text-left font-medium">Format</th>
              <th className="py-1 pr-2 text-left font-medium">Header signature</th>
              <th className="py-1 pr-2 text-right font-medium"></th>
            </tr>
          </thead>
          <tbody>
            {profiles.data.map((p) => (
              <tr key={p.id} className="border-t border-line">
                <td className="money py-1 pr-2">{p.name}</td>
                <td className="py-1 pr-2">{p.institution}</td>
                <td className="py-1 pr-2">
                  <span className="flex gap-1">
                    <Chip>{p.format}</Chip>
                    {p.is_system ? <Chip tone="dim">built-in</Chip> : null}
                  </span>
                </td>
                <td className="money py-1 pr-2 text-text-dim">
                  {isCsvSpec(p.spec)
                    ? p.spec.header_signature.join(" | ")
                    : "TRNTYPE → flags: " +
                      Object.entries(p.spec.flags_by_trntype)
                        .map(([k, v]) => `${k} ${v.join("+")}`)
                        .join(", ")}
                </td>
                <td className="py-1 pr-2 text-right">
                  {!p.is_system && isCsvSpec(p.spec) ? (
                    <span className="flex justify-end gap-1">
                      <Button
                        variant="quiet"
                        onClick={() => {
                          setEditor({ profile: p, draft: null, sample: null });
                        }}
                      >
                        Edit
                      </Button>
                      <Button
                        variant="quiet"
                        disabled={remove.isPending}
                        onClick={() => {
                          remove.mutate(p.id, {
                            onSuccess: () => {
                              pushNotice({ tone: "info", text: `Deleted profile ${p.name}.` });
                            },
                            onError: (error) => {
                              pushNotice({ tone: "negative", text: error.message });
                            },
                          });
                        }}
                      >
                        Delete
                      </Button>
                    </span>
                  ) : null}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {editor ? (
        <ProfileEditor
          state={editor}
          onClose={() => {
            setEditor(null);
          }}
        />
      ) : null}
    </Panel>
  );
}

function initialSpec(state: EditorState): ProfileSpec {
  if (state.draft) return state.draft.spec;
  if (state.profile && isCsvSpec(state.profile.spec)) return state.profile.spec;
  return {
    header_signature: [],
    skip_rows: 0,
    date: { column: "", format: "%Y-%m-%d" },
    effective_date: null,
    amount: { kind: "single_signed", column: "" },
    payee: { column: "" },
    memo: null,
    status: null,
    external_id: null,
    balance: null,
    currency: null,
    sign_convention: "account_pov",
    flags_by_type: null,
    row_flags: [],
    skip_when: null,
  };
}

function payeeColumn(spec: ProfileSpec): string | null {
  return "column" in spec.payee ? spec.payee.column : null;
}

/** Edit a CSV mapping, test it against a file, save it. The JSON view edits the same mapping. */
function ProfileEditor({ state, onClose }: { state: EditorState; onClose: () => void }) {
  const create = useCreateProfile();
  const update = useUpdateProfile();
  const test = useTestProfile();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [name, setName] = useState(state.profile?.name ?? "");
  const [institution, setInstitution] = useState(state.profile?.institution ?? "");
  const [spec, setSpecState] = useState<ProfileSpec>(() => initialSpec(state));
  const [json, setJson] = useState(() => JSON.stringify(initialSpec(state), null, 2));
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [sample, setSample] = useState<ImportSource | null>(state.sample);
  const [sampleError, setSampleError] = useState<string | null>(null);

  const setSpec = (next: ProfileSpec) => {
    setSpecState(next);
    setJson(JSON.stringify(next, null, 2));
    setJsonError(null);
  };
  const patch = (changes: Partial<ProfileSpec>) => {
    setSpec({ ...spec, ...changes });
  };
  const onJson = (text: string) => {
    setJson(text);
    try {
      const parsed = JSON.parse(text) as ProfileSpec;
      setSpecState(parsed);
      setJsonError(null);
    } catch (error: unknown) {
      setJsonError(error instanceof Error ? error.message : String(error));
    }
  };
  const header = spec.header_signature;
  const saving = create.isPending || update.isPending;
  const saveError = create.isError ? create.error : update.isError ? update.error : null;

  const chooseSample = async () => {
    setSampleError(null);
    try {
      const selected = await open({
        multiple: false,
        title: "Choose a file to test against",
        filters: [{ name: "CSV exports", extensions: ["csv", "txt"] }],
      });
      if (typeof selected === "string") setSample({ kind: "path", path: selected });
    } catch (error: unknown) {
      setSampleError(toAppError(error).message);
    }
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const input = { name: name.trim(), institution: institution.trim(), spec };
    const done = (p: Profile) => {
      pushNotice({ tone: "positive", text: `Saved profile ${p.name}.` });
      onClose();
    };
    if (state.profile) update.mutate({ id: state.profile.id, input }, { onSuccess: done });
    else create.mutate(input, { onSuccess: done });
  };

  const columnSelect = (
    label: string,
    value: string,
    onChange: (column: string) => void,
    none: boolean,
  ) => (
    <Select
      label={label}
      value={value}
      onChange={(e) => {
        onChange(e.target.value);
      }}
    >
      {none ? <option value="">none</option> : null}
      {value !== "" && !header.includes(value) ? <option value={value}>{value}</option> : null}
      {header.map((h) => (
        <option key={h} value={h}>
          {h}
        </option>
      ))}
    </Select>
  );
  const amount = spec.amount;
  const setAmount = (next: AmountSpec) => {
    patch({ amount: next });
  };

  return (
    <Dialog
      open
      width="lg"
      title={state.profile ? `Edit profile ${state.profile.name}` : "New institution profile"}
      description="The header signature identifies the export; the mapping names the columns the ledger reads. Test it against a file before saving."
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
    >
      <form className="flex flex-col gap-3" onSubmit={submit}>
        <div className="grid grid-cols-3 gap-3">
          <TextField
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            error={saveError?.field === "name" ? saveError.message : null}
            mono
          />
          <TextField
            label="Institution"
            value={institution}
            onChange={(e) => {
              setInstitution(e.target.value);
            }}
          />
          <TextField
            label="Preamble rows to skip"
            value={String(spec.skip_rows)}
            onChange={(e) => {
              const n = Number(e.target.value);
              if (/^\d{0,2}$/.test(e.target.value)) patch({ skip_rows: Number.isNaN(n) ? 0 : n });
            }}
            mono
          />
        </div>
        <p className="money text-12 text-text-dim">
          header signature:{" "}
          {header.length > 0 ? header.join(" | ") : "(read it from a sample file)"}
        </p>
        <div className="grid grid-cols-3 gap-3">
          {columnSelect(
            "Date column",
            spec.date.column,
            (column) => {
              patch({ date: { ...spec.date, column } });
            },
            false,
          )}
          <TextField
            label="Date format (chrono)"
            value={spec.date.format}
            onChange={(e) => {
              patch({ date: { ...spec.date, format: e.target.value } });
            }}
            hint="%Y-%m-%d, %m/%d/%Y, %Y-%m-%dT%H:%M:%S …"
            mono
          />
          <Select
            label="Sign convention"
            value={spec.sign_convention}
            onChange={(e) => {
              patch({
                sign_convention:
                  e.target.value === "card_statement" ? "card_statement" : "account_pov",
              });
            }}
          >
            <option value="account_pov">account's point of view (as exported)</option>
            <option value="card_statement">card statement (purchases positive; negate)</option>
          </Select>
          {columnSelect(
            "Effective (transaction) date column",
            spec.effective_date?.column ?? "",
            (column) => {
              patch({
                effective_date:
                  column === ""
                    ? null
                    : { column, format: spec.effective_date?.format ?? spec.date.format },
              });
            },
            true,
          )}
          <TextField
            label="Effective date format"
            value={spec.effective_date?.format ?? ""}
            disabled={spec.effective_date === null}
            onChange={(e) => {
              if (spec.effective_date) {
                patch({ effective_date: { ...spec.effective_date, format: e.target.value } });
              }
            }}
            mono
          />
          <Select
            label="Amount layout"
            value={amount.kind}
            onChange={(e) => {
              const kind = e.target.value;
              if (kind === "debit_credit") setAmount({ kind, debit: "", credit: "" });
              else if (kind === "amount_with_type") {
                setAmount({ kind, column: "", type_column: "", debit_values: ["Debit"] });
              } else setAmount({ kind: "single_signed", column: "" });
            }}
          >
            <option value="single_signed">one signed amount column</option>
            <option value="debit_credit">debit and credit columns</option>
            <option value="amount_with_type">unsigned amount plus a type column</option>
          </Select>
          {amount.kind === "single_signed"
            ? columnSelect(
                "Amount column",
                amount.column,
                (column) => {
                  setAmount({ kind: "single_signed", column });
                },
                false,
              )
            : null}
          {amount.kind === "debit_credit" ? (
            <>
              {columnSelect(
                "Debit column (outflows)",
                amount.debit,
                (debit) => {
                  setAmount({ ...amount, debit });
                },
                false,
              )}
              {columnSelect(
                "Credit column (inflows)",
                amount.credit,
                (credit) => {
                  setAmount({ ...amount, credit });
                },
                false,
              )}
            </>
          ) : null}
          {amount.kind === "amount_with_type" ? (
            <>
              {columnSelect(
                "Amount column",
                amount.column,
                (column) => {
                  setAmount({ ...amount, column });
                },
                false,
              )}
              {columnSelect(
                "Type column",
                amount.type_column,
                (type_column) => {
                  setAmount({ ...amount, type_column });
                },
                false,
              )}
              <TextField
                label="Type values that mean an outflow (comma-separated)"
                value={amount.debit_values.join(", ")}
                onChange={(e) => {
                  setAmount({
                    ...amount,
                    debit_values: e.target.value
                      .split(",")
                      .map((v) => v.trim())
                      .filter((v) => v !== ""),
                  });
                }}
              />
            </>
          ) : null}
          {payeeColumn(spec) !== null ? (
            columnSelect(
              "Payee column",
              payeeColumn(spec) ?? "",
              (column) => {
                patch({ payee: { column } });
              },
              false,
            )
          ) : (
            <p className="self-end text-12 text-text-dim">
              Payee: a multi-column or counterparty mapping; edit it in the JSON below.
            </p>
          )}
          {columnSelect(
            "Memo column",
            spec.memo && "column" in spec.memo ? spec.memo.column : "",
            (column) => {
              patch({ memo: column === "" ? null : { column } });
            },
            true,
          )}
          {columnSelect(
            "Running balance column",
            spec.balance?.column ?? "",
            (column) => {
              patch({ balance: column === "" ? null : { column } });
            },
            true,
          )}
          {columnSelect(
            "Status column",
            spec.status?.column ?? "",
            (column) => {
              patch({
                status:
                  column === ""
                    ? null
                    : { column, pending_values: spec.status?.pending_values ?? ["Pending"] },
              });
            },
            true,
          )}
          <TextField
            label="Values that mean pending (comma-separated)"
            value={spec.status?.pending_values.join(", ") ?? ""}
            disabled={spec.status === null}
            onChange={(e) => {
              if (spec.status) {
                patch({
                  status: {
                    ...spec.status,
                    pending_values: e.target.value
                      .split(",")
                      .map((v) => v.trim())
                      .filter((v) => v !== ""),
                  },
                });
              }
            }}
          />
          {columnSelect(
            "External id column",
            spec.external_id?.column ?? "",
            (column) => {
              patch({ external_id: column === "" ? null : { column } });
            },
            true,
          )}
          {columnSelect(
            "Currency column",
            spec.currency?.column ?? "",
            (column) => {
              patch({ currency: column === "" ? null : { column } });
            },
            true,
          )}
        </div>
        <details>
          <summary className="cursor-pointer text-12 text-text-dim">
            Advanced: the whole mapping as JSON (flags by type, row flags, skip rules)
          </summary>
          <textarea
            aria-label="Mapping JSON"
            className="money mt-2 h-48 w-full rounded-2 border border-line bg-bg-inset p-2 text-12"
            value={json}
            onChange={(e) => {
              onJson(e.target.value);
            }}
            spellCheck={false}
          />
          {jsonError ? (
            <p role="alert" className="text-12 text-negative">
              {jsonError}
            </p>
          ) : null}
        </details>
        <div className="flex flex-wrap items-center gap-2 border-t border-line pt-3">
          <Button
            variant="secondary"
            onClick={() => {
              void chooseSample();
            }}
          >
            {sample ? "Choose another test file…" : "Choose a test file…"}
          </Button>
          <Button
            variant="secondary"
            disabled={sample === null || test.isPending}
            onClick={() => {
              if (sample) test.mutate({ spec, source: sample });
            }}
          >
            Test against the file
          </Button>
          {sample?.kind === "path" ? (
            <span className="money text-12 text-text-dim break-all">{baseName(sample.path)}</span>
          ) : null}
          {sampleError ? (
            <span role="alert" className="text-12 text-negative">
              {sampleError}
            </span>
          ) : null}
          {test.isError ? (
            <span role="alert" className="text-12 text-negative">
              {test.error.message}
            </span>
          ) : null}
        </div>
        {test.data ? (
          <div className="max-h-64 overflow-auto rounded-2 border border-line bg-bg-inset p-2">
            <PreviewView preview={test.data} />
          </div>
        ) : null}
        {saveError && saveError.field !== "name" ? (
          <p role="alert" className="text-12 text-negative">
            {saveError.message}
          </p>
        ) : null}
        <div className="flex gap-2">
          <Button
            type="submit"
            variant="primary"
            disabled={saving || name.trim() === "" || jsonError !== null}
          >
            {state.profile ? "Save changes" : "Create profile"}
          </Button>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
