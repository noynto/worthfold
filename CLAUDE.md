# CLAUDE.md

Ce fichier guide Claude Code (claude.ai/code) lorsqu'il travaille sur le code de ce dépôt.

Le README est le point d'entrée unique du projet : présentation, pile technique, commandes, règles qui s'appliquent toujours, et index de tous les documents de `docs/`. Il est importé ici et sert de référence pour tout travail :

@README.md

Avant de travailler sur un sujet, ouvrir les documents que l'index du README liste pour lui. Mettre le README à jour dans la même modification lorsqu'un document est ajouté, renommé ou supprimé, ou lorsqu'une règle, la pile ou une commande change. Mettre chaque document de `docs/` à jour dans la même modification que le code qu'il décrit. Ne pas ajouter de contenu propre au projet dans ce fichier : il va dans le README ou dans un document qu'il indexe.

Avant de considérer une modification comme terminée, lancer `cargo clippy --workspace --all-targets` (aucun avertissement) et `cargo nextest run`.
