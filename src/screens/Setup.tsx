import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import { Button } from "../components/Button";
import { api } from "../lib/ipc";
import { useStatusMutation } from "../lib/queries";
import { toAppError } from "../lib/ipc";

/** First run: choose the data folder. Everything Kept writes lives under it. */
export function Setup() {
  const choose = useStatusMutation(api.chooseDataDir);
  const [pickError, setPickError] = useState<string | null>(null);

  const pick = async () => {
    setPickError(null);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choose the Kept data folder",
      });
      if (typeof selected === "string") {
        choose.mutate(selected);
      }
    } catch (error: unknown) {
      setPickError(toAppError(error).message);
    }
  };

  const error = pickError ?? (choose.isError ? choose.error.message : null);

  return (
    <main className="flex h-full items-center justify-center p-8">
      <div className="flex w-[520px] flex-col gap-4 rounded-2 border border-line bg-bg-raised p-6">
        <h1 className="text-20 font-semibold">Kept</h1>
        <p className="text-14 text-text">Choose a data folder.</p>
        <p className="text-14 text-text-dim">
          Kept is portable: the folder you pick will hold <code className="money">kept.db</code>{" "}
          (encrypted), <code className="money">logs/</code>, <code className="money">backups/</code>{" "}
          and <code className="money">exports/</code>. Nothing is written anywhere else, and nothing
          leaves this machine.
        </p>
        <div className="flex items-center gap-3">
          <Button
            variant="primary"
            onClick={() => {
              void pick();
            }}
            disabled={choose.isPending}
          >
            Choose folder…
          </Button>
          {choose.isPending ? (
            <span className="text-12 text-text-dim">Preparing folder…</span>
          ) : null}
        </div>
        {error ? (
          <p role="alert" className="text-14 text-negative">
            {error}
          </p>
        ) : null}
      </div>
    </main>
  );
}
