#!/usr/bin/env bash
# Cygnus — garde-fou des règles du thème.
#
# Échoue si, hors `packages/ui-kit/src/theme/` et hors `#[cfg(test)]`,
# on trouve :
#   - `CygnusTheme::dark()` (les widgets doivent lire `ui.cygnus_theme()`
#     ou `ctx.cygnus_theme()`, repli sombre automatique) ;
#   - `Color32::from_rgb` / `Color32::from_gray` (les couleurs naissent
#     uniquement dans `theme/colors.rs`).
#
# Sont ignorés : les lignes de commentaires (`//`, `//!`, `///`) et
# tout le contenu des modules `mod tests` (toujours en fin de fichier
# dans ce workspace).
#
# Exceptions légitimes (autorisées, non détectées comme faute) :
#   - `Color32::WHITE` : teinte neutre d'une texture (aucune teinte) ;
#   - `Color32::TRANSPARENT` : absence de peinture (fonds, survols) ;
#   - les montages de test (`#[cfg(test)]`, ex. pixels d'une `ColorImage`
#     de test).
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FAIL=0

# Imprime les occurrences fautives ("fichier:ligne:contenu").
# $1 = motif grep -E.
flag_hors_theme_et_tests() {
    local motif="$1"
    grep -rn -E "$motif" "$ROOT/packages" "$ROOT/apps" --include='*.rs' \
        | grep -v 'packages/ui-kit/src/theme/' \
        | while IFS= read -r hit; do
            local file="${hit%%:*}"
            local rest="${hit#*:}"
            local line="${rest%%:*}"
            local content="${rest#*:}"
            # Ligne de commentaire : ignorée.
            case "$content" in
                *"//"*)
                    trimmed="$(printf '%s' "$content" | sed 's/^[[:space:]]*//')"
                    case "$trimmed" in
                        "//"*) continue ;;
                    esac
                    ;;
            esac
            # Dans un `mod tests` ? (les modules de test sont en fin
            # de fichier : tout ce qui suit `mod tests` est du test).
            if awk -v target="$line" '
                NR < target && $0 ~ /mod tests/ { seen = 1 }
                NR == target { exit (seen ? 1 : 0) }
            ' "$file"; then
                printf '%s\n' "$hit"
            fi
        done
}

echo "== CygnusTheme::dark() hors theme/ et hors tests =="
DARK=$(flag_hors_theme_et_tests 'CygnusTheme::dark\(\)')
if [ -n "$DARK" ]; then
    printf '%s\n' "$DARK"
    FAIL=1
else
    echo "OK : aucun."
fi

echo "== Color32::from_rgb / from_gray hors theme/ et hors tests =="
COLORS=$(flag_hors_theme_et_tests 'Color32::from_(rgb|gray)')
if [ -n "$COLORS" ]; then
    printf '%s\n' "$COLORS"
    FAIL=1
else
    echo "OK : aucun."
fi

if [ "$FAIL" -ne 0 ]; then
    echo "ERREUR : règles du thème violées (voir ci-dessus)." >&2
    exit 1
fi
echo "Règles du thème : vert."
