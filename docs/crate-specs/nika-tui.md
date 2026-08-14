# Crate spec — `nika-tui`

| | |
|---|---|
| Status | **CANDIDATE** — Gate 1 (this document) authored 2026-08-12 · D-2026-08-11-N6 (T27 APRÈS T28 · le renderer est le premier consommateur natif de `nika-tui-core`) |
| Layer | L4 — interfaces (la surface terminal native) |
| Design | Le renderer ratatui de la session · un `Buffer` de cellules, les effets APRÈS écriture (tachyonfx), le widget cascade, la boucle `crossterm`. Toute la loi vient de `nika-tui-core` (session · dérivation · seating · claims) · ce crate ne calcule rien, il PEINT. |
| LOC budget | ≤6,000 src prod · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` |
| Dependencies | `ratatui` · `crossterm` · `tachyonfx` · `nika-tui-core` (la loi) · `serde`/`serde_json` (les types de l'ingress) — dev: `proptest` |
| NIKA codes | none owed — le renderer ne refuse pas · il affiche le refus que le moteur a rendu |
| Depends on | **T28 admis** (nika-tui-core hors wip) · le crate ne se build pas sur une loi en mouvement |

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

## 4. Ordre d'implémentation (la carte, §7)

1. le contrat généré inclus (`enum Key` matché exhaustivement) · le
   squelette compile avant de dessiner
2. le buffer + le châssis en arbre (`permits · spend · proof · live`)
3. le fil et ses blocs (`say` · `draft` · `gate`)
4. la cascade en `Widget` (le repli du fan-out et sa traîne)
5. tachyonfx · deux effets, aux deux moments déclarés
6. le reste du contrat

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
