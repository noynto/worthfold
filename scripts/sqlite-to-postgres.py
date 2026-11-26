#!/usr/bin/env python3
"""Reprend une base SQLite de worthfold (ancien format) dans PostgreSQL.

Lit le fichier SQLite et écrit sur la sortie standard un script SQL, à envoyer à `psql` une fois que
l'application a créé le schéma (elle le fait à son premier démarrage) :

    python3 scripts/sqlite-to-postgres.py worthfold.db \\
      | docker compose exec -T db psql -U worthfold -d worthfold -v ON_ERROR_STOP=1

- Tout est dans une seule transaction : une erreur n'importe rien.
- Le script refuse de s'exécuter si la base cible contient déjà des données.
- Ne sont pas repris : le mot de passe et les sessions (l'authentification est nouvelle : choisis ton mot de
  passe à la première ouverture), les demandes de consentement en cours et les opérations en attente (elles
  se relisent depuis la banque).
- Aucune dépendance : seulement la bibliothèque standard de Python. Le fichier SQLite n'est ouvert qu'en lecture.
"""

import sqlite3
import sys

# Dans l'ordre des clés étrangères : un parent avant ses enfants.
TABLES = [
    "assets",
    "valuations",
    "categories",
    "category_rules",
    "transactions",
    "envelopes",
    "savings_plan",
    "planned_payments",
    "planned_payment_steps",
    "planned_payment_exceptions",
    "bank_connections",
    "bank_accounts",
]
# Les colonnes qui étaient des entiers 0/1 dans SQLite et sont des booléens dans PostgreSQL.
BOOLEANS = {("planned_payments", "observed")}


def literal(table: str, column: str, value) -> str:
    if value is None:
        return "NULL"
    if (table, column) in BOOLEANS:
        return "TRUE" if value else "FALSE"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):  # jamais attendu : les montants sont du texte décimal exact
        raise SystemExit(f"{table}.{column} : un flottant ({value}) ; les montants doivent être du texte.")
    return "'" + str(value).replace("'", "''") + "'"


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
    existing = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
    tables = [t for t in TABLES if t in existing]

    out = sys.stdout
    out.write("-- Généré par scripts/sqlite-to-postgres.py\nBEGIN;\n")
    # Refuse d'écraser quoi que ce soit : la base cible doit être vide.
    checks = " OR ".join(f"EXISTS (SELECT 1 FROM {t})" for t in TABLES)
    out.write(
        "DO $$ BEGIN IF " + checks + " THEN "
        "RAISE EXCEPTION 'La base PostgreSQL contient déjà des données : import annulé.'; END IF; END $$;\n"
    )
    counts = {}
    for table in tables:
        cursor = db.execute(f"SELECT * FROM {table}")
        columns = [d[0] for d in cursor.description]
        n = 0
        for row in cursor:
            values = ", ".join(literal(table, c, v) for c, v in zip(columns, row))
            out.write(f"INSERT INTO {table} ({', '.join(columns)}) VALUES ({values});\n")
            n += 1
        counts[table] = n
    out.write("COMMIT;\n")
    summary = ", ".join(f"{t} : {n}" for t, n in counts.items())
    print(f"Repris : {summary}", file=sys.stderr)


if __name__ == "__main__":
    main()
