// Cygnus — Suite créative professionnelle open source
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Révisions de la scène : le compteur partagé [`Revision`](ids::Revision).
//!
//! Stratégie (volontairement simple, aucun cache ici) :
//!
//! - [`Scene`](crate::Scene) possède un compteur monotone (`tree_revision`).
//! - Chaque mutation structurelle ou d'attribut alloue UNE révision et
//!   l'applique au nœud touché **et à tous ses ancêtres** : la révision
//!   d'un parent reflète donc toujours l'état de sa sous-arborescence.
//! - Les frères et les autres branches gardent leur révision : un
//!   observateur détecte un changement de sous-arbre par simple
//!   comparaison d'entiers, sans parcourir.
//! - Les écritures sans effet (même valeur, rattachement identique,
//!   suppression d'un id inconnu) n'allouent RIEN et se distinguent des
//!   mutations par leur valeur de retour (`None` vs `Some(revision)`).
//!
//! Un futur cache de rendu utilisera ces révisions comme clés
//! d'invalidation — il n'est pas construit ici.

pub use ids::Revision;
