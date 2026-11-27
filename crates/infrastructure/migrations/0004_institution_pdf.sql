-- Le format du relevé PDF d'une banque (facultatif) : toutes les colonnes ensemble, ou aucune.
ALTER TABLE institutions
    ADD COLUMN pdf_date_separator TEXT CHECK (pdf_date_separator IN ('dot', 'slash')),
    ADD COLUMN pdf_debit_header   TEXT,
    ADD COLUMN pdf_credit_header  TEXT,
    ADD COLUMN pdf_balance_marker TEXT,
    ADD COLUMN pdf_end_marker     TEXT,
    ADD CONSTRAINT institutions_pdf_all_or_none CHECK (
        (pdf_date_separator IS NULL AND pdf_debit_header IS NULL AND pdf_credit_header IS NULL
            AND pdf_balance_marker IS NULL AND pdf_end_marker IS NULL)
        OR (pdf_date_separator IS NOT NULL AND pdf_debit_header IS NOT NULL AND pdf_credit_header IS NOT NULL
            AND pdf_balance_marker IS NOT NULL AND pdf_end_marker IS NOT NULL));
