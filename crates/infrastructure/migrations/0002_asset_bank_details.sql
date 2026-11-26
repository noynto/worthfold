-- La banque et le numéro d'un compte bancaire (facultatifs : les comptes existants n'en ont pas).
ALTER TABLE assets
    ADD COLUMN bank_name      TEXT,
    ADD COLUMN account_number TEXT;
