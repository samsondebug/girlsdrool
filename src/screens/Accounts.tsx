import { useState, type FormEvent } from "react";

import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { EmptyState } from "../components/EmptyState";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { Select } from "../components/Select";
import { TextField } from "../components/TextField";
import { ACCOUNT_KINDS, type Account, type AccountKind } from "../lib/ipc";
import { parseCentsInput } from "../lib/money";
import { useAccounts, useCreateAccount, useUpdateAccount, useVentures } from "../lib/queries";
import { useUiStore } from "../lib/store";

/** Accounts are created here and archived here; they are never deleted. */
export function Accounts() {
  const accounts = useAccounts();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const update = useUpdateAccount();

  const toggle = (account: Account, patch: { firewalled?: boolean; archived?: boolean }) => {
    update.mutate(
      { id: account.id, patch },
      {
        onSuccess: (updated) => {
          pushNotice({
            tone: "info",
            text: `${updated.name}: ${patch.firewalled !== undefined ? (updated.firewalled ? "firewalled" : "firewall removed") : updated.archived ? "archived" : "restored"}.`,
            undo: {
              label: "Undo",
              run: () => {
                update.mutate({
                  id: account.id,
                  patch:
                    patch.firewalled !== undefined
                      ? { firewalled: account.firewalled }
                      : { archived: account.archived },
                });
              },
            },
          });
        },
        onError: (error) => {
          pushNotice({ tone: "negative", text: error.message });
        },
      },
    );
  };

  return (
    <div className="grid h-full grid-cols-[1fr_380px] gap-3 overflow-hidden p-4">
      <Panel title="Accounts" className="min-h-0 overflow-auto">
        {accounts.data === undefined ? (
          <p className="text-14 text-text-dim">Loading…</p>
        ) : accounts.data.length === 0 ? (
          <EmptyState
            missing="No accounts yet."
            fix="Add each bank, card, brokerage and payment app with its opening balance and date, then import statements."
          />
        ) : (
          <table className="w-full border-collapse text-14">
            <thead>
              <tr className="text-left text-12 text-text-dim">
                <th className="py-1 pr-3 font-medium">Name</th>
                <th className="py-1 pr-3 font-medium">Institution</th>
                <th className="py-1 pr-3 font-medium">Kind</th>
                <th className="py-1 pr-3 text-right font-medium">Opening</th>
                <th className="py-1 pr-3 font-medium">Opening date</th>
                <th className="py-1 pr-3 font-medium">Owner</th>
                <th className="py-1 pr-3 font-medium">Firewalled</th>
                <th className="py-1 pr-3 font-medium">Archived</th>
              </tr>
            </thead>
            <tbody>
              {accounts.data.map((a) => (
                <tr
                  key={a.id}
                  className={`border-t border-line ${a.archived ? "text-text-dim" : ""}`}
                >
                  <td className="py-1 pr-3">{a.name}</td>
                  <td className="py-1 pr-3">{a.institution}</td>
                  <td className="py-1 pr-3">{a.kind}</td>
                  <td className="py-1 pr-3 text-right">
                    <Money cents={a.opening_balance_cents} />
                  </td>
                  <td className="money py-1 pr-3">{a.opening_date}</td>
                  <td className="py-1 pr-3">
                    <OwnerSelect account={a} />
                  </td>
                  <td className="py-1 pr-3">
                    <Checkbox
                      label={a.firewalled ? "yes" : "no"}
                      checked={a.firewalled}
                      onChange={(e) => {
                        toggle(a, { firewalled: e.target.checked });
                      }}
                    />
                  </td>
                  <td className="py-1 pr-3">
                    <Checkbox
                      label={a.archived ? "yes" : "no"}
                      checked={a.archived}
                      onChange={(e) => {
                        toggle(a, { archived: e.target.checked });
                      }}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
      <Panel title="Add account" className="min-h-0 overflow-auto">
        <NewAccountForm />
      </Panel>
    </div>
  );
}

/** Personal, or owned by a venture: a venture-owned account never counts as personal cash. */
function OwnerSelect({ account }: { account: Account }) {
  const ventures = useVentures();
  const update = useUpdateAccount();
  const pushNotice = useUiStore((s) => s.pushNotice);
  return (
    <Select
      label="Owner"
      compact
      value={account.venture_id === null ? "" : String(account.venture_id)}
      onChange={(e) => {
        const venture_id = e.target.value === "" ? null : Number(e.target.value);
        update.mutate(
          { id: account.id, patch: { venture_id } },
          {
            onError: (error) => {
              pushNotice({ tone: "negative", text: error.message });
            },
          },
        );
      }}
    >
      <option value="">personal</option>
      {(ventures.data ?? []).map((v) => (
        <option key={v.id} value={v.id}>
          venture: {v.name}
        </option>
      ))}
    </Select>
  );
}

function NewAccountForm() {
  const create = useCreateAccount();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [name, setName] = useState("");
  const [institution, setInstitution] = useState("");
  const [kind, setKind] = useState<AccountKind>("checking");
  const [opening, setOpening] = useState("0.00");
  const [openingDate, setOpeningDate] = useState("");
  const [firewalled, setFirewalled] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const error = create.isError ? create.error : null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setLocalError(null);
    const cents = parseCentsInput(opening);
    if (cents === null) {
      setLocalError(
        "Opening balance must be an amount like 1,234.56 (negative for a card balance owed).",
      );
      return;
    }
    create.mutate(
      {
        name,
        institution,
        kind,
        opening_balance_cents: cents,
        opening_date: openingDate,
        venture_id: null,
        firewalled,
      },
      {
        onSuccess: (account) => {
          pushNotice({ tone: "positive", text: `Added ${account.name}.` });
          setName("");
          setInstitution("");
          setOpening("0.00");
          setOpeningDate("");
          setFirewalled(false);
        },
      },
    );
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <TextField
        label="Name"
        value={name}
        onChange={(e) => {
          setName(e.target.value);
        }}
        error={error?.field === "name" ? error.message : null}
        required
      />
      <TextField
        label="Institution"
        value={institution}
        onChange={(e) => {
          setInstitution(e.target.value);
        }}
      />
      <Select
        label="Kind"
        value={kind}
        onChange={(e) => {
          setKind(e.target.value as AccountKind);
        }}
      >
        {ACCOUNT_KINDS.filter((k) => k !== "venture").map((k) => (
          <option key={k} value={k}>
            {k}
          </option>
        ))}
      </Select>
      <TextField
        label="Opening balance"
        hint="From the account's point of view: a card balance owed is negative."
        value={opening}
        onChange={(e) => {
          setOpening(e.target.value);
        }}
        error={localError ?? (error?.field === "opening_balance_cents" ? error.message : null)}
        mono
      />
      <TextField
        label="Opening date"
        hint="YYYY-MM-DD: the balance above is the balance at the start of this day."
        value={openingDate}
        onChange={(e) => {
          setOpeningDate(e.target.value);
        }}
        error={error?.field === "date" || error?.field === "opening_date" ? error.message : null}
        mono
        required
      />
      <Checkbox
        label="Firewalled (never counted as available cash; outflows need an acknowledgment)"
        checked={firewalled}
        onChange={(e) => {
          setFirewalled(e.target.checked);
        }}
      />
      {error &&
      !["name", "opening_balance_cents", "date", "opening_date"].includes(error.field ?? "") ? (
        <p role="alert" className="text-14 text-negative">
          {error.message}
        </p>
      ) : null}
      <Button type="submit" variant="primary" disabled={create.isPending}>
        Add account
      </Button>
    </form>
  );
}
