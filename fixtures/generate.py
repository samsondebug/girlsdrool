#!/usr/bin/env python3
"""
Kept fixtures (ADR-0028). One explicit, deterministic row list is the single source of truth:
this script writes the seven institutions' CSV exports and EXPECTED.md, showing every running
balance so the known answers can be checked by eye. Nothing here calls the Kept engine.

Run:  python3 fixtures/generate.py      (rewrites fixtures/**/*.csv and fixtures/EXPECTED.md)

Money is integer cents throughout. Dates are civil YYYY-MM-DD. The story covers the three
statement months 2026-07, 2026-08 and 2026-09 for one person with:
  three banks (Northbank checking + savings, Riverside checking), two cards (Summit Visa, Summit
  Amex), one firewalled brokerage (Harbor) and a Venmo balance.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from datetime import date, datetime

ROOT = os.path.dirname(os.path.abspath(__file__))


# ---------------------------------------------------------------------------------------------
# Accounts
# ---------------------------------------------------------------------------------------------

@dataclass(frozen=True)
class Account:
    key: str
    name: str
    institution: str
    kind: str
    opening_cents: int
    opening_date: str
    firewalled: bool = False


ACCOUNTS = {
    "nbc": Account("nbc", "Northbank Checking", "Northbank", "checking", 321_455, "2026-07-01"),
    "nbs": Account("nbs", "Northbank Savings", "Northbank", "savings", 1_200_000, "2026-07-01"),
    "rvc": Account("rvc", "Riverside Checking", "Riverside Bank", "checking", 105_000, "2026-07-01"),
    "sv": Account("sv", "Summit Visa", "Summit Card Services", "credit", -182_040, "2026-07-01"),
    "sa": Account("sa", "Summit Amex", "Summit Card Services", "credit", -31_218, "2026-07-01"),
    "hb": Account("hb", "Harbor Brokerage", "Harbor Securities", "brokerage", 40_000, "2026-07-01", True),
    "vm": Account("vm", "Venmo", "Venmo", "payment_app", 0, "2026-07-01"),
}


# ---------------------------------------------------------------------------------------------
# Rows. amount is from the account's point of view (inflow +, outflow −), in cents.
# role tags drive the hand-computed answers: "transfer:<pair>", "income", "refund:<pair>",
# "interest", "fee", "proceeds", "cash", and a category code used from M2 on.
# ---------------------------------------------------------------------------------------------

@dataclass
class Row:
    account: str
    posted: str
    description: str
    amount: int
    category: str                      # category code the M2 rules assign (or 'review')
    effective: str | None = None       # transaction date when the export has one
    status: str = "posted"
    pair: str | None = None            # transfer pair id (both legs share it)
    refund_of: str | None = None       # description of the original purchase (same account)
    kind: str = ""                     # export-specific type column (Sale/Payment/Return/…)
    external_id: str | None = None
    note: str = ""
    extra: dict = field(default_factory=dict)

    @property
    def eff(self) -> str:
        return self.effective or self.posted


R = Row

ROWS: list[Row] = [
    # ---- Northbank Checking -----------------------------------------------------------------
    R("nbc", "2026-07-01", "LAKESHORE PROPERTIES RENT", -240_000, "fixed.rent"),
    R("nbc", "2026-07-02", "ONLINE TRANSFER TO SAV ...5678", -50_000, "transfer.internal", pair="sav-07"),
    R("nbc", "2026-07-03", "ZELLE PAYMENT FROM MORGAN AVERY", 120_000, "fixed.rent", note="roommate's half of the rent"),
    R("nbc", "2026-07-06", "COMED ELECTRIC", -11_240, "fixed.utilities"),
    R("nbc", "2026-07-10", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-07-12", "XFINITY", -8_999, "fixed.internet"),
    R("nbc", "2026-07-14", "JEWEL-OSCO #3421", -8_734, "variable.groceries"),
    R("nbc", "2026-07-18", "T-MOBILE", -7_500, "fixed.phone"),
    R("nbc", "2026-07-20", "SUMMIT CARD SERVICES PAYMENT", -182_040, "transfer.card_payment", pair="visa-07"),
    R("nbc", "2026-07-21", "PEOPLES GAS", -4_312, "fixed.utilities"),
    R("nbc", "2026-07-22", "SUMMIT CARD SVCS AMEX PYMT", -31_218, "transfer.card_payment", pair="amex-07"),
    R("nbc", "2026-07-24", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-07-25", "ATM WITHDRAWAL 1120 N STATE", -10_000, "variable.cash"),
    R("nbc", "2026-07-28", "SHELL OIL 57442", -5_218, "variable.fuel"),

    R("nbc", "2026-08-01", "LAKESHORE PROPERTIES RENT", -240_000, "fixed.rent"),
    R("nbc", "2026-08-03", "ZELLE PAYMENT FROM MORGAN AVERY", 120_000, "fixed.rent"),
    R("nbc", "2026-08-04", "ONLINE TRANSFER TO SAV ...5678", -50_000, "transfer.internal", pair="sav-08"),
    R("nbc", "2026-08-06", "COMED ELECTRIC", -13_875, "fixed.utilities"),
    R("nbc", "2026-08-07", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-08-07", "VENMO CASHOUT", 60_000, "transfer.internal", pair="venmo-08"),
    R("nbc", "2026-08-12", "XFINITY", -8_999, "fixed.internet"),
    R("nbc", "2026-08-13", "JEWEL-OSCO #3421", -11_206, "variable.groceries"),
    R("nbc", "2026-08-18", "T-MOBILE", -7_500, "fixed.phone"),
    R("nbc", "2026-08-20", "SUMMIT CARD SERVICES PAYMENT", -100_000, "transfer.card_payment", pair="visa-08"),
    R("nbc", "2026-08-21", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-08-21", "PEOPLES GAS", -3_890, "fixed.utilities"),
    R("nbc", "2026-08-22", "SUMMIT CARD SVCS AMEX PYMT", -11_600, "transfer.card_payment", pair="amex-08"),
    R("nbc", "2026-08-26", "SHELL OIL 57442", -4_890, "variable.fuel"),
    R("nbc", "2026-08-28", "ZELLE PAYMENT TO CHRIS PARK", -30_000, "transfer.loan_repayment", note="informal loan repayment 1 of 2"),

    R("nbc", "2026-09-01", "LAKESHORE PROPERTIES RENT", -240_000, "fixed.rent"),
    R("nbc", "2026-09-02", "ZELLE PAYMENT FROM MORGAN AVERY", 120_000, "fixed.rent"),
    R("nbc", "2026-09-03", "ONLINE TRANSFER TO SAV ...5678", -50_000, "transfer.internal", pair="sav-09"),
    R("nbc", "2026-09-04", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-09-08", "COMED ELECTRIC", -12_110, "fixed.utilities"),
    R("nbc", "2026-09-12", "XFINITY", -8_999, "fixed.internet"),
    R("nbc", "2026-09-14", "VENMO *MORGAN AVERY", -8_500, "review", note="bank-funded Venmo payment; purpose never guessed"),
    R("nbc", "2026-09-16", "ACH DEPOSIT HARBOR BROKERAGE", 250_000, "transfer.internal", pair="harbor-09"),
    R("nbc", "2026-09-18", "T-MOBILE", -7_500, "fixed.phone"),
    R("nbc", "2026-09-18", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-09-19", "JEWEL-OSCO #3421", -9_452, "variable.groceries"),
    R("nbc", "2026-09-20", "SUMMIT CARD SERVICES PAYMENT", -120_000, "transfer.card_payment", pair="visa-09"),
    R("nbc", "2026-09-21", "PEOPLES GAS", -3_655, "fixed.utilities"),
    R("nbc", "2026-09-22", "GEICO AUTO INS ANNUAL", -128_400, "irregular.insurance"),
    R("nbc", "2026-09-22", "SUMMIT CARD SVCS AMEX PYMT", -11_600, "transfer.card_payment", pair="amex-09"),
    R("nbc", "2026-09-28", "ZELLE PAYMENT TO CHRIS PARK", -30_000, "transfer.loan_repayment", note="informal loan repayment 2 of 2"),
    R("nbc", "2026-09-29", "SHELL OIL 57442", -5_033, "variable.fuel"),

    # ---- Northbank Savings ------------------------------------------------------------------
    R("nbs", "2026-07-02", "ONLINE TRANSFER FROM CHK ...1234", 50_000, "transfer.internal", pair="sav-07"),
    R("nbs", "2026-07-31", "INTEREST PAYMENT", 1_023, "income.interest"),
    R("nbs", "2026-08-04", "ONLINE TRANSFER FROM CHK ...1234", 50_000, "transfer.internal", pair="sav-08"),
    R("nbs", "2026-08-31", "INTEREST PAYMENT", 1_067, "income.interest"),
    R("nbs", "2026-09-03", "ONLINE TRANSFER FROM CHK ...1234", 50_000, "transfer.internal", pair="sav-09"),
    R("nbs", "2026-09-30", "INTEREST PAYMENT", 1_109, "income.interest"),

    # ---- Riverside Checking (transaction date + posted date; debit/credit columns) -----------
    R("rvc", "2026-07-15", "DIRECT DEPOSIT - FREELANCE INVOICE 118", 45_000, "income.other", effective="2026-07-15"),
    R("rvc", "2026-07-23", "COSTCO WHSE #1101", -14_387, "variable.groceries", effective="2026-07-22"),
    R("rvc", "2026-08-12", "DIRECT DEPOSIT - FREELANCE INVOICE 121", 30_000, "income.other", effective="2026-08-11"),
    R("rvc", "2026-08-15", "ATM WITHDRAWAL BANCO AZTECA CDMX", -16_342, "variable.cash", effective="2026-08-14"),
    R("rvc", "2026-08-15", "NON-NETWORK ATM FEE", -300, "debt.fees", effective="2026-08-14"),
    R("rvc", "2026-08-15", "INTL TRANSACTION FEE", -490, "debt.fees", effective="2026-08-14"),
    R("rvc", "2026-08-17", "RESTAURANTE EL CARDENAL", -6_215, "variable.dining", effective="2026-08-16"),
    R("rvc", "2026-09-10", "COSTCO WHSE #1101", -15_790, "variable.groceries", effective="2026-09-09"),
    R("rvc", "2026-09-25", "DIRECT DEPOSIT - FREELANCE INVOICE 125", 60_000, "income.other", effective="2026-09-25"),

    # ---- Summit Visa (card statement convention: Sale positive in the file) -----------------
    R("sv", "2026-07-02", "TRADER JOE S #702", -6_431, "variable.groceries", kind="Sale"),
    R("sv", "2026-07-05", "CHIPOTLE 1192", -1_388, "variable.dining", kind="Sale"),
    R("sv", "2026-07-09", "AMAZON.COM*HG6 AMZN.COM/BILL", -4_299, "variable.shopping", kind="Sale"),
    R("sv", "2026-07-11", "UBER TRIP", -2_314, "variable.transport", kind="Sale"),
    R("sv", "2026-07-15", "WALGREENS #5821", -1_876, "variable.health", kind="Sale"),
    R("sv", "2026-07-19", "PAYMENT - THANK YOU", 182_040, "transfer.card_payment", kind="Payment", pair="visa-07"),
    R("sv", "2026-07-22", "TRADER JOE S #702", -7_215, "variable.groceries", kind="Sale"),
    R("sv", "2026-07-26", "NETFLIX.COM", -1_549, "fixed.subscriptions", kind="Sale"),
    R("sv", "2026-07-29", "CHIPOTLE 1192", -1_527, "variable.dining", kind="Sale"),
    R("sv", "2026-07-31", "INTEREST CHARGE ON PURCHASES", -2_287, "debt.interest", kind="Interest"),

    R("sv", "2026-08-03", "TRADER JOE S #702", -5_890, "variable.groceries", kind="Sale"),
    R("sv", "2026-08-09", "TARGET 00012345", -8_417, "variable.shopping", kind="Sale"),
    R("sv", "2026-08-12", "UBER TRIP", -1_890, "variable.transport", kind="Sale"),
    R("sv", "2026-08-16", "TRADER JOE S #702", -6_702, "variable.groceries", kind="Sale"),
    R("sv", "2026-08-19", "PAYMENT - THANK YOU", 100_000, "transfer.card_payment", kind="Payment", pair="visa-08"),
    R("sv", "2026-08-20", "TARGET 00012345", 8_417, "variable.shopping", kind="Return", refund_of="TARGET 00012345"),
    R("sv", "2026-08-24", "CHIPOTLE 1192", -1_412, "variable.dining", kind="Sale"),
    R("sv", "2026-08-26", "NETFLIX.COM", -1_549, "fixed.subscriptions", kind="Sale"),
    R("sv", "2026-08-31", "INTEREST CHARGE ON PURCHASES", -1_944, "debt.interest", kind="Interest"),
    # the pending row appears in the August export only; see PENDING_AMAZON below

    R("sv", "2026-09-01", "AMAZON.COM*2K4 AMZN.COM/BILL", -6_250, "variable.shopping", kind="Sale", effective="2026-08-30"),
    R("sv", "2026-09-06", "TRADER JOE S #702", -7_340, "variable.groceries", kind="Sale"),
    R("sv", "2026-09-10", "WALGREENS #5821", -2_210, "variable.health", kind="Sale"),
    R("sv", "2026-09-13", "UBER TRIP", -2_760, "variable.transport", kind="Sale"),
    R("sv", "2026-09-19", "PAYMENT - THANK YOU", 120_000, "transfer.card_payment", kind="Payment", pair="visa-09"),
    R("sv", "2026-09-21", "TRADER JOE S #702", -6_118, "variable.groceries", kind="Sale"),
    R("sv", "2026-09-26", "NETFLIX.COM", -1_549, "fixed.subscriptions", kind="Sale"),
    R("sv", "2026-09-27", "CHIPOTLE 1192", -1_495, "variable.dining", kind="Sale"),
    R("sv", "2026-09-30", "INTEREST CHARGE ON PURCHASES", -1_872, "debt.interest", kind="Interest"),

    # ---- Summit Amex ------------------------------------------------------------------------
    R("sa", "2026-07-15", "LINEAR.APP", -9_600, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-07-15", "VERCEL INC", -2_000, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-07-22", "PAYMENT - THANK YOU", 31_218, "transfer.card_payment", kind="Payment", pair="amex-07"),
    R("sa", "2026-08-15", "LINEAR.APP", -9_600, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-08-15", "VERCEL INC", -2_000, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-08-22", "PAYMENT - THANK YOU", 11_600, "transfer.card_payment", kind="Payment", pair="amex-08"),
    R("sa", "2026-09-15", "LINEAR.APP", -9_600, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-09-15", "VERCEL INC", -2_000, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-09-22", "PAYMENT - THANK YOU", 11_600, "transfer.card_payment", kind="Payment", pair="amex-09"),

    # ---- Harbor Brokerage (firewalled; cash balance only) -----------------------------------
    R("hb", "2026-07-15", "DIVIDEND", 1_842, "income.interest", kind="Dividend", extra={"symbol": "VTI", "qty": "", "price": ""}),
    R("hb", "2026-09-12", "SELL", 250_000, "transfer.securities_sale_proceeds", kind="Sell", extra={"symbol": "VTI", "qty": "20", "price": "125.00"}),
    R("hb", "2026-09-15", "ACH TRANSFER TO NORTHBANK ...1234", -250_000, "transfer.internal", kind="Transfer Out", pair="harbor-09", extra={"symbol": "", "qty": "", "price": ""}),

    # ---- Venmo (balance-held rows only; bank-funded rows are skipped by the profile) --------
    R("vm", "2026-08-05", "Chris Park", 60_000, "review", kind="Payment", external_id="4211000001", note="til payday",
      extra={"time": "14:03:11", "from": "Chris Park", "to": "Dave Kept", "funding": "Venmo balance", "destination": ""}),
    R("vm", "2026-08-06", "Northbank Bank *1234", -60_000, "transfer.internal", kind="Standard Transfer", external_id="4211000002", pair="venmo-08",
      extra={"time": "09:15:40", "from": "Dave Kept", "to": "", "funding": "Venmo balance", "destination": "Northbank Bank *1234"}),
    R("vm", "2026-09-20", "Morgan Avery", 4_200, "review", kind="Payment", external_id="4211000004", note="dinner",
      extra={"time": "20:11:37", "from": "Morgan Avery", "to": "Dave Kept", "funding": "Venmo balance", "destination": ""}),
]

# The Visa August export carries this pending row; the September export carries it posted
# (ROWS above, 2026-09-01) with a longer descriptor. Dedup must update, not duplicate.
PENDING_AMAZON = R("sv", "2026-08-30", "AMAZON.COM", -6_250, "variable.shopping", kind="Sale", status="pending")

# This Venmo payment was funded straight from the bank, so it never touched the Venmo balance.
# The profile skips it (reported, not silent); the bank's own row (NBC 2026-09-14) is the ledger.
VENMO_BANK_FUNDED = R("vm", "2026-09-13", "Morgan Avery", -8_500, "review", kind="Payment", external_id="4211000003", note="concert tix",
                      extra={"time": "19:42:05", "from": "Dave Kept", "to": "Morgan Avery", "funding": "Northbank Checking", "destination": ""})

# The Northbank Aug+Sep overlap export repeats every row and writes one descriptor differently.
OVERLAP_VARIANT = ("2026-08-13", "JEWEL-OSCO #3421", "JEWEL-OSCO #3421 CHICAGO")


# ---------------------------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------------------------

def money(cents: int) -> str:
    sign = "-" if cents < 0 else ""
    a = abs(cents)
    return f"{sign}{a // 100:,}.{a % 100:02d}"


def plain(cents: int) -> str:
    sign = "-" if cents < 0 else ""
    a = abs(cents)
    return f"{sign}{a // 100}.{a % 100:02d}"


def mdy(iso: str) -> str:
    d = date.fromisoformat(iso)
    return f"{d.month:02d}/{d.day:02d}/{d.year}"


def month_of(iso: str) -> str:
    return iso[:7]


def rows_for(account: str, month: str | None = None) -> list[Row]:
    out = [r for r in ROWS if r.account == account and (month is None or month_of(r.posted) == month)]
    return sorted(out, key=lambda r: (r.posted, ROWS.index(r)))


MONTHS = ["2026-07", "2026-08", "2026-09"]


def closing(account: str, through_month: str) -> int:
    bal = ACCOUNTS[account].opening_cents
    for r in rows_for(account):
        if month_of(r.posted) <= through_month:
            bal += r.amount
    return bal


def write(path: str, text: str) -> None:
    full = os.path.join(ROOT, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w", newline="\n") as f:
        f.write(text)


# ---------------------------------------------------------------------------------------------
# Payee normalisation and Jaro–Winkler, re-implemented here independently of the engine so the
# expected similarities are hand answers, not engine output (ADR-0018 / ADR-0037).
# ---------------------------------------------------------------------------------------------

PREFIXES = ["pos debit ", "checkcard ", "debit card purchase ", "sq *", "tst* ", "tst*", "paypal *", "pp*"]


def normalize(payee: str) -> str:
    s = payee.lower().strip()
    for p in PREFIXES:
        if s.startswith(p):
            s = s[len(p):]
    if "*" in s and len(s.split("*", 1)[0].strip()) >= 3:
        s = s.split("*", 1)[0]
    s = "".join(ch if ch.isalnum() else " " for ch in s)
    tokens = [t for t in s.split() if not t.isdigit() and not (len(t) <= 5 and any(c.isdigit() for c in t))]
    return " ".join(tokens)


def jaro_winkler(a: str, b: str) -> float:
    if a == b:
        return 1.0
    if not a or not b:
        return 0.0
    window = max(len(a), len(b)) // 2 - 1
    a_flags = [False] * len(a)
    b_flags = [False] * len(b)
    matches = 0
    for i, ca in enumerate(a):
        lo, hi = max(0, i - window), min(len(b), i + window + 1)
        for j in range(lo, hi):
            if not b_flags[j] and b[j] == ca:
                a_flags[i] = b_flags[j] = True
                matches += 1
                break
    if matches == 0:
        return 0.0
    a_m = [a[i] for i in range(len(a)) if a_flags[i]]
    b_m = [b[j] for j in range(len(b)) if b_flags[j]]
    transpositions = sum(1 for x, y in zip(a_m, b_m) if x != y) // 2
    jaro = (matches / len(a) + matches / len(b) + (matches - transpositions) / matches) / 3
    prefix = 0
    for x, y in zip(a, b):
        if x == y and prefix < 4:
            prefix += 1
        else:
            break
    return jaro + prefix * 0.1 * (1 - jaro)


# ---------------------------------------------------------------------------------------------
# CSV writers, one per institution format
# ---------------------------------------------------------------------------------------------

def csv_field(s: str) -> str:
    if any(c in s for c in ",\"\n"):
        return '"' + s.replace('"', '""') + '"'
    return s


def northbank(account: str, rows: list[Row], opening: int, variant: tuple | None = None) -> str:
    lines = ["Date,Description,Amount,Running Bal."]
    bal = opening
    for r in rows:
        bal += r.amount
        desc = r.description
        if variant and (r.posted, r.description) == (variant[0], variant[1]):
            desc = variant[2]
        lines.append(",".join([mdy(r.posted), csv_field(desc), csv_field(money(r.amount)), csv_field(money(bal))]))
    return "\n".join(lines) + "\n"


def riverside(rows: list[Row], opening: int) -> str:
    lines = ["Transaction Date,Posted Date,Description,Debit,Credit,Balance,Currency"]
    bal = opening
    for r in rows:
        bal += r.amount
        debit = plain(-r.amount) if r.amount < 0 else ""
        credit = plain(r.amount) if r.amount > 0 else ""
        lines.append(",".join([r.eff, r.posted, csv_field(r.description), debit, credit, plain(bal), "USD"]))
    return "\n".join(lines) + "\n"


def summit(rows: list[Row]) -> str:
    # card statement convention: Sale/Interest/Fee positive, Payment/Return negative
    lines = ["Transaction Date,Post Date,Description,Type,Amount,Status"]
    for r in rows:
        file_amount = -r.amount
        status = "Pending" if r.status == "pending" else "Posted"
        lines.append(",".join([mdy(r.eff), mdy(r.posted), csv_field(r.description), r.kind, plain(file_amount), status]))
    return "\n".join(lines) + "\n"


def harbor(rows: list[Row], opening: int) -> str:
    lines = ["Date,Activity,Symbol,Quantity,Price,Amount,Cash Balance"]
    bal = opening
    for r in rows:
        bal += r.amount
        x = r.extra
        lines.append(",".join([mdy(r.posted), csv_field(r.description), x["symbol"], x["qty"], x["price"], plain(r.amount), plain(bal)]))
    return "\n".join(lines) + "\n"


def venmo(rows: list[Row]) -> str:
    # Venmo's export starts with two preamble lines before the header; amounts read "+ $600.00".
    lines = [
        "Account Statement - (@dave-kept) - Jul 1 2026 to Sep 30 2026",
        "Account Activity",
        "ID,Datetime,Type,Status,Note,From,To,Amount (total),Funding Source,Destination",
    ]
    for r in rows:
        x = r.extra
        sign = "+" if r.amount > 0 else "-"
        amount = f"{sign} ${abs(r.amount) // 100:,}.{abs(r.amount) % 100:02d}"
        lines.append(",".join([
            r.external_id or "", f"{r.posted}T{x['time']}", r.kind, "Complete", csv_field(r.note),
            csv_field(x["from"]), csv_field(x["to"]), csv_field(amount), csv_field(x["funding"]), csv_field(x["destination"]),
        ]))
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------
# Emit files
# ---------------------------------------------------------------------------------------------

def emit_csvs() -> dict[str, str]:
    files: dict[str, str] = {}

    for m in MONTHS:
        prev = closing("nbc", f"2026-{int(m[-2:]) - 1:02d}") if m != "2026-07" else ACCOUNTS["nbc"].opening_cents
        files[f"northbank/northbank_checking_{m}.csv"] = northbank("nbc", rows_for("nbc", m), prev)
        prev_s = closing("nbs", f"2026-{int(m[-2:]) - 1:02d}") if m != "2026-07" else ACCOUNTS["nbs"].opening_cents
        files[f"northbank/northbank_savings_{m}.csv"] = northbank("nbs", rows_for("nbs", m), prev_s)
    # byte-identical duplicate of July
    files["northbank/northbank_checking_2026-07_copy.csv"] = files["northbank/northbank_checking_2026-07.csv"]
    # overlapping Aug+Sep export with one descriptor variant
    files["northbank/northbank_checking_2026-08_09_overlap.csv"] = northbank(
        "nbc", rows_for("nbc", "2026-08") + rows_for("nbc", "2026-09"), closing("nbc", "2026-07"), OVERLAP_VARIANT)

    for m in MONTHS:
        prev = closing("rvc", f"2026-{int(m[-2:]) - 1:02d}") if m != "2026-07" else ACCOUNTS["rvc"].opening_cents
        files[f"riverside/riverside_checking_{m}.csv"] = riverside(rows_for("rvc", m), prev)
    files["riverside/riverside_checking_2026-07_eur.csv"] = files["riverside/riverside_checking_2026-07.csv"].replace(",USD", ",EUR")

    for m in MONTHS:
        rows = rows_for("sv", m)
        if m == "2026-08":
            rows = rows + [PENDING_AMAZON]
        files[f"summit/summit_visa_{m}.csv"] = summit(rows)
        files[f"summit/summit_amex_{m}.csv"] = summit(rows_for("sa", m))

    files["harbor/harbor_brokerage_2026-Q3.csv"] = harbor(rows_for("hb"), ACCOUNTS["hb"].opening_cents)

    vm_rows = sorted(rows_for("vm") + [VENMO_BANK_FUNDED], key=lambda r: r.posted)
    files["venmo/venmo_2026-Q3.csv"] = venmo(vm_rows)

    for path, text in files.items():
        write(path, text)
    return files


# ---------------------------------------------------------------------------------------------
# EXPECTED.md
# ---------------------------------------------------------------------------------------------

def expected_md(files: dict[str, str]) -> str:
    out: list[str] = []
    p = out.append
    p("# Fixture known answers")
    p("")
    p("Generated by `fixtures/generate.py` from one explicit row list; every figure below is derived from")
    p("that list with plain arithmetic (running balances are shown so they can be checked by eye). The")
    p("engine is wrong until the fixture is shown wrong, and that showing is an ADR (ADR-0028).")
    p("")
    p("Period: 2026-07-01 through 2026-09-30. Money in dollars with cents; the engine holds integer cents.")
    p("")

    # Accounts
    p("## Accounts")
    p("")
    p("| key | name | institution | kind | opening date | opening balance | firewalled |")
    p("|---|---|---|---|---|---|---|")
    for a in ACCOUNTS.values():
        p(f"| {a.key} | {a.name} | {a.institution} | {a.kind} | {a.opening_date} | {money(a.opening_cents)} | {'yes' if a.firewalled else 'no'} |")
    p("")

    # Files
    p("## Files")
    p("")
    p("| file | account | profile | rows | note |")
    p("|---|---|---|---|---|")
    notes = {
        "northbank/northbank_checking_2026-07_copy.csv": "byte-identical to the July file: file-level no-op",
        "northbank/northbank_checking_2026-08_09_overlap.csv": "Aug+Sep rows again; one descriptor variant → quarantine",
        "riverside/riverside_checking_2026-07_eur.csv": "Currency column reads EUR: whole batch rejected (Unsupported)",
        "summit/summit_visa_2026-08.csv": "carries the pending AMAZON.COM row",
        "summit/summit_visa_2026-09.csv": "carries the same Amazon row posted on 2026-09-01",
        "venmo/venmo_2026-Q3.csv": "two preamble lines; one bank-funded row skipped by the profile",
    }
    profile_of = {"northbank": "northbank_csv", "riverside": "riverside_csv", "summit": "summit_card_csv", "harbor": "harbor_brokerage_csv", "venmo": "venmo_csv"}
    acct_of_file = {}
    for path in sorted(files):
        inst = path.split("/")[0]
        base = os.path.basename(path)
        key = {"northbank_checking": "nbc", "northbank_savings": "nbs", "riverside_checking": "rvc", "summit_visa": "sv",
               "summit_amex": "sa", "harbor_brokerage": "hb", "venmo": "vm"}[base.rsplit("_20", 1)[0]]
        acct_of_file[path] = key
        data_rows = len([ln for ln in files[path].strip().split("\n")]) - (3 if inst == "venmo" else 1)
        p(f"| `{path}` | {key} | {profile_of[inst]} | {data_rows} | {notes.get(path, '')} |")
    p("")

    # Per-account ledgers with running balances
    p("## Ledger per account (posted rows, running balance)")
    p("")
    for a in ACCOUNTS.values():
        p(f"### {a.name} (`{a.key}`)")
        p("")
        p(f"Opening {a.opening_date}: **{money(a.opening_cents)}**")
        p("")
        p("| posted | effective | description | amount | running | role |")
        p("|---|---|---|---:|---:|---|")
        bal = a.opening_cents
        for r in rows_for(a.key):
            bal += r.amount
            role = r.category
            if r.pair:
                role += f" · pair `{r.pair}`"
            if r.refund_of:
                role += f" · refund of `{r.refund_of}`"
            p(f"| {r.posted} | {r.eff} | {r.description} | {money(r.amount)} | {money(bal)} | {role} |")
        p("")
        p("Closing per statement month:")
        p("")
        for m in MONTHS:
            p(f"- {m}: **{money(closing(a.key, m))}**")
        p("")

    # Dedup
    p("## Dedup outcomes (M1)")
    p("")
    p("Import order: every monthly/quarterly file once, then the duplicate, overlap and EUR files.")
    p("")
    total_rows = len(ROWS)
    p(f"- Rows in the ledger after importing each file once: **{total_rows}** (the Visa pending row counts once: it is")
    p("  inserted pending from the August file and updated to posted by the September file).")
    p("- `northbank_checking_2026-07_copy.csv`: same account, profile and SHA-256 → batch recorded with inserted 0,")
    p(f"  skipped {len(rows_for('nbc', '2026-07'))}, reason `duplicate_file_of_batch`.")
    aug_sep = len(rows_for("nbc", "2026-08")) + len(rows_for("nbc", "2026-09"))
    p(f"- `northbank_checking_2026-08_09_overlap.csv`: {aug_sep} rows; {aug_sep - 1} skipped by `source_row_hash`;")
    v = OVERLAP_VARIANT
    sim = jaro_winkler(normalize(v[1]), normalize(v[2]))
    p(f"  1 quarantined: `{v[2]}` {v[0]} vs ledger `{v[1]}` — same account, same amount, same date,")
    p(f"  payee_norm `{normalize(v[2])}` vs `{normalize(v[1])}`, Jaro–Winkler {sim:.4f} ≥ 0.85, both posted → suspected duplicate, not inserted.")
    pend_sim = jaro_winkler(normalize(PENDING_AMAZON.description), normalize("AMAZON.COM*2K4 AMZN.COM/BILL"))
    p(f"- Visa pending → posted: August row `{PENDING_AMAZON.description}` pending {PENDING_AMAZON.posted} and September row")
    p(f"  `AMAZON.COM*2K4 AMZN.COM/BILL` posted 2026-09-01: same amount, 2 days apart, payee_norm `{normalize(PENDING_AMAZON.description)}`")
    p(f"  vs `{normalize('AMAZON.COM*2K4 AMZN.COM/BILL')}`, Jaro–Winkler {pend_sim:.4f} → the pending row is updated to posted 2026-09-01 with the")
    p("  new descriptor; the September batch reports updated 1, inserted 8.")
    p(f"- `venmo_2026-Q3.csv`: 4 data rows; 1 skipped by the profile's funding-source rule ({VENMO_BANK_FUNDED.external_id},")
    p(f"  funded from `{VENMO_BANK_FUNDED.extra['funding']}`), 3 inserted; every inserted row flagged `payment_app_unknown | needs_review`.")
    p("- `riverside_checking_2026-07_eur.csv`: rejected before any write with `Unsupported` (currency EUR).")
    p("- Re-importing every file a second time: 0 inserted, 0 updated, 0 user fields changed (`import_idempotent`).")
    p("")

    # Normalisation table
    p("## payee_norm for descriptors the tests check (ADR-0018)")
    p("")
    p("| descriptor | payee_norm |")
    p("|---|---|")
    for d in ["AMAZON.COM*2K4 AMZN.COM/BILL", "AMAZON.COM", "JEWEL-OSCO #3421", "JEWEL-OSCO #3421 CHICAGO", "TRADER JOE S #702",
              "SHELL OIL 57442", "ONLINE TRANSFER TO SAV ...5678", "PAYMENT - THANK YOU", "WALGREENS #5821", "ZELLE PAYMENT FROM MORGAN AVERY"]:
        p(f"| `{d}` | `{normalize(d)}` |")
    p("")

    # Transfer pairs (M2)
    p("## Transfer pairs (M2)")
    p("")
    p("| pair | out | in | kind |")
    p("|---|---|---|---|")
    pairs: dict[str, list[Row]] = {}
    for r in ROWS:
        if r.pair:
            pairs.setdefault(r.pair, []).append(r)
    for pid, legs in pairs.items():
        out_leg = next(l for l in legs if l.amount < 0)
        in_leg = next(l for l in legs if l.amount > 0)
        kind = "card_payment" if ACCOUNTS[in_leg.account].kind == "credit" else "internal"
        p(f"| `{pid}` | {out_leg.account} {out_leg.posted} {money(out_leg.amount)} | {in_leg.account} {in_leg.posted} {money(in_leg.amount)} | {kind} |")
    p("")
    p("Refund link (M2): Visa `TARGET 00012345` 2026-08-20 +84.17 ↔ Visa `TARGET 00012345` 2026-08-09 −84.17 (exact payee and amount, 11 days).")
    p("")

    # Spending vs cash (M2)
    p("## Spending view vs cash view, 2026-07-01..2026-09-30 (M2)")
    p("")
    spend_rows = [r for r in ROWS if r.pair is None and not r.category.startswith("income.")
                  and r.category != "transfer.securities_sale_proceeds"]
    spending_out = -sum(r.amount for r in spend_rows if r.amount < 0)
    refunds = sum(r.amount for r in spend_rows if r.amount > 0 and r.refund_of)
    reimburse = sum(r.amount for r in spend_rows if r.amount > 0 and not r.refund_of and r.category != "review")
    review_in = sum(r.amount for r in spend_rows if r.amount > 0 and r.category == "review")
    p("Spending view = every non-transfer, non-income, non-proceeds leaf row, by category; linked refunds and")
    p("same-category reimbursements net against their category; rows still in review are listed separately.")
    p("")
    p(f"- Gross outflows: **{money(spending_out)}**")
    p(f"- Linked refunds: {money(refunds)} (Target return)")
    p(f"- Same-category reimbursements: {money(reimburse)} (roommate rent split, 3 × 1,200.00)")
    p(f"- Net spending: **{money(spending_out - refunds - reimburse)}**")
    p(f"- Positive rows awaiting review (not spending, not income until classified): {money(review_in)}")
    p("")
    cash_accounts = {k for k, a in ACCOUNTS.items() if a.kind in ("checking", "savings", "cash", "payment_app")}
    cash_rows = [r for r in ROWS if r.account in cash_accounts]
    pair_accounts = {}
    for pid, legs in pairs.items():
        pair_accounts[pid] = {l.account for l in legs}
    def crosses_out_of_cash(r: Row) -> bool:
        return r.pair is not None and not pair_accounts[r.pair] <= cash_accounts
    cash_out = -sum(r.amount for r in cash_rows if r.amount < 0 and (r.pair is None or crosses_out_of_cash(r)))
    cash_in = sum(r.amount for r in cash_rows if r.amount > 0 and (r.pair is None or crosses_out_of_cash(r)))
    p("Cash view = rows on cash accounts (nbc, nbs, rvc, vm) by date; transfers between two cash accounts net")
    p("to zero and are excluded; card payments and the brokerage deposit count on their own dates.")
    p("")
    p(f"- Cash outflows: **{money(cash_out)}**")
    p(f"- Cash inflows: **{money(cash_in)}**")
    net_cash = sum(closing(k, '2026-09') - ACCOUNTS[k].opening_cents for k in cash_accounts)
    p(f"- Net change in cash accounts (Σ closing − opening): **{money(net_cash)}** = inflows − outflows ({money(cash_in - cash_out)})")
    p(f"- Spending-view gross outflows minus cash-view outflows: **{money(spending_out - cash_out)}**")
    p("  (card purchases counted on the cards vs card payments counted on the bank, plus brokerage-side rows).")
    p("")

    # Plan inputs for M4 (defined now, numbers derived later from these definitions)
    p("## Plan inputs (defined now for M4/M5; their answers are appended at those milestones)")
    p("")
    p("- As-of date for safe-to-spend: **2026-09-30**. Timing buffer: **500.00**.")
    p("- Confirmed income: `Meridian payroll`, biweekly, anchor 2026-07-10, 3,412.77 net, deposit nbc →")
    p("  next confirmed income date **2026-10-02**.")
    p("- Confirmed obligations: rent 2,400.00 monthly day 1 (nbc); Xfinity 89.99 day 12; T-Mobile 75.00 day 18;")
    p("  ComEd variable (expected 125.00 ± 25.00) day 7; Peoples Gas (expected 40.00 ± 10.00) day 21;")
    p("  GEICO annual 1,284.00 on 09-22; Visa minimum and Amex minimum as debt minimums (M6).")
    p("- Earmarks: rent earmark funded 1,200.00 per paycheck from nbc; emergency reserve 12,000.00 in nbs.")
    p("")
    return "\n".join(out) + "\n"


def main() -> None:
    files = emit_csvs()
    write("EXPECTED.md", expected_md(files))
    print(f"wrote {len(files)} csv files and EXPECTED.md ({len(ROWS)} ledger rows)")


if __name__ == "__main__":
    main()
