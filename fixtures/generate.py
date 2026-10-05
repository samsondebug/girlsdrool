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

import json
import os
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta

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
# M2: the ordered rule set (payee_norm contains → category, optional venture) and the heuristics
# that run after it. Written down here, emitted to rules.json for the tests and to EXPECTED.md.
# ---------------------------------------------------------------------------------------------

VENTURE = {"name": "Ledgerline", "status": "fund", "cash_cap_cents": 500_000}

RULES: list[tuple[str, str, str, str | None]] = [
    # (name, payee_norm contains, category code, venture)
    ("Rent", "lakeshore properties", "fixed.rent", None),
    ("Rent share from Morgan", "zelle payment from morgan avery", "fixed.rent", None),
    ("ComEd", "comed", "fixed.utilities", None),
    ("Peoples Gas", "peoples gas", "fixed.utilities", None),
    ("Payroll", "meridian cap", "income.salary", None),
    ("Xfinity", "xfinity", "fixed.internet", None),
    ("T-Mobile", "t mobile", "fixed.phone", None),
    ("Jewel-Osco", "jewel osco", "variable.groceries", None),
    ("Trader Joe's", "trader joe", "variable.groceries", None),
    ("Costco", "costco", "variable.groceries", None),
    ("Shell", "shell oil", "variable.fuel", None),
    ("Chipotle", "chipotle", "variable.dining", None),
    ("Restaurants abroad", "restaurante", "variable.dining", None),
    ("Amazon", "amazon", "variable.shopping", None),
    ("Target", "target", "variable.shopping", None),
    ("Uber", "uber", "variable.transport", None),
    ("Walgreens", "walgreens", "variable.health", None),
    ("Netflix", "netflix", "fixed.subscriptions", None),
    ("GEICO annual", "geico", "irregular.insurance", None),
    ("Freelance", "freelance invoice", "income.other", None),
    ("Savings interest", "interest payment", "income.interest", None),
    ("Dividends", "dividend", "income.interest", None),
    ("Linear (Ledgerline)", "linear app", "venture.operating_expense", "Ledgerline"),
    ("Vercel (Ledgerline)", "vercel", "venture.operating_expense", "Ledgerline"),
    ("Repayment to Chris", "zelle payment to chris park", "transfer.loan_repayment", None),
]

# M3: the mutated August export transposes two digits of one amount (−112.06 → −121.06), so the
# ledger built from it is 9.00 short of the bank's statement closing.
MUTATION = ("2026-08-13", "JEWEL-OSCO #3421", -12_106)

# ---------------------------------------------------------------------------------------------
# M4: the plan (one confirmed income stream, six confirmed obligations, two earmarks) and the
# safe-to-spend answer as of AS_OF, hand-computed here from ROWS and these definitions.
# ---------------------------------------------------------------------------------------------

AS_OF = "2026-09-30"
TIMING_BUFFER_CENTS = 50_000
MATCH_BEFORE_DAYS, MATCH_AFTER_DAYS = 10, 5      # a receipt/payment posts within [due−10, due+5]
OVERDUE_LOOKBACK_DAYS = 120
UPCOMING_DAYS = 14

INCOME_STREAMS = [
    dict(name="Meridian payroll", kind="base", cycle="biweekly", anchor_date="2026-07-10", expected_net_cents=341_277,
         variability_cents=0, confidence="confirmed", weekend_rule="previous_business_day", deposit_account="nbc",
         match_payee_contains="meridian cap"),
]

OBLIGATIONS = [
    dict(name="Rent", kind="bill", due_rule="monthly_day", due_day=1, due_month=None, expected_cents=240_000, variability_cents=0,
         source_account="nbc", autopay=False, category="fixed.rent", match_payee_contains="lakeshore properties"),
    dict(name="ComEd", kind="bill", due_rule="monthly_day", due_day=7, due_month=None, expected_cents=12_500, variability_cents=2_500,
         source_account="nbc", autopay=True, category="fixed.utilities", match_payee_contains="comed"),
    dict(name="Xfinity", kind="bill", due_rule="monthly_day", due_day=12, due_month=None, expected_cents=8_999, variability_cents=0,
         source_account="nbc", autopay=True, category="fixed.internet", match_payee_contains="xfinity"),
    dict(name="T-Mobile", kind="bill", due_rule="monthly_day", due_day=18, due_month=None, expected_cents=7_500, variability_cents=0,
         source_account="nbc", autopay=True, category="fixed.phone", match_payee_contains="t mobile"),
    dict(name="Peoples Gas", kind="bill", due_rule="monthly_day", due_day=21, due_month=None, expected_cents=4_000, variability_cents=1_000,
         source_account="nbc", autopay=True, category="fixed.utilities", match_payee_contains="peoples gas"),
    dict(name="GEICO annual", kind="bill", due_rule="annual", due_day=22, due_month=9, expected_cents=128_400, variability_cents=0,
         source_account="nbc", autopay=False, category="irregular.insurance", match_payee_contains="geico"),
]

EARMARKS = [
    dict(name="Rent", kind="obligation", funding_account="nbc", obligation="Rent", target_cents=240_000, target_date=None,
         schedule="per_paycheck", schedule_amount_cents=120_000, schedule_day=None, schedule_income_stream="Meridian payroll",
         entries=[("2026-09-18", "fund", 120_000, "first per-paycheck funding, from the 09-18 pay")]),
    dict(name="Emergency reserve", kind="emergency_reserve", funding_account="nbs", obligation=None, target_cents=1_200_000,
         target_date=None, schedule="none", schedule_amount_cents=None, schedule_day=None, schedule_income_stream=None,
         entries=[("2026-07-01", "adjust", 1_200_000, "existing savings set aside")]),
]

CASH_KINDS = ("checking", "savings", "cash", "payment_app")

# ---------------------------------------------------------------------------------------------
# M5: the forecast as of AS_OF — variable-spend model, 91-day daily engine, scenarios — computed
# here from ROWS and the plan, emitted to EXPECTED.md and forecast.json.
# ---------------------------------------------------------------------------------------------

HORIZON_DAYS = 90        # the engine runs days 0..=90; the 30-day view is days 0..29, the 13-week view is 13 seven-day buckets
BUCKET_DAYS = 30
PAY_SHIFT_DAYS = 7       # downside: the next confirmed base-pay occurrence lands this many civil days late
SURPRISE_BILLS = {
    "downside_bill": ("2026-10-05", 1_500_000),   # dents the committed buffer, never the balance
    "bill": ("2026-10-05", 3_000_000),            # overdraws the balance by a few dollars until the next pay
}
SCENARIOS = {
    "baseline": dict(downside=False, surprise=None),
    "downside": dict(downside=True, surprise=None),
    "downside_bill": dict(downside=True, surprise=SURPRISE_BILLS["downside_bill"]),
    "bill": dict(downside=False, surprise=SURPRISE_BILLS["bill"]),
}
# every child of the variable root, in seed sort order (migrations 0001 and 0003)
VARIABLE_CATEGORIES = ["variable.cash", "variable.uncategorized", "variable.groceries", "variable.dining", "variable.fuel",
                       "variable.transport", "variable.shopping", "variable.health", "variable.entertainment", "variable.personal"]


def allocate(total: int, parts: int) -> list[int]:
    """Largest remainder, earlier parts first — the same answer as money::allocate (total >= 0 here)."""
    base, rem = divmod(total, parts)
    return [base + (1 if i < rem else 0) for i in range(parts)]


def variable_model(as_of: date) -> list[dict]:
    """Per variable category: three 30-day buckets ending yesterday, net outflow of posted, non-transfer rows
    (refunds net against their category) floored at 0, and the median of the three."""
    out = []
    for code in VARIABLE_CATEGORIES:
        buckets = []
        for k in range(3):
            end = as_of - timedelta(days=1 + BUCKET_DAYS * k)
            start = end - timedelta(days=BUCKET_DAYS - 1)
            net = -sum(r.amount for r in ROWS if r.pair is None and automation(r)[0] == code
                       and start <= date.fromisoformat(r.posted) <= end)
            buckets.append({"start": start.isoformat(), "end": end.isoformat(), "net_outflow_cents": max(0, net)})
        out.append({"category": code, "buckets": buckets, "median_cents": sorted(b["net_outflow_cents"] for b in buckets)[1]})
    return out


def forecast_run(scenario: dict, as_of: date, received: set, paid: set, model: list[dict]) -> dict:
    """One scenario of the daily engine (ARCHITECTURE §5.6): events per day, running balance, committed, headroom."""
    horizon = as_of + timedelta(days=HORIZON_DAYS)
    days = [as_of + timedelta(days=i) for i in range(HORIZON_DAYS + 1)]
    a_accounts = [k for k, a in ACCOUNTS.items() if a.kind in CASH_KINDS and not a.firewalled]
    opening = sum(ACCOUNTS[k].opening_cents + sum(r.amount for r in rows_for(k) if r.posted <= as_of.isoformat()) for k in a_accounts)
    events: dict[date, list] = {d: [] for d in days}
    # income: confirmed streams only, unreceived occurrences; the downside shifts the next base-pay occurrence
    pay_dates: dict[str, list[date]] = {}
    for st in INCOME_STREAMS:
        if st["confidence"] != "confirmed":
            continue
        occs = [d for d in income_occurrences(st, as_of, horizon) if (st["name"], d.isoformat()) not in received]
        if scenario["downside"] and st["kind"] == "base" and occs:
            occs[0] = occs[0] + timedelta(days=PAY_SHIFT_DAYS)
        occs = [d for d in occs if d <= horizon]
        pay_dates[st["name"]] = occs
        for d in occs:
            events[d].append(("income", st["name"], st["expected_net_cents"]))
    # obligations: unpaid confirmed occurrences; an overdue one lands on day 0
    for ob in OBLIGATIONS:
        opened = date.fromisoformat(ACCOUNTS[ob["source_account"]].opening_date)
        for due in obligation_occurrences(ob, max(as_of - timedelta(days=OVERDUE_LOOKBACK_DAYS), opened), horizon):
            if (ob["name"], due.isoformat()) in paid:
                continue
            events[max(due, as_of)].append(("obligation", ob["name"], -ob["expected_cents"]))
    # variable spend: each category's model allocated over every 30-day block by largest remainder
    for m in model:
        if m["median_cents"] == 0:
            continue
        parts = allocate(m["median_cents"], BUCKET_DAYS)
        for i, d in enumerate(days):
            events[d].append(("variable", m["category"], -parts[i % BUCKET_DAYS]))
    if scenario["surprise"]:
        sd, cents = scenario["surprise"]
        events[date.fromisoformat(sd)].append(("surprise", "Surprise bill", -cents))
    # committed = buffer + each earmark's remaining projected by its schedule: released when its obligation
    # falls due, funded per paycheck on the scenario's pay dates (capped at the target); none = constant
    remaining = {em["name"]: sum(c for (dt, k, c, n) in em["entries"] if dt <= as_of.isoformat()) for em in EARMARKS}
    series, bal, lowest, first_shortfall, first_breach = [], opening, None, None, None
    for i, d in enumerate(days):
        inflow = sum(c for (_, _, c) in events[d] if c > 0)
        outflow = -sum(c for (_, _, c) in events[d] if c < 0)
        bal = bal + inflow - outflow
        for em in EARMARKS:
            r = remaining[em["name"]]
            if em["obligation"] and any(k == "obligation" and n == em["obligation"] for (k, n, _) in events[d]):
                r -= min(max(0, r), next(o for o in OBLIGATIONS if o["name"] == em["obligation"])["expected_cents"])
            if em["schedule"] == "per_paycheck" and d in pay_dates.get(em["schedule_income_stream"], []):
                r += min(em["schedule_amount_cents"], max(0, em["target_cents"] - max(0, r)))
            remaining[em["name"]] = r
        committed = TIMING_BUFFER_CENTS + sum(max(0, r) for r in remaining.values())
        headroom = bal - committed
        series.append({"day": i, "date": d.isoformat(), "inflows_cents": inflow, "outflows_cents": outflow, "closing_cents": bal,
                       "committed_cents": committed, "headroom_cents": headroom,
                       "events": [{"kind": k, "name": n, "cents": c} for (k, n, c) in events[d]]})
        if lowest is None or bal < lowest["cents"]:
            lowest = {"date": d.isoformat(), "cents": bal}
        if first_shortfall is None and bal < 0:
            first_shortfall = {"date": d.isoformat(), "cents": bal}
        if first_breach is None and headroom < 0:
            first_breach = {"date": d.isoformat(), "cents": headroom}
    inflows_total = sum(x["inflows_cents"] for x in series)
    outflows_total = sum(x["outflows_cents"] for x in series)
    assert series[-1]["closing_cents"] == opening + inflows_total - outflows_total   # forecast_ties
    weeks = []
    for w in range(13):
        chunk = series[7 * w: 7 * w + 7]
        weeks.append({"week": w, "start": chunk[0]["date"], "end": chunk[-1]["date"],
                      "inflows_cents": sum(x["inflows_cents"] for x in chunk), "outflows_cents": sum(x["outflows_cents"] for x in chunk),
                      "closing_cents": chunk[-1]["closing_cents"], "lowest_cents": min(x["closing_cents"] for x in chunk)})
    return {"downside": scenario["downside"],
            "surprise": None if not scenario["surprise"] else {"date": scenario["surprise"][0], "cents": scenario["surprise"][1]},
            "opening_cents": opening, "inflows_cents": inflows_total, "outflows_cents": outflows_total,
            "closing_cents": series[-1]["closing_cents"], "lowest": lowest, "first_shortfall": first_shortfall,
            "first_buffer_breach": first_breach, "pay_dates": {k: [d.isoformat() for d in v] for k, v in pay_dates.items()},
            "days": series, "weeks": weeks}



