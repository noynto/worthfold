# Banques et formats de relevé (CSV et PDF)

Les banques n'ont pas de format d'export commun. Plutôt que de le deviner, worthfold le fait **décrire une fois par banque** : un compte bancaire choisit sa banque, et l'import de ses relevés ([Transactions](transactions.md)) lit le fichier selon les réglages de cette banque. Rien n'est deviné.

## Une banque

- Un **nom**, unique (sans tenir compte de la casse), 100 caractères au plus, sans `/` ni `\`. Il identifie la banque et **ne change pas** : un compte s'y rattache par ce nom.
- Un **format CSV** (ci-dessous), que l'on modifie à tout moment. Modifier le format ne change pas les opérations déjà importées.
- Elle se **supprime** seulement si aucun compte ne s'y rattache (le refus nomme les comptes). Supprimer une banque ne touche pas aux opérations importées.

## Le format CSV

| Réglage | Sens |
|---|---|
| Encodage | UTF-8, ou Windows-1252 (Latin-1), fréquent chez les banques françaises. Un fichier qui n'est pas de l'UTF-8 valide est refusé avec le conseil de changer l'encodage |
| Séparateur de colonnes | `;`, `,` ou tabulation |
| Ligne de l'en-tête | Numéro de ligne (à partir de 1, de 1 à 100) de la ligne qui nomme les colonnes. Les lignes avant (nom du compte, période…) sont ignorées |
| Colonne de la date, du libellé | Le nom de l'en-tête, tel qu'il est écrit dans le fichier ; la casse, les accents et les espaces en trop ne comptent pas |
| Écriture des dates | `jj/mm/aaaa`, `jj/mm/aa`, `jj-mm-aaaa`, `jj.mm.aaaa` ou `aaaa-mm-jj` |
| Montant | Soit **une colonne signée**, soit **deux colonnes** débit et crédit. Une colonne débit est toujours lue comme une dépense, saisie en positif ou en négatif |
| Séparateur des décimales | Virgule (`1 234,56`) ou point (`1,234.56`). L'autre séparateur, les espaces, le signe euro et les lettres sont ignorés ; le signe moins typographique est accepté |
| Inverser le signe | Pour une banque qui note les dépenses en positif. Sans effet avec deux colonnes débit et crédit |

Proposé pour une nouvelle banque : UTF-8, `;`, en-tête en ligne 1, « Date », « Libellé », « Montant », `jj/mm/aaaa`, virgule.

## Le format PDF (facultatif)

Pour une banque qui ne propose pas de CSV, on décrit aussi comment lire son **relevé PDF**. Le PDF doit contenir du **texte sélectionnable** : une image scannée est refusée (pas d'OCR, qui se trompe sur les chiffres).

Un PDF n'a pas de colonnes : le texte est lu avec sa **position** (`PdfReader`, adaptateur `pdf-extract`), puis `service::bank_pdf` retrouve le tableau.

| Réglage | Sens |
|---|---|
| En-têtes des débits et des crédits | Le texte imprimé au-dessus des deux colonnes (« Débit », « Crédit »). La ligne qui porte les deux ouvre le tableau, sur chaque page. Un montant est une dépense s'il est du côté de la colonne des débits, un revenu côté crédits |
| Dates des opérations | Jour et mois séparés par un point (`15.10`) ou une barre (`15/10`) ; une ligne d'opération commence par cette date. La date de valeur, en fin de ligne, n'est pas gardée |
| Lignes de solde | Elles commencent par ce texte (« SOLDE » : « SOLDE CREDITEUR AU 15.10.2026 »). La première est le solde de départ, la dernière le solde final. Un solde dans la colonne des débits est négatif |
| Fin du tableau | La ligne qui commence par ce texte (« TOTAL DES OPERATIONS ») termine les opérations ; les soldes qui suivent sont encore lus |

- **L'année** vient de la période du relevé (« du 15 octobre 2026 au 15 novembre 2026 » ou « du 15/10/2026 au 15/11/2026 ») : l'année la plus récente qui ne dépasse pas la fin de la période (un relevé à cheval sur deux ans est géré).
- Le libellé garde ses lignes de suite (tant qu'elles suivent à l'écart normal entre deux lignes) ; les pieds de page sont ignorés.
- **Contrôle d'intégrité** : solde de départ + opérations lues = solde final. Sinon le relevé est refusé **en entier** (« ne tombe pas juste »), avec les chiffres : une opération mal lue ou manquée ne passe jamais.
- Le solde final du relevé devient une valorisation du compte (le solde saisi à l'import l'emporte).
- Les montants sont lus avec la virgule décimale.
- Limite connue : une banque dont la mise en page change (ou dont les montants ne sont pas dans deux colonnes) demande d'ajuster ces réglages ; un relevé qui ne correspond pas est refusé, jamais deviné. Un PDF malformé peut, au pire, arrêter le processus (profil `panic = "abort"`) : le conteneur redémarre.

## Comptes existants

À la mise à jour, chaque banque déjà saisie en texte libre sur un compte devient une banque, avec le format proposé ci-dessus : à ajuster avant le premier import. Un compte sans banque ne peut pas importer (le message dit de la choisir).

## Refus

Un fichier CSV qui ne correspond pas au format est refusé **en entier**, avec la raison : la colonne introuvable (avec la ligne et les colonnes lues, pour repérer un mauvais séparateur ou une mauvaise ligne d'en-tête), la date ou le montant illisible (avec la ligne).
