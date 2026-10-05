import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { TextField } from "../components/TextField";
import { api, type AppError, type AppStatus, type Theme } from "../lib/ipc";
import { useSettings, useStatusMutation, useUpdateSetting } from "../lib/queries";

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
      <div className="grid max-w-[960px] grid-cols-2 gap-3">
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

        <Panel title="About">
          <p className="text-14">Kept v{status.version}</p>
          <p className="mt-1 text-12 text-text-dim">
            Local-only. No cloud, no account, no telemetry. The only network use is a file you
            import yourself.
          </p>
        </Panel>
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
