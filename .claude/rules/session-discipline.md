# Session Discipline — rules per session

## Avant de commencer (5 min obligatoire)

1. Lire MEMORY.md Quick State (HEAD, phase active, count crates)
2. Lire le handoff actif (IMPLEMENTATION_PLAN_PHASE1.md ou équivalent)
3. Grep-verify les 3 chiffres clés :
   ```bash
   git log --oneline -1                    # HEAD correct ?
   ls tools/*/Cargo.toml 2>/dev/null | wc -l  # crates admises ?
   cargo clippy --workspace 2>&1 | tail -3  # CI green ?
   ```
4. Si divergence avec docs → STOP, escalader, fixer avant de coder

## Pendant la session

- **1 crate à la fois** — pas de parallel admission
- **12 gates séquentielles** — spec → TDD → impl → clippy → mutation → ...
- **Stop après chaque crate admise** — rapporter, attendre validation user
- **Expliquer le Rust** quand le user demande — c'est une feature, pas du bruit
- **Pas de nouveau fichier mémoire** sauf si session > 3h ET décision locked

## Fin de session (5 min)

1. Update STATE.md si crate admise (HEAD, count, LOC)
2. Si Phase milestone → tag git `v0.90.0-alpha.N`
3. Si documents canoniques touchés → vérifier cohérence MEMORY.md
4. 1 paragraphe résumé inline (PAS de fichier completion memo)

## Anti-patinage

Signaux d'alerte que la session patine :
- Tu écris plus de texte que de code
- Tu spawns > 3 sub-agents dans 1 session
- Tu proposes un "nouveau plan" alors que le plan est locked
- Tu écris un "completion memo" de > 200 mots
- Tu changes d'avis sur une décision locked (POST_AUDIT authority)

→ Si tu détectes ça : STOP, reviens aux 12 gates de la crate en cours.

## Max files modifiés par session

- Max 3 fichiers mémoire touchés par session (éviter drift)
- Max 1 fichier canonique modifié (POST_AUDIT, PRE_LAUNCH, DIAMOND)
- Max 0 fichier canonique CRÉÉ (les canoniques existent déjà, update seulement)

## Notifications au user

Rapporter au user quand :
- Crate admise (12/12 gates, commit SHA, LOC, mutation score)
- Gate échouée (laquelle, pourquoi, fix proposé)
- Découverte inattendue (shadow zone, dep circulaire, LOC > budget)
- Doute sur une décision (escalader, pas décider solo)
