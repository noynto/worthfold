# Budget et épargne

Le budget compare ce qui est dépensé à des plafonds mensuels par catégorie (les **enveloppes**), puis dit combien verser sur un livret en fin de mois sans jamais entamer un **matelas de sécurité** évalué d'après la situation réelle. Il s'appuie sur les [transactions](transactions.md) et leurs [catégories](categories.md).

Seules les opérations comptabilisées comptent : celles que la banque annonce en attente ne comptent qu'une fois comptabilisées (le [prévisionnel](forecast.md), lui, les retranche du solde de départ).

Un budget est tenu **par devise** (l'euro, seule devise proposée : voir [Patrimoine net](net-worth.md#limites-actuelles)).

## Synthèse d'un mois

Pour un mois (`service::budget::month_summary`), toutes les transactions des comptes bancaires sont rangées selon le sens de leur catégorie :

| Catégorie | Compte comme |
|---|---|
| Revenu | un **revenu** |
| Dépense | une **dépense**, nette des remboursements (une entrée dans une catégorie de dépense réduit la dépense) |
| Virement interne | **ni l'un ni l'autre** : c'est de l'argent qui change de poche (compté à part comme « viré vers l'épargne ») |
| Sans catégorie | une sortie compte comme **dépense** (on ne sait pas ce que c'est : mieux vaut ne pas la cacher) ; une entrée est ignorée (rien ne dit que c'est un revenu) |

**Ce qu'il reste du mois** = revenus − dépenses (négatif si l'on a dépensé plus que gagné). La page signale le nombre d'opérations sans catégorie, qui faussent le résultat tant qu'elles ne sont pas classées.

## Enveloppes

Une enveloppe est un **plafond mensuel pour une catégorie de dépense**. Seules les catégories de dépense en ont : un revenu ou un virement interne est refusé.

- **L'historique des plafonds ne s'écrase jamais** (règle 5). Fixer un plafond **ajoute** une ligne qui s'applique **à partir d'un mois** ; le plafond d'un mois est celui de la ligne la plus récente qui commence au plus tard ce mois-là (`budget::effective_limit`). Les mois passés gardent donc le plafond qu'ils avaient, même après une hausse. À mois égal, la dernière saisie l'emporte.
- **Retirer** une enveloppe ajoute aussi une ligne (sans plafond) à partir du mois : les mois précédents gardent leur enveloppe. Retirer une enveloppe qui n'existe pas ce mois-là est refusé.
- Le plafond est positif ou nul. Un plafond à zéro est dépassé dès la première dépense.
- Pas de report : ce qui n'est pas dépensé un mois n'augmente pas le plafond du suivant. Le restant du mois va à l'épargne (voir ci-dessous).

### État d'une enveloppe

Dépensé = les dépenses du mois dans la catégorie, nettes des remboursements, jamais négatif.

| État | Condition | Affichage |
|---|---|---|
| Dans les clous | moins de 80 % du plafond | « Reste 120,00 € » |
| Proche | de 80 % au plafond **inclus** | « Reste 20,00 € · proche du plafond » |
| Dépassée | plus que le plafond | « Dépassé de 50,00 € » |

Le pourcentage est arrondi à l'unité, 0,5 vers le haut. La jauge est plafonnée à 100 %, mais le pourcentage réel est annoncé aux lecteurs d'écran. La couleur ne porte jamais seule l'information : l'état est aussi écrit.

## Matelas de sécurité

Le matelas est ce qu'on garde sur les comptes courants : de quoi traverser un mois **sans revenu** sans passer à découvert. Il n'est pas fixé d'avance, il est **évalué d'après les dépenses réelles** (`service::budget::recommended_cushion`) :

> **le mois le plus cher parmi les 3 derniers mois complets, arrondi aux 50 € supérieurs.**

- Les virements internes ne comptent pas ; les sorties sans catégorie, si.
- Un mois est **complet** quand les relevés importés le couvrent : une opération dans ses 7 premiers jours et une dans ses 7 derniers. Le mois en cours ne l'est jamais.
- Sans aucun mois complet, le matelas est **inconnu** : aucun versement n'est suggéré, et la page demande d'importer au moins un mois complet.
- Il se recalcule à chaque import : il suit la situation (une hausse du train de vie monte le matelas).
- On peut le **fixer à la main** dans le réglage de l'épargne ; il remplace alors l'évaluation, qui reste affichée pour comparaison.

Pourquoi le mois le plus cher plutôt que la moyenne : le matelas doit couvrir un mauvais mois, pas un mois moyen. Les engagements connus jusqu'au prochain salaire (paiements récurrents) l'affineront quand ils existeront.

## Versement d'épargne

Le réglage de l'épargne (`SavingsPlan`) donne le **livret de destination** (un actif de type livret d'épargne, dans la devise du budget), son **plafond de dépôt** et, facultativement, un matelas fixé à la main. Le plafond proposé est celui du Livret A, **22 950 €** : c'est une valeur de départ modifiable, à vérifier car il peut changer.

Le versement suggéré pour un mois (`service::budget::savings_suggestion`) est **le plus petit de trois montants** :

1. ce qu'il est **resté du mois**, moins ce qui a déjà été viré vers l'épargne ce mois-là ;
2. ce que les **comptes courants contiennent au-delà du matelas** (leur solde du jour, d'après les dernières valorisations) ;
3. la **place restante sous le plafond** du livret (plafond moins sa dernière valeur).

Aucun de ces montants n'est négatif : un mois déficitaire, un compte sous le matelas ou un livret plein donnent « Rien à verser », avec la raison. Le plus petit des trois évite tout versement qui ferait passer sous le matelas ou au-dessus du plafond.

**worthfold ne déplace pas d'argent** : il suggère, tu fais le virement à ta banque. Une fois le relevé importé, le virement (classé en catégorie « Virement interne », par une règle sur son libellé ou à la main) compte comme **déjà viré** : la suggestion tombe à zéro d'elle-même, sans suivi à tenir.

Pour un mois passé, la suggestion utilise les soldes d'aujourd'hui ; la page le précise.

## Supprimer

- Supprimer une **catégorie** supprime aussi ses enveloppes.
- Supprimer le **livret de destination** supprime le réglage de l'épargne.

## Limites actuelles

- Pas de report d'enveloppe d'un mois sur l'autre.
- Les enveloppes se règlent mois par mois (à partir d'un mois) : pas de plafond annuel.
- Pas de budget par compte : toutes les opérations des comptes bancaires comptent ensemble.
- Le matelas ne connaît pas encore les [paiements prévus](payments.md) (fermage trimestriel, impôts…) : il vient du passé. Le prévisionnel s'en chargera.
- Le plafond du livret est saisi à la main (22 950 € par défaut).
