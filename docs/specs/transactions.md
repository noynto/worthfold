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

### Lecture du fichier

Le fichier est un **CSV** ou un **PDF** (reconnu à son contenu), lu selon le format de la banque du compte : pour un PDF, voir [Banques](banks.md#le-format-pdf-facultatif). Pour un CSV, le fichier est lu selon le **format CSV de la banque du compte** (encodage, séparateur, ligne d'en-tête, colonnes, écriture des dates et des montants) : voir [Banques et format CSV](banks.md). Rien n'est deviné ; un compte sans banque ne peut pas importer.

Les lignes vides et les pieds de page sans date ni montant sont ignorés (`service::bank_csv`).

Un fichier qui ne correspond pas au format est refusé en entier, avec la colonne introuvable ou la ligne et la valeur en cause (`Ligne 3 : la date « hier » n'est pas au format jj/mm/aaaa.`) : rien n'est enregistré à moitié.

### Pas de doublons

Réimporter un relevé qui en recouvre un autre ne duplique rien (`service::new_transactions`). Une opération est reconnue à sa **date, son montant et son libellé** (sans tenir compte de la casse ni des espaces). On compare les **nombres** d'occurrences : deux cafés au même prix le même jour restent deux opérations, mais si le compte en connaît déjà un, un seul des deux du fichier est nouveau.

**Une même opération vue par deux sources.** Le relevé et la banque connectée ne libellent pas un mouvement de la même façon : la banque l'abrège (`PAIEMENT PAR CARTE X2947 ANTHROPIC* CLAUDE SU 24/11`), le relevé y ajoute la contrepartie (`… 24/11 - Anthropic`). Après la comparaison exacte, une opération qui reste est donc rapprochée d'une opération **de l'autre source** de même date et de même montant, quand au moins 80 % des mots du plus court libellé se retrouvent dans l'autre, car la banque et le relevé n'écrivent pas toujours le même identifiant de mandat (accents, casse et ponctuation ignorés : `PRELEVEMENT ORANGE SA Votre abonnement` et `Prélèvement ORANGE SA - Orange - Votre abonnement…`). Chaque opération connue ne sert qu'une fois, et deux opérations d'une même source ne se rapprochent jamais ainsi : deux prélèvements distincts du même jour restent deux opérations. La première arrivée est conservée, la seconde n'est pas enregistrée.

Limites : un libellé qui **change** sans que 80 % des mots du plus court se retrouvent dans l'autre (une opération en attente devenue définitive, par exemple) n'est pas reconnu et s'importe une seconde fois ; les doublons déjà enregistrés avant ce rapprochement ne sont pas fusionnés (une transaction ne se supprime pas).

### Catégorie à l'import

Chaque nouvelle opération est classée par les règles de [catégories](categories.md), si l'une d'elles la reconnaît.

### Refus

- Un compte inconnu, ou qui n'est pas un compte bancaire, ou un compte sans banque.
- Un fichier illisible (voir ci-dessus) ou sans aucune opération.
- Une opération datée d'une date **future** : c'est presque toujours une erreur de fichier ou de format de date (`01/02/2026` lu en mois/jour). Le futur relève du prévisionnel.

### Solde du compte

Le champ facultatif « Solde après la dernière opération » crée une **valorisation** du compte, à la date de la dernière opération du relevé. Le compte entre ainsi dans le [patrimoine net](net-worth.md) sans autre saisie. Sans solde indiqué, aucune valorisation n'est créée : le relevé ne dit pas, à lui seul, ce que le compte vaut.

## Affichage

La page des opérations montre le nombre d'opérations, puis les **200 plus récentes**, chacune avec sa catégorie modifiable (de la plus récente à la plus ancienne ; à date égale, la dernière importée d'abord). Au-delà, le titre le dit (`200 dernières opérations sur 1 250`). Voir [Écrans](../design/screens.md).

## À venir

Synchronisation par une banque connectée (EnableBanking) : reportée, l'import de relevés CSV est la voie principale pour le moment.
