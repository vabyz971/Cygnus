# engines/photo-engine/src/paint.rs

- StrokeMode · enum · L29-L34 — pub enum StrokeMode
- BrushParams · struct · L38-L48 — pub struct BrushParams
- paint_stroke_rgba · function · L53-L55 — pub fn paint_stroke_rgba(rgba: &mut [u8], w: u32, h: u32, points: &[(f32, f32)], b: &BrushParams)
- paint_stroke_ellipse · function · L60-L70 — pub fn paint_stroke_ellipse(
- paint_stroke_impl · function · L72-L193 — fn paint_stroke_impl(
- StrokeCommit · struct · L198-L207 — pub struct StrokeCommit
- fmt · function · L210-L216 — fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
- commit_stroke · function · L225-L233 — pub fn commit_stroke(
- commit_stroke_locked · function · L236-L318 — fn commit_stroke_locked(
- tests · module · L321-L511 — mod tests
- trait_opaque_sur_fond_transparent · function · L325-L347 — fn trait_opaque_sur_fond_transparent()
- opacite_50_sur_fond_blanc · function · L350-L369 — fn opacite_50_sur_fond_blanc()
- gomme_opaque_efface_le_centre_preserve_les_bords · function · L372-L393 — fn gomme_opaque_efface_le_centre_preserve_les_bords()
- gomme_50_reduit_alpha_de_moitie · function · L396-L415 — fn gomme_50_reduit_alpha_de_moitie()
- gomme_sur_pixel_deja_transparent_sans_effet_bord · function · L418-L436 — fn gomme_sur_pixel_deja_transparent_sans_effet_bord()
- ellipse_respecte_les_rayons_d_axes · function · L439-L467 — fn ellipse_respecte_les_rayons_d_axes()
- commit_stroke_sur_calque_redimensionne_adapte_le_rayon · function · L470-L510 — fn commit_stroke_sur_calque_redimensionne_adapte_le_rayon()
