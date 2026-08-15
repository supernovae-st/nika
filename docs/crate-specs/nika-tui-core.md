# Crate spec — `nika-tui-core`

| | |
|---|---|
| Status | **ADMITTED 2026-08-14** — les 12 gates · Gate 1 (this document) authored 2026-08-12 · tranché D-2026-08-11-N6 (T28 d'abord · T27, le renderer ratatui, en est le premier consommateur natif) |
| Layer | L4 — interfaces (libraries only) · le précédent WASM : `nika-check-wasm` (WIP · ADR-107) |
| Design | La LOI de la session TUI en UN crate Rust — le modèle de session (Session · l'enum fermé Block), la couche de dérivation (vagues · idle · goulot · totaux), le seating du board de plan, les CLAIMS en assertions exécutables. Compilé natif (T27) ET en WASM (le studio web) · le DESSIN reste en SSOT YAML projetée (un essai de design ne demande jamais un `wasm-pack build`). |
| LOC budget | ≤4,000 src prod · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — interface crate, jamais sur crates.io (le WASM part vers npm `@supernovae-st`, le précédent check-wasm) |
| Dependencies | **mesurées sur `Cargo.toml`, pas déclarées ici** · `serde` · `serde_json` (feature `float_roundtrip` — la parité compare des BITS) · `thiserror` · `wasm-bindgen` (**optionnel** · feature `wasm` · son impl blanket `Upcast` atterrissait sinon sur chaque type public en AVAL) · dev: `proptest`. **ZÉRO dépendance interne** — ce tableau annonçait `nika-event` et `nika-graph`, la crate n'en a jamais eu aucune : `ingress.rs` porte un miroir typé de `graph_format: 2`, sans lien à la compilation. Corrigé après qu'une lentille Gate-11 ait lu les deux fichiers. |
| NIKA codes | **none owed** — la couche ne refuse pas au sens moteur. ⚠️ Cette ligne disait « ses invariants sont des `debug_assert!` » · aucun ne l'était. Les CLAIMS sont des prédicats, et **deux des quatre** voyagent par la porte wasm (`claims.chain_intact` · `claims.bottleneck`) ; les deux autres ne peuvent pas, et `src/claims.rs` dit lesquels et pourquoi. |

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

---

## 7. Gate status (amendment · 2026-08-12)

| Gate | Verdict | Preuve |
|---|---|---|
| 5 **Mutation** | ✅ **99,5 %** · 209 mutants · 189 pris · 19 non viables · **1 survivant nommé** (2026-08-14, arbre committé, répertoire de build DÉDIÉ) | ⚠️ Cette ligne a porté **90,3 %** pendant deux jours · un chiffre pris **sur le shim**, avec un autre instrument, que la §7bis dessous démonte. Une lentille Gate-11 a refusé de le croire, et elle avait raison. Le chemin mesuré · 87 % ROUGE le matin → 92,7 % (dont **14 survivants tous sur deux fonctions ajoutées une heure plus tôt**) → 99,5 %. La comptabilité SOMME · 189+1+19=209. Survivant · `<` → `<=` sur une tolérance flottante · intuable sans tester la représentation des f64 plutôt que la loi · sa DIRECTION est délibérée (`<` couronne moins). |
| 6 **Property** | 5 propriétés (`77283178e`) | partition des vagues · idle ≥ 0 · durée couvrante · goulot couronné a des attendants · groupage partitionne · cycles inclus |
| 7 **Benchmarks** | ⚠️ **AMENDÉ 2026-08-14 · trois gates d'ÉCHELLE, pas un bench** | La ligne disait « **no hot path, décidé** · la dérivation sur 24 tâches est de l'ordre de la microseconde ». Vrai sur 24 tâches, et **faux comme conclusion** · `MAX_INPUT_BYTES` admet ~1 M de tâches par la porte, et une revue Gate-11 a trouvé **quatre** quadratiques que ce raisonnement avait couverts (le memo de `waves()` par racine · le balayage de `groups_of` · trois formes singulières appelées n fois par la porte). La chaîne de 5000 déjà épinglée par le test de sécurité passait **14,35 s** — verte tout du long, parce qu'elle n'assertait aucune borne. Un bench aurait mesuré la plateforme ; ce qui manquait était un gate de **COMPLEXITÉ**. Trois posés (`waves` · `groups_of` · la porte de bout en bout **avec** des steps) · un RAPPORT et non un chrono, donc indépendant de la machine · ×4 l'entrée doit coûter <×8. Chacun prouvé par mutation en rougissant sur l'ancien code (×18,4 · ×20,1 · ×14,3). La chaîne pinnée est à **0,02 s**. |
| 9 **Canary E2E** | **la forme du crate n'a pas de canary workflow** · l'E2E réel est le harnais Node sur le lot construit (`test.mjs` · fixtures réelles à travers la frontière) + le gate `wasm-parity` du studio | un canary `.nika.yaml` n'invoque pas une bibliothèque de loi |
| 10 **Parity legacy** | n/a · pas de brouillon pour ce crate (loi neuve) · la parité de RÉFÉRENCE est le studio TS (fixtures `gen-parity`, rejouées bit à bit) | — |
| 11 **Review swarm** | ✅ **3 lentilles indépendantes · 2026-08-14** (contrat Nika · craft Rust · justesse et honnêteté des tests) · P0/P1 fermés dans la session | **Deux lentilles ont convergé sur les MÊMES trous sans se voir** — c'est ce qui justifie d'en lancer trois. Trouvé · un **P0** (la loi `ran()` appliquée à 3 coutures sur 4 ⇒ une étape morte étirait sa vague et une étape finie à l'heure était rapportée comme ayant attendu pour elle) · un quadratique que la première réparation avait manqué · 4 marqueurs `non_exhaustive` sans constructeur · deux de MES surengagements de doc. ⚠️ **Une trouvaille était FABRIQUÉE** (une citation `Cargo.toml:53` dans un fichier de 37 lignes) · chaque constat a été vérifié sur la source avant tout commit, celui-là n'a produit aucun changement. |
| 12 **Atomic commit** | ✅ **l'admission 2026-08-14** · `nika-tui-core` sort du tableau `wip` | Le tableau `wip` est le SEUL registre d'admission · le compte se DÉRIVE (`bash scripts/crate-metrics.sh --wip`), il ne se tape pas. |

### ⚠️ Gate 5 · le chiffre de 90,3 % a été pris avec un AUTRE instrument

Mesuré 2026-08-13, dans le workspace : `check-mutation-floor.sh nika-tui-core`
rend **0 pris sur 165 viables**. Ce n'est pas une régression de la suite, c'est
une faute de portée. La porte invoque `cargo mutants -- --lib` (convention
diamant + la règle macOS sans Keychain), or ce crate n'a **aucun**
`#[cfg(test)]` dans `src/` et **1047 lignes de tests dans `tests/`**. Aucun
test ne tourne, donc rien n'est tué, donc le ratio ne mesure rien.

Le 90,3 % de la ligne ci-dessus avait été pris **« sur le shim »**, quand le
crate vivait encore en standalone avec son propre `[workspace]`. Le shim est
parti à `c5c8f96cc`. Le chiffre est honnête pour sa configuration et muet pour
celle-ci.

Deux crates du workspace occupent ce trou (`nika-acp` et celui-ci) ; les 61
autres gardent leurs tests unitaires inline et sont mesurés correctement.
La porte a été corrigée le même jour (`df59b5f8a`) pour **refuser de rendre un
ratio quand elle n'a exécuté aucun test** : elle sort désormais en 3 (outillage)
avec la cause nommée, au lieu de 2 (sous le plancher), parce qu'un rouge qui
accuse le mauvais sujet envoie réparer un crate qui n'est pas cassé.

### ✅ La vraie mesure, prise le 2026-08-13 · **87 %**, sous le plancher

`cargo mutants -p nika-tui-core` sans la restriction `--lib`, 55 minutes ·
**181 mutants · 144 pris · 21 survivants · 16 non viables** ⇒ **144/165
viables = 87 %**, sous le plancher de 90. Gate 5 est **ROUGE**, pour de vrai
cette fois, et le 90,3 % du tableau ci-dessus ne tient pas dans le workspace.

**Les survivants se groupent, et le groupe est parlant** · cinq d'entre eux
sont sur `derive.rs::bottleneck`, tous sur ses opérateurs de comparaison
(`<` vers `<=` · `>` vers `>=` · `>` vers `==` · `>` vers `<`). Trois autres
sont sur `Waves::contains` et `Waves::is_empty`, rendus constants sans qu'un
test le voie.

Le goulot est l'une des huit fonctions de dérivation que ce crate existe pour
posséder UNE fois plutôt que trois. La propriété de Gate 6 (« goulot couronné
a des attendants ») prouve qu'il en a ; elle ne pinne pas la DIRECTION de la
comparaison qui le couronne. C'est exactement le trou qu'une loi partagée ne
peut pas se permettre · une surface qui la lit hériterait du mauvais goulot
sans jamais diverger visiblement.

**Ce que Gate 5 exigeait ici** · des tests qui tuent ces 21 survivants, en
priorité les bornes de `bottleneck`. Fait le 2026-08-13, six grappes,
**chacune prouvée par mutation isolée** (le mutant posé, le test rougit, le
fichier restauré byte-identique) ·

| grappe | ce qu'elle protège |
|---|---|
| les 5 échappements d'embarquement | la moitié SÉCURITÉ · le test qui les couvrait était satisfait en n'échappant que `<`, les 4 autres bras roulaient gratis |
| la couronne de `bottleneck` (×3) | QUEL goulot est couronné quand deux vagues en ont un · une surface lirait le mauvais et ne divergerait jamais visiblement |
| le seuil à 0,05 | strict · une fixture naïve donne 0.050000000000000044 et ne discrimine rien |
| `Waves::contains` / `is_empty` | zéro appelant en `src` · la loi du consommateur-zéro à l'échelle d'une méthode |
| le repli de code · le bras `task_skipped` | la fixture enregistrée ne les exerce JAMAIS · les tests écrivent leur journal à la main |
| le plafond 64 MiB · le plus lent d'une égalité | l'arithmétique du plafond, et la stabilité d'un rapport à durées égales |

⚠️ ~~**Un non-tué DÉLIBÉRÉ**~~ · le mutant `>` → `>=` sur la borne
d'admission était déclaré ici comme trop coûteux à tuer (« fabriquer un test
qui alloue 64 Mo coûte plus que ce qu'il prouve »). **TUÉ le 2026-08-13** ·
allouer deux fois 64 Mo coûte 60 ms, et la borne EXACTE valait d'être une
décision plutôt qu'un accident. La déclaration est conservée barrée · une
exemption qu'on lève doit se voir, sinon la prochaine se justifiera par
celle-ci.

### ✅ **La mesure qui compte · 99,5 %**, Gate 5 VERT · 2026-08-13 soir

`cargo mutants -p nika-tui-core` sur l'arbre committé, répertoire de build
DÉDIÉ (partager celui d'une campagne empoisonne la suivante · payé quatre
tours ce soir) · **209 mutants · 191 pris · 17 non viables · 1 survivant**
⇒ **191/192 viables = 99,5 %**, très au-dessus du plancher de 90.
La comptabilité SOMME · 191 + 1 + 17 = 209.

Le chemin, parce qu'il enseigne · 87 % (ROUGE) le matin → les six grappes
ci-dessus → **92,7 %** en début de soirée, dont **les 14 survivants étaient
TOUS les deux fonctions par lot ajoutées une heure plus tôt** (`wave_ends` ·
`idles`) · elles pouvaient rendre `vec![]` sans qu'un test rougisse, parce
que les fixtures de parité épinglent les formes SINGULIÈRES et que le gate
d'échelle de la porte ne juge que le temps. La propriété d'équivalence
(le lot répond exactement ce que n questions singulières répondent, au bit
près) les a tuées toutes les quatorze.

⚠️ **Le seul survivant, nommé plutôt que maquillé** · `< 1e-6` → `<= 1e-6`
dans le couronnement de `bottleneck` (`derive.rs`) · une borne STRICTE sur
une tolérance flottante. La tuer exigerait un écart valant *exactement*
`1e-6` en f64 · on testerait la représentation des flottants, pas la loi.
La direction est le point · `<` couronne MOINS de candidats que `<=`, donc
c'est le choix serré, et il est délibéré. Il reste vivant, et il est écrit
ici pour que personne ne le compte comme couvert.

Et au passage · tant que les tests de ce crate vivent en `tests/`, sa mesure
de mutation demande l'invocation longue, hors de la convention `--lib` ·
c'est un coût récurrent que le rapatriement des tests unitaires en `src/`
supprimerait.

---

### 📋 Ce qui reste AVANT l'admission (mesuré · 2026-08-13 soir)

Le swarm Gate-11 (3 lentilles indépendantes) a fermé son P0 et ses P1 dans la
session. Ce qui suit est ce qu'il a nommé et que la session a **délibérément
laissé**, avec son coût mesuré — pour que l'admission ait une liste, pas une
surprise.

| item | mesure | pourquoi pas ce soir |
|---|---|---|
| `#[non_exhaustive]` sur les 6 **structs** de `model.rs` (`Task` · `Workflow` · `Failure` · `Step` · `Run` · et `Group` l'enum l'a déjà) | **23 sites** de construction littérale à réécrire (17 en `tests/`, 6 en `src/`) + 6 constructeurs | Les 4 **enums** de `model.rs` le portent déjà · l'écart est réel. Mais 23 réécritures de tests contre un forward-compat qui protège une crate `publish = false` sans consommateur externe, c'est le mauvais échange à minuit. Item d'admission, pas item de session. |
| `run_from_journal` à **99 lignes** (cap 100) | `scripts/ci/check-fn-length.sh` · **VERT** | Le plafond est GARDÉ, pas seulement écrit. 99 passe ; la 101ᵉ ligne fait rougir le gate. Pré-découper une fonction de pli à froid introduit un risque pour zéro bénéfice mesuré — la structure fera le travail au moment où il faut. |
| `extra` catch-all sur les types de `model.rs` | — | **Intentionnel, et différent d'`ingress::Node`.** Node en avait besoin parce que `plan::print_of` promet «tout SAUF l'id» — une promesse que le type trahissait. Les types de `model.rs` n'ont pas d'empreinte «tout le reste» · `derive::signature` NOMME ses trois champs (verbe, outil, needs triés). Pas de promesse, pas de trahison. |

**Trois copies d'une même loi fondues le même soir** · `Verb::as_str` se déclare
« the ONE wire voice » et un `verb_name` privé plus les littéraux de
`cost_by_verb` la recopiaient. Les copies sont supprimées, pas la déclaration.
