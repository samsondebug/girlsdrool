import { Chip } from "./Chip";
import { Money } from "./Money";
import type { Preview } from "../lib/ipc";

/** The first rows of a statement as the ledger would see them, with the file's own facts. */
export function PreviewView({ preview }: { preview: Preview }) {
  return (
    <div className="flex flex-col gap-2 text-14">
      <div className="flex flex-wrap items-center gap-2">
        {preview.profile ? (
          <Chip tone="positive">profile: {preview.profile.name}</Chip>
        ) : preview.candidates.length > 1 ? (
          <Chip tone="warning">{preview.candidates.length} profiles match; choose one above</Chip>
        ) : (
          <Chip tone="warning">no profile matches this header; choose one above</Chip>
        )}
        <Chip>{preview.total_rows} rows</Chip>
        {preview.blank_rows > 0 ? <Chip>{preview.blank_rows} blank</Chip> : null}
        {preview.ofx ? (
          <Chip tone="info">
            OFX {preview.ofx.form} · {preview.ofx.acct_type.toLowerCase()} …
            {preview.ofx.acct_id.slice(-4)}
            {preview.ofx.start && preview.ofx.end
              ? ` · ${preview.ofx.start} .. ${preview.ofx.end}`
              : ""}
          </Chip>
        ) : null}
        <span className="money text-12 text-text-dim">
          {preview.format === "ofx" ? "fields" : "header"}: {preview.header.join(" | ")}
        </span>
      </div>
      {preview.closing ? (
        <p className="text-12 text-text-dim">
          The file states a closing balance of <Money cents={preview.closing.cents} /> on{" "}
          <span className="money">{preview.closing.date}</span>; after the import, Reconcile can
          take it as statement source <span className="money">file</span>.
        </p>
      ) : null}
      {preview.problem ? (
        <p role="alert" className="text-negative">
          Row {preview.problem.row}
          {preview.problem.column ? `, column ${preview.problem.column}` : ""}:{" "}
          {preview.problem.message}
        </p>
      ) : null}
      {preview.rows.length > 0 ? (
        <table className="w-full border-collapse text-12">
          <thead className="text-text-dim">
            <tr>
              <th className="py-1 pr-2 text-left font-medium">#</th>
              <th className="py-1 pr-2 text-left font-medium">Posted</th>
              <th className="py-1 pr-2 text-left font-medium">Effective</th>
              <th className="py-1 pr-2 text-left font-medium">Payee (normalized)</th>
              <th className="py-1 pr-2 text-left font-medium">Memo</th>
              <th className="py-1 pr-2 text-right font-medium">Amount</th>
              <th className="py-1 pr-2 text-left font-medium">State</th>
            </tr>
          </thead>
          <tbody>
            {preview.rows.map((r) => (
              <tr
                key={r.row}
                className={`border-t border-line ${r.skipped ? "text-text-dim" : ""}`}
              >
                <td className="money py-1 pr-2">{r.row}</td>
                <td className="money py-1 pr-2">{r.posted_date}</td>
                <td className="money py-1 pr-2">{r.effective_date}</td>
                <td className="py-1 pr-2" title={r.payee_raw}>
                  {r.payee_norm || r.payee_raw}
                </td>
                <td className="py-1 pr-2">{r.memo}</td>
                <td className="py-1 pr-2 text-right">
                  <Money cents={r.amount_cents} />
                </td>
                <td className="py-1 pr-2">
                  <span className="flex flex-wrap gap-1">
                    {r.status === "pending" ? <Chip tone="info">pending</Chip> : null}
                    {r.flags.map((f) => (
                      <Chip key={f}>{f.replace(/_/g, " ")}</Chip>
                    ))}
                    {r.skipped ? (
                      <Chip tone="warning" title={r.skipped}>
                        skipped by profile
                      </Chip>
                    ) : null}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
      {preview.total_rows > preview.rows.length ? (
        <p className="text-12 text-text-dim">
          Showing the first {preview.rows.length} of {preview.total_rows} rows.
        </p>
      ) : null}
    </div>
  );
}
