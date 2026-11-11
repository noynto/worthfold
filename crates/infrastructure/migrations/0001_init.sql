CREATE TABLE assets (
    id       TEXT PRIMARY KEY NOT NULL,
    name     TEXT NOT NULL,
    kind     TEXT NOT NULL,
    currency TEXT NOT NULL
);

-- Historique : une ligne par valorisation, jamais mise à jour.
CREATE TABLE valuations (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    asset_id  TEXT NOT NULL REFERENCES assets (id),
    amount    TEXT NOT NULL,  -- décimal exact, jamais un REAL
    currency  TEXT NOT NULL,
    valued_on TEXT NOT NULL   -- date ISO 8601 (AAAA-MM-JJ)
);

CREATE INDEX valuations_asset_id ON valuations (asset_id, valued_on);
