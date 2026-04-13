# Evolution Rules — comment notre DX évolue avec le projet

## Quand mettre à jour QUOI

```
ÉVÉNEMENT                         →  MISE À JOUR

Crate admise au workspace         →  DIAMOND.md table (status → DONE)
                                  →  STATE.md live numbers
                                  →  MEMORY.md Quick State crate count

Gate trouvée verte (shadow zone)  →  PRE_LAUNCH_GATES.md (cocher la gate)
                                  →  STATE.md shadow zones section

Décision architecturale prise     →  POST_AUDIT_REVISIONS.md (si change plan)
                                  →  BRAINSTORM_PHASE1_DECISIONS.md (si Phase 1)
                                  →  .claude/rules/nika-invariants.md (si nouveau invariant)

Bug / hallucination détecté       →  PRE_LAUNCH_GATES.md (si nouvelle shadow zone)
                                  →  STATE.md "Phase 1 Audit findings"

Fin de semaine (vendredi)         →  STATE.md live numbers (grep-verified)
                                  →  MEMORY.md Quick State (HEAD + count)

Fin de phase                      →  DIAMOND.md current state section
                                  →  MEMORY.md Quick State
                                  →  Archive des handoffs phase terminée
```

## Quand ARCHIVER

Un fichier mémoire va dans `archive/` quand :
- Son handoff est SUPERSEDED par un plus récent
- Il mentionne un session number > 2 sessions old
- Il n'a pas été lu depuis 3 sessions (grep access logs)
- Il contredit POST_AUDIT_REVISIONS.md (autorité suprême)

## Quand NE PAS créer de nouveau fichier

STOP avant de créer un .md. Demande-toi :
- Est-ce qu'un fichier existant couvre ce sujet ? → UPDATE, pas create
- Est-ce que c'est éphémère (1 session) ? → NE PAS créer, répondre inline
- Est-ce que ça va être lu dans 2 semaines ? → Si non, ne pas créer

## Limite de fichiers mémoire

Max 25 fichiers actifs dans memory/ (hors archive/).
Si > 25 → archiver les plus anciens / moins lus.
MEMORY.md reste sous 200 lignes TOUJOURS (limite Claude Code).

## Rule of freshness

Chaque fichier canonique porte un `lastUpdated` dans son frontmatter.
Si `lastUpdated` > 2 semaines → re-vérifier grep avant de citer.
Si `lastUpdated` > 1 mois → candidat archivage ou update.

## Comment on CRAFT, pas "extract"

Ce projet n'est PAS une extraction de code.
C'est de l'artisanat : chaque crate est RÉÉCRITE proprement.

- JAMAIS copy-paste du legacy main
- TOUJOURS comprendre d'abord, réécrire ensuite
- TOUJOURS tests d'abord (TDD), code ensuite
- L'user apprend Rust en parallèle = expliquer le code
- Prendre le temps. 11-12 mois. Pas de rush.
