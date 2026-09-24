# Crate spec — `nika-tui`

| | |
|---|---|
| Status | **WIP · in the workspace since 2026-09-21** (the `nika-tui-core` precedent) · Gate 1 (this document) authored 2026-08-12, amended 2026-09-21 by ADR-139 (the renderer architecture: inline-first, one owner of the terminal) · D-2026-08-11-N6 (T27 APRÈS T28 · le renderer est le premier consommateur natif de `nika-tui-core`) |
| Layer | L4 — interfaces (la surface terminal native) |
| Design | Le renderer Ratatui de la session (ADR-139) · UN propriétaire du terminal (raw mode · bracketed paste · focus · protocole clavier sondé · écran alternatif, activés dans un ordre fixe et restaurés en sens inverse depuis un seul endroit, le hook de panique restaure AVANT le message) · présentation INLINE d'abord (`Viewport::Inline` + `insert_before` avec régions de défilement : les blocs finis vivent dans le scrollback du terminal) · présentation FOCUS sur demande (écran alternatif, transcript défilable, brouillon conservé) · UN courtier d'événements, mis en pause autour de chaque requête de position du curseur · un composeur (`ratatui-textarea` derrière un wrapper : Entrée envoie, Alt+Entrée saute une ligne, un collage est une donnée, l'historique aux bords du tampon). Toute la loi vient de `nika-tui-core` · ce crate ne calcule rien, il PEINT et il ÉCOUTE. |
| LOC budget | ≤6,000 src prod · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` |
| Dependencies | **mesurées sur `Cargo.toml`, pas déclarées ici** · `ratatui` 0.30 (feature `scrolling-regions`) · `crossterm` 0.29 (`event-stream` · `bracketed-paste`) · `ratatui-textarea` 0.9 · `tokio` · `futures-util` · `unicode-width` · dev : `expectrl` (la preuve PTY). `tachyonfx` et `nika-tui-core` arrivent avec les vagues UX-2+ (ADR-139 §Consequences). |
| NIKA codes | none owed — le renderer ne refuse pas · il affiche le refus que le moteur a rendu |
| Depends on | **T28 admis** (nika-tui-core hors wip, fait 2026-08-14) · ADR-139 proposée (confirmée ou renversée par les deux prototypes de la vague UX-1 sur les mêmes fixtures) |

---

## 1. Purpose

Le terminal natif est la surface qui ne peut pas mentir : une grille de
cellules, un caractère et un style par cellule, rien d'autre. Le studio web
a été écrit pour se porter (même modèle de buffer, effets après écriture) ·
ce crate est le portage — et la carte existe (`PORTING.md`, la SSOT du
studio).

Ce qui le distingue d'un rewrite · **il n'invente aucune loi**. Le modèle
de session, les dérivations (vagues · goulot · totaux), la loi des cases du
board et les claims exécutables viennent de `nika-tui-core`, compilé natif.
Ce crate contient exactement ce que le navigateur ne peut pas fournir : la
boucle d'événements (`crossterm::event::read`), la géométrie du terminal
(les colonnes réelles — aucune des quatre erreurs de mesure du studio
n'est portable), les widgets ratatui, et les deux effets tachyonfx.

## 2. La couche sémantique devient exécutoire ICI

Le trou connu de la carte de portage (§4) se ferme dans ce crate ·

```rust
pub enum Role { BarWork, BarIdle, BarCritical, /* … */ }
impl Role { pub fn color(self, theme: &Theme) -> Color { /* la table */ } }
```

Un `Role` résolu à la peinture rend la couche sémantique exécutoire ·
citer une primitive dans un widget devient une erreur de type. Le gate
d'étendue de palette du studio mesure alors ce qu'il prétend mesurer.

## 3. Ce qui est porté tel quel (la carte, §5)

- `sweepOver` n'est PAS `fx::sweep_in` · l'opacité à zéro devant le front
  est juste pour une chose qui arrive, fausse pour une chose qu'on
  regarde · la variante ne déplace qu'une tête vive.
- Et elle parcourt l'ENCRE, pas les colonnes (une tête qui avance en `x`
  tombe dans le blanc à mi-course · mesuré · le geste clignote).
- Les 9 goldens du studio sont la preuve de RENDU · le crate les
  reproduit au caractère près (le harnais goldens descend ici).

## 4. Ordre d'implémentation (ADR-139 · les vagues du produit)

L'ordre de la carte de portage (contrat généré · buffer · fil · cascade ·
tachyonfx) est remplacé par les vagues du produit, chacune finie quand le
scénario complet est qualifié sur le VRAI binaire, jamais quand le code
existe :

1. **UX-1 · la preuve du renderer** (2026-09-21) · la coque Ratatui, les
   deux présentations (inline · focus) sur la même fixture (`Script::demo`),
   le spike du composeur, le cycle de vie du terminal prouvé depuis un PTY
   (`tests/pty_restore.rs` : fermeture normale · deux Ctrl+C · panique dans
   la boucle · SIGTERM · un collage `yes`/`/quit` inerte à travers un
   basculement focus · un tube refusé avec le code 2 et zéro séquence).
2. **UX-2 · les cinq premières secondes** · le vrai `SessionRuntime` branché
   par les mêmes beats typés, derrière un interrupteur explicite tant que
   les goldens PTY de la CLI ne sont pas recoupés · le premier écran, l'aide
   locale, la latence.
3. **UX-3 · cognition contextuelle et récupération** · le picker
   d'intelligence, la récupération typée.
4. **UX-4 · l'objet workflow vivant** · clarification typée, review dans
   l'ordre mandaté, inspecteur, sauvegarde exacte, Check.
5. **UX-5 · exécution** · run, porte, reprise, résultat, preuve.
6. **UX-6 · durcissement** · la matrice de terminaux, les tailles, tmux,
   SSH, `TERM=dumb`, le monochrome.
7. **UX-7 · qualification humaine** · les goldens A à O, le dogfood, l'étude
   modérée.

## 5. Determinism contract

- Même état de session (venant de `nika-tui-core`) ⇒ même buffer · la
  peinture est pure, l'horloge n'entre que par les effets (tachyonfx
  porte le temps, les widgets ne le lisent jamais).
- Aucune I/O hors la boucle d'événements et le terminal · les lectures
  moteur arrivent par `nika-tui-core`.

## 6. Related

- `docs/crate-specs/nika-tui-core.md` · la loi (T28 · wip `c5c8f96cc`)
- la carte de portage du studio (sa SSOT · la table de correspondance ·
  les deux divergences assumées avec tachyonfx · l'ordre)
- les 9 goldens du studio · la preuve de rendu à reproduire
- D-2026-08-11-N6 · l'arbitrage d'ordre

## Fresh local Run cost decision

`session::Live::with_run_review` accepts the existing CLI host's typed child
runner through an acyclic L4 dependency (`nika-tui` → `nika-cli-host`, never the
reverse). A pending Run question is separate from Session authoring and Save
consent, survives only while that child is alive, and is never persisted.
The broker discards input queued before the question is painted. A new `yes`
answers only this question; `no`, cancellation, revision and leaving drop the
child. Ctrl+C invalidates a pending decision immediately. Native catalog
currency evidence and unknown USD remain distinct; the renderer invents no
price, policy exception, endpoint, grant or reusable admission authority.

The Session's one-time unknown-cost choice (`SessionRuntime::waiting_cost_choice`)
is the second fresh spending question and keeps the same broker contract:
typeahead from before it was painted is discarded, Ctrl+C cancels it through the
Session's own answer path (nothing sent), and `details` reads
`cost_choice_details` without a turn. Its first screen is headed as an authoring
decision that never approves a Save or a Run; the Run question approves one Run
that no authoring or Save approval does. Both first screens close on
`yes / no / details`, and the hint row names the same choices in words.
