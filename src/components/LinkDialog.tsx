import { useState } from "react";

import { TRANSFER_KINDS, type LedgerRow, type TransferKind, type TxnRecord } from "../lib/ipc";
import { formatBps, formatCents } from "../lib/money";
import {
  useAccounts,
  useLinkCandidates,
  useLinkDetails,
  useLinkRefund,
  useLinkTransfer,
  useUnlinkRefund,
  useUnlinkTransfer,
} from "../lib/queries";
import { useUiStore } from "../lib/store";
import { Button } from "./Button";
import { Chip } from "./Chip";
import { Dialog } from "./Dialog";
import { Money } from "./Money";
import { Select } from "./Select";

interface LinkDialogProps {
  row: LedgerRow;
  onClose: () => void;
}

const KIND_LABELS: Record<TransferKind, string> = {
  internal: "internal transfer",
  card_payment: "card payment",
  loan_repayment: "loan repayment",
  venture_contribution: "venture contribution",
  venture_withdrawal: "venture withdrawal",
};

function daysLabel(days: number): string {
  if (days === 0) return "same day";
  return `${days} day${days === 1 ? "" : "s"} apart`;
}

/**
 * The links a row is part of and the rows it could be linked to. Linking here records the link
 * as `user`; detection never overrides it. Unlinking sends both rows back to the review queue.
 */
