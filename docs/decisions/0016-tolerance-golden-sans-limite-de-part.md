# 0016 — Golden images : retrait de la limite de part de pixels

- **Date** : 2026-10-31
- **Statut** : acceptée (validée par l'utilisateur le 2026-10-31)
- **Exigences concernées** : spec section 2.1 (critère d'acceptation) ; modifie 0013

## Contexte

0013 fixait la tolérance initiale entre adaptateurs à **1 pas de f16 par canal** sur la valeur linéaire, et **au plus 0,01 % des pixels** différents, à ne desserrer que sur preuve.

## Preuve (étape B4, 2026-10-31)

Golden image `normal-opacity` (640 × 400, 16 bits), référence produite sur lavapipe (Mesa 26.1.8), rendue sur une NVIDIA RTX 3080 Ti (Vulkan, pilote 615.78.08) :

- **une seule couleur** diffère, celle de l'aplat rouge à 50 % sur fond clair. Référence (58040, 45468, 46938, 65535), RTX (58040, 45468, 46937, 65535) : **1 unité sur 65 535 sur le canal bleu** ;
- cette couleur remplit **37 900 pixels (14,8 %)** de l'image ;
- aucun canal ne dépasse 1 pas de f16 (plus un pas de quantification de l'export) ;
- l'autre cas (`transparency-8bit`) est identique au bit près.

L'écart vient d'un arrondi différent d'une même valeur à la quantification 16 bits de l'export (fonctions `pow` du sRGB évaluées par des pilotes différents). Dans une image de test faite d'aplats, la part de pixels différents mesure la **surface** de l'aplat concerné, pas le nombre d'écarts. Le critère de part n'a donc pas de sens pour ces images.

## Décision

- **Critère de réussite** : aucun canal d'aucun pixel n'est au-delà de **1 pas de f16** de la valeur linéaire, plus **un pas de quantification** de l'export 16 bits.
- La **part de pixels différents est rapportée** dans la sortie des tests, mais n'est plus limitée.
- **Inchangé** : même adaptateur, aperçu contre export, identité bit à bit (écart 0).

## Conséquences

- Les tests passent sur la RTX ; Metal et WARP seront jugés en CI sur le même critère.
- Le critère par canal reste strict : toute erreur réelle de formule (de l'ordre de dizaines ou de milliers d'unités) le dépasse largement.
- Tout desserrement ultérieur du critère par canal exige une nouvelle preuve et une nouvelle décision.
