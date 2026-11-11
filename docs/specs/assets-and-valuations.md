# Actifs et valorisations

## Actifs

Un actif est ce que l'on possède ou ce que l'on doit : un nom, un type et une devise.

| Type | Code | Sens |
|---|---|---|
| Compte bancaire | `bank_account` | Avoir |
| Livret d'épargne | `savings_book` | Avoir |
| Assurance vie | `life_insurance` | Avoir |
| Titres (PEA, compte-titres) | `securities` | Avoir |
| Immobilier | `real_estate` | Avoir |
| Crypto-actifs | `crypto` | Avoir |
| Prêt | `loan` | Dette |

- Le **nom** est obligatoire, nettoyé de ses espaces autour, et limité à 100 caractères (`Asset::validate_name`).
- Le **code** du type est stable : c'est lui qui est enregistré en base et envoyé par les formulaires.
- Le **sens** est donné par `AssetKind::is_liability`. Ajouter un type oblige à décider s'il s'agit d'une dette : le `match` liste tous les types, sans joker.

## Devises et montants

- Devise proposée : l'euro (EUR) seulement, pour l'instant (voir [Patrimoine net](net-worth.md#limites-actuelles)). Le code gère déjà plusieurs devises : en ajouter une, c'est une variante de `Currency`, son code et son symbole ; le choix de la devise réapparaît alors dans les formulaires, et les tests du refus de mélanger les devises sont à rétablir (voir `crates/shared/src/lib.rs`).
- Un montant est un `Money` : une valeur décimale exacte et sa devise. Additionner deux devises différentes est refusé.
- Une valorisation prend la devise de son actif.

## Valorisations

Une valorisation est la valeur d'un actif à une date.

- **L'historique ne s'écrase jamais** : chaque saisie ajoute une ligne, même à une date déjà valorisée.
- La date vaut aujourd'hui par défaut dans le formulaire ; elle ne peut pas être dans le futur (une valeur future serait ignorée par le total, sans explication).
- Un avoir peut avoir une valeur négative (un compte à découvert).
- Une valorisation de plus de 90 jours est **ancienne** (`Valuation::is_stale`) : l'actif est signalé « à mettre à jour ». À 90 jours pile, elle est encore à jour.
- L'historique complet se consulte sur la page de l'actif, de la plus récente à la plus ancienne valorisation ; à date égale, la dernière saisie passe devant (c'est aussi elle qui compte dans le total).
- Chaque valorisation a son identifiant (UUID v7, trié par date de saisie), ce qui départage deux valeurs du même jour.

## Supprimer une valorisation

- Une valorisation saisie par erreur se supprime depuis l'historique de son actif, après une confirmation qui cite sa date et son montant. C'est définitif.
- Le total, l'historique et le graphique se recalculent d'eux-mêmes : la valeur précédente redevient la dernière connue.
- Supprimer la seule valorisation d'un actif le rend « jamais valorisé », et sa devise et son sens redeviennent modifiables.
- Une valorisation ne se supprime que depuis la page de son propre actif : désignée depuis un autre actif (lien périmé ou forgé), elle est traitée comme absente.

## Prêts

- On saisit **le capital restant dû, en positif** : `182 340`. Il est enregistré en négatif (`−182 340`) et vient en déduction du patrimoine.
- Un montant négatif saisi sur un prêt est refusé, pour éviter une double négation.
- Le capital restant dû se met à jour à la main, par une nouvelle valorisation.

## Modifier un actif

- Le nom et le type sont toujours modifiables.
- **Dès la première valorisation, deux choses sont figées** :
  - la **devise**, sinon l'historique mélangerait les devises ;
  - le **sens** (avoir ou dette) : un avoir ne devient pas un prêt, ni l'inverse, sinon l'historique changerait de signe.
- Pour corriger un actif figé, on le supprime et on le recrée.

## Supprimer un actif

- La suppression emporte **tout l'historique** de valorisations de l'actif. Elle est définitive ; la page demande une confirmation.
- `DeleteAsset` supprime d'abord les valorisations, puis l'actif (la base refuse une valorisation sans actif).
- Limite connue : ces deux écritures ne sont pas dans une même transaction. Si la seconde échoue, l'actif reste, sans historique.
