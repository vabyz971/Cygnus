# 0012 — Pipeline de rendu et intégration du canevas (J2)

- **Date** : 2026-10-30
- **Statut** : acceptée avec conditions le 2026-10-30 (phase A du jalon J2) ; conditions intégrées ci-dessous
- **Exigences concernées** : CAL-3 (préparation), AFF-1 (zoom, panoramique) ; spec sections 2.1, 7.3

## Contexte

L'arbre est compilé en passes GPU. Les formules de fusion n'existent qu'en un exemplaire WGSL (`AGENTS.md`, règle 1). Le canevas est dessiné par wgpu directement dans la fenêtre egui. L'aperçu à 100 % et l'export doivent être identiques.

## Proposition : compilation de l'arbre

1. **Programme de composition.** À chaque révision, l'arbre est compilé côté processeur en une liste d'opérations, de bas en haut. Pour J2 :
   - `Layer { slot_table, opacity, blend }` pour chaque calque pixel visible.

   Prévus pour J3 :
   - `PushGroup` et `PopGroup { opacity, blend }` pour les groupes isolés ;
   - `Mask { slot_table }` ;
   - `Adjust { kind, params }`.
2. **Passe de composition.** Un **shader de calcul unique** traite un lot de tuiles. Pour chaque pixel, il exécute la liste d'opérations avec un accumulateur en f32, plus une petite pile pour les groupes de J3 (profondeur maximale fixée et vérifiée à la compilation de la liste). Il écrit la tuile composée en `Rgba16Float`. Une tuile se compose en **une seule passe**, quel que soit le nombre de calques, et l'arrondi en f16 n'arrive qu'à la fin.
3. **Passes annexes** :
   - décodage des tuiles de calque (entier encodé vers f16 linéaire prémultiplié, voir 0011) ;
   - réduction pour la pyramide de niveaux ;
   - export (encodage, quantification, lecture vers le processeur) ;
   - affichage.

## Proposition : organisation du WGSL

```text
photo/render/src/shaders/
  color.wgsl      sRGB <-> linéaire, (dé)prémultiplication, quantification. Seul endroit.
  blend.wgsl      fn blend(mode: u32, dst: vec4f, src: vec4f) -> vec4f. Seul endroit des formules.
  composite.wgsl  interprète la liste d'opérations (utilise blend.wgsl, color.wgsl)
  decode.wgsl     tuile stockée -> f16 linéaire prémultiplié
  downsample.wgsl niveau n -> n+1
  display.wgsl    affichage : échantillonnage, fond, encodage écran
  export.wgsl     encodage et quantification 8/16 bits
```

Les fichiers sont assemblés à la compilation (`include_str!` et concaténation des modules communs) et validés par `naga` dans un test, sans GPU.

**Module unique de fusion.** `blend.wgsl` contient une fonction `blend` avec un `switch` sur le code du mode. En J2, il n'y a que `Normal` : `src + dst * (1 - src.a)` en prémultiplié. **Ajouter un mode (J3) ne touche que `blend.wgsl`**, plus le code numérique déjà réservé dans l'énumération Rust `BlendMode` (0009). La correspondance entre codes Rust et WGSL est générée : les constantes WGSL sont produites depuis l'énumération Rust au moment d'assembler le shader, pour qu'ils ne puissent pas diverger. Aucun calcul de fusion n'existe ailleurs, ni côté processeur, ni dans les tests (voir 0013).

## Proposition : canevas dans egui

- **Même périphérique.** Le canevas utilise le `Device` et la `Queue` d'eframe (`frame.wgpu_render_state()`). Les besoins du rendu (limites, formats) sont exprimés par **une seule fonction** de `photo/render`, `device_descriptor(adapter)`, passée à eframe par `WgpuSetupCreateNew::device_descriptor` et utilisée aussi en mode sans fenêtre. Le rendu tourne ainsi avec les mêmes exigences partout. Le moteur OpenGL qu'egui ajoute par défaut est exclu (`Backends::PRIMARY` : Vulkan, Metal, DirectX 12), car notre composition exige des shaders de calcul.
- **Rappel de dessin** : un `egui_wgpu::CallbackTrait`.
  - `prepare` : téléverse les tuiles salies, lance les passes de décodage, de réduction et de composition des tuiles visibles manquantes, dans nos propres tampons de commandes.
  - `paint` : dessine le rectangle du canevas dans la passe d'egui. Le shader d'affichage calcule pour chaque pixel d'écran la position dans le document, trouve la tuile par la table des pages, fait un **filtrage bilinéaire manuel** (quatre lectures, sans couture entre tuiles), compose sur le damier ou la couleur du fond, puis encode en sRGB.
- **Zoom et panoramique minimaux** :
  - zoom à la molette et au pincement, centré sur le pointeur, entre 1 % et 3200 % ;
  - panoramique au glisser du bouton du milieu ou avec Espace maintenu (PIN-5) ;
  - « Adapter à l'écran » et « 100 % » dans le menu Affichage et dans la barre de zoom.

  L'état de la vue (zoom, position) appartient à la vue, pas au document : ce ne sont pas des commandes.
