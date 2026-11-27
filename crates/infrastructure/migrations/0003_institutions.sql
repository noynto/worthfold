-- Les banques et la façon de décoder leurs exports CSV. Le nom identifie la banque.
CREATE TABLE institutions (
    name              TEXT PRIMARY KEY,
    encoding          TEXT    NOT NULL CHECK (encoding IN ('utf8', 'windows1252')),
    delimiter         TEXT    NOT NULL CHECK (delimiter IN ('semicolon', 'comma', 'tab')),
    header_line       INTEGER NOT NULL CHECK (header_line BETWEEN 1 AND 100),
    date_column       TEXT    NOT NULL,
    date_format       TEXT    NOT NULL CHECK (date_format IN ('dmy_slash', 'dmy2_slash', 'dmy_dash', 'dmy_dot', 'ymd_dash')),
    label_column      TEXT    NOT NULL,
    amount_column     TEXT,
    debit_column      TEXT,
    credit_column     TEXT,
    decimal_separator TEXT    NOT NULL CHECK (decimal_separator IN ('comma', 'dot')),
    invert_sign       BOOLEAN NOT NULL,
    -- Soit une colonne de montant signé, soit deux colonnes débit et crédit.
    CHECK ((amount_column IS NOT NULL AND debit_column IS NULL AND credit_column IS NULL)
        OR (amount_column IS NULL AND debit_column IS NOT NULL AND credit_column IS NOT NULL))
);

-- Les banques déjà saisies en texte libre deviennent des banques, avec le format le plus courant à ajuster.
INSERT INTO institutions (name, encoding, delimiter, header_line, date_column, date_format,
                          label_column, amount_column, decimal_separator, invert_sign)
SELECT DISTINCT bank_name, 'utf8', 'semicolon', 1, 'Date', 'dmy_slash', 'Libellé', 'Montant', 'comma', FALSE
FROM assets
WHERE bank_name IS NOT NULL;

-- Une banque ne se supprime pas tant qu'un compte s'y rattache ; son nom ne change pas sans les comptes.
ALTER TABLE assets
    ADD CONSTRAINT assets_bank_name_fkey FOREIGN KEY (bank_name) REFERENCES institutions (name);
