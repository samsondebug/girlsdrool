# The data folder

Kept is portable (ADR-0007): the executable plus one folder you choose. Nothing is written
anywhere else, and nothing leaves the machine.

```
Kept.exe
kept.config.json            ← { "data_dir": "D:\\Kept" }   (beside the executable)

D:\Kept\                    ← the data folder
├── kept.db                 ← SQLCipher database (plus kept.db-wal / kept.db-shm while open)
├── logs\kept.YYYY-MM-DD.log   ← daily rotating log, 14 files kept, no amounts or payees ever
├── backups\                ← kept-YYYY-MM-DD.db (daily), kept-pre-vN-….db (before a migration),
│                              manual and pre-restore copies; all encrypted
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
- Changing it (M9) takes a fresh backup first, then rekeys in place.

## What is safe to copy

The database, WAL/SHM files and every backup are encrypted with SQLCipher 4 (AES-256-CBC,
HMAC-SHA512, PBKDF2 at 256 000 iterations). Exports are not encrypted: treat `exports\` as
plaintext financial history.
