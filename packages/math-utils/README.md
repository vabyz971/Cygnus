# math-utils

Géométrie : re-exporte [`Transform2D`](src/transform2d.rs)
et [`Vec2`](src/vec2.rs) de `datatypes` pour simplifier les
imports des apps.

Fonctions clés :
- [`Transform2D`] (matrice 3×3, matrice inverse, etc.)
- [`Vec2`] (vecteur 2D)

Aucune logique : ces types sont définis dans `datatypes`
et réexportés ici pour éviter de dupliquer les imports.