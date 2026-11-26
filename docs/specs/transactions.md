# Transactions et import de relevés

Une transaction est un mouvement d'un **compte bancaire** : un débit ou un crédit, à une date. C'est la matière première du suivi des comptes courants (budget et prévisionnel viendront ensuite).

## Transaction

| Champ | Sens |
|---|---|
| Compte | L'actif (type compte bancaire) auquel appartient l'opération |
| Date | La date d'opération (pas la date de valeur) |
| Montant | Signé, dans la devise du compte : négatif pour une dépense, positif pour un revenu |
| Libellé | Tel que la banque le donne, espaces superflus retirés |
| Source | D'où elle vient : `csv` (d'autres viendront : saisie manuelle, banque connectée) |
| Catégorie | Facultative : voir [Catégories et règles](categories.md) |

- Une transaction importée **ne se modifie pas** (règle 5 : l'historique ne s'écrase jamais), à une exception : sa **catégorie**. Elle disparaît seulement avec son compte.
- Seul un compte bancaire a des transactions : les autres types d'actifs refusent l'import.
- Supprimer un compte supprime ses transactions, avec son historique de valorisations.

## Import d'un relevé CSV

Depuis la page des opérations d'un compte (`/assets/{id}/transactions`), on choisit le fichier exporté de la banque.

### Reconnaissance du fichier

Les banques n'ont pas de format commun : le fichier est reconnu à ses **en-têtes**, pas à l'ordre des colonnes (`service::bank_csv`).

- Le séparateur (`;`, `,` ou tabulation) est deviné.
- Les lignes avant l'en-tête (nom du compte, période…) et les pieds de page sans date ni montant sont ignorés.
- Colonnes reconnues : une **date** (en-tête commençant par « date » ; « date d'opération » est préférée à « date de valeur »), un **libellé** (libellé, label, description, intitulé, détail, motif) et soit un **montant** signé, soit deux colonnes **débit** et **crédit**.
- Dates : `jj/mm/aaaa`, `aaaa-mm-jj`, `jj-mm-aaaa`, `jj/mm/aa`, `jj.mm.aaaa`.
- Montants : `1 234,56`, `1.234,56`, `-1234.56`, `12,50 €`, avec le signe moins typographique. Une colonne débit est toujours lue comme une dépense, qu'elle soit saisie en positif ou en négatif.
- Encodage : UTF-8, sinon Windows-1252.

Un fichier non reconnu est refusé en entier, avec la ligne et la valeur en cause (`Ligne 3 : la date « hier » n'est pas reconnue…`) : rien n'est enregistré à moitié.

### Pas de doublons

Réimporter un relevé qui en recouvre un autre ne duplique rien (`service::new_transactions`). Une opération est reconnue à sa **date, son montant et son libellé** (sans tenir compte de la casse ni des espaces). On compare les **nombres** d'occurrences : deux cafés au même prix le même jour restent deux opérations, mais si le compte en connaît déjà un, un seul des deux du fichier est nouveau.

Limite connue : si la banque **change le libellé** d'une opération entre deux exports (une opération en attente qui devient définitive, par exemple), elle n'est plus reconnue et s'importe une seconde fois. Un identifiant d'opération fourni par la banque, avec la synchronisation par banque connectée, supprimera ce cas.

### Catégorie à l'import

Chaque nouvelle opération est classée par les règles de [catégories](categories.md), si l'une d'elles la reconnaît.

### Refus

- Un compte inconnu, ou qui n'est pas un compte bancaire.
- Un fichier illisible (voir ci-dessus) ou sans aucune opération.
- Une opération datée d'une date **future** : c'est presque toujours une erreur de fichier ou de format de date (`01/02/2026` lu en mois/jour). Le futur relève du prévisionnel.

### Solde du compte

Le champ facultatif « Solde après la dernière opération » crée une **valorisation** du compte, à la date de la dernière opération du relevé. Le compte entre ainsi dans le [patrimoine net](net-worth.md) sans autre saisie. Sans solde indiqué, aucune valorisation n'est créée : le relevé ne dit pas, à lui seul, ce que le compte vaut.

## Affichage

La page des opérations montre le nombre d'opérations, puis les **200 plus récentes**, chacune avec sa catégorie modifiable (de la plus récente à la plus ancienne ; à date égale, la dernière importée d'abord). Au-delà, le titre le dit (`200 dernières opérations sur 1 250`). Voir [Écrans](../design/screens.md).

## À venir

Paiements récurrents, prévisionnel de trésorerie, puis synchronisation par une banque connectée (EnableBanking).