# ---------------------------------------------------------------------------------------------
# M6: debts and informal loans — the two-debt schedule (and the cards) hand-computed here in
# interest cents, avalanche vs snowball vs custom, the informal_first policy, the 12-month scenario.
# ---------------------------------------------------------------------------------------------

DEBT_EXTRA_CENTS = 30_000       # monthly extra for the comparison: a user input until a review supplies the surplus
DEBT_PERIODS_MAX = 120
INFORMAL_SCENARIO_PERIODS = 12
DEBTS = [
    # linked card debts: owed = max(0, −posted balance of the account) as of the date
    dict(key="visa", name="Summit Visa", kind="credit_card", account="sv", standalone_opening=None, apr_bps=2_499, promo_apr_bps=None,
         promo_end=None, interest_method="monthly_nominal", minimum_rule="interest_plus_percent", minimum_fixed_cents=0, minimum_bps=100,
         minimum_floor_cents=2_500, due_day=19, participation=True, custom_order=3, payment_account="nbc",
         match_payee_contains="summit card services payment"),
    dict(key="amex", name="Summit Amex", kind="credit_card", account="sa", standalone_opening=None, apr_bps=0, promo_apr_bps=None,
         promo_end=None, interest_method="monthly_nominal", minimum_rule="full_balance", minimum_fixed_cents=0, minimum_bps=0,
         minimum_floor_cents=0, due_day=22, participation=True, custom_order=4, payment_account="nbc",
         match_payee_contains="summit card svcs amex pymt"),
    # standalone debts: owed = opening − Σ recorded payments
    dict(key="auto", name="Auto loan", kind="loan", account=None, standalone_opening=320_000, apr_bps=649, promo_apr_bps=None,
         promo_end=None, interest_method="actual_365", minimum_rule="fixed", minimum_fixed_cents=9_500, minimum_bps=0,
         minimum_floor_cents=0, due_day=15, participation=True, custom_order=1, payment_account="nbc",
         match_payee_contains="lakeside auto finance"),
    dict(key="transfer", name="Balance transfer card", kind="credit_card", account=None, standalone_opening=480_000, apr_bps=2_499,
         promo_apr_bps=0, promo_end="2026-12-31", interest_method="monthly_nominal", minimum_rule="percent_of_balance",
         minimum_fixed_cents=0, minimum_bps=200, minimum_floor_cents=2_500, due_day=5, participation=True, custom_order=2,
         payment_account="nbc", match_payee_contains="meridian bank card"),
]
INFORMAL = [
    dict(key="chris", counterparty="Chris Park", original_cents=60_000, borrowed_date="2026-08-05",
         promised_terms="300 on each of the next two paydays", promised_date="2026-09-30",
         proceeds=("vm", "2026-08-05", "Chris Park"), repayment_account="nbc", repayment_needle="zelle payment to chris park",
         schedule=[("2026-08-28", 30_000), ("2026-09-28", 30_000)], participation=True),
    dict(key="mom", counterparty="Mom", original_cents=200_000, borrowed_date="2026-06-15",
         promised_terms="pay it back within the year", promised_date="2027-06-15",
         proceeds=None, repayment_account="nbc", repayment_needle="zelle payment to mom", schedule=[], participation=True),
]
STRATEGIES = ["avalanche", "snowball", "custom"]


def mul_div_round(a: int, b: int, d: int) -> int:
    """a × b / d rounded half away from zero — money::mul_div_round."""
    n = a * b
    q, r = divmod(abs(n), d)
    if 2 * r >= d:
        q += 1
    return q if n >= 0 else -q


def balance_as_of(account: str, as_of: date) -> int:
    return ACCOUNTS[account].opening_cents + sum(r.amount for r in rows_for(account) if r.posted <= as_of.isoformat())


def period_bounds(as_of: date, k: int) -> tuple[date, date]:
    """Period k ≥ 1 is the k-th calendar month after the as-of month."""
    import calendar
    y, m = as_of.year, as_of.month
    for _ in range(k):
        y, m = (y + 1, 1) if m == 12 else (y, m + 1)
    return date(y, m, 1), date(y, m, calendar.monthrange(y, m)[1])


def informal_repayments(loan: dict, as_of: date) -> list[dict]:
    rows = [r for r in rows_for(loan["repayment_account"]) if r.amount < 0]
    taken: set = set()
    out = []
    for due, r in match_rows([date.fromisoformat(d) for (d, _) in loan["schedule"]], rows, 0, 10**9, loan["repayment_needle"], taken):
        if r.posted <= as_of.isoformat():
            out.append({"due_date": due.isoformat(), "account": r.account, "posted": r.posted, "description": r.description, "amount_cents": -r.amount})
    return out


def debt_states(as_of: date) -> list[dict]:
    """Every participating debt with a balance, cards and loans first (in definition order), informal loans after."""
    out = []
    for d in DEBTS:
        owed = max(0, -balance_as_of(d["account"], as_of)) if d["account"] else d["standalone_opening"]
        out.append(dict(d, owed_cents=owed, informal=False, schedule=[]))
    for loan in INFORMAL:
        paid = sum(x["amount_cents"] for x in informal_repayments(loan, as_of))
        out.append(dict(key=loan["key"], name=f"Loan from {loan['counterparty']}", kind="informal", apr_bps=0, promo_apr_bps=None,
                        promo_end=None, interest_method="monthly_nominal", minimum_rule="none", minimum_fixed_cents=0, minimum_bps=0,
                        minimum_floor_cents=0, due_day=None, participation=loan["participation"], custom_order=None,
                        owed_cents=max(0, loan["original_cents"] - paid), informal=True, promised_date=loan["promised_date"],
                        schedule=loan["schedule"]))
    return out


def period_interest(d: dict, opening: int, start: date, end: date) -> int:
    apr = d["promo_apr_bps"] if d["promo_apr_bps"] is not None and d["promo_end"] and start.isoformat() <= d["promo_end"] else d["apr_bps"]
    if d["interest_method"] == "actual_365":
        return mul_div_round(opening, apr * ((end - start).days + 1), 3_650_000)
    return mul_div_round(opening, apr, 120_000)


def effective_apr(d: dict, start: date) -> int:
    return d["promo_apr_bps"] if d["promo_apr_bps"] is not None and d["promo_end"] and start.isoformat() <= d["promo_end"] else d["apr_bps"]


def period_minimum(d: dict, opening: int, interest: int, start: date, end: date) -> int:
    rule = d["minimum_rule"]
    if rule == "fixed":
        m = d["minimum_fixed_cents"]
    elif rule == "percent_of_balance":
        m = max(d["minimum_floor_cents"], mul_div_round(opening, d["minimum_bps"], 10_000))
    elif rule == "interest_plus_percent":
        m = interest + max(d["minimum_floor_cents"], mul_div_round(opening, d["minimum_bps"], 10_000))
    elif rule == "full_balance":
        m = opening + interest
    else:  # none: an informal loan's schedule rows falling due in the period
        m = sum(c for (dt, c) in d["schedule"] if start.isoformat() <= dt <= end.isoformat())
    return min(m, opening + interest)


def run_strategy(strategy: str, as_of: date, extra: int) -> dict:
    """ARCHITECTURE §5.8: every open debt gets its minimum; the budget (extra + first-period minimums,
    constant) pays informal loans first (earliest promised date), then the strategy's target."""
    debts = [dict(d, balance=d["owed_cents"], rows=[], payoff=None, interest_total=0) for d in debt_states(as_of)
             if d["participation"] and d["owed_cents"] > 0]
    budget = None
    for k in range(1, DEBT_PERIODS_MAX + 1):
        open_debts = [d for d in debts if d["balance"] > 0]
        if not open_debts:
            break
        start, end = period_bounds(as_of, k)
        for d in open_debts:
            d["opening"] = d["balance"]
            d["interest"] = period_interest(d, d["opening"], start, end)
            d["minimum"] = period_minimum(d, d["opening"], d["interest"], start, end)
            d["payment"] = d["minimum"]
        if budget is None:
            budget = extra + sum(d["minimum"] for d in open_debts)
        pool = budget - sum(d["payment"] for d in open_debts)
        informal = sorted([d for d in open_debts if d["informal"]], key=lambda d: (d["promised_date"] or "9999-12-31", d["name"]))
        others = [d for d in open_debts if not d["informal"]]
        if strategy == "avalanche":
            others.sort(key=lambda d: (-effective_apr(d, start), d["opening"], d["name"]))
        elif strategy == "snowball":
            others.sort(key=lambda d: (d["opening"], d["name"]))
        else:
            others.sort(key=lambda d: (d["custom_order"] if d["custom_order"] is not None else 10**9, d["name"]))
        for d in informal + others:
            room = d["opening"] + d["interest"] - d["payment"]
            add = min(max(0, pool), room)
            d["payment"] += add
            pool -= add
        for d in open_debts:
            d["balance"] = d["opening"] + d["interest"] - d["payment"]
            d["interest_total"] += d["interest"]
            d["rows"].append({"period": k, "start": start.isoformat(), "end": end.isoformat(), "opening_cents": d["opening"],
                              "interest_cents": d["interest"], "minimum_cents": d["minimum"], "payment_cents": d["payment"],
                              "closing_cents": d["balance"]})
            if d["balance"] == 0 and d["payoff"] is None:
                d["payoff"] = end.isoformat()
    return {"strategy": strategy, "extra_cents": extra, "budget_cents": budget or extra,
            "total_interest_cents": sum(d["interest_total"] for d in debts),
            "payoff_date": max((d["payoff"] for d in debts if d["payoff"]), default=None),
            "debts": [{"key": d["key"], "name": d["name"], "informal": d["informal"], "owed_cents": d["owed_cents"],
                       "total_interest_cents": d["interest_total"], "payoff_date": d["payoff"], "periods": d["rows"]} for d in debts]}


def informal_scenario(as_of: date, extra: int) -> dict:
    run = run_strategy("avalanche", as_of, extra)   # informal_first makes the informal payoff strategy-independent
    loans = [d for d in run["debts"] if d["informal"]]
    gap = sum(next((r["closing_cents"] for r in d["periods"] if r["period"] == INFORMAL_SCENARIO_PERIODS), 0) for d in loans)
    payoff = max((d["payoff_date"] for d in loans if d["payoff_date"]), default=None)
    return {"extra_cents": extra, "periods": INFORMAL_SCENARIO_PERIODS, "remaining_cents": sum(d["owed_cents"] for d in loans),
            "achievable": bool(loans) and all(d["payoff_date"] is not None and d["periods"][-1]["period"] <= INFORMAL_SCENARIO_PERIODS for d in loans),
            "gap_cents": gap, "payoff_date": payoff}


# ---------------------------------------------------------------------------------------------
# M7: the venture — Ledgerline's details, the account the person marks as venture-owned, and the
# rollup hand-computed from ROWS: buckets, operating cash flow, cap used and utilization, the
# milestone countdown, the stop-condition alert, and venture spend as a share of take-home.
# ---------------------------------------------------------------------------------------------

VENTURE_DETAILS = dict(status="fund", cash_cap_cents=500_000, time_budget_hours=120, milestone="First paying customer",
                       milestone_date="2026-12-31", stop_condition="Kill if no paying customer by the milestone date or the cap is used up")
VENTURE_ACCOUNT = "sa"            # the Summit Amex carries only Ledgerline charges and is paid from personal checking
TRAILING_MONTHS = 12
VENTURE_BUCKETS = ["customer_revenue", "operating_expense", "owner_contribution", "financing", "withdrawal"]


