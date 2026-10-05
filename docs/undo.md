# Undo coverage

Every destructive action in Kept either names its undo on the toast it raises, or is listed
here with the reason it cannot be undone and what stands in for it. Undo is the inverse command,
run as a new audited command group (ADR-0047): the row comes back with the same fields and a
new id, and the audit log keeps both steps.

| Screen         | Action                                           | Undo on the toast | Inverse command                                                                                               |
| -------------- | ------------------------------------------------ | ----------------- | ------------------------------------------------------------------------------------------------------------- |
| Import         | Import a batch                                   | Undo batch N      | `undo_import_batch` (refused if a row changed since)                                                          |
| Import         | Resolve a suspected duplicate (insert / discard) | —                 | Not undoable: the decision is recorded; importing the file again shows the row again                          |
| Ledger         | Save a view                                      | Undo              | `delete_saved_view`                                                                                           |
| Ledger         | Recategorize, edit a field, split, unsplit       | —                 | Edits are re-editable in place; a split is undone with Unsplit; every change is audited with its before state |
| Queue / Ledger | Unlink a transfer                                | Undo              | `link_transfer` with the same rows and kind                                                                   |
| Queue / Ledger | Unlink a refund                                  | Undo              | `link_refund` with the same rows                                                                              |
| Rules          | Delete a rule                                    | Undo              | `create_rule` from the deleted rule's fields                                                                  |
| Reconcile      | Delete a period                                  | Undo              | `reconcile` with the same statement closing and source                                                        |
| Plan           | Unmatch an income receipt                        | Undo              | `record_receipt` for the same due date and row                                                                |
| Plan           | Delete an obligation candidate                   | Undo              | `create_obligation` with status `candidate`                                                                   |
| Plan           | Unmatch an obligation payment                    | Undo              | `record_payment` for the same due date and row                                                                |
| Plan           | Remove an earmark entry                          | Undo              | `add_earmark_entry` with the same fields                                                                      |
| Forecast       | Set or clear a variable-spend override           | Undo              | `set_variable_spend_override` with the previous value                                                         |
| Debts          | Remove a payment                                 | Undo              | `record_debt_payment` with the same fields                                                                    |
| Debts          | Remove an informal schedule row                  | Undo              | `add_informal_schedule_row` with the same date and amount                                                     |
| Review         | Abandon a review                                 | —                 | Nothing was stored; Start review begins again                                                                 |
| Accounts       | Archive, restore, firewall, unfirewall           | Undo              | `update_account` with the previous flag                                                                       |
| Settings       | Delete an institution profile                    | Undo              | `create_import_profile` with the same mapping                                                                 |
| Settings       | Restore from backup                              | —                 | The verified pre-restore copy in `backups/`; restore it the same way                                          |
| Settings       | Change the passphrase                            | —                 | Not undoable by design; the backup taken first opens with the previous passphrase                             |

What is never destructive: the ledger is append-only in spirit, imports never delete rows, and
every write is one transaction with one `command` row and an `audit_event` per touched row
(ARCHITECTURE §11), so the audit log holds the before state of everything above.

## Notice lifetime (ADR-0048 §9)

A notice that names an undo or an action stays until it is dismissed or used. A notice with
neither leaves after eight seconds. The stack holds the newest five; the oldest leaves first.
