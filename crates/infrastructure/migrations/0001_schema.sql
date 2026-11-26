-- Schéma de worthfold sur PostgreSQL.
--
-- Choix de types : les montants sont des NUMERIC (décimal exact, jamais un flottant), les jours des DATE,
-- les instants des TIMESTAMPTZ. Les identifiants applicatifs sont des UUID v7 en TEXT (triés par date de
-- création : `ORDER BY id` redonne l'ordre de saisie).
--
-- Une migration appliquée ne se modifie jamais (somme de contrôle) : on en ajoute une nouvelle.

CREATE TABLE assets (
    id       TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    kind     TEXT NOT NULL,
    currency TEXT NOT NULL
);

-- Historique : une ligne par valorisation, jamais mise à jour.
CREATE TABLE valuations (
    id        TEXT PRIMARY KEY,
    asset_id  TEXT NOT NULL REFERENCES assets (id),
    amount    NUMERIC NOT NULL,
    currency  TEXT NOT NULL,
    valued_on DATE NOT NULL
);
CREATE INDEX valuations_asset_id ON valuations (asset_id, valued_on);

-- Catégories de transactions, et règles qui les affectent d'après le libellé.
CREATE TABLE categories (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL            -- expense, income ou transfer
);

CREATE TABLE category_rules (
    id          TEXT PRIMARY KEY,
    category_id TEXT NOT NULL REFERENCES categories (id),
    pattern     TEXT NOT NULL
);
CREATE INDEX category_rules_category_id ON category_rules (category_id);

-- Mouvements des comptes bancaires. Une ligne par opération ; seule la catégorie change après l'import.
CREATE TABLE transactions (
    id          TEXT PRIMARY KEY,
    asset_id    TEXT NOT NULL REFERENCES assets (id),
    booked_on   DATE NOT NULL,
    amount      NUMERIC NOT NULL,   -- négatif pour une dépense
    currency    TEXT NOT NULL,
    label       TEXT NOT NULL,
    source      TEXT NOT NULL,      -- csv ou bank
    category_id TEXT REFERENCES categories (id)
);
CREATE INDEX transactions_asset_id ON transactions (asset_id, booked_on);
CREATE INDEX transactions_category_id ON transactions (category_id);

-- Opérations annoncées par la banque mais pas encore comptabilisées. Pas un historique : chaque lecture
-- de la banque remplace toutes les lignes du compte.
CREATE TABLE pending_transactions (
    id        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    asset_id  TEXT NOT NULL REFERENCES assets (id),
    booked_on DATE NOT NULL,
    amount    NUMERIC NOT NULL,
    currency  TEXT NOT NULL,
    label     TEXT NOT NULL
);
CREATE INDEX pending_transactions_asset_id ON pending_transactions (asset_id);

-- Plafonds mensuels par catégorie. Une ligne par changement, jamais mise à jour : le plafond d'un mois
-- est celui de la ligne la plus récente qui commence au plus tard ce mois-là.
CREATE TABLE envelopes (
    id          TEXT PRIMARY KEY,
    category_id TEXT NOT NULL REFERENCES categories (id),
    amount      NUMERIC,            -- NULL : plus d'enveloppe à partir de ce mois
    currency    TEXT,
    from_month  DATE NOT NULL       -- premier jour du mois
);
CREATE INDEX envelopes_category_id ON envelopes (category_id, from_month);

-- Le réglage de l'épargne : une seule ligne.
CREATE TABLE savings_plan (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    destination      TEXT NOT NULL REFERENCES assets (id),
    ceiling_amount   NUMERIC NOT NULL,
    ceiling_currency TEXT NOT NULL,
    cushion_amount   NUMERIC,       -- NULL : matelas évalué d'après les dépenses
    cushion_currency TEXT
);

-- Paiements prévus : quand (le rythme), combien (les paliers, à part), pendant combien de temps (la fenêtre).
CREATE TABLE planned_payments (
    id              TEXT PRIMARY KEY,
    label           TEXT NOT NULL,
    pattern         TEXT NOT NULL,                     -- mot du libellé bancaire qui reconnaît le paiement
    direction       TEXT NOT NULL,                     -- in ou out
    account_id      TEXT NOT NULL REFERENCES assets (id),
    category_id     TEXT REFERENCES categories (id),
    -- Le rythme : monthly, every_n_months, on_months ou once.
    schedule_kind   TEXT NOT NULL,
    day_kind        TEXT,                              -- day ou last (absent pour once)
    day_value       INTEGER,                           -- 1 à 31 quand day_kind = day
    every_n         INTEGER,
    first_month     DATE,
    months          TEXT,                              -- mois choisis, séparés par des virgules : 10,11,12
    once_date       DATE,
    -- Le montant : les paliers sont dans planned_payment_steps.
    observed        BOOLEAN NOT NULL,                  -- la valeur vient des paiements réels
    revision_month  INTEGER,
    revision_day    INTEGER,
    revision_change NUMERIC,                           -- pourcentage
    -- La fenêtre.
    window_from     DATE,
    window_until    DATE,
    window_count    INTEGER
);
CREATE INDEX planned_payments_account_id ON planned_payments (account_id);

-- Un palier de montant : valable à partir de sa date, jamais mis à jour (on en ajoute un nouveau).
CREATE TABLE planned_payment_steps (
    payment_id TEXT NOT NULL REFERENCES planned_payments (id),
    from_date  DATE NOT NULL,
    amount     NUMERIC NOT NULL,
    margin     NUMERIC NOT NULL,
    currency   TEXT NOT NULL,
    PRIMARY KEY (payment_id, from_date)
);

-- Une exception sur une échéance précise : la sauter ou en changer le montant.
CREATE TABLE planned_payment_exceptions (
    payment_id    TEXT NOT NULL REFERENCES planned_payments (id),
    original_date DATE NOT NULL,
    action        TEXT NOT NULL,                       -- skip ou amount
    amount        NUMERIC,
    currency      TEXT,
    PRIMARY KEY (payment_id, original_date)
);

-- Connexion bancaire (agrégateur). Une session par consentement ; le secret (clé privée de l'application)
-- n'est jamais en base : l'identifiant de session seul ne permet aucun appel.
CREATE TABLE bank_connections (
    id          TEXT PRIMARY KEY,
    bank_name   TEXT NOT NULL,
    country     TEXT NOT NULL,
    session_id  TEXT NOT NULL,
    valid_until DATE NOT NULL,      -- dernier jour du consentement
    created_on  DATE NOT NULL
);

CREATE TABLE bank_accounts (
    uid            TEXT PRIMARY KEY,
    connection_id  TEXT NOT NULL REFERENCES bank_connections (id),
    identity       TEXT NOT NULL,
    label          TEXT NOT NULL,
    currency       TEXT NOT NULL,
    asset_id       TEXT REFERENCES assets (id),
    last_synced_on DATE
);

-- Les demandes de consentement en cours : l'état renvoyé par la banque ne sert qu'une fois.
CREATE TABLE bank_pending (
    state      TEXT PRIMARY KEY,
    bank_name  TEXT NOT NULL,
    country    TEXT NOT NULL,
    created_on DATE NOT NULL
);

-- Authentification d'un seul utilisateur. Ni le mot de passe ni les jetons de session ne sont conservés :
-- seulement leurs empreintes (le mot de passe par Argon2id, le jeton par SHA-256).
CREATE TABLE credentials (
    id            INTEGER PRIMARY KEY CHECK (id = 1),  -- une seule ligne
    password_hash TEXT NOT NULL
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
