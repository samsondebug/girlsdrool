# Performance record

The 200k-row fixture (ADR-0047) is derived, never typed: `tests/perf_200k.rs` builds 200 000 rows
from one formula over the row index, spread over ten years and seven accounts, writes them as
CSV files (one per account and year, seventy batches) and imports them through the real
pipeline (detect, parse, hash, dedup against the ledger, rules, reconciliation refresh, audit
rows) into a real SQLCipher file. Then it times the reads the UI makes.

Run it with:

```
KEPT_PERF=1 cargo test --no-default-features --test perf_200k -- --ignored --nocapture
```

The numbers below are what this host produced. They are not a benchmark of Windows; the
Windows 11 machine Kept ships on records its own run in this file when the installer build is
exercised (see the open box in `MILESTONES.md`).

## Host

|         |                                                                                        |
| ------- | -------------------------------------------------------------------------------------- |
| Machine | Linux dev container, kernel 6.18, 4 vCPU Intel Xeon @ 2.80 GHz, 15 GB RAM, virtio disk |
| Build   | `cargo test` profile (unoptimized, debug info); SQLCipher 4 with vendored OpenSSL      |
| Date    | 2026-10-05                                                                             |

## Results

Every row below is one run of the same test on the same host; only the code changed between
rows. Debug-profile numbers are shown once to make the point that the budget is judged on the
optimized build.

| Run                                                                                                             | Build   |                                                            Import (70 batches) | Slowest batch | First page | Deep page | Text filter |                       Hero | DB size |
| --------------------------------------------------------------------------------------------------------------- | ------- | -----------------------------------------------------------------------------: | ------------: | ---------: | --------: | ----------: | -------------------------: | ------: |
| As committed at M9                                                                                              | debug   | killed after 40 min (quadratic: transfer detection scanned the ledger per row) |             — |          — |         — |           — |                          — |       — |
| + `txn_amount_date`                                                                                             | debug   |                                                                        274.5 s |         4.7 s |     882 ms |    730 ms |      907 ms |                   1,021 ms |  518 MB |
| + `txn_amount_date`                                                                                             | release |                                                                        120.0 s |         2.5 s |     481 ms |    332 ms |      664 ms |                     775 ms |  518 MB |
| + `txn_date_id`, row-value cursor, covering `txn_account_status_date`, leaf view by `NOT IN`, cached statements | release |                                                                         65.1 s |         1.3 s |      38 ms |     33 ms |      276 ms |                     289 ms |  530 MB |
| + partial flagged index (non-covering)                                                                          | release |                                                                         65.1 s |         1.3 s |      38 ms |     33 ms |      276 ms |                     289 ms |  532 MB |
| + covering `txn_flagged`, `cache_size = 64 MB` (shipped)                                                        | release |                                                                     **40.8 s** |         0.8 s |  **31 ms** | **34 ms** |      319 ms | **92 ms cold, 65 ms warm** |  532 MB |

Budget (ARCHITECTURE §15): page fetch < 50 ms at any depth, hero < 100 ms, 200k-row import
< 60 s — met on the shipped row. The test asserts these in the release profile.

Where the shipped hero's time goes, from the same run: one account's posted balance 3 ms
(covering index, index-only sum), the flagged-inflow lookup 0 ms (covering partial index), the
trust report 0 ms; the rest is seven accounts' worth of the above plus the plan terms. The
page-cache change mattered most because every page read from disk costs an AES decrypt and an
HMAC check; with 64 MB the hot indexes stay decrypted in memory.

"Deep page" is the page at row 100 000 (cursor `2021-12-31`); its cost equals the first page's
because the keyset cursor is a row-value comparison that seeks the `(posted_date, id)` index.
"Text filter" is the chip `jewel` (a `LIKE` over `payee_norm` with totals over every match); it
has no index and no budget, and is the next thing to improve (FTS) if it ever matters. The file
is large because every imported row also writes its audit event with full before-and-after
JSON, as the audit rules require; compacting those is the lever if import time ever matters.

## What the numbers mean for the UI

- The Ledger is virtualized (TanStack Virtual) over keyset pages of 200 rows ordered by
  `(posted_date, id)` with an index on `(account_id, posted_date)` and the default scan on
  `posted_date`; a page costs the same at row 1 and at row 100 000 (the deep page above), so
  scrolling is one page fetch per 200 rows and never a full scan.
- The hero (`safe_to_spend`) sums posted balances per account and reads pending rows, earmarks
  and the plan; its cost grows with the number of accounts and plan items, not with the ledger.
- Import cost is per row (hash, exact and fuzzy dedup lookups by `(account_id, amount_cents,
posted_date)`, rules) plus one reconciliation refresh per batch; a year of statements for one
  account is one batch of a few thousand rows.
