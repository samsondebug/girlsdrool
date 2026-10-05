# The data folder

Kept is portable (ADR-0007): the executable plus one folder you choose. Nothing is written
anywhere else, and nothing leaves the machine.

```
Kept.exe
kept.config.json            ← { "data_dir": "D:\\Kept" }   (beside the executable)

D:\Kept\                    ← the data folder
├── kept.db                 ← SQLCipher database (plus kept.db-wal / kept.db-shm while open)
├── logs\kept.YYYY-MM-DD.log   ← daily rotating log, 14 files kept, no amounts or payees ever
├── backups\                ← kept-YYYY-MM-DD.db (daily, the newest N kept), kept-pre-vN-….db
│                              (before a migration), kept-manual-….db, kept-pre-restore-….db;
│                              all encrypted, each verified before it is logged
├── restore-staging\        ← only while a restore is being compared; removed on confirm or cancel
└── exports\                ← CSV/JSON exports and audit packs you ask for (plaintext by design)
```

## Choosing and moving the folder

- First run asks for the folder and writes `kept.config.json` beside `Kept.exe`.
- To move: lock Kept, copy the whole folder, then on the unlock screen choose the new location.
  The config file is rewritten; the old folder is left untouched.
- `KEPT_DATA_DIR=<path>` overrides the config file (tests, a portable drive, a second profile).

## The passphrase

- It is the encryption key for `kept.db`. Kept cannot recover it; neither can anyone else.
- "Remember on this computer" stores it in Windows Credential Manager under the service `Kept`,
  keyed by a hash of the data folder path, protected by your Windows logon.
- Changing it (Settings → Passphrase) takes a fresh verified backup under the current passphrase
  first, then rekeys in place. Earlier backups keep the passphrase they were taken with.

## Backups and restore

- The first unlock of each day writes `backups\kept-YYYY-MM-DD.db`; Settings → Backups writes a
  manual copy any time. Every copy is opened again with its passphrase and compared table by
  table with the live database before it is listed as verified.
- Restore (Settings → Backups → Restore from backup…): choose the copy, type the passphrase it
  was taken with, and compare. Kept shows every table's row count side by side and the
  safe-to-spend figure from both databases. Nothing changes until you confirm; confirming first
  writes a verified pre-restore copy of the live database, then swaps the files. The restored
  database uses the passphrase you are unlocked with.

## What is safe to copy

The database, WAL/SHM files and every backup are encrypted with SQLCipher 4 (AES-256-CBC,
HMAC-SHA512, PBKDF2 at 256 000 iterations). Exports are not encrypted: treat `exports\` as
plaintext financial history.