- **À 100 %**, chaque pixel d'écran tombe sur un texel de niveau 0. Le filtrage se réduit alors à une lecture directe, et la valeur affichée est exactement celle de l'export 8 bits sRGB.

## Plan d'extension du rendu (J3 et au-delà), sans refonte

Le programme de composition (liste d'opérations), l'interpréteur par pixel et le cache par tuile restent les mêmes. Chaque extension ajoute des opérations ou des étapes, sans changer leur forme.

1. **Groupes isolés.** `PushGroup` ouvre un nouvel accumulateur transparent sur la pile de l'interpréteur. `PopGroup { opacity, blend, mask }` le fusionne dans l'accumulateur parent par `blend()`, le même module que les calques. Un groupe en mode « passe-à-travers » ne produit ni `Push` ni `Pop` : ses enfants sont mis à plat dans la liste. La profondeur de pile est une constante unique, vérifiée par le compilateur de la liste. Un document qui la dépasse est composé en étapes (point 3), jamais refusé.
2. **Réglages (opérations ponctuelles).** `Adjust { kind, params, opacity, mask }` transforme l'accumulateur courant, donc tout ce qui est dessous dans le même groupe : `acc = mix(acc, adjust(kind, acc), opacity × masque)`. Chaque réglage n'existe qu'une fois, dans `adjust.wgsl` (même principe que `blend.wgsl`). Les paramètres sont dans un tampon de stockage, et les courbes et niveaux sont des tables 1D. Un réglage n'a pas besoin de voisins : il reste dans la passe unique.
3. **Filtres de voisinage (flou gaussien, netteté) : étapes avec tuiles intermédiaires et marge.**
   - **Coupure en étapes.** Le compilateur coupe la liste en étapes à chaque filtre. L'étape qui précède le filtre est **matérialisée** en tuiles intermédiaires, stockées dans le même réservoir et le même format de calcul. La clé de cache est l'empreinte du sous-programme qui les produit.
   - **Passe de filtre.** Elle lit ces tuiles **avec une marge** égale au rayon du noyau, arrondi au-dessus et réduit selon le niveau de détail. La lecture passe par la table des pages, comme le filtrage manuel de l'affichage : les marges ne sont pas recopiées physiquement. Une tuile voisine absente compte comme transparente, et une tuile voisine non encore calculée est calculée avant. Le flou gaussien est séparable (deux passes, horizontale puis verticale, avec une tuile intermédiaire).
   - **Reprise.** L'étape suivante reprend par une opération `Base { slot_table }`, qui charge l'accumulateur depuis la sortie du filtre. C'est la seule opération nouvelle pour l'interpréteur.
   - **Invalidation.** Une zone salie est **dilatée du rayon** de chaque filtre traversé : une modification salit aussi les tuiles voisines concernées, et seulement elles.
   - **Niveaux de détail.** À l'export et à 100 %, le noyau est exact (niveau 0). En vue réduite, le rayon est divisé par 2^niveau : c'est un aperçu, documenté comme tel.
4. **Masques.** `Mask { slot_table }` module la couverture de l'opération qui suit (calque, `PopGroup`, `Adjust`). Un masque partagé (0009) est un seul jeu de tuiles référencé deux fois.
5. **Objets incorporés transformés (TRA-3).** Une passe de rééchantillonnage produit, par tuile et par niveau, la version transformée de la source, mise en cache par empreinte (source et matrice). Elle entre ensuite dans le programme comme un calque pixel.

Aucune de ces extensions ne crée de seconde copie des formules de fusion, de couleur ou de réglage : `blend.wgsl`, `color.wgsl` et `adjust.wgsl` restent les seuls endroits de ces calculs.

## Alternatives écartées

- **Une passe de rendu par calque avec fusion fixe du GPU** : seule la fusion Normal s'exprime ainsi. Les autres modes exigent de lire la destination, impossible dans wgpu sans copie intermédiaire (*ping-pong*) : une passe par calque, et des arrondis f16 à chaque étape.
- **Composer dans une texture de la taille de la fenêtre** : il faut tout recalculer à chaque panoramique, sans cache par tuile, à l'opposé de la spec 7.3.
- **Dessiner le canevas dans une `egui::Image`** : il faudrait passer par une texture egui, avec ses conversions et sa gestion propres, sans maîtrise de l'encodage couleur.

## Risques

- L'interpréteur par pixel doit rester simple pour les compilateurs de shaders, en particulier sur les pilotes de GPU intégrés. Le banc d'essai (0014) le mesurera tôt, dès l'étape B4.
- Le filtrage manuel coûte quatre lectures et quatre recherches dans la table des pages par pixel d'écran. Le banc d'essai le mesurera. Repli possible : une texture de vue mise en cache, recalculée seulement quand la caméra change.
