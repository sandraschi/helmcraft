# HelmCraft PRD

Status 13 November 2026.

## What it does

- Reads sibling SQLite files for the suite apps when the file exists and the query succeeds.
- Records provenance in `data_notes`. Treasury cash is the closing balance of BuchCraft's bank account (`bank_account` in that book's meta, otherwise 2800). A demo ledger is reported and then ignored. When the ledger is missing, cash is `HELMCRAFT_CASH_CENTS` or the app setting.
- `net_bank_flow_cents` stays the sum of `bank_tx`. It is movement, and it is not the cash figure.
- The board text and the window say when cash or AZG was not measured. A green zero is not a compliance finding.
- Saves the snapshot.

## What it does not do yet

- Treat a demo BuchCraft file as the company treasury.
- Replace Geckoboard or Baremetrics.

## Done when

The bank-account balance, the burn, and the AZG count are covered by a test that builds temporary databases. See `BACKOFFICE_THIN_RUNT_PLAN.md`.
