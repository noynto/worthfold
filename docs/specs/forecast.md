# Prévisions de trésorerie

La page **Prévisions** (`/forecast`) projette le solde des comptes courants, puis le patrimoine net, mois après mois, sur le mois en cours et les 12 suivants. Elle est en lecture seule : tout vient des [paiements prévus](payments.md), des [transactions](transactions.md) et des valorisations. Le calcul est une fonction pure (`service::forecast::forecast`), sans apprentissage : chaque chiffre s'explique.

## Le calcul

1. **Point de départ** : la somme des dernières valorisations des comptes courants (en euros), **moins les opérations en attente** que la banque annonce (elles ne sont pas dans le solde comptabilisé). Ces opérations servent aussi à reconnaître un paiement déjà en route : un loyer en attente n'est pas compté une seconde fois comme « prévu ». Le patrimoine net projeté suit.
2. **Échéances** : toutes les échéances des paiements prévus encore à venir, à leur date, avec leur montant attendu et leur marge. Une échéance dont la date est passée depuis peu mais dont l'opération n'est pas encore arrivée (jusqu'à 7 jours) compte dans le mois en cours ; une échéance déjà honorée, sautée ou manquante ne compte pas.
3. **Dépenses variables** : tout ce qui sort des comptes sans correspondre à un paiement déclaré, hors virements internes et revenus (un remboursement classé en dépense vient en déduction), mesuré sur les derniers mois complets (jusqu'à 3, les mêmes que pour le [matelas](budget.md)). Le mois en cours n'en compte que le reste, au prorata des jours.
4. **Patrimoine net** : celui d'aujourd'hui, plus l'écart de solde. Les autres actifs sont supposés stables.

## Trois scénarios

| Scénario | Échéances | Dépenses variables |
|---|---|---|
| **Prudent** | au pire de leur marge | le mois le plus cher |
| **Attendu** | à leur valeur | la moyenne des mois |
| **Favorable** | au mieux de leur marge | le mois le moins cher |

La fourchette s'élargit avec les mois : les marges et les écarts s'additionnent.

## Les alertes

- Le solde **attendu** passe à découvert : la page le dit, avec le mois.
- À défaut, le solde **prudent** peut passer à découvert.
- Le solde attendu passe **sous le matelas** (fixé, sinon évalué) : le versement vers le livret serait à reporter.

## Limites

- Les **versements vers le livret** ne sont pas prévus : c'est une suggestion, pas un engagement. Ils ne changent pas le patrimoine net.
- Un revenu qui n'est pas déclaré en paiement prévu n'est pas prévu : déclare le salaire.
- Sans mois complet de relevés, les dépenses variables sont nulles et la page l'écrit.
- L'amortissement des prêts n'est pas modélisé (ni taux ni durée) : la dette reste à sa dernière valorisation.
- Tout est en euros, comme le reste de l'application.
