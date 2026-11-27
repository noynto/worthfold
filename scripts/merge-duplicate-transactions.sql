-- Fusionne les opérations enregistrées en double : un même mouvement lu chez la banque (source `bank`) et
-- importé d'un relevé (source `csv`), avant que l'application ne les rapproche toute seule.
--
-- Règle (la même que `service::transactions::new_transactions`) : sur un même compte, même date et même
-- montant, et des libellés identiques ou dont tous les mots de l'un se retrouvent dans l'autre (accents, casse
-- et ponctuation ignorés : « PRELEVEMENT ORANGE SA » et « Prélèvement ORANGE SA - Orange - … »). Chaque
-- opération ne sert qu'une fois. On garde la ligne du relevé (son libellé est le plus complet) et on supprime
-- celle de la banque ; si la ligne gardée n'a pas de catégorie et que l'autre en avait une, elle la reprend.
-- Deux opérations d'une même source ne sont jamais fusionnées.
--
-- Par défaut : SIMULATION. Le script affiche ce qu'il fusionnerait, puis annule tout.
--   docker compose exec -T db psql -U worthfold -d worthfold -f - < scripts/merge-duplicate-transactions.sql
-- Pour fusionner pour de bon (après une sauvegarde, voir docs/tech/deployment.md) :
--   docker compose exec -T db psql -U worthfold -d worthfold -v apply=1 -f - < scripts/merge-duplicate-transactions.sql

BEGIN;

CREATE FUNCTION pg_temp.norm(text) RETURNS text LANGUAGE sql IMMUTABLE AS
$$ SELECT lower(regexp_replace(btrim($1), '\s+', ' ', 'g')) $$;

-- Les mots d'un libellé : sans accents, en minuscules, séparés par la ponctuation.
CREATE FUNCTION pg_temp.words(text) RETURNS text[] LANGUAGE sql IMMUTABLE AS
$$ SELECT string_to_array(btrim(regexp_replace(
     translate(lower($1), 'àâäéèêëîïôöùûüç', 'aaaeeeeiioouuuc'), '[^a-z0-9]+', ' ', 'g')), ' ') $$;

CREATE TEMP TABLE pairs (csv_id text PRIMARY KEY, bank_id text UNIQUE) ON COMMIT DROP;

DO $$
DECLARE
  b record;
  c text;
BEGIN
  -- Passe 1 : libellés identiques. Passe 2 : l'un commence par l'autre.
  FOR pass IN 1..2 LOOP
    FOR b IN
      SELECT * FROM transactions t
      WHERE t.source = 'bank' AND NOT EXISTS (SELECT 1 FROM pairs p WHERE p.bank_id = t.id)
      ORDER BY t.booked_on, t.id
    LOOP
      c := NULL;
      SELECT t.id INTO c FROM transactions t
      WHERE t.source = 'csv'
        AND t.asset_id = b.asset_id AND t.booked_on = b.booked_on
        AND t.amount = b.amount AND t.currency = b.currency
        AND NOT EXISTS (SELECT 1 FROM pairs p WHERE p.csv_id = t.id)
        AND (
          (pass = 1 AND pg_temp.norm(t.label) = pg_temp.norm(b.label))
          OR (pass = 2 AND pg_temp.words(t.label) <> '{""}' AND pg_temp.words(b.label) <> '{""}'
              AND (pg_temp.words(t.label) <@ pg_temp.words(b.label)
                   OR pg_temp.words(b.label) <@ pg_temp.words(t.label)))
        )
      ORDER BY t.id
      LIMIT 1;
      IF c IS NOT NULL THEN
        INSERT INTO pairs (csv_id, bank_id) VALUES (c, b.id);
      END IF;
    END LOOP;
  END LOOP;
END
$$;

-- Ce qui sera fusionné.
SELECT a.name AS compte, c.booked_on AS date, c.amount AS montant,
       left(c.label, 50) AS libelle_releve, left(b.label, 50) AS libelle_banque
FROM pairs p
JOIN transactions c ON c.id = p.csv_id
JOIN transactions b ON b.id = p.bank_id
JOIN assets a ON a.id = c.asset_id
ORDER BY c.booked_on DESC, a.name;

SELECT count(*) AS doublons_a_fusionner FROM pairs;

-- La ligne gardée reprend la catégorie de l'autre si elle n'en a pas.
UPDATE transactions c SET category_id = b.category_id
FROM pairs p JOIN transactions b ON b.id = p.bank_id
WHERE c.id = p.csv_id AND c.category_id IS NULL AND b.category_id IS NOT NULL;

DELETE FROM transactions WHERE id IN (SELECT bank_id FROM pairs);

\if :{?apply}
  \echo 'FUSION APPLIQUEE.'
  COMMIT;
\else
  \echo 'SIMULATION : rien n''a ete modifie. Relancer avec -v apply=1 pour fusionner.'
  ROLLBACK;
\endif