def venture_rollup(as_of: date) -> dict:
    """ARCHITECTURE §5.9. Rows tagged to the venture bucket by category code; a transfer between a
    personal account and a venture-owned one is an owner contribution (into the venture) or a
    withdrawal (out of it) whatever its stored kind."""
    window_start = date(as_of.year - 1, as_of.month, as_of.day)
    in_window = lambda r: window_start < date.fromisoformat(r.posted) <= as_of
    buckets = {b: {"cents": 0, "rows": 0} for b in VENTURE_BUCKETS}
    from_personal = 0
    for r in ROWS:
        if not in_window(r):
            continue
        cat, _, _ = automation(r)
        if cat and cat.startswith("venture.") and r.pair is None:
            b = cat.split(".", 1)[1]
            buckets[b]["cents"] += abs(r.amount)
            buckets[b]["rows"] += 1
            if b == "operating_expense" and r.account != VENTURE_ACCOUNT:
                from_personal += abs(r.amount)
        elif r.pair is not None and r.account == VENTURE_ACCOUNT:
            legs = pair_legs(r.pair)
            other = next(l for l in legs if l.account != VENTURE_ACCOUNT)
            if other.account != VENTURE_ACCOUNT:
                b = "owner_contribution" if r.amount > 0 else "withdrawal"
                buckets[b]["cents"] += abs(r.amount)
                buckets[b]["rows"] += 1
    ocf = buckets["customer_revenue"]["cents"] - buckets["operating_expense"]["cents"]
    cap_used = buckets["owner_contribution"]["cents"] + from_personal - buckets["withdrawal"]["cents"]
    cap = VENTURE_DETAILS["cash_cap_cents"]
    utilization_bps = mul_div_round(cap_used, 10_000, cap) if cap else 0
    milestone = date.fromisoformat(VENTURE_DETAILS["milestone_date"])
    alerts = []
    if cap_used >= cap:
        alerts.append("cap used")
    if milestone < as_of:
        alerts.append("milestone date passed")
    take_home = sum(r.amount for r in ROWS if in_window(r) and automation(r)[0] == "income.salary")
    share_bps = mul_div_round(buckets["operating_expense"]["cents"], 10_000, take_home) if take_home else 0
    venture_balances = [{"account": VENTURE_ACCOUNT, "balance_cents": balance_as_of(VENTURE_ACCOUNT, as_of)}]
    return {"name": VENTURE["name"], "window_start": window_start.isoformat(), "buckets": buckets,
            "operating_expense_from_personal_cents": from_personal, "operating_cash_flow_cents": ocf,
            "cap_cents": cap, "cap_used_cents": cap_used, "cap_remaining_cents": cap - cap_used, "cap_utilization_bps": utilization_bps,
            "milestone_days": (milestone - as_of).days, "alerts": alerts, "take_home_cents": take_home,
            "spend_share_bps": share_bps, "accounts": venture_balances}


# ---------------------------------------------------------------------------------------------
# M8: the weekly review as of AS_OF over the whole fixture state (rules, imports, reconciliations,
# plan, debts, venture): the dependable surplus (ARCHITECTURE §5.10), what each step shows, the
# three actions the person commits, and the snapshot a completion stores.
# ---------------------------------------------------------------------------------------------

SURPLUS_INCOME_DAYS = 90
REVIEW_HORIZON_DAYS = 14
REVIEW_ACTIONS = [
    "Acknowledge the Harbor transfer and classify the two ATM withdrawals",
    "Set up autopay for the auto loan so the 15th never slips",
    "Send Mom the 300-a-month plan drafted under Debts",
]


def monthly_equivalent(ob: dict) -> int:
    """ARCHITECTURE §5.10: monthly rules ×1, biweekly ×26/12, weekly ×52/12; annual and once are irregular (0 here)."""
    rule = ob["due_rule"]
    if rule in ("monthly_day", "nth_weekday"):
        return ob["expected_cents"]
    if rule == "biweekly":
        return mul_div_round(ob["expected_cents"], 26, 12)
    if rule == "weekly":
        return mul_div_round(ob["expected_cents"], 52, 12)
    return 0


# ---------------------------------------------------------------------------------------------
# M9: the same rows as OFX/QFX exports (one SGML, one XML), the backup/restore roundtrip and the
# audit pack, hand-stated here.
# ---------------------------------------------------------------------------------------------

OFX_FILES = [
    # (file, account, month, form, bank id, account id)
    ("riverside/riverside_checking_2026-08.qfx", "rvc", "2026-08", "sgml", "071000000", "2222333344"),
    ("northbank/northbank_savings_2026-09.ofx", "nbs", "2026-09", "xml", "071000013", "0000005678"),
]
SCHEMA_TABLES = 35          # user tables in migrations 0001..0004 (txn_leaf is a view)


def fitid(account: str, row: Row, seq: int) -> str:
    return f"{account.upper()}-{row.posted.replace('-', '')}-{seq:04d}"


def ofx_amount(cents: int) -> str:
    return plain(cents)


def ofx_date(iso: str) -> str:
    return iso.replace("-", "")


def ofx_trntype(cents: int) -> str:
    return "CREDIT" if cents > 0 else "DEBIT"


def ofx_text(account: str, month: str, form: str, bank_id: str, acct_id: str) -> str:
    rows = rows_for(account, month)
    closing_cents = closing(account, month)
    end = month_end(month)
    kind = {"checking": "CHECKING", "savings": "SAVINGS"}[ACCOUNTS[account].kind]
    trns = []
    for i, r in enumerate(rows, 1):
        fields = [("TRNTYPE", ofx_trntype(r.amount)), ("DTPOSTED", ofx_date(r.posted))]
        if r.effective and r.effective != r.posted:
            fields.append(("DTUSER", ofx_date(r.effective)))
        fields += [("TRNAMT", ofx_amount(r.amount)), ("FITID", fitid(account, r, i)), ("NAME", r.description)]
        if r.note:
            fields.append(("MEMO", r.note))
        trns.append(fields)
    if form == "sgml":
        out = ["OFXHEADER:100", "DATA:OFXSGML", "VERSION:102", "SECURITY:NONE", "ENCODING:USASCII", "CHARSET:1252",
               "COMPRESSION:NONE", "OLDFILEUID:NONE", "NEWFILEUID:NONE", "", "<OFX>",
               f"<SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO</STATUS><DTSERVER>{ofx_date(end)}120000<LANGUAGE>ENG"
               f"<FI><ORG>{ACCOUNTS[account].institution}<FID>{bank_id[-4:]}</FI></SONRS></SIGNONMSGSRSV1>",
               "<BANKMSGSRSV1><STMTTRNRS><TRNUID>1<STATUS><CODE>0<SEVERITY>INFO</STATUS>",
               f"<STMTRS><CURDEF>USD<BANKACCTFROM><BANKID>{bank_id}<ACCTID>{acct_id}<ACCTTYPE>{kind}</BANKACCTFROM>",
               f"<BANKTRANLIST><DTSTART>{ofx_date(month + '-01')}<DTEND>{ofx_date(end)}"]
        for fields in trns:
            out.append("<STMTTRN>" + "".join(f"<{k}>{v}" for k, v in fields) + "</STMTTRN>")
        out += ["</BANKTRANLIST>", f"<LEDGERBAL><BALAMT>{ofx_amount(closing_cents)}<DTASOF>{ofx_date(end)}</LEDGERBAL>",
                "</STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>", ""]
        return "\n".join(out)
    out = ['<?xml version="1.0" encoding="UTF-8" standalone="no"?>',
           '<?OFX OFXHEADER="200" VERSION="220" SECURITY="NONE" OLDFILEUID="NONE" NEWFILEUID="NONE"?>',
           "<OFX>", "<SIGNONMSGSRSV1><SONRS><STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>",
           f"<DTSERVER>{ofx_date(end)}120000</DTSERVER><LANGUAGE>ENG</LANGUAGE><FI><ORG>{ACCOUNTS[account].institution}</ORG><FID>{bank_id[-4:]}</FID></FI></SONRS></SIGNONMSGSRSV1>",
           "<BANKMSGSRSV1><STMTTRNRS><TRNUID>1</TRNUID><STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>",
           f"<STMTRS><CURDEF>USD</CURDEF><BANKACCTFROM><BANKID>{bank_id}</BANKID><ACCTID>{acct_id}</ACCTID><ACCTTYPE>{kind}</ACCTTYPE></BANKACCTFROM>",
           f"<BANKTRANLIST><DTSTART>{ofx_date(month + '-01')}</DTSTART><DTEND>{ofx_date(end)}</DTEND>"]
    for fields in trns:
        out.append("<STMTTRN>" + "".join(f"<{k}>{v}</{k}>" for k, v in fields) + "</STMTTRN>")
    out += ["</BANKTRANLIST>", f"<LEDGERBAL><BALAMT>{ofx_amount(closing_cents)}</BALAMT><DTASOF>{ofx_date(end)}</DTASOF></LEDGERBAL>",
            "</STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>", ""]
    return "\n".join(out)


def ofx_answers() -> list[dict]:
    out = []
    for (file, account, month, form, bank_id, acct_id) in OFX_FILES:
        rows = rows_for(account, month)
        out.append({"file": file, "account": account, "month": month, "form": form, "bank_id": bank_id, "acct_id": acct_id,
                    "rows": len(rows), "sum_cents": sum(r.amount for r in rows),
                    "fitids": [fitid(account, r, i) for i, r in enumerate(rows, 1)],
                    "ledger_balance_cents": closing(account, month), "ledger_balance_date": month_end(month),
                    "csv_file": {"rvc": f"riverside/riverside_checking_{month}.csv", "nbs": f"northbank/northbank_savings_{month}.csv"}[account]})
    return out


def business_day_before(x: date) -> date:
    while x.weekday() >= 5:
        x -= timedelta(days=1)
    return x


def clamp_day(y: int, m: int, day: int) -> date:
    import calendar
    return date(y, m, min(day, calendar.monthrange(y, m)[1]))


def income_occurrences(stream: dict, start: date, end: date) -> list[date]:
    step = {"weekly": 7, "biweekly": 14}[stream["cycle"]]
    out, x = [], date.fromisoformat(stream["anchor_date"])
    while x <= end + timedelta(days=7):
        y = business_day_before(x) if stream["weekend_rule"] == "previous_business_day" else x
        if start <= y <= end:
            out.append(y)
        x += timedelta(days=step)
    return out


def obligation_occurrences(ob: dict, start: date, end: date) -> list[date]:
    out = []
    if ob["due_rule"] == "monthly_day":
        y, m = start.year, start.month
        while True:
            x = clamp_day(y, m, ob["due_day"])
            if x > end:
                break
            if x >= start:
                out.append(x)
            y, m = (y + 1, 1) if m == 12 else (y, m + 1)
    elif ob["due_rule"] == "annual":
        for y in range(start.year, end.year + 1):
            x = clamp_day(y, ob["due_month"], ob["due_day"])
            if start <= x <= end:
                out.append(x)
    return out


def match_rows(dues: list[date], rows: list[Row], expected: int, variability: int, needle: str, taken: set) -> list[tuple[date, Row]]:
    """For each due date the closest untaken row (by |posted − due|, then date) whose payee contains
    the needle, whose |amount| is within expected ± variability and which posted inside the window."""
    pairs = []
    for due in dues:
        best = None
        for r in rows:
            if id(r) in taken or needle not in normalize(r.description):
                continue
            if not (expected - variability <= abs(r.amount) <= expected + variability):
                continue
            posted = date.fromisoformat(r.posted)
            if not (due - timedelta(days=MATCH_BEFORE_DAYS) <= posted <= due + timedelta(days=MATCH_AFTER_DAYS)):
                continue
            dist = abs((posted - due).days)
            if best is None or dist < best[0] or (dist == best[0] and r.posted < best[1].posted):
                best = (dist, r)
        if best:
            taken.add(id(best[1]))
            pairs.append((due, best[1]))
    return pairs


