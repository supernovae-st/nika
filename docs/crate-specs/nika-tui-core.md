# Crate spec — `nika-tui-core`

| | |
|---|---|
| Status | **CANDIDATE** — Gate 1 (this document) authored 2026-08-12 · tranché D-2026-08-11-N6 (T28 d'abord · T27, le renderer ratatui, en est le premier consommateur natif) |
| Layer | L4 — interfaces (libraries only) · le précédent WASM : `nika-check-wasm` (WIP · ADR-107) |
| Design | La LOI de la session TUI en UN crate Rust — le modèle de session (Session · l'enum fermé Block), la couche de dérivation (vagues · idle · goulot · totaux), le seating du board de plan, les CLAIMS en assertions exécutables. Compilé natif (T27) ET en WASM (le studio web) · le DESSIN reste en SSOT YAML projetée (un essai de design ne demande jamais un `wasm-pack build`). |
| LOC budget | ≤4,000 src prod · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — interface crate, jamais sur crates.io (le WASM part vers npm `@supernovae-st`, le précédent check-wasm) |
| Dependencies | `serde` · `serde_json` · `thiserror` · `nika-event` (les kinds du journal) · `nika-graph` (RevGraph, la projection canonique) — dev: `proptest` · WASM wrapper : `wasm-bindgen` (cdylib séparé si la loi le demande, sinon la double crate-type du précédent) |
| NIKA codes | **none owed** — la couche ne refuse pas au sens moteur · ses invariants sont des `debug_assert!` issus des CLAIMS de la SSOT (la carte de portage §3) |

---

## 1. Purpose

L'interface aura TROIS surfaces (studio web · ratatui natif · cloud). Sans ce
crate, la même loi est écrite trois fois — et la doctrine dit que deux
implémentations divergent TOUJOURS (payé deux fois le 10 août · les types
`Graph`/`RevGraph`, le nom du bloc `gate`/`choix`). `nika-tui-core` est la
loi écrite UNE fois, en Rust, à la version du moteur — les surfaces
n'écrivent plus que de la peinture.

Le périmètre exact de « loi » est lu, pas deviné · il est prouvé aujourd'hui
par le studio de design (le simulateur TS dont chaque primitive a été écrite
pour se porter — cellules, effets après écriture, enum fermé des blocs) et
se porte :

- **le modèle de session** · `Session` + l'enum fermé `Block` (six genres)
  + les transitions que le fil connaît · y compris le seating du board de
  plan (assis sur le graphe que le MOTEUR projette — jamais un modèle
  maison du DAG).
- **la dérivation** · vagues · fin de vague · idle · goulot · coût total ·
  temps total · verbes utilisés · coût par verbe · des fonctions pures sur
  (Workflow, Run), chacune déjà prouvée par les gates du studio.
- **les CLAIMS** · les phrases que la surface a le droit d'écrire, avec
  leur condition — portées en `debug_assert!` (la SSOT les a écrites après
  avoir attrapé quatre revendications fausses en une soirée · elles sont
  la partie qui ne pardonne pas).
- **l'ingress moteur** · `RevGraph` (de `nika-graph` · ce que
  `nika inspect --format json` rend) et les événements du journal (de
  `nika-event`) → les types de la session. L'adaptation vit ICI, une fois.

It does **not** own: la peinture (cellules · effets · les `Span` — T27 et
la projection TS) · la couche sémantique des couleurs (`Role` — le trou
connu de la carte de portage se ferme au portage ratatui, pas ici) · la
boucle d'événements (`crossterm`, T27) · le check lui-même
(`nika-check-wasm` le sert déjà au navigateur) · aucune I/O non injectée
(les lectures arrivent en paramètres).

## 2. Determinism contract

- Même (Workflow, Run, état de session) ⇒ mêmes dérivations, sur tout hôte
  et en WASM · pas d'horloge, pas d'aléa, pas de `HashMap` dans les
  signatures publiques (la marée montante est itérée dans l'ordre du DAG).
- Les derivations ne RE-mesurent jamais · elles lisent le journal du run
  (les durées sont les durées enregistrées, ou l'absence dite).
- `#[non_exhaustive]` sur les enums publics dès le premier jour.

## 3. WASM · la forme (précédent ADR-107)

La bibliothèque reste une bibliothèque (`rlib`, pure, testée native). La
surface navigateur est soit la même crate en `crate-type = ["cdylib",
"rlib"]` (le précédent des deux crates browser-wasm maison), soit un
wrapper fin — tranché à l'IMPL, jamais avant d'avoir mesuré la taille du
lot. `wasm-pack --target web` · `publish = false` · le studio consomme le
lot npm local, jamais un artefact reconstruit à la main.

## 4. La preuve de parité

Le corpus de frames de référence du studio (ses goldens) prouve le RENDU —
il reste la preuve de T27. La preuve de T28 est la **dérivation** : les
fixtures exportées du studio (un Workflow + un Run + les valeurs dérivées
attendues) rejouées contre le crate, à parité EXACTE (vagues · idle ·
goulot · totaux). Une divergence de dérivation est un bug du crate, jamais
une « différence de plateforme » — c'est pour ça que la loi déménage.

## 5. Ordre d'admission envisagé

1. les types (Session · Block · Workflow · Run · Step) + l'ingress RevGraph
2. la dérivation (les huit fonctions de la couche lue au studio, TDD depuis les fixtures)
3. le seating du board de plan
4. les CLAIMS en `debug_assert!`
5. le lot WASM + le studio consommateur (une lentille pilote)

## 6. Related

- la carte du portage du studio (sa SSOT) · ce que T27 portera
- D-2026-08-11-N6 · l'arbitrage d'ordre (T28 avant T27)
- ADR-107 · le précédent WASM (nika-check-wasm)
