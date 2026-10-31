# 0014 — Banc d'essai de fluidité et GPU minimal (J2)

- **Date** : 2026-10-30
- **Statut** : acceptée avec conditions le 2026-10-30 (phase A du jalon J2) ; conditions intégrées ci-dessous
- **Exigences concernées** : spec section 5 (fluidité, mémoire, plateformes), section 11.1 (GPU minimal)

## Contexte

La spec propose 60 images/s pour le pinceau, le zoom et le panoramique sur une photo de 24 Mpx en 16 bits avec une dizaine de calques. La cible doit être confirmée ou corrigée par une mesure sur du matériel réel avant la fin de J2. Le GPU minimal reste à définir.

## Proposition : scénario du banc d'essai

- **Document** : 6000 × 4000 px (24 Mpx), 16 bits, sRGB, **10 calques pixel** entièrement couverts (bruit et dégradés générés par commandes `pixels.write`, graine fixe), en mode Normal avec des opacités de 30 à 100 %.
- **Vue** : 2560 × 1440 pixels physiques.
- **Parcours de caméra scripté**, identique à chaque exécution :
  1. « Adapter à l'écran » (≈ 36 %), cache froid, puis 120 images immobiles (cache chaud) ;
  2. panoramique à 100 % sur toute la largeur, en 240 images ;
  3. zoom continu de 5 % à 800 %, centré, en 240 images ;
  4. panoramique en diagonale à 300 %, en 120 images ;
  5. modification de l'opacité du calque 5 (commande `layer.update`), puis temps jusqu'à la vue entièrement à jour.
- **Mesures relevées** pour chaque phase :
  - temps d'image (médiane, p95, p99, maximum) côté processeur ;
  - temps GPU par requêtes d'horodatage (*timestamp queries*) quand l'adaptateur les fournit, sinon temps mur avec attente de fin du GPU ;
  - images par seconde dérivées ;
  - tuiles composées par image et taux de succès du cache ;
  - mémoire GPU occupée par les réservoirs ;
  - mémoire processeur du document.
- **Deux façons de le lancer** :
  - **sans fenêtre** : `cargo run --release -p cygnus-photo-render --example fluidity -- --out rapport.json`. Il est reproductible, mais ne mesure ni la présentation à l'écran ni la synchronisation verticale ;
  - **dans l'application** : `CYGNUS_BENCH=fluidity cargo run --release -p cygnus-photo` joue le même parcours dans la vraie fenêtre, avec la synchronisation verticale désactivée, puis écrit le rapport. Ce mode de développement, activé seulement par cette variable, n'ajoute rien à l'interface. **C'est cette mesure qui fait foi** pour la spec.
- **Lecture du résultat** : la cible est atteinte si le **p95** du temps d'image est ≤ 16,7 ms dans les phases 2 à 4 sur la machine de référence. Le rapport, avec la description de la machine (GPU, pilote, processeur, RAM, système, résolution), est consigné dans une décision datée. L'utilisateur confirme ou corrige alors la cible. Rien n'est promis à l'avance.
- **Machine de référence connue** : AMD Ryzen 7 5800X, 32 Gio, NVIDIA RTX 3080 Ti (pilote 615.78.08), NixOS, Wayland (niri). C'est une machine haut de gamme ; elle ne suffit pas à fixer le minimum.

## GPU minimal (décidé, Q5)

**Le GPU minimal est défini par des fonctions et des limites wgpu, vérifiées au démarrage**, et non par une liste de modèles. Une seule fonction de `photo/render` (`check_adapter`) compare l'adaptateur aux exigences. L'application, la ligne de commande et les tests l'utilisent. Un adaptateur insuffisant donne un message clair qui nomme chaque exigence manquante. Les mesures de performance sur machines modestes (ci-dessous) restent informatives : elles éclairent la cible de fluidité, elles ne définissent pas le minimum.

## Proposition initiale : GPU minimal

1. **Exigences fonctionnelles** : un adaptateur est **pris en charge** s'il satisfait `device_descriptor` sans repli. Cela suppose :
   - Vulkan 1.2, Metal (macOS 12 ou plus récent) ou DirectX 12 ;
   - `Rgba16Float` en écriture depuis les shaders de calcul ;
   - limites `max_texture_dimension_2d` ≥ 8192, `max_texture_array_layers` ≥ 256 et `max_storage_buffer_binding_size` ≥ 128 Mio ;
   - un budget de réservoir d'au moins 256 Mio.

   Ces exigences sont vérifiables automatiquement : au démarrage, un adaptateur insuffisant donne un message clair qui nomme la limite manquante, jamais un plantage.
2. **Exigences de performance** : passer le banc d'essai sur **au moins deux machines modestes**, par exemple un GPU intégré Intel Iris Xe ou UHD 620, un AMD Radeon Vega 8 ou 680M, un Apple M1. Le minimum proposé est le GPU le plus faible qui tient **p95 ≤ 33 ms (30 images/s) en panoramique et en zoom** sur le scénario. 60 images/s reste la cible « recommandée ».
3. **Résultat** : une décision datée en fin de J2, avec la table des machines mesurées, qui renseigne la ligne « GPU minimal » de la spec (sections 5 et 11.1).

**Question Q5** : de quelles machines modestes disposez-vous (ou des testeurs) pour l'étape 2 ?

## Risques

- Sans machine modeste disponible, le GPU minimal ne pourra être fixé que sur les exigences fonctionnelles, et la cible de fluidité restera non vérifiée pour le bas de gamme.
