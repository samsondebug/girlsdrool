import { open } from "@tauri-apps/plugin-dialog";
import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { TextField } from "../components/TextField";
import { api, toAppError, type AppStatus } from "../lib/ipc";
import { useStatusMutation } from "../lib/queries";

interface UnlockProps {
  status: AppStatus;
}

export function Unlock({ status }: UnlockProps) {
  const [passphrase, setPassphrase] = useState("");
  const [remember, setRemember] = useState(false);
  const [pickError, setPickError] = useState<string | null>(null);

  const unlock = useStatusMutation((input: { passphrase: string; remember: boolean }) =>
    api.unlock(input.passphrase, input.remember),
  );
  const unlockRemembered = useStatusMutation(() => api.unlockRemembered());
  const forget = useStatusMutation(() => api.forgetRemembered());
  const choose = useStatusMutation(api.chooseDataDir);

  const active = unlock.isError
    ? unlock.error
    : unlockRemembered.isError
      ? unlockRemembered.error
      : null;
  const passphraseError =
    active?.kind === "WrongPassphrase" || active?.field === "passphrase" ? active.message : null;
  const otherError =
    active && !passphraseError
      ? active.message
      : forget.isError
        ? forget.error.message
        : choose.isError
          ? choose.error.message
          : pickError;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    unlock.mutate({ passphrase, remember });
  };

  const changeFolder = async () => {
    setPickError(null);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choose the Kept data folder",
      });
      if (typeof selected === "string") choose.mutate(selected);
    } catch (error: unknown) {
      setPickError(toAppError(error).message);
    }
  };

  const busy = unlock.isPending || unlockRemembered.isPending;

  return (
    <main className="flex h-full items-center justify-center p-8">
      <form
        onSubmit={submit}
        className="flex w-[520px] flex-col gap-4 rounded-2 border border-line bg-bg-raised p-6"
      >
        <h1 className="text-20 font-semibold">Unlock Kept</h1>
        <p className="text-14 text-text-dim">
          Data folder: <span className="money text-text">{status.data_dir ?? ""}</span>
        </p>
        <TextField
          label="Passphrase"
          type="password"
          autoComplete="current-password"
          value={passphrase}
          onChange={(e) => {
            setPassphrase(e.target.value);
          }}
          error={passphraseError}
          autoFocus
        />
        <Checkbox
          label="Remember on this computer (Windows Credential Manager)"
          checked={remember}
          onChange={(e) => {
            setRemember(e.target.checked);
          }}
        />
        {otherError ? (
          <p role="alert" className="text-14 text-negative">
            {otherError}
          </p>
        ) : null}
        <div className="flex flex-wrap items-center gap-3">
          <Button type="submit" variant="primary" disabled={busy}>
            {unlock.isPending ? "Unlocking…" : "Unlock"}
          </Button>
          {status.remembered ? (
            <>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => {
                  unlockRemembered.mutate(undefined);
                }}
              >
                Use remembered passphrase
              </Button>
              <Button
                variant="quiet"
                disabled={busy || forget.isPending}
                onClick={() => {
                  forget.mutate(undefined);
                }}
              >
                Forget it
              </Button>
            </>
          ) : null}
          <Button
            variant="quiet"
            className="ml-auto"
            disabled={busy || choose.isPending}
            onClick={() => {
              void changeFolder();
            }}
          >
            Change folder…
          </Button>
        </div>
      </form>
    </main>
  );
}
