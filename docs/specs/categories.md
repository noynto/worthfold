# Catégories et règles

Une catégorie range les [transactions](transactions.md) (courses, loyer, salaire…). C'est la base du [budget par enveloppes](budget.md) et du prévisionnel, qui s'appuient sur elles.

## Catégorie

- Un **nom** : obligatoire, nettoyé de ses espaces, 60 caractères au plus (`Category::validate_name`). Deux catégories ne portent pas le même nom, sans tenir compte de la casse ni des accents (`énergie` et `Energie` sont la même).
- Un **sens**, qui décidera si une opération compte dans les dépenses, les revenus, ou aucun des deux :

| Sens | Code | Exemples |
|---|---|---|
| Dépense | `expense` | Courses, Logement, Santé |
| Revenu | `income` | Salaire, Fermages |
| Virement interne | `transfer` | Versement sur le Livret A |

Un **virement interne** est neutre : sans lui, un virement de ton compte courant vers ton livret compterait à la fois comme une dépense et comme un revenu.

### Catégories usuelles

Au premier lancement, la page Catégories propose **Ajouter les catégories usuelles** (`Category::DEFAULTS`, 14 catégories : courses, logement, énergie et eau, transports, santé, restaurants et sorties, loisirs, abonnements, impôts et taxes, assurances, salaire, fermages et loyers perçus, autres revenus, virement interne). Rien n'est créé sans cette demande, et l'action peut être rejouée sans doublon : une catégorie dont le nom existe déjà est laissée telle quelle.

### Supprimer une catégorie

Elle supprime la catégorie, **ses règles et ses enveloppes de budget**. Les transactions qu'elle classait ne sont pas supprimées : elles redeviennent **sans catégorie**. La page demande une confirmation qui le dit. Comme pour `DeleteAsset`, ce sont plusieurs écritures sans transaction commune (voir [Architecture](../tech/architecture.md#limites-connues)).

## Règle d'affectation

Une règle dit : **tout libellé qui contient le motif reçoit la catégorie**.

- Le motif est cherché **sans tenir compte de la casse, des accents ni des espaces multiples** : `électricité` reconnaît `PRLV  ELECTRICITE DE FRANCE`.
- Le motif est obligatoire et fait 100 caractères au plus.
- **La règle la plus précise l'emporte** : quand plusieurs règles conviennent, c'est celle au motif le plus long (`carrefour city` passe avant `carrefour`). À précision égale, la plus ancienne. Il n'y a donc pas de priorité à régler à la main (`category::matching_category`).
- Un même motif ne mène qu'à **une** catégorie : en ajouter un second, même vers une autre catégorie, est refusé avec le nom de la catégorie qui le porte. Sinon le résultat dépendrait de l'ordre de création.
- Supprimer une règle ne déclasse pas les opérations qu'elle a déjà classées.

## Quand les règles s'appliquent

- **À l'import** d'un relevé : chaque nouvelle opération arrive déjà classée si une règle la reconnaît.
- **À la demande**, avec **Classer les opérations** sur la page Catégories (`ApplyCategoryRules`) : pour les opérations déjà importées, par exemple après avoir ajouté une règle. Ajouter une règle ne reclasse rien d'elle-même.
- **Jamais sur une opération déjà classée**, qu'elle l'ait été à la main ou par une règle (`service::rule_assignments`) : un choix de l'utilisateur ne doit pas être écrasé par une règle ajoutée plus tard. Pour changer une opération classée par une règle, on la reclasse à la main.

## Classer à la main

Chaque ligne de la page des opérations d'un compte porte une liste de catégories et un bouton **Classer** (`CategorizeTransaction`). Choisir **Sans catégorie** retire la catégorie. Après l'enregistrement, la page revient sur la ligne modifiée (ancre `#t-{id}`), pour ne pas perdre sa place dans une longue liste. Une opération désignée depuis la page d'un autre compte (lien périmé ou forgé) est traitée comme absente.

C'est la seule modification possible d'une transaction après son import : sa date, son montant et son libellé ne changent jamais.

## Limites actuelles

- Pas de catégories imbriquées (Logement > Loyer) : une liste à plat. Les enveloppes du budget s'en contentent.
- Pas de renommage ni de changement de sens d'une catégorie : on la supprime et on la recrée (les opérations redeviennent sans catégorie).
- Pas de détection automatique des virements internes : on les classe par une règle (par exemple le libellé de ton virement vers le Livret A) ou à la main.
