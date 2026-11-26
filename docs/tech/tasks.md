# Tâches

worthfold n'a pas de planificateur interne. Un traitement périodique est une **tâche one-shot**, sur le modèle d'EOSA : le binaire `worthfold` démarre, assemble les cas d'usage, exécute la tâche, journalise son compte rendu et s'arrête, **sans démarrer le serveur web**. Un planificateur externe la lance : cron, un timer systemd ou un CronJob Kubernetes.

## Les tâches existantes

| Variable | Tâche | Cas d'usage |
|---|---|---|
| `WORTHFOLD_BANK_SYNC_TASK` | Lit les opérations et soldes des comptes bancaires rattachés ([Connexion bancaire](../specs/bank.md)). Code 1 si un compte ou un consentement est en échec. À lancer au plus quelques fois par jour : les banques limitent les lectures | `SyncBankAccounts` |
| `WORTHFOLD_RESET_PASSWORD_TASK` | Mot de passe oublié : retire le mot de passe et toutes les sessions ([Sécurité](security.md#authentification)). Au démarrage suivant du serveur, la page de première configuration redemande un mot de passe. À lancer à la main, jamais planifiée | `ResetPassword` |

Les tâches vivent dans `crates/bootstrap/src/tasks.rs` (deux pour l'instant : une crate `tasks` séparée n'apportera rien de plus). Candidates : le relevé mensuel du patrimoine, la récupération des cours, le rappel des valeurs à mettre à jour.

## Fonctionnement

- Une variable d'environnement **active** la tâche : `WORTHFOLD_<NOM>_TASK=true`. Elle n'est définie que pour l'exécution de la tâche (le CronJob, l'entrée cron), jamais pour le serveur.
- Le `main` lit la configuration et assemble les cas d'usage comme pour le serveur (la base est ouverte et les migrations appliquées), puis exécute la première tâche activée.
- **Code de sortie** : 0 si la tâche a réussi, 1 sinon. C'est lui que lit le planificateur pour signaler un échec. Une erreur de configuration donne aussi le code 1.
- Un seul binaire, donc une seule image : la tâche tourne avec la même version (et les mêmes migrations) que le serveur.

## Règles d'écriture

- Une tâche **ne fait qu'appeler un cas d'usage** et journaliser son `Output` (le nombre d'éléments traités, ceux en échec). La logique et les tests sont dans le cas d'usage.
- Une tâche est **idempotente** : relancée ou exécutée deux fois, elle ne fait rien deux fois (une date déjà relevée, un rappel déjà envoyé).
- Le fonctionnement de l'application ne dépend pas de la cadence : une tâche qui passe en retard ne rend rien faux.

## Ajouter une tâche

1. Le cas d'usage dans `application`, avec ses tests, et son champ dans `UseCases`.
2. La crate `tasks` (adaptateur, comme `server`) si c'est la première tâche ; une fonction par tâche, qui reçoit les cas d'usage et rend le code de sortie.
3. Son activation dans le `main` de `bootstrap` : si la variable est définie, exécuter la tâche et sortir avec son code.
4. Sa planification (cron, CronJob) et une ligne dans ce document.
