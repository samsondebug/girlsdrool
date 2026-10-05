import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { TextField } from "../components/TextField";
import { api } from "../lib/ipc";
import { useStatusMutation } from "../lib/queries";

interface CreateDatabaseProps {
  dataDir: string;
}

/** The data folder exists but holds no database yet: choose the passphrase that becomes the key. */
export function CreateDatabase({ dataDir }: CreateDatabaseProps) {
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [remember, setRemember] = useState(false);
  const create = useStatusMutation(
    (input: { passphrase: string; confirm: string; remember: boolean }) =>
      api.createDatabase(input.passphrase, input.confirm, input.remember),
  );

  const field = create.isError ? create.error.field : null;
  const message = create.isError ? create.error.message : null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    create.mutate({ passphrase, confirm, remember });
  };

  return (
    <main className="flex h-full items-center justify-center p-8">
      <form
        onSubmit={submit}
        className="flex w-[520px] flex-col gap-4 rounded-2 border border-line bg-bg-raised p-6"
      >
        <h1 className="text-20 font-semibold">Create the database</h1>
        <p className="text-14 text-text-dim">
          Data folder: <span className="money text-text">{dataDir}</span>
        </p>
        <p className="text-14 text-text-dim">
          This passphrase is the encryption key for <code className="money">kept.db</code>. Kept
          cannot recover it. A long phrase you can type daily beats a short one you must write down.
        </p>
        <TextField
          label="Passphrase"
          type="password"
          autoComplete="new-password"
          value={passphrase}
          onChange={(e) => {
            setPassphrase(e.target.value);
          }}
          error={field === "passphrase" ? message : null}
          autoFocus
        />
        <TextField
          label="Confirm passphrase"
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => {
            setConfirm(e.target.value);
          }}
          error={field === "confirm" ? message : null}
        />
        <Checkbox
          label="Remember on this computer (Windows Credential Manager)"
          checked={remember}
          onChange={(e) => {
            setRemember(e.target.checked);
          }}
        />
        {message && field !== "passphrase" && field !== "confirm" ? (
          <p role="alert" className="text-14 text-negative">
            {message}
          </p>
        ) : null}
        <div className="flex items-center gap-3">
          <Button type="submit" variant="primary" disabled={create.isPending}>
            {create.isPending ? "Creating…" : "Create database"}
          </Button>
        </div>
      </form>
    </main>
  );
}
