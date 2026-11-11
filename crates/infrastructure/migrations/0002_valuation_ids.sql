-- Les valorisations reçoivent un identifiant applicatif (UUID v7, texte), pour être désignées et supprimées
-- une à une. SQLite ne sait pas changer le type d'une clé primaire : on recrée la table.
CREATE TABLE valuations_new (
    id        TEXT PRIMARY KEY NOT NULL,
    asset_id  TEXT NOT NULL REFERENCES assets (id),
    amount    TEXT NOT NULL,  -- décimal exact, jamais un REAL
    currency  TEXT NOT NULL,
    valued_on TEXT NOT NULL   -- date ISO 8601 (AAAA-MM-JJ)
);

-- Les anciens identifiants entiers deviennent des UUID au temps zéro, numérotés sur 12 chiffres :
-- ils gardent leur ordre de saisie entre eux et passent avant toute valorisation saisie ensuite.
INSERT INTO valuations_new (id, asset_id, amount, currency, valued_on)
SELECT printf('00000000-0000-7000-8000-%012d', id), asset_id, amount, currency, valued_on
FROM valuations;

DROP TABLE valuations;
ALTER TABLE valuations_new RENAME TO valuations;

CREATE INDEX valuations_asset_id ON valuations (asset_id, valued_on);