export function LinkDialog({ row, onClose }: LinkDialogProps) {
  const accounts = useAccounts();
  const details = useLinkDetails(row.id);
  const candidates = useLinkCandidates(row.id);
  const linkTransfer = useLinkTransfer();
  const unlinkTransfer = useUnlinkTransfer();
  const linkRefund = useLinkRefund();
  const unlinkRefund = useUnlinkRefund();
  const pushNotice = useUiStore((s) => s.pushNotice);
  const [kind, setKind] = useState<TransferKind | "">("");

  const accountName = (id: number) =>
    accounts.data?.find((a) => a.id === id)?.name ?? `account ${id}`;
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  const busy =
    linkTransfer.isPending ||
    unlinkTransfer.isPending ||
    linkRefund.isPending ||
    unlinkRefund.isPending;
  const describeOther = (t: TxnRecord) =>
    `${t.posted_date} · ${accountName(t.account_id)} · ${t.payee_norm} · ${formatCents(t.amount_cents)}`;

  const transfer = details.data?.transfer ?? null;
  const transferOther = details.data?.transfer_other ?? null;
  const refund = details.data?.refund ?? null;
  const refundOther = details.data?.refund_other ?? null;

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={`Links for ${row.payee_norm}`}
      description={`${row.posted_date} · ${row.account_name} · ${formatCents(row.amount_cents)}`}
      width="lg"
    >
      <div className="flex flex-col gap-4 text-14">
        <section className="flex flex-col gap-2">
          <h3 className="text-12 font-medium text-text-dim">Current links</h3>
          {details.isPending ? <p className="text-text-dim">Loading…</p> : null}
          {details.isError ? <p className="text-negative">{details.error.message}</p> : null}
          {transfer && transferOther ? (
            <div className="flex items-center gap-2">
              <Chip tone="positive">{KIND_LABELS[transfer.kind]}</Chip>
              <Chip>{transfer.confidence === "user" ? "linked by you" : "detected"}</Chip>
              <span className="truncate">{describeOther(transferOther)}</span>
              <Button
                variant="danger"
                className="ml-auto"
                disabled={busy}
                onClick={() => {
                  unlinkTransfer.mutate(transfer.id, {
                    onSuccess: () => {
                      pushNotice({
                        tone: "info",
                        text: "Transfer unlinked; both rows are back in the review queue.",
                        undo: {
                          label: "Undo",
                          run: () => {
                            linkTransfer.mutate(
                              {
                                outTxnId: transfer.out_txn_id,
                                inTxnId: transfer.in_txn_id,
                                kind: transfer.kind,
                              },
                              { onError: fail },
                            );
                          },
                        },
                      });
                    },
                    onError: fail,
                  });
                }}
              >
                Unlink
              </Button>
            </div>
          ) : null}
          {refund && refundOther ? (
            <div className="flex items-center gap-2">
              <Chip tone="positive">refund of</Chip>
              <Chip>{refund.confidence === "user" ? "linked by you" : "detected"}</Chip>
              <span className="truncate">{describeOther(refundOther)}</span>
              <Button
                variant="danger"
                className="ml-auto"
                disabled={busy}
                onClick={() => {
                  unlinkRefund.mutate(refund.id, {
                    onSuccess: () => {
                      pushNotice({
                        tone: "info",
                        text: "Refund unlinked; the row is back in the review queue.",
                        undo: {
                          label: "Undo",
                          run: () => {
                            linkRefund.mutate(
                              {
                                originalTxnId: refund.original_txn_id,
                                refundTxnId: refund.refund_txn_id,
                              },
                              { onError: fail },
                            );
                          },
                        },
                      });
                    },
                    onError: fail,
                  });
                }}
              >
                Unlink
              </Button>
            </div>
          ) : null}
          {details.data && !transfer && !refund ? (
            <p className="text-text-dim">Not linked.</p>
          ) : null}
        </section>

        <section className="flex flex-col gap-2">
          <div className="flex items-end gap-2">
            <h3 className="text-12 font-medium text-text-dim">Transfer candidates</h3>
            <span className="text-12 text-text-dim">
              the opposite amount on another account, within 3 days
            </span>
            <Select
              label="Kind"
              compact
              className="ml-auto"
              value={kind}
              onChange={(e) => {
                const value = e.target.value;
                setKind(TRANSFER_KINDS.find((k) => k === value) ?? "");
              }}
            >
              <option value="">kind from the accounts</option>
              {TRANSFER_KINDS.map((k) => (
                <option key={k} value={k}>
                  {KIND_LABELS[k]}
                </option>
              ))}
            </Select>
          </div>
          {candidates.isError ? <p className="text-negative">{candidates.error.message}</p> : null}
          {candidates.data?.transfers.length === 0 ? (
            <p className="text-text-dim">
              No candidate: the other leg is not imported yet, or it is already linked.
            </p>
          ) : null}
          {(candidates.data?.transfers ?? []).map((c) => (
            <div
              key={c.txn_id}
              className="flex items-center gap-3 rounded-2 border border-line px-3 py-2"
            >
              <span className="money">{c.posted_date}</span>
              <span className="truncate">{c.account_name}</span>
              <span className="truncate text-text-dim">{c.payee_norm}</span>
              <Money cents={c.amount_cents} />
              <Chip>{daysLabel(c.days_apart)}</Chip>
              <Button
                variant="primary"
                className="ml-auto"
                disabled={busy || row.transfer_link_id !== null || row.refund_link_id !== null}
                onClick={() => {
                  const outTxnId = row.amount_cents < 0 ? row.id : c.txn_id;
                  const inTxnId = row.amount_cents < 0 ? c.txn_id : row.id;
                  linkTransfer.mutate(
                    { outTxnId, inTxnId, kind: kind === "" ? null : kind },
                    {
                      onSuccess: (link) => {
                        pushNotice({
                          tone: "positive",
                          text: `Linked as ${KIND_LABELS[link.kind]}; both rows left the review queue.`,
                          undo: {
                            label: "Undo",
                            run: () => {
                              unlinkTransfer.mutate(link.id, { onError: fail });
                            },
                          },
                        });
                      },
                      onError: fail,
                    },
                  );
                }}
              >
                Link as transfer
              </Button>
            </div>
          ))}
        </section>

        {row.amount_cents > 0 ? (
          <section className="flex flex-col gap-2">
            <div className="flex items-end gap-2">
              <h3 className="text-12 font-medium text-text-dim">Refund candidates</h3>
              <span className="text-12 text-text-dim">
                an earlier purchase of this size on this account, within 90 days
              </span>
            </div>
            {candidates.data?.refunds.length === 0 ? (
              <p className="text-text-dim">No earlier purchase of this size on this account.</p>
            ) : null}
            {(candidates.data?.refunds ?? []).map(({ candidate: c, similarity_bps }) => (
              <div
                key={c.txn_id}
                className="flex items-center gap-3 rounded-2 border border-line px-3 py-2"
              >
                <span className="money">{c.posted_date}</span>
                <span className="truncate">{c.payee_norm}</span>
                <Money cents={c.amount_cents} />
                <Chip>{daysLabel(c.days_apart)}</Chip>
                <Chip tone={similarity_bps === 10_000 ? "positive" : "dim"}>
                  payee {formatBps(similarity_bps, 0)} similar
                </Chip>
                <Button
                  variant="primary"
                  className="ml-auto"
                  disabled={busy || row.refund_link_id !== null || row.transfer_link_id !== null}
                  onClick={() => {
                    linkRefund.mutate(
                      { originalTxnId: c.txn_id, refundTxnId: row.id },
                      {
                        onSuccess: (link) => {
                          pushNotice({
                            tone: "positive",
                            text: "Linked as a refund; it nets against the purchase in the spending view.",
                            undo: {
                              label: "Undo",
                              run: () => {
                                unlinkRefund.mutate(link.id, { onError: fail });
                              },
                            },
                          });
                        },
                        onError: fail,
                      },
                    );
                  }}
                >
                  Link as refund
                </Button>
              </div>
            ))}
          </section>
        ) : null}
      </div>
    </Dialog>
  );
}
