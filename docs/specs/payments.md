# Paiements prévus

Un paiement prévu est ce qui revient ou arrive à date connue : salaire, fermage, impôt, abonnement. Il alimente le calendrier des échéances, repère ce qui manque, et nourrit le [prévisionnel de trésorerie](forecast.md). Il s'appuie sur les [transactions](transactions.md) pour se reconnaître dans les relevés.

Un paiement se décrit en **trois blocs** : *quand* il tombe, *combien*, et *pendant combien de temps*. Une fonction pure (`recurrence::occurrences`) les déplie en échéances datées.

## Un paiement

| Champ | Sens |
|---|---|
| Nom | Obligatoire, 100 caractères au plus |
| **Mot du libellé** | Le mot qui le reconnaît dans les relevés (« salaire », « edf »). Obligatoire |
| Sens | **Entrée** ou **sortie** d'argent. Le montant se saisit toujours en positif, le sens lui donne son signe |
| Compte | Un compte bancaire (les autres types d'actifs sont refusés) |
| Catégorie | Facultative, et cohérente avec le sens : une entrée va avec un revenu, une sortie avec une dépense, un virement interne va avec les deux |

Deux paiements du même compte, du même sens et reconnus par le même mot sont refusés : ils se disputeraient les mêmes opérations. Le même mot dans l'autre sens est autorisé (un remboursement).

## Quand (`Schedule`)

| Rythme | Exemple |
|---|---|
| **Chaque mois** | loyer le 5, abonnement le 12, salaire le 28 |
| **Tous les N mois** (2 à 24), à partir d'un mois | fermage trimestriel (N = 3), assurance annuelle (N = 12) |
| **Certains mois de l'année**, chaque année | complément d'impôt d'octobre à décembre |
| **Une seule fois** | une réparation prévue |

- Le **jour** est un jour du mois (1 à 31) ou **le dernier jour du mois**. Dans un mois plus court, « le 31 » devient le dernier jour (le 28 février, le 29 en année bissextile) : une échéance ne saute jamais un mois.
- Pour « certains mois », le formulaire demande une **plage** (« de novembre à février ») : elle peut passer le 31 décembre.
- Le rythme « tous les N mois » est ancré sur son **premier mois** : c'est lui qui fixe les mois suivants, jamais avant.

## Combien (`Amount`)

Le montant va **par paliers**. Le premier vaut dès le début ; chaque changement connu **ajoute** un palier (règle 5 : l'historique ne s'écrase jamais). Entre deux paliers, le dernier commencé s'applique ; les échéances d'avant un palier gardent leur ancien montant. Un second palier à la **même date** corrige le premier.

Chaque palier a une **marge**. Une marge à zéro dit un montant exact (l'avis d'imposition, l'indice publié) ; sinon c'est une estimation « ± la marge ».

### Montant variable : observé

Pour un salaire qui varie, on coche « le tirer de mes relevés ». La valeur devient **la médiane des 6 derniers paiements réels** reconnus par le mot du libellé, et la marge **le plus grand écart** à cette médiane (`service::recurrence::observed_amount`). Sans paiement réel, le premier palier sert. L'échéance est alors affichée comme une estimation.

### Révision annuelle estimée

Un fermage ou un impôt ne bouge pas de 2 % en continu : il **change de palier à une date**, et le nouveau montant n'est connu qu'une fois publié. La révision est donc **récurrente** et ne sert qu'à prévoir ce qu'on ne connaît pas encore : « chaque 15 octobre, +2 % ».

- Chaque 15 octobre écoulé depuis le dernier vrai palier applique la variation **une fois** : deux ans sans vrai montant donnent deux révisions.
- La marge de l'échéance s'élargit de l'écart ainsi estimé.
- Dès qu'on saisit le **vrai montant** (un nouveau palier à la date de publication), il remplace l'estimation, et l'année suivante repart de lui.

Pour le complément d'impôt, on saisit une estimation avec une marge ; l'avis d'imposition arrivé, on ajoute un palier exact.

## Pendant combien de temps (`Window`)

Une date de début, une date de fin, et un **nombre d'échéances**, tous facultatifs. Un paiement en quatre fois, c'est « chaque mois » avec quatre échéances à partir d'une date. Les échéances se comptent **depuis le début de la fenêtre**, même quand on regarde plus tard : la quatrième reste la dernière.

Limiter le nombre d'échéances demande une date de début ; une fin avant le début est refusée.

## Exceptions

On peut corriger **une échéance précise** sans toucher au paiement :

- **Sauter** : pas d'échéance ce jour-là (un mois sans prélèvement). Elle reste visible, barrée, et ne compte plus.
- **Fixer un autre montant**, exact.
- **Rétablir** : retirer l'exception.

L'échéance doit exister : une date qui n'est pas une échéance du rythme est refusée. Une échéance déjà honorée n'a plus rien à corriger.

## Rapprochement avec le réel

Chaque échéance est rapprochée des opérations réelles du compte (`service::recurrence::payment_lines`). Une opération **honore** une échéance si son libellé contient le mot du paiement, si son sens est le bon, et si elle tombe de **3 jours avant à 7 jours après** la date (un prélèvement décalé par un week-end). Une opération n'honore qu'**une** échéance : chaque échéance prend la plus proche qui n'a pas déjà servi.

| État | Condition |
|---|---|
| **Constaté** | l'opération est arrivée (le montant réel est affiché) |
| **Prévu** | la date n'est pas passée de plus de 7 jours |
| **Manquant** | la date est passée de plus de 7 jours sans opération : prélèvement refusé, abonnement résilié… un signal à regarder |
| **Sautée** | une exception la saute |

Le calendrier montre les **45 derniers jours** (constatées ou manquantes) et les **120 prochains** (attendues).

## Suggestions détectées

La page propose ce que l'historique laisse penser régulier (`service::recurrence::detect`). Il faut **au moins trois** opérations du même compte, du même sens et des mêmes mots significatifs (hors « prlv », « vir », « sepa »…), à intervalles réguliers : **25 à 35 jours** (mensuel) ou **85 à 95 jours** (trimestriel), dont la dernière est récente (70 jours avant l'opération la plus récente pour un mensuel, 130 pour un trimestriel). Un montant stable à 2 % près donne un montant fixe, sinon un montant observé.

Sont écartés : les virements internes, et ce qu'un paiement déjà déclaré reconnaît. Une suggestion n'est **jamais créée d'office** : un clic sur « Ajouter » la crée, et le rythme et le montant sont alors **recalculés d'après l'historique**, jamais lus dans le formulaire (une requête forgée ne peut pas inventer un paiement).

## Supprimer

- Supprimer un **paiement** supprime ses paliers et ses exceptions. Les opérations réelles qu'il reconnaissait restent dans le compte.
- Supprimer un **compte** supprime ses paiements.
- Supprimer une **catégorie** détache ses paiements (ils restent, sans catégorie).

## Limites actuelles

- Un paiement ne se modifie pas : on ajoute un palier de montant, on corrige une échéance, ou on le supprime et on le recrée.
- Pas de jour ouvré (« le dernier jour ouvré du mois ») ni de rythme hebdomadaire.
- La marge est un montant, pas un pourcentage.
- La détection ne repère que le mensuel et le trimestriel, et groupe les opérations par mots du libellé : un libellé qui change de mots d'un mois à l'autre n'est pas reconnu.
- Pas encore d'aperçu des prochaines échéances pendant la saisie : le paiement ajouté apparaît tout de suite dans le calendrier.
- Le prévisionnel de trésorerie et la prédiction du patrimoine qui s'appuieront sur ces échéances restent à faire.