def detect_candidates() -> list[dict]:
    """Recurring outflows: ≥ 3 posted, unlinked, non-income/transfer rows per (account, payee_norm) with
    every consecutive gap in 25..36 days and every |amount| within ±25% of the (upper) median."""
    groups: dict[tuple[str, str], list[Row]] = {}
    for r in ROWS:
        if r.amount >= 0 or r.pair or r.category.startswith(("income.", "transfer.")):
            continue
        groups.setdefault((r.account, normalize(r.description)), []).append(r)
    out = []
    for (acct, norm), rows in sorted(groups.items()):
        rows = sorted(rows, key=lambda r: r.posted)
        if len(rows) < 3:
            continue
        gaps = [(date.fromisoformat(b.posted) - date.fromisoformat(a.posted)).days for a, b in zip(rows, rows[1:])]
        if any(g < 25 or g > 36 for g in gaps):
            continue
        amounts = sorted(abs(r.amount) for r in rows)
        median = amounts[len(amounts) // 2]
        if any(abs(a - median) * 4 > median for a in amounts):
            continue
        days = sorted(date.fromisoformat(r.posted).day for r in rows)
        out.append(dict(account=acct, payee_norm=norm, rows=len(rows), due_day=days[len(days) // 2], expected_cents=median,
                        variability_cents=max(abs(a - median) for a in amounts), row_dates=[r.posted for r in rows]))
    return out


PAYMENT_APP_WORDS = ("venmo", "zelle", "cash app", "paypal")


def rule_for(norm: str) -> tuple[str, str, str, str | None] | None:
    for rule in RULES:
        if rule[1] in norm:
            return rule
    return None


def automation(row: Row) -> tuple[str | None, str, set[str]]:
    """What rules → heuristics → linking leave on a row: (category code, why, flags)."""
    norm = normalize(row.description)
    if row.pair:
        kind = "card_payment" if ACCOUNTS[[l.account for l in pair_legs(row.pair) if l.amount > 0][0]].kind == "credit" else "internal"
        flags = set()
        if ACCOUNTS[row.account].firewalled and row.amount < 0:
            flags.add("needs_review")  # firewall touch awaiting acknowledgment
        return f"transfer.{kind}", f"heuristic:{kind}_pair", flags
    rule = rule_for(norm)
    if rule:
        return rule[2], f"rule:{rule[0]}", set()
    if row.kind == "Sell":
        return "transfer.securities_sale_proceeds", "heuristic:securities_sale", {"securities_sale"}
    if "fee" in norm:
        return "debt.fees", "heuristic:fee_charge", {"fee"}
    if "interest" in norm and row.amount < 0:
        return "debt.interest", "heuristic:interest_charge", {"interest"}
    if "interest" in norm and row.amount > 0:
        return "income.interest", "heuristic:interest_income", {"interest"}
    if "atm" in norm:
        return None, "heuristic:atm_withdrawal", {"cash_withdrawal", "needs_review"}
    if row.account == "vm" or any(w in norm for w in PAYMENT_APP_WORDS):
        return None, "heuristic:payment_app_row", {"payment_app_unknown", "needs_review"}
    return None, "unclassified", {"needs_review"}


def pair_legs(pair_id: str) -> list[Row]:
    return [r for r in ROWS if r.pair == pair_id]


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


def month_end(month: str) -> str:
    import calendar
    y, m = int(month[:4]), int(month[5:7])
    return f"{month}-{calendar.monthrange(y, m)[1]:02d}"


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


def northbank(account: str, rows: list[Row], opening: int, variant: tuple | None = None,
              mutate: tuple | None = None) -> str:
    lines = ["Date,Description,Amount,Running Bal."]
    bal = opening
    for r in rows:
        amount = r.amount
        if mutate and (r.posted, r.description) == (mutate[0], mutate[1]):
            amount = mutate[2]
        bal += amount
        desc = r.description
        if variant and (r.posted, r.description) == (variant[0], variant[1]):
            desc = variant[2]
        lines.append(",".join([mdy(r.posted), csv_field(desc), csv_field(money(amount)), csv_field(money(bal))]))
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

def emit_ofx() -> list[str]:
    names = []
    for (file, account, month, form, bank_id, acct_id) in OFX_FILES:
        write(file, ofx_text(account, month, form, bank_id, acct_id))
        names.append(file)
    return names


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
    # August with one amount mutated (M3): never imported alongside the real August file
    files["northbank/northbank_checking_2026-08_mutated.csv"] = northbank(
        "nbc", rows_for("nbc", "2026-08"), closing("nbc", "2026-07"), mutate=MUTATION)

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
        "northbank/northbank_checking_2026-08_mutated.csv": "August with one amount transposed (−112.06 → −121.06): the M3 off scenario, imported instead of the real August file",
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
    def auto_cat(r: Row) -> str | None:
        return automation(r)[0]
    # ARCHITECTURE §5.2: a row whose category root is income or transfer is not spending, whether
    # or not it is one leg of a linked pair (the loan repayments to Chris are transfer-root rows).
    spend_rows = [r for r in ROWS if r.pair is None
                  and not (auto_cat(r) or "").startswith(("income.", "transfer."))]
    spending_out = -sum(r.amount for r in spend_rows if r.amount < 0)
    refunds = sum(r.amount for r in spend_rows if r.amount > 0 and r.refund_of)
    reimburse = sum(r.amount for r in spend_rows if r.amount > 0 and not r.refund_of and auto_cat(r) is not None)
    review_in = sum(r.amount for r in spend_rows if r.amount > 0 and auto_cat(r) is None)
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

    # Rules, heuristics, links and the review queue (M2)
    p("## Rules (M2) — ordered, first match wins, applied before heuristics")
    p("")
    p("`fixtures/rules.json` is the machine-readable copy. Venture `Ledgerline` (fund, cap 5,000.00) must exist.")
    p("")
    p("| # | name | payee_norm contains | category | venture |")
    p("|---|---|---|---|---|")
    for i, (n, m, c, v) in enumerate(RULES, 1):
        p(f"| {i} | {n} | `{m}` | `{c}` | {v or ''} |")
    p("")
    p("Heuristics, in order, for rows no rule matched: `securities_sale` flag → `transfer.securities_sale_proceeds`;")
    p("payee contains `fee` → `debt.fees` + fee flag; `interest` on an outflow → `debt.interest` + interest flag;")
    p("`interest` on an inflow → `income.interest` + interest flag; `atm` → cash_withdrawal + needs_review, no category;")
    p("payment-app rows (Venmo export, or a bank row naming venmo/zelle/cash app/paypal) → payment_app_unknown +")
    p("needs_review, no category. Anything else stays unclassified with needs_review. Transfer and refund links run")
    p("after heuristics; a linked leg takes the transfer category and drops needs_review / payment_app_unknown.")
    p("An outflow on a firewalled account keeps needs_review until it is acknowledged (policy `firewall_exclusion`).")
    p("")
    p("### Automation outcome per row (category, why, flags) — rows that differ from their final category")
    p("")
    p("| account | posted | description | amount | category after M2 | why | flags |")
    p("|---|---|---|---:|---|---|---|")
    for r in ROWS:
        cat, why, flags = automation(r)
        final = None if r.category == "review" else r.category
        if cat != final or flags:
            p(f"| {r.account} | {r.posted} | {r.description} | {money(r.amount)} | {cat or '—'} | {why} | {', '.join(sorted(flags)) or ''} |")
    p("")
    queue = [(r, automation(r)) for r in ROWS]
    queue = [(r, a) for (r, a) in queue if "needs_review" in a[2] or a[0] is None]
    queue.sort(key=lambda t: (-abs(t[0].amount), t[0].posted))
    p(f"### Review queue after M2: {len(queue)} rows, ordered by |amount| descending")
    p("")
    p("| # | account | posted | description | amount | why |")
    p("|---|---|---|---|---:|---|")
    for i, (r, a) in enumerate(queue, 1):
        p(f"| {i} | {r.account} | {r.posted} | {r.description} | {money(r.amount)} | {a[1]} |")
    p("")
    unclassified = [r for (r, a) in queue if a[0] is None]
    p(f"Rows without a category after M2: **{len(unclassified)}** (the firewall-touch transfer leg has its category but still needs its acknowledgment).")
    p("")
    cats = {}
    for r in ROWS:
        cat, why, flags = automation(r)
        cats[cat or "—"] = cats.get(cat or "—", 0) + 1
    p("### Rows per category after M2")
    p("")
    p("| category | rows |")
    p("|---|---:|")
    for k in sorted(cats):
        p(f"| {k} | {cats[k]} |")
    p("")

    # Reconciliation (M3)
    p("## Reconciliation (M3)")
    p("")
    p("A period is `[period_start, period_end]` per account. Its opening is the account's opening balance for the first")
    p("period, else the statement closing of the last **balanced** period (roll-forward); `computed = opening + Σ posted rows`")
    p("in the period, `difference = computed − statement`, `balanced` iff the difference is exactly 0 (ADR-0021). The")
    p("hero's contributing accounts are the cash-kind accounts that are neither firewalled nor archived: "
      + ", ".join(k for k, a in ACCOUNTS.items() if a.kind in ("checking", "savings", "cash", "payment_app") and not a.firewalled) + ".")
    p("")
    p("### Every account, monthly periods (balanced fixture)")
    p("")
    p("| account | period | opening | Σ posted | computed closing | statement closing | status |")
    p("|---|---|---:|---:|---:|---:|---|")
    recon_periods = []
    for a in ACCOUNTS.values():
        prev_close = a.opening_cents
        for m in MONTHS:
            start = a.opening_date if m == "2026-07" else f"{m}-01"
            end = month_end(m)
            total = sum(r.amount for r in rows_for(a.key, m))
            computed = prev_close + total
            statement = closing(a.key, m)
            assert computed == statement
            p(f"| {a.key} | {start}..{end} | {money(prev_close)} | {money(total)} | {money(computed)} | {money(statement)} | balanced |")
            recon_periods.append({"account": a.key, "period_start": start, "period_end": end, "opening_cents": prev_close,
                                  "sum_cents": total, "computed_cents": computed, "statement_cents": statement})
            prev_close = statement
    p("")
    p("### Mutated fixture: `northbank_checking_2026-08_mutated.csv` instead of the real August file")
    p("")
    real = next(r for r in ROWS if (r.account, r.posted, r.description) == ("nbc", MUTATION[0], MUTATION[1]))
    delta = MUTATION[2] - real.amount
    aug_sum = sum(r.amount for r in rows_for("nbc", "2026-08")) + delta
    aug_computed = closing("nbc", "2026-07") + aug_sum
    aug_diff = aug_computed - closing("nbc", "2026-08")
    sep_sum = aug_sum + sum(r.amount for r in rows_for("nbc", "2026-09"))
    sep_computed = closing("nbc", "2026-07") + sep_sum
    sep_diff = sep_computed - closing("nbc", "2026-09")
    p(f"- Row {real.posted} `{real.description}` reads {money(MUTATION[2])} instead of {money(real.amount)} (delta {money(delta)}).")
    p(f"- August: opening {money(closing('nbc', '2026-07'))}, Σ {money(aug_sum)}, computed **{money(aug_computed)}** vs statement")
    p(f"  {money(closing('nbc', '2026-08'))} → difference **{money(aug_diff)}**, status `off`.")
    p(f"- September, entered next with statement {money(closing('nbc', '2026-09'))}: it rolls forward from July (the last balanced")
    p(f"  period), so its period is 2026-08-01..2026-09-30 with opening {money(closing('nbc', '2026-07'))}; computed")
    p(f"  **{money(sep_computed)}** → difference **{money(sep_diff)}**, status `off`. The difference carries until the row is fixed.")
    p("- Undoing the mutated batch and importing the real August file recomputes both periods to `balanced` in the")
    p("  same transaction; no statement is re-entered. The hero is untrusted while nbc is off, naming Northbank Checking.")
    p("")
    aug_rows = rows_for("nbc", "2026-08")
    before = [r for r in rows_for("nbc") if "2026-07-27" <= r.posted <= "2026-07-31"]
    after = [r for r in rows_for("nbc") if "2026-09-01" <= r.posted <= "2026-09-05"]
    p("### Difference explorer for the mutated August period")
    p("")
    p(f"- Rows in the period: **{len(aug_rows)}**, with a running balance from the opening; the mutated row is among them.")
    p(f"- Posted rows within 5 days before the period (2026-07-27..2026-07-31): **{len(before)}** — "
      + "; ".join(f"{r.posted} `{r.description}` {money(r.amount)}" for r in before) + ".")
    p(f"- Posted rows within 5 days after the period (2026-09-01..2026-09-05): **{len(after)}** — "
      + "; ".join(f"{r.posted} `{r.description}` {money(r.amount)}" for r in after) + ".")
    p("- Pending rows on nbc: **0**. Quarantined rows for nbc: **0** (1 when the overlap file was also imported; the explorer lists it).")
    p("")
    as_of = "2026-10-05"
    stale_days = 45
    def days_between(a: str, b: str) -> int:
        from datetime import date
        return (date.fromisoformat(b) - date.fromisoformat(a)).days
    p(f"### Trust as of {as_of} (stale window {stale_days} days, ADR-0021)")
    p("")
    p("| scenario | account | contributes | latest period end | days | status |")
    p("|---|---|---|---|---:|---|")
    contrib = {k: a.kind in ("checking", "savings", "cash", "payment_app") and not a.firewalled for k, a in ACCOUNTS.items()}
    d_sep = days_between("2026-09-30", as_of)
    d_jul = days_between("2026-07-31", as_of)
    trust_rows = []
    for k in ACCOUNTS:
        p(f"| A: every month balanced | {k} | {'yes' if contrib[k] else 'no'} | 2026-09-30 | {d_sep} | reconciled |")
        trust_rows.append({"scenario": "A", "account": k, "contributes": contrib[k], "latest_period_end": "2026-09-30", "days": d_sep, "status": "reconciled"})
    p(f"| B: mutated August on nbc, others as A | nbc | yes | 2026-09-30 (off) | {d_sep} | off |")
    trust_rows.append({"scenario": "B", "account": "nbc", "contributes": True, "latest_period_end": "2026-09-30", "days": d_sep, "status": "off"})
    p(f"| C: nbs balanced through July only, others as A | nbs | yes | 2026-07-31 | {d_jul} | stale ({d_jul} > {stale_days}) |")
    trust_rows.append({"scenario": "C", "account": "nbs", "contributes": True, "latest_period_end": "2026-07-31", "days": d_jul, "status": "stale"})
    p(f"| C with a 90-day override on nbs | nbs | yes | 2026-07-31 | {d_jul} | reconciled ({d_jul} ≤ 90) |")
    p("| D: no statement entered | any | — | — | — | never_reconciled |")
    p("")
    p("Hero trust: A → trusted. B → untrusted, naming Northbank Checking (off by "
      + money(aug_diff) + "). C → untrusted, naming Northbank Savings (stale). D → untrusted, naming every contributing account.")
    p("Cards and the firewalled brokerage never enter the hero's set, so their status marks only their own figures.")
    p("")
    global RECON_JSON
    RECON_JSON = {
        "periods": recon_periods,
        "mutated": {"file": "northbank/northbank_checking_2026-08_mutated.csv", "account": "nbc",
                    "row": {"posted": real.posted, "description": real.description, "real_cents": real.amount, "mutated_cents": MUTATION[2]},
                    "august": {"period_start": "2026-08-01", "period_end": "2026-08-31", "opening_cents": closing("nbc", "2026-07"),
                               "computed_cents": aug_computed, "statement_cents": closing("nbc", "2026-08"), "difference_cents": aug_diff},
                    "september": {"period_start": "2026-08-01", "period_end": "2026-09-30", "opening_cents": closing("nbc", "2026-07"),
                                  "computed_cents": sep_computed, "statement_cents": closing("nbc", "2026-09"), "difference_cents": sep_diff},
                    "explorer": {"in_period": len(aug_rows), "before": len(before), "after": len(after), "pending": 0, "quarantine": 0}},
        "trust": {"as_of": as_of, "stale_after_days": stale_days, "rows": trust_rows,
                  "hero_contributing": [k for k in ACCOUNTS if contrib[k]]},
    }

    # Safe-to-spend (M4): every figure below is computed here from ROWS and the plan definitions
    p("## Safe-to-spend (M4)")
    p("")
    as_of_d = date.fromisoformat(AS_OF)
    p(f"As-of date **{AS_OF}**; timing buffer **{money(TIMING_BUFFER_CENTS)}** (setting `timing_buffer_cents`). Receipts and payments")
    p(f"match a posted row on the stream's / obligation's account whose payee contains the match text, whose |amount| is within")
    p(f"expected ± variability, posted within [due − {MATCH_BEFORE_DAYS}, due + {MATCH_AFTER_DAYS}] days; the closest row wins, each row once.")
    p("")
    p("### Plan")
    p("")
    for st in INCOME_STREAMS:
        p(f"- Income stream `{st['name']}`: {st['kind']}, {st['cycle']} from {st['anchor_date']}, {money(st['expected_net_cents'])} net ± {money(st['variability_cents'])},")
        p(f"  {st['confidence']}, weekend rule {st['weekend_rule']}, deposit {st['deposit_account']}, payee contains `{st['match_payee_contains']}`.")
    for ob in OBLIGATIONS:
        rule = f"monthly day {ob['due_day']}" if ob["due_rule"] == "monthly_day" else f"annual on {ob['due_month']:02d}-{ob['due_day']:02d}"
        p(f"- Obligation `{ob['name']}` ({ob['kind']}, confirmed): {rule}, {money(ob['expected_cents'])} ± {money(ob['variability_cents'])}, from {ob['source_account']},")
        p(f"  autopay {'yes' if ob['autopay'] else 'no'}, category `{ob['category']}`, payee contains `{ob['match_payee_contains']}`.")
    for em in EARMARKS:
        sched = em["schedule"] if em["schedule"] == "none" else f"{em['schedule']} {money(em['schedule_amount_cents'])}"
        p(f"- Earmark `{em['name']}` ({em['kind']}): funded from {em['funding_account']}, target {money(em['target_cents'])}, schedule {sched}"
          + (f", linked to obligation `{em['obligation']}`" if em["obligation"] else "") + ".")
        for (dt, kind, cents, note) in em["entries"]:
            p(f"  - entry {dt} {kind} {money(cents)} — {note}")
    p("- Visa and Amex minimums become debt-minimum obligations at M6; they are not in this answer.")
    p("")
    # receipts
    taken: set = set()
    receipts = []
    for st in INCOME_STREAMS:
        rows = [r for r in rows_for(st["deposit_account"]) if r.amount > 0]
        dues = income_occurrences(st, date.fromisoformat(st["anchor_date"]), as_of_d + timedelta(days=MATCH_AFTER_DAYS))
        for due, r in match_rows(dues, rows, st["expected_net_cents"], st["variability_cents"], st["match_payee_contains"], taken):
            receipts.append({"stream": st["name"], "due_date": due.isoformat(), "account": r.account, "posted": r.posted,
                             "description": r.description, "amount_cents": r.amount})
    p(f"### Receipts matched: {len(receipts)}")
    p("")
    p("| stream | due | row |")
    p("|---|---|---|")
    for rc in receipts:
        p(f"| {rc['stream']} | {rc['due_date']} | {rc['account']} {rc['posted']} `{rc['description']}` {money(rc['amount_cents'])} |")
    p("")
    # payments
    taken = set()
    payments = []
    for ob in OBLIGATIONS:
        rows = [r for r in rows_for(ob["source_account"]) if r.amount < 0]
        dues = obligation_occurrences(ob, date(2026, 7, 1), as_of_d + timedelta(days=MATCH_AFTER_DAYS))
        for due, r in match_rows(dues, rows, ob["expected_cents"], ob["variability_cents"], ob["match_payee_contains"], taken):
            payments.append({"obligation": ob["name"], "due_date": due.isoformat(), "account": r.account, "posted": r.posted,
                             "description": r.description, "amount_cents": r.amount})
    paid = {(pm["obligation"], pm["due_date"]) for pm in payments}
    p(f"### Payments matched: {len(payments)}")
    p("")
    p("| obligation | due | row |")
    p("|---|---|---|")
    for pm in payments:
        p(f"| {pm['obligation']} | {pm['due_date']} | {pm['account']} {pm['posted']} `{pm['description']}` {money(pm['amount_cents'])} |")
    p("")
    # next income
    received = {(rc["stream"], rc["due_date"]) for rc in receipts}
    next_income = None
    for st in INCOME_STREAMS:
        if st["confidence"] != "confirmed":
            continue
        for due in income_occurrences(st, as_of_d, as_of_d + timedelta(days=400)):
            if (st["name"], due.isoformat()) not in received:
                if next_income is None or due < next_income[0]:
                    next_income = (due, st)
                break
    assert next_income is not None
    ni_date, ni_stream = next_income
    # available
    avail_accounts = []
    for k, a in ACCOUNTS.items():
        if a.kind in CASH_KINDS and not a.firewalled:
            avail_accounts.append({"account": k, "posted_cents": closing(k, "2026-09"), "pending_in_cents": 0, "pending_out_cents": 0})
    available = sum(x["posted_cents"] + x["pending_in_cents"] - x["pending_out_cents"] for x in avail_accounts)
    # earmarks
    em_items = []
    for em in EARMARKS:
        remaining = sum(c for (dt, kind, c, note) in em["entries"] if dt <= AS_OF)
        em_items.append({"earmark": em["name"], "remaining_cents": remaining})
    earmarks_total = sum(max(0, x["remaining_cents"]) for x in em_items)
    remaining_by_ob = {em["obligation"]: sum(c for (dt, k2, c, n) in em["entries"] if dt <= AS_OF) for em in EARMARKS if em["obligation"]}
    # obligations before next income (overdue within the lookback, unpaid)
    ob_items = []
    for ob in OBLIGATIONS:
        cover_left = remaining_by_ob.get(ob["name"], 0)
        opened = date.fromisoformat(ACCOUNTS[ob["source_account"]].opening_date)
        for due in obligation_occurrences(ob, max(as_of_d - timedelta(days=OVERDUE_LOOKBACK_DAYS), opened), ni_date):
            if (ob["name"], due.isoformat()) in paid:
                continue
            covered = min(ob["expected_cents"], max(0, cover_left))
            cover_left -= covered
            ob_items.append({"obligation": ob["name"], "due_date": due.isoformat(), "expected_cents": ob["expected_cents"],
                             "earmark_covered_cents": covered, "counted_cents": ob["expected_cents"] - covered, "overdue": due < as_of_d})
    obligations_total = sum(x["counted_cents"] for x in ob_items)
    safe = available - earmarks_total - obligations_total - TIMING_BUFFER_CENTS
    p(f"### Hero as of {AS_OF}")
    p("")
    p(f"- Next confirmed income: **{ni_date.isoformat()}** (`{ni_stream['name']}`, {money(ni_stream['expected_net_cents'])}, {(ni_date - as_of_d).days} days away).")
    p("- available = Σ over nbc, nbs, rvc, vm of posted balance as of the date (no pending rows exist in the fixture):")
    for x in avail_accounts:
        p(f"  - {x['account']}: {money(x['posted_cents'])}")
    p(f"  → **{money(available)}**")
    p("- earmarks_unfunded = Σ earmark remaining (entries dated ≤ as-of) over earmarks funded from those accounts:")
    for x in em_items:
        p(f"  - {x['earmark']}: {money(x['remaining_cents'])}")
    p(f"  → **{money(earmarks_total)}**")
    p(f"- obligations_before_next_income = unpaid confirmed occurrences due ≤ {ni_date.isoformat()} (overdue ones within {OVERDUE_LOOKBACK_DAYS} days")
    p("  included, never before the source account's opening date), each reduced by what its earmark holds (no dollar subtracted twice):")
    for x in ob_items:
        p(f"  - {x['obligation']} due {x['due_date']}: expected {money(x['expected_cents'])}, earmark covers {money(x['earmark_covered_cents'])} → counted {money(x['counted_cents'])}")
    p(f"  → **{money(obligations_total)}**")
    p(f"- minimum_buffer = **{money(TIMING_BUFFER_CENTS)}**")
    p(f"- safe = {money(available)} − {money(earmarks_total)} − {money(obligations_total)} − {money(TIMING_BUFFER_CENTS)} = **{money(safe)}**")
    p("- Excluded: hb (firewalled, " + money(closing("hb", "2026-09")) + "); the cards are liabilities, not cash. No venture-owned account,")
    p("  no pending flagged inflow. The hero is trusted only when nbc, nbs, rvc and vm are reconciled (M3).")
    p("")
    # next 14 days: the same unpaid occurrences as the hero (overdue within the lookback first), out to +14
    upcoming = []
    for ob in OBLIGATIONS:
        cover_left = remaining_by_ob.get(ob["name"], 0)
        opened = date.fromisoformat(ACCOUNTS[ob["source_account"]].opening_date)
        for due in obligation_occurrences(ob, max(as_of_d - timedelta(days=OVERDUE_LOOKBACK_DAYS), opened), as_of_d + timedelta(days=UPCOMING_DAYS)):
            if (ob["name"], due.isoformat()) in paid:
                continue
            covered = min(ob["expected_cents"], max(0, cover_left))
            cover_left -= covered
            upcoming.append({"obligation": ob["name"], "due_date": due.isoformat(), "expected_cents": ob["expected_cents"],
                             "variability_cents": ob["variability_cents"], "earmark_covered_cents": covered, "autopay": ob["autopay"],
                             "overdue": due < as_of_d, "days_away": (due - as_of_d).days})
    upcoming.sort(key=lambda x: (x["due_date"], x["obligation"]))
    p(f"### Next {UPCOMING_DAYS} days from {AS_OF}: {len(upcoming)} obligations")
    p("")
    p(f"Unpaid confirmed occurrences due in [{AS_OF} − {OVERDUE_LOOKBACK_DAYS} days, {AS_OF} + {UPCOMING_DAYS} days]; a past-due one is listed first and marked overdue.")
    p("")
    p("| due | obligation | expected | earmark covers | autopay | overdue |")
    p("|---|---|---:|---:|---|---|")
    for x in upcoming:
        p(f"| {x['due_date']} | {x['obligation']} | {money(x['expected_cents'])} ± {money(x['variability_cents'])} | {money(x['earmark_covered_cents'])} | {'yes' if x['autopay'] else 'no'} | {'yes' if x['overdue'] else 'no'} |")
    p("")
    # candidates
    cands = detect_candidates()
    covered_needles = [ob["match_payee_contains"] for ob in OBLIGATIONS]
    cands_with_plan = [c for c in cands if not any(n in c["payee_norm"] for n in covered_needles)]
    p(f"### Recurring-row candidates: {len(cands)} payees ({len(cands_with_plan)} once the plan's obligations exist)")
    p("")
    p("Rule: ≥ 3 posted, unlinked outflow rows per (account, payee_norm) whose category root is not income or transfer,")
    p("every consecutive gap 25..36 days, every |amount| within ±25% of the median; due day = median day of month,")
    p("expected = median |amount|, variability = largest deviation. A payee an existing obligation already matches is skipped.")
    p("A candidate never enters the hero until it is confirmed.")
    p("")
    p("| account | payee_norm | rows | due day | expected | variability | covered by the plan |")
    p("|---|---|---:|---:|---:|---:|---|")
    for c in cands:
        covered = any(n in c["payee_norm"] for n in covered_needles)
        p(f"| {c['account']} | `{c['payee_norm']}` | {c['rows']} | {c['due_day']} | {money(c['expected_cents'])} | {money(c['variability_cents'])} | {'yes' if covered else 'no'} |")
    p("")
    global PLAN_JSON
    PLAN_JSON = {
        "as_of": AS_OF, "timing_buffer_cents": TIMING_BUFFER_CENTS, "match_before_days": MATCH_BEFORE_DAYS, "match_after_days": MATCH_AFTER_DAYS,
        "overdue_lookback_days": OVERDUE_LOOKBACK_DAYS, "upcoming_days": UPCOMING_DAYS,
        "income_streams": INCOME_STREAMS, "obligations": OBLIGATIONS,
        "earmarks": [dict(em, entries=[{"entry_date": dt, "kind": k, "amount_cents": c, "note": n} for (dt, k, c, n) in em["entries"]]) for em in EARMARKS],
        "receipts": receipts, "payments": payments,
        "next_income": {"date": ni_date.isoformat(), "stream": ni_stream["name"], "expected_net_cents": ni_stream["expected_net_cents"], "days_away": (ni_date - as_of_d).days},
        "hero": {"available": {"total_cents": available, "accounts": avail_accounts},
                 "earmarks": {"total_cents": earmarks_total, "items": em_items},
                 "obligations": {"total_cents": obligations_total, "items": ob_items},
                 "buffer_cents": TIMING_BUFFER_CENTS, "safe_cents": safe,
                 "excluded_firewalled": [{"account": "hb", "posted_cents": closing("hb", "2026-09")}]},
        "upcoming": upcoming,
        "candidates": cands, "candidates_with_plan": [c["payee_norm"] for c in cands_with_plan],
    }

    # Forecast (M5)
    p("## Forecast (M5)")
    p("")
    horizon_d = as_of_d + timedelta(days=HORIZON_DAYS)
    model = variable_model(as_of_d)
    runs = {name: forecast_run(sc, as_of_d, received, paid, model) for name, sc in SCENARIOS.items()}
    base = runs["baseline"]
    p(f"As of **{AS_OF}**, days 0..={HORIZON_DAYS} ({AS_OF} .. {horizon_d.isoformat()}). Opening = Σ posted balance of nbc, nbs, rvc, vm")
    p(f"= **{money(base['opening_cents'])}** (no pending rows in those accounts). Only confirmed streams are income; expected or rumored")
    p("streams never enter; nothing is invented to avoid a low point. Every day: closing = opening + inflows − outflows (`forecast_ties`).")
    p("")
    p("### Variable-spend model (ARCHITECTURE §5.7)")
    p("")
    b = model[0]["buckets"]
    p(f"Three 30-day buckets ending yesterday: [{b[0]['start']}..{b[0]['end']}], [{b[1]['start']}..{b[1]['end']}], [{b[2]['start']}..{b[2]['end']}].")
    p("Net outflow of posted, non-transfer rows per category (the Target return nets against August shopping), floored at 0;")
    p("the model is the median of the three. Rows still in review (ATM cash, Venmo) have no category and count for nothing.")
    p("")
    p("| category | bucket 1 | bucket 2 | bucket 3 | median per 30 days |")
    p("|---|---:|---:|---:|---:|")
    for m in model:
        bb = m["buckets"]
        p(f"| `{m['category']}` | {money(bb[0]['net_outflow_cents'])} | {money(bb[1]['net_outflow_cents'])} | {money(bb[2]['net_outflow_cents'])} | **{money(m['median_cents'])}** |")
    model_total = sum(m["median_cents"] for m in model)
    day0_variable = sum(allocate(m["median_cents"], BUCKET_DAYS)[0] for m in model if m["median_cents"])
    p("")
    p(f"Σ model = **{money(model_total)}** per 30 days. Each category is allocated over every 30-day block by largest remainder")
    p(f"(`money::allocate(median, 30)`): day 0 carries {money(day0_variable)}, days 0..29 sum to the model exactly, days 30..59 and 60..89 repeat it,")
    p(f"day 90 opens a fourth block. A `variable_spend_override` replaces a category's median.")
    p("")
    p("### Scheduled events in the window")
    p("")
    pays = base["pay_dates"]["Meridian payroll"]
    p(f"- Income: Meridian payroll on {', '.join(d[5:] for d in pays)} ({len(pays)} × {money(INCOME_STREAMS[0]['expected_net_cents'])} = {money(base['inflows_cents'] - 0)} of inflows; nothing else is income).")
    ob_total = sum(-e["cents"] for x in base["days"] for e in x["events"] if e["kind"] == "obligation")
    p(f"- Obligations: Rent on the 1st (2,400.00), ComEd 7th (125.00), Xfinity 12th (89.99), T-Mobile 18th (75.00), Peoples Gas 21st (40.00),")
    p(f"  each three times; GEICO's next occurrence is 2027-09-22. Σ = {money(ob_total)}. Variable spend over 91 days = {money(base['outflows_cents'] - ob_total)}.")
    p(f"- Committed = buffer {money(TIMING_BUFFER_CENTS)} + Emergency reserve {money(1_200_000)} + the Rent earmark projected: released when rent")
    p("  falls due, funded 1,200.00 on each pay date up to its 2,400.00 target. Headroom = closing − committed.")
    p(f"- Downside: the next base-pay occurrence ({pays[0]}) lands {PAY_SHIFT_DAYS} civil days late ({runs['downside']['pay_dates']['Meridian payroll'][0]}); later occurrences keep their dates.")
    p(f"- Surprise bills (scenario inputs, not ledger rows): {money(SURPRISE_BILLS['downside_bill'][1])} on {SURPRISE_BILLS['downside_bill'][0]} with the downside;")
    p(f"  {money(SURPRISE_BILLS['bill'][1])} on {SURPRISE_BILLS['bill'][0]} on the baseline.")
    p("")
    p("### Scenarios")
    p("")
    p("| scenario | Σ inflows | Σ outflows | closing day 90 | lowest balance | first shortfall (closing < 0) | first buffer breach (headroom < 0) |")
    p("|---|---:|---:|---:|---|---|---|")
    def pt(x):
        return "none" if x is None else f"{x['date']} ({money(x['cents'])})"
    for name, r in runs.items():
        p(f"| {name} | {money(r['inflows_cents'])} | {money(r['outflows_cents'])} | {money(r['closing_cents'])} | **{money(r['lowest']['cents'])} on {r['lowest']['date']}** | {pt(r['first_shortfall'])} | {pt(r['first_buffer_breach'])} |")
    p("")
    p("The downside moves the lowest point down (and here later: seven more days of bills and spending before the pay lands).")
    p("")
    p("### First 14 days, baseline")
    p("")
    p("| day | date | inflows | outflows | closing | committed | headroom | events |")
    p("|---:|---|---:|---:|---:|---:|---:|---|")
    for x in base["days"][:14]:
        named = [f"{e['name']} {money(e['cents'])}" for e in x["events"] if e["kind"] != "variable"]
        named.append(f"variable {money(sum(e['cents'] for e in x['events'] if e['kind'] == 'variable'))}")
        p(f"| {x['day']} | {x['date']} | {money(x['inflows_cents'])} | {money(x['outflows_cents'])} | {money(x['closing_cents'])} | {money(x['committed_cents'])} | {money(x['headroom_cents'])} | {'; '.join(named)} |")
    p("")
    p("### 13 weeks, baseline")
    p("")
    p("| week | start | end | inflows | outflows | closing | lowest |")
    p("|---:|---|---|---:|---:|---:|---:|")
    for w in base["weeks"]:
        p(f"| {w['week']} | {w['start']} | {w['end']} | {money(w['inflows_cents'])} | {money(w['outflows_cents'])} | {money(w['closing_cents'])} | {money(w['lowest_cents'])} |")
    p("")
    # Debts and informal loans (M6)
    p("## Debts and informal loans (M6)")
    p("")
    states = debt_states(as_of_d)
    p(f"As of **{AS_OF}**. A linked debt owes `max(0, −posted balance)` of its account; a standalone debt owes its opening minus")
    p("recorded payments; an informal loan owes its original minus the repayments matched to its schedule. Interest per period")
    p("(ARCHITECTURE §5.8, `mul_div_round`): monthly nominal `opening × apr_bps / 120 000`; actual/365 `opening × apr_bps × days / 3 650 000`;")
    p("the promo APR applies while the period starts on or before `promo_end`. Periods are the calendar months after the as-of month.")
    p("")
    p("| debt | kind | balance source | owed | APR | method | minimum rule | period-1 minimum |")
    p("|---|---|---|---:|---|---|---|---:|")
    for d in states:
        if d["informal"]:
            continue
        src = f"linked `{d['account']}`" if d["account"] else f"standalone {money(d['standalone_opening'])} on {AS_OF}"
        apr = f"{d['apr_bps'] / 100:.2f}%" + (f" (promo {d['promo_apr_bps'] / 100:.2f}% through {d['promo_end']})" if d["promo_apr_bps"] is not None else "")
        rule = {"fixed": f"fixed {money(d['minimum_fixed_cents'])}", "percent_of_balance": f"{d['minimum_bps'] / 100:.0f}% of balance, floor {money(d['minimum_floor_cents'])}",
                "interest_plus_percent": f"interest + {d['minimum_bps'] / 100:.0f}% of balance, floor {money(d['minimum_floor_cents'])}", "full_balance": "full balance"}[d["minimum_rule"]]
        st, en = period_bounds(as_of_d, 1)
        i1 = period_interest(d, d["owed_cents"], st, en)
        m1 = period_minimum(d, d["owed_cents"], i1, st, en) if d["owed_cents"] > 0 else 0
        p(f"| {d['name']} | {d['kind']} | {src} | {money(d['owed_cents'])} | {apr} | {d['interest_method']} | {rule} | {money(m1)} |")
    p("")
    p(f"Summit Visa carries a credit balance of {money(balance_as_of('sv', as_of_d))} on {AS_OF} (the August payment exceeded the balance), so it owes")
    p("nothing and has no schedule and no minimum obligation; the Amex owes its September charges.")
    p("")
    p("### Informal loans")
    p("")
    repayments_all = {}
    for loan in INFORMAL:
        reps = informal_repayments(loan, as_of_d)
        repayments_all[loan["key"]] = reps
        remaining = loan["original_cents"] - sum(x["amount_cents"] for x in reps)
        src = f"proceeds row {loan['proceeds'][0]} {loan['proceeds'][1]} `{loan['proceeds'][2]}` (flagged borrowing, never income)" if loan["proceeds"] else "borrowed before the ledger starts; no proceeds row"
        p(f"- `{loan['counterparty']}`: {money(loan['original_cents'])} borrowed {loan['borrowed_date']}, promised \"{loan['promised_terms']}\" by {loan['promised_date']}; {src}.")
        if loan["schedule"]:
            p("  - schedule: " + ", ".join(f"{dt} {money(c)}" for (dt, c) in loan["schedule"]))
        for x in reps:
            p(f"  - repayment {x['due_date']} ← {x['account']} {x['posted']} `{x['description']}` {money(x['amount_cents'])} (a transfer to a liability, never an expense)")
        p(f"  - remaining **{money(remaining)}**")
    p("")
    informal_total = sum(d["owed_cents"] for d in states if d["informal"])
    p(f"Dashboard: total debt (cards and loans) **{money(sum(d['owed_cents'] for d in states if not d['informal']))}**, informal remaining **{money(informal_total)}**.")
    p("")
    strategy_runs = {st: run_strategy(st, as_of_d, DEBT_EXTRA_CENTS) for st in STRATEGIES}
    base_run = strategy_runs["avalanche"]
    p(f"### Strategies with {money(DEBT_EXTRA_CENTS)} extra per month")
    p("")
    p(f"Budget = extra + the first period's minimums = **{money(base_run['budget_cents'])}** per month, constant: a paid-off debt's minimum")
    p("rolls to the next target. Every open debt gets its minimum; policy `informal_first` sends the rest to informal loans by")
    p("promised date; then avalanche = highest effective APR this period (tie: smaller balance), snowball = smallest balance,")
    p("custom = the user's order (auto loan, balance transfer, Visa, Amex). A payment never exceeds opening + interest.")
    p("")
    p("| strategy | total interest | last payoff | " + " | ".join(f"{d['name']}" for d in base_run["debts"]) + " |")
    p("|---|---:|---|" + "---|" * len(base_run["debts"]))
    for st, r in strategy_runs.items():
        cells = " | ".join(f"{money(d['total_interest_cents'])} by {d['payoff_date']}" for d in r["debts"])
        p(f"| {st} | **{money(r['total_interest_cents'])}** | {r['payoff_date']} | {cells} |")
    p("")
    p("### Avalanche, first six periods per debt (the full schedules are in debts.json)")
    p("")
    for d in base_run["debts"]:
        p(f"**{d['name']}** — owed {money(d['owed_cents'])}, interest {money(d['total_interest_cents'])}, paid off {d['payoff_date']}")
        p("")
        p("| period | start | end | opening | interest | minimum | payment | closing |")
        p("|---:|---|---|---:|---:|---:|---:|---:|")
        for r in d["periods"][:6]:
            p(f"| {r['period']} | {r['start']} | {r['end']} | {money(r['opening_cents'])} | {money(r['interest_cents'])} | {money(r['minimum_cents'])} | {money(r['payment_cents'])} | {money(r['closing_cents'])} |")
        p("")
    scen = [informal_scenario(as_of_d, DEBT_EXTRA_CENTS), informal_scenario(as_of_d, 0)]
    p(f"### Informal loans repaid within {INFORMAL_SCENARIO_PERIODS} months — a scenario, not an assumption")
    p("")
    for sc in scen:
        verdict = "achievable" if sc["achievable"] else f"not achievable: gap after {INFORMAL_SCENARIO_PERIODS} months {money(sc['gap_cents'])}"
        p(f"- Extra {money(sc['extra_cents'])} per month: remaining {money(sc['remaining_cents'])}; {verdict}; the budget repays it by **{sc['payoff_date'] or 'never'}**.")
    p("")
    p("### Debt-minimum obligations the engine keeps in sync")
    p("")
    p("Every active debt with a minimum rule, a due day and a payment account gets one confirmed obligation of kind `debt_minimum`")
    p("(expected = the current period's minimum, re-derived on every write; retired when the debt owes nothing or is inactive).")
    p("None falls due before the next confirmed income (2026-10-02), so the M4 hero is unchanged; the forecast counts them as outflows.")
    p("")
    min_obls = []
    st, en = period_bounds(as_of_d, 1)
    for d in states:
        if d["informal"] or d["owed_cents"] == 0 or d["minimum_rule"] == "none" or not d["due_day"] or not d["payment_account"]:
            continue
        i1 = period_interest(d, d["owed_cents"], st, en)
        min_obls.append({"debt": d["key"], "name": f"{d['name']} minimum", "due_day": d["due_day"], "expected_cents": period_minimum(d, d["owed_cents"], i1, st, en),
                         "source_account": d["payment_account"], "match_payee_contains": d["match_payee_contains"]})
    p("| obligation | due day | expected | source | payee contains |")
    p("|---|---:|---:|---|---|")
    for o in min_obls:
        p(f"| {o['name']} | {o['due_day']} | {money(o['expected_cents'])} | {o['source_account']} | `{o['match_payee_contains']}` |")
    p("")
    global DEBTS_JSON
    DEBTS_JSON = {"as_of": AS_OF, "extra_cents": DEBT_EXTRA_CENTS, "periods_max": DEBT_PERIODS_MAX, "scenario_periods": INFORMAL_SCENARIO_PERIODS,
                  "debts": [dict(d, owed_cents=next(s_["owed_cents"] for s_ in states if s_["key"] == d["key"])) for d in DEBTS],
                  "informal": [dict(loan, proceeds=None if not loan["proceeds"] else {"account": loan["proceeds"][0], "posted": loan["proceeds"][1], "description": loan["proceeds"][2]},
                                    schedule=[{"due_date": dt, "amount_cents": c} for (dt, c) in loan["schedule"]],
                                    repayments=repayments_all[loan["key"]],
                                    remaining_cents=loan["original_cents"] - sum(x["amount_cents"] for x in repayments_all[loan["key"]])) for loan in INFORMAL],
                  "total_debt_cents": sum(d["owed_cents"] for d in states if not d["informal"]), "informal_remaining_cents": informal_total,
                  "strategies": strategy_runs, "informal_scenarios": scen, "minimum_obligations": min_obls}

    # Ventures (M7)
    p("## Ventures (M7)")
    p("")
    roll = venture_rollup(as_of_d)
    vd = VENTURE_DETAILS
    p(f"As of **{AS_OF}**. Venture `{VENTURE['name']}`: status {vd['status']}, cash cap {money(vd['cash_cap_cents'])}, time budget {vd['time_budget_hours']} h,")
    p(f"milestone \"{vd['milestone']}\" by {vd['milestone_date']}, stop condition \"{vd['stop_condition']}\". The person marks the Summit Amex")
    p("(`sa`) as owned by the venture: it carries only Ledgerline charges and is paid from personal checking, so each card payment is")
    p("an owner contribution (personal cash into the venture) whatever the link's stored kind, and the SaaS charges are operating")
    p("expenses paid from the venture's own account. Rows are bucketed by their venture category code; the window is the trailing")
    p(f"{TRAILING_MONTHS} months ({roll['window_start']} < posted ≤ {AS_OF}), which here covers every row.")
    p("")
    p("| bucket | rows | cents |")
    p("|---|---:|---:|")
    for b in VENTURE_BUCKETS:
        p(f"| {b} | {roll['buckets'][b]['rows']} | {money(roll['buckets'][b]['cents'])} |")
    p("")
    p(f"- operating_cash_flow = revenue − operating expense = **{money(roll['operating_cash_flow_cents'])}**")
    p(f"- cap_used = owner contribution {money(roll['buckets']['owner_contribution']['cents'])} + operating expense paid from personal accounts")
    p(f"  {money(roll['operating_expense_from_personal_cents'])} − withdrawals {money(roll['buckets']['withdrawal']['cents'])} = **{money(roll['cap_used_cents'])}**; cap remaining {money(roll['cap_remaining_cents'])};")
    p(f"  utilization **{roll['cap_utilization_bps']} bps** ({roll['cap_utilization_bps'] / 100:.2f}% of {money(roll['cap_cents'])}) — the dashboard gauge")
    p(f"- milestone countdown **{roll['milestone_days']} days**; stop-condition alert: {', '.join(roll['alerts']) or 'none'} (fires when cap used ≥ cap or the milestone date has passed)")
    p(f"- venture spend share of take-home = operating expense {money(roll['buckets']['operating_expense']['cents'])} / confirmed base-pay receipts {money(roll['take_home_cents'])} = **{roll['spend_share_bps']} bps**")
    p(f"- venture-owned account balance: sa {money(roll['accounts'][0]['balance_cents'])} (what the card owes, not personal cash; never in the hero)")
    p("- The freelance invoices on Riverside stay personal income (`income.other` by rule); nothing is customer revenue until the person says so.")
    p("- Verdict is the person's (`fund|freeze|kill`); sunk cost is not an input. Lowering the cap below what is used, or letting the")
    p("  milestone date pass, raises the alert; the test checks both.")
    p("")
    global VENTURES_JSON
    VENTURES_JSON = {"as_of": AS_OF, "trailing_months": TRAILING_MONTHS, "venture": dict(VENTURE, **vd), "venture_account": VENTURE_ACCOUNT,
                     "rollup": roll}

    # Weekly review (M8)
    p("## Weekly review (M8)")
    p("")
    window_from = as_of_d - timedelta(days=SURPLUS_INCOME_DAYS)
    income_rows = [rc for rc in receipts if window_from < date.fromisoformat(rc["posted"]) <= as_of_d]
    income_90 = sum(rc["amount_cents"] for rc in income_rows)
    income_monthly = mul_div_round(income_90, 30, SURPLUS_INCOME_DAYS)
    fixed_items = [{"name": ob["name"], "monthly_cents": monthly_equivalent(ob)} for ob in OBLIGATIONS if ob["due_rule"] not in ("annual", "once")]
    fixed_total = sum(x["monthly_cents"] for x in fixed_items)
    debt_items = [{"name": o["name"], "monthly_cents": o["expected_cents"]} for o in min_obls]
    informal_12m = 0   # no unpaid informal schedule row: Chris is repaid, Mom has no schedule
    debt_service = sum(x["monthly_cents"] for x in debt_items) + mul_div_round(informal_12m, 1, 12)
    irregular_items = [{"name": ob["name"], "monthly_cents": mul_div_round(ob["expected_cents"], 1, 12)} for ob in OBLIGATIONS if ob["due_rule"] == "annual"]
    irregular_total = sum(x["monthly_cents"] for x in irregular_items)
    variable_total = model_total
    surplus = income_monthly - fixed_total - debt_service - irregular_total - variable_total
    p(f"As of **{AS_OF}**, over everything the earlier milestones installed. The review is a mode: it walks the steps below, states the")
    p("dependable surplus, and completes only with exactly three non-empty actions (enforced in the completing transaction). It stores")
    p("what each step showed, the surplus with its terms, and a `plan` snapshot. No previous review exists, so \"since the last review\"")
    p("means the whole ledger.")
    p("")
    p("### Dependable surplus (monthly equivalent, ARCHITECTURE §5.10)")
    p("")
    p(f"- income = confirmed-stream receipts posted in the trailing {SURPLUS_INCOME_DAYS} days ({window_from.isoformat()} < posted ≤ {AS_OF}): {len(income_rows)} × {money(INCOME_STREAMS[0]['expected_net_cents'])} = {money(income_90)},")
    p(f"  × 30/{SURPLUS_INCOME_DAYS} = **{money(income_monthly)}** (borrowing, asset sales and the freelance deposits cannot enter: only receipts of confirmed streams count)")
    p("- fixed = confirmed obligations that are not debt minimums, monthly equivalent (annuals are irregular): " + ", ".join(f"{x['name']} {money(x['monthly_cents'])}" for x in fixed_items) + f" = **{money(fixed_total)}**")
    p("- debt_service = debt-minimum obligations " + ", ".join(f"{x['name']} {money(x['monthly_cents'])}" for x in debt_items) + f" = {money(sum(x['monthly_cents'] for x in debt_items))} + unpaid informal schedule rows due within 12 months ÷ 12 = {money(mul_div_round(informal_12m, 1, 12))} → **{money(debt_service)}**")
    p("- irregular = annual obligations ÷ 12: " + ", ".join(f"{x['name']} {money(x['monthly_cents'])}" for x in irregular_items) + f" + sinking-fund schedules (none) = **{money(irregular_total)}**")
    p(f"- variable = the variable-spend model = **{money(variable_total)}**")
    p(f"- surplus = {money(income_monthly)} − {money(fixed_total)} − {money(debt_service)} − {money(irregular_total)} − {money(variable_total)} = **{money(surplus)}**")
    p("")
    p("### Steps")
    p("")
    # 1. balances
    balances = [{"account": k, "balance_cents": balance_as_of(k, as_of_d)} for k in ACCOUNTS]
    available_now = sum(b["balance_cents"] for b in balances if ACCOUNTS[b["account"]].kind in CASH_KINDS and not ACCOUNTS[b["account"]].firewalled and b["account"] != VENTURE_ACCOUNT)
    p("1. Balances: every account's posted balance with its reconciliation status; " + ", ".join(f"{b['account']} {money(b['balance_cents'])}" for b in balances) + f"; available (the hero's set) {money(available_now)}; every cash account reconciled.")
    # 2. unreviewed rows: the M2 queue minus the loan's proceeds row
    chris = INFORMAL[0]["proceeds"]
    queue_now = [(r, automation(r)) for r in ROWS]
    queue_now = [(r, a) for (r, a) in queue_now if ("needs_review" in a[2] or a[0] is None)
                 and not (r.account == chris[0] and r.posted == chris[1] and r.description == chris[2])]
    queue_now.sort(key=lambda t: (-abs(t[0].amount), t[0].posted))
    queue_total = sum(abs(r.amount) for (r, _) in queue_now)
    p(f"2. Unreviewed rows: **{len(queue_now)}** (the M2 queue minus the Venmo inflow that became the loan's proceeds), Σ|amount| {money(queue_total)}, largest first: "
      + "; ".join(f"{r.account} {r.posted} `{r.description}` {money(r.amount)}" for (r, _) in queue_now) + ".")
    # 3. obligations in 14 days: plan obligations unpaid + the debt minimums anchored today
    horizon_14 = as_of_d + timedelta(days=REVIEW_HORIZON_DAYS)
    due_14 = []
    for ob in OBLIGATIONS:
        opened = date.fromisoformat(ACCOUNTS[ob["source_account"]].opening_date)
        for due in obligation_occurrences(ob, max(as_of_d - timedelta(days=OVERDUE_LOOKBACK_DAYS), opened), horizon_14):
            if (ob["name"], due.isoformat()) not in paid:
                due_14.append({"obligation": ob["name"], "due_date": due.isoformat(), "expected_cents": ob["expected_cents"]})
    for o in min_obls:
        for due in obligation_occurrences({"due_rule": "monthly_day", "due_day": o["due_day"]}, as_of_d, horizon_14):
            due_14.append({"obligation": o["name"], "due_date": due.isoformat(), "expected_cents": o["expected_cents"]})
    due_14.sort(key=lambda x: (x["due_date"], x["obligation"]))
    p(f"3. Obligations in the next {REVIEW_HORIZON_DAYS} days (to {horizon_14.isoformat()}): **{len(due_14)}**, Σ expected {money(sum(x['expected_cents'] for x in due_14))}: "
      + "; ".join(f"{x['obligation']} {x['due_date']} {money(x['expected_cents'])}" for x in due_14) + ".")
    # 4. plan variance
    p("4. Plan variance: no plan snapshot exists yet, so there is nothing to compare; completing this review stores one.")
    # 5. debts
    total_debt = sum(d["owed_cents"] for d in states if not d["informal"])
    p(f"5. Debt and informal-loan progress: total debt {money(total_debt)}, informal remaining {money(informal_total)}; no earlier review to compare against.")
    # 6. venture cap
    p(f"6. Venture cap: {VENTURE['name']} {money(roll['cap_used_cents'])} of {money(roll['cap_cents'])} ({roll['cap_utilization_bps']} bps), alerts: {', '.join(roll['alerts']) or 'none'}.")
    # 7. flags
    borrowing = [r for r in ROWS if r.account == chris[0] and r.posted == chris[1] and r.description == chris[2]]
    proceeds = [r for r in ROWS if "securities_sale" in automation(r)[2]]
    touches = [r for r in ROWS if ACCOUNTS[r.account].firewalled and r.amount < 0]
    p(f"7. Flags since the last review: borrowing {len(borrowing)} ({'; '.join(f'{r.account} {r.posted} {money(r.amount)}' for r in borrowing)}), securities sale {len(proceeds)} ({'; '.join(f'{r.account} {r.posted} {money(r.amount)}' for r in proceeds)}),")
    p(f"   firewall touches awaiting acknowledgment {len(touches)} ({'; '.join(f'{r.account} {r.posted} {money(r.amount)}' for r in touches)}), acknowledged 0.")
    p("")
    p("### Completion")
    p("")
    p("- Three actions, committed in one transaction with the review; two or four are refused with `Validation` and nothing is stored.")
    for i, a in enumerate(REVIEW_ACTIONS, 1):
        p(f"  {i}. {a}")
    snapshot = {"safe_cents": safe, "available_cents": available, "earmarks_cents": earmarks_total, "obligations_cents": obligations_total,
                "buffer_cents": TIMING_BUFFER_CENTS, "trusted": True, "total_debt_cents": total_debt, "informal_remaining_cents": informal_total,
                "venture_cap_used_cents": roll["cap_used_cents"]}
    p(f"- The `plan` snapshot stored at completion carries the hero as of {AS_OF} (safe {money(snapshot['safe_cents'])}, available {money(snapshot['available_cents'])}, earmarks {money(snapshot['earmarks_cents'])},")
    p(f"  obligations {money(snapshot['obligations_cents'])}, buffer {money(snapshot['buffer_cents'])}, trusted), total debt {money(snapshot['total_debt_cents'])}, informal remaining {money(snapshot['informal_remaining_cents'])},")
    p(f"  venture cap used {money(snapshot['venture_cap_used_cents'])}, and every account's balance. A `daily` snapshot is taken once per civil day on unlock (unique per day); trends read snapshots only.")
    p("- History persists: the completed review, its surplus and its three actions are there after the database is closed and reopened.")
    p("")
    global REVIEW_JSON
    REVIEW_JSON = {"as_of": AS_OF, "income_window_days": SURPLUS_INCOME_DAYS, "horizon_days": REVIEW_HORIZON_DAYS, "period_start": ROWS[0].posted if ROWS else AS_OF,
                   "surplus": {"income_90_cents": income_90, "income_receipts": len(income_rows), "income_cents": income_monthly,
                               "fixed_cents": fixed_total, "fixed_items": fixed_items, "debt_service_cents": debt_service, "debt_items": debt_items,
                               "informal_schedule_12m_cents": informal_12m, "irregular_cents": irregular_total, "irregular_items": irregular_items,
                               "variable_cents": variable_total, "surplus_cents": surplus},
                   "steps": {"balances": {"accounts": balances, "available_cents": available_now},
                             "unreviewed": {"count": len(queue_now), "total_abs_cents": queue_total,
                                            "rows": [{"account": r.account, "posted": r.posted, "description": r.description, "amount_cents": r.amount} for (r, _) in queue_now]},
                             "obligations_14": {"count": len(due_14), "expected_cents": sum(x["expected_cents"] for x in due_14), "items": due_14},
                             "debts": {"total_debt_cents": total_debt, "informal_remaining_cents": informal_total},
                             "ventures": {"cap_used_cents": roll["cap_used_cents"], "cap_cents": roll["cap_cents"], "utilization_bps": roll["cap_utilization_bps"]},
                             "flags": {"borrowing": [{"account": r.account, "posted": r.posted, "description": r.description, "amount_cents": r.amount} for r in borrowing],
                                       "securities_sale": [{"account": r.account, "posted": r.posted, "description": r.description, "amount_cents": r.amount} for r in proceeds],
                                       "firewall_unacknowledged": [{"account": r.account, "posted": r.posted, "description": r.description, "amount_cents": r.amount} for r in touches]}},
                   "actions": REVIEW_ACTIONS, "snapshot": snapshot}

    # OFX/QFX, backup/restore, audit pack (M9)
    p("## OFX/QFX, backup and restore, audit pack (M9)")
    p("")
    answers = ofx_answers()
    p("### OFX/QFX")
    p("")
    p("Two exports of rows the CSVs already carry: one in the SGML form (`OFXHEADER:100`, unclosed tags) and one in the")
    p("XML form (OFX 2.2, closed tags). `FITID` → `external_id`; `NAME` → payee, `MEMO` → memo; `DTUSER` → effective date;")
    p("`TRNAMT` is signed from the account's view; `<LEDGERBAL>` → the import report's file closing, offered to Reconcile")
    p("as statement source `file`. `CURDEF` other than USD is refused.")
    p("")
    p("| file | form | account | rows | Σ amount | ledger balance | as of |")
    p("|---|---|---|---:|---:|---:|---|")
    for a in answers:
        p(f"| `{a['file']}` | {a['form']} | {a['account']} | {a['rows']} | {money(a['sum_cents'])} | {money(a['ledger_balance_cents'])} | {a['ledger_balance_date']} |")
    p("")
    for a in answers:
        p(f"- `{a['file']}`: FITIDs " + ", ".join(f"`{f}`" for f in a["fitids"]) + ".")
    p("")
    p("- Into an empty account each file inserts every row with its FITID; the same file again inserts nothing (file-level")
    p("  idempotency). After the same month's CSV, every OFX row is a better observation of the row already there (same")
    p("  account, amount, date window and payee, and it carries a FITID the CSV row lacks): the ledger row gains the FITID")
    p("  as `external_id` (ADR-0017 update rule), nothing is inserted or quarantined, and a user-edited field is never overwritten.")
    p("- The ledger balance equals the CSV closing for the month, so a `file`-sourced reconciliation of that period balances.")
    p("")
    p("### Backup and restore")
    p("")
    p(f"Over the whole fixture state: a backup re-encrypted under a new passphrase opens only with the new one; restored into a")
    p(f"temporary data folder and migrated, all {SCHEMA_TABLES} user tables have the same row counts as the source and the hero")
    p(f"as of {AS_OF} is the same {money(safe)}. The pre-restore backup is logged; the swap happens only on confirmation.")
    p("")
    p("### Audit pack and full export")
    p("")
    av = run_strategy("avalanche", as_of_d, 0)
    sched_rows = sum(len(d["periods"]) for d in av["debts"])
    p(f"- `ledger.csv`: {len(ROWS)} rows (every leaf row, decimal strings, sign from the account's view).")
    p(f"- `reconciliation.csv`: {len(recon_periods)} periods with opening, computed, statement and difference.")
    p(f"- `safe_to_spend.json`: the hero's terms with row ids as of the export day; `forecast.json`: the baseline's 91 days.")
    p(f"- `debt_schedule.csv`: the avalanche schedule with no extra (minimums only), {sched_rows} period rows across the {len(av['debts'])} debts that owe something.")
    p("- `venture_rollup.csv`: one row per venture (1) with the five buckets, cap used and utilization.")
    p("- `README.md`: what each file holds and the sign convention. The full export is one CSV per table plus `kept.json`.")
    p("")
    global M9_JSON
    M9_JSON = {"as_of": AS_OF, "ofx": answers, "schema_tables": SCHEMA_TABLES, "hero_safe_cents": safe,
               "audit_pack": {"ledger_rows": len(ROWS), "reconciliation_rows": len(recon_periods), "debt_schedule_rows": sched_rows, "venture_rows": 1}}

    global FORECAST_JSON
    FORECAST_JSON = {"as_of": AS_OF, "horizon_days": HORIZON_DAYS, "bucket_days": BUCKET_DAYS, "pay_shift_days": PAY_SHIFT_DAYS,
                     "timing_buffer_cents": TIMING_BUFFER_CENTS, "model": model, "model_total_cents": model_total, "scenarios": runs}

    return "\n".join(out) + "\n"


def emit_rules_json() -> None:
    import json
    payload = {
        "venture": VENTURE,
        "rules": [
            {"name": n, "match_payee_contains": m, "category_code": c, "venture": v}
            for (n, m, c, v) in RULES
        ],
    }
    write("rules.json", json.dumps(payload, indent=2) + "\n")


RECON_JSON: dict = {}
PLAN_JSON: dict = {}
FORECAST_JSON: dict = {}
DEBTS_JSON: dict = {}
VENTURES_JSON: dict = {}
REVIEW_JSON: dict = {}
M9_JSON: dict = {}


def emit_plan_json() -> None:
    write("plan.json", json.dumps(PLAN_JSON, indent=1) + "\n")


def emit_forecast_json() -> None:
    write("forecast.json", json.dumps(FORECAST_JSON, indent=1) + "\n")


def emit_debts_json() -> None:
    write("debts.json", json.dumps(DEBTS_JSON, indent=1) + "\n")


def emit_ventures_json() -> None:
    write("ventures.json", json.dumps(VENTURES_JSON, indent=1) + "\n")


def emit_review_json() -> None:
    write("review.json", json.dumps(REVIEW_JSON, indent=1) + "\n")


def emit_m9_json() -> None:
    write("m9.json", json.dumps(M9_JSON, indent=1) + "\n")


def emit_recon_json() -> None:
    write("recon.json", json.dumps(RECON_JSON, indent=1) + "\n")


def emit_automation_json() -> None:
    """Per-row expected automation outcome, the machine-readable twin of the EXPECTED.md tables."""
    import json
    rows = []
    for r in ROWS:
        cat, why, flags = automation(r)
        rows.append({
            "account": r.account,
            "posted": r.posted,
            "description": r.description,
            "amount_cents": r.amount,
            "category": cat,
            "why": why,
            "flags": sorted(flags),
            "pair": r.pair,
            "refund_of": r.refund_of,
        })
    pairs = {}
    for r in ROWS:
        if r.pair:
            pairs.setdefault(r.pair, {})
            legs = pair_legs(r.pair)
            in_leg = next(l for l in legs if l.amount > 0)
            pairs[r.pair] = {"kind": "card_payment" if ACCOUNTS[in_leg.account].kind == "credit" else "internal"}
    write("automation.json", json.dumps({"rows": rows, "pairs": pairs}, indent=2) + "\n")


def main() -> None:
    files = emit_csvs()
    ofx_names = emit_ofx()
    emit_rules_json()
    emit_automation_json()
    write("EXPECTED.md", expected_md(files))
    emit_recon_json()
    emit_plan_json()
    emit_forecast_json()
    emit_debts_json()
    emit_ventures_json()
    emit_review_json()
    emit_m9_json()
    print(f"wrote {len(files)} csv files, {len(ofx_names)} ofx files, rules.json, automation.json, recon.json, plan.json, forecast.json, debts.json, ventures.json, review.json, m9.json and EXPECTED.md ({len(ROWS)} ledger rows)")


if __name__ == "__main__":
    main()
