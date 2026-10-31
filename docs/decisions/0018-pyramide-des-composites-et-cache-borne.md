# 0018 — Pyramide des tuiles composées et cache borné du canevas

- **Date** : 2026-10-31
- **Statut** : acceptée (écart signalé à l'utilisateur ; modifie 0011)
- **Exigences concernées** : spec section 5 (mémoire, fluidité), section 7.3 ; étape B7 de 0015

## Contexte

0011 prévoyait une pyramide de niveaux de détail **par calque** : chaque calque aurait ses tuiles réduites, et la composition se ferait au niveau voulu. À l'implémentation (B7), deux limites sont apparues :

- un tableau de textures est limité à **256 couches** par les limites WebGPU par défaut, qui définissent le GPU minimal (0014). Une réserve de tuiles en format de calcul tient donc au plus 128 Mio ;
- avec dix calques, une pyramide par calque multiplie par dix les tuiles à garder et à reconstruire, alors que l'affichage n'a besoin que du résultat.

## Décision

1. **Pyramide des tuiles composées.** Le niveau 0 contient les tuiles composées en pleine résolution. Le niveau L est construit à partir des quatre tuiles enfants du niveau L − 1, par une moyenne 2 × 2 en lumière linéaire prémultipliée (`downsample.wgsl`, seul endroit de ce filtre). Pour le mode Normal, qui est linéaire, le résultat est le même qu'une composition de calques réduits, aux arrondis près. Pour les modes non linéaires de J3, c'est plus juste : on réduit le vrai résultat au lieu de fondre des calques réduits.
2. **Cache borné.** Le canevas garde au plus 256 tuiles (128 Mio), tous niveaux confondus. Les tuiles visibles du niveau affiché, avec une tuile de marge, ne sont jamais évincées. Les autres partent de la moins récemment utilisée à la plus récente. Une tuile est reconstruite à la demande, en profondeur d'abord : construire une tuile grossière ne retient qu'une chaîne d'enfants par niveau.
3. **Choix du niveau.** Le niveau affiché est celui dont un pixel couvre entre un et deux pixels d'écran. À 100 % et au-delà : niveau 0 et lecture au plus proche, ce qui garde l'identité au bit près avec l'export. En dessous de 100 % : filtrage bilinéaire dans le niveau.
4. **Budget insuffisant.** Si les tuiles visibles ne tiennent pas dans le cache, le niveau supérieur, plus grossier, est affiché. Il n'y a jamais d'échec ni de plantage, sauf si même la tuile unique du dernier niveau ne tient pas.
5. **Mémoire GPU insuffisante.** La réserve est allouée dans une portée d'erreur `OutOfMemory`. En cas d'échec, sa taille est divisée par deux, jusqu'à 16 tuiles. L'export n'utilise pas ce cache : il procède par lots (0011) et n'a jamais besoin de tout le document.
6. **Invalidation.** Une tuile de niveau 0 modifiée invalide la tuile qui la contient à chaque niveau. Les changements de propriétés ou de structure invalident tout.

## Conséquences

- Les documents ne sont plus limités à 256 tuiles : un document de 24 Mpx s'affiche (testé), le niveau 0 n'étant calculé que là où il est visible ou nécessaire à un niveau réduit.
- Une première vue réduite d'un très grand document compose une fois tout le niveau 0, par morceaux. C'est mesuré par le banc d'essai (B8).
- **Non fait en B7** : l'affichage dans la barre d'état d'un message signalant un repli (niveau plus grossier, cache réduit). Les deux cas sont rares avec 256 tuiles : une vue de 2560 × 1440 à 100 % en demande environ 100. Le message passera par une notification de l'interface, à prévoir avec la barre d'état réelle.
- Un cache plus grand que 256 tuiles exigerait plusieurs tableaux de textures ou une limite plus haute que celles du GPU minimal : ce serait une nouvelle décision.
