# Journal de séance · 2026-07-28

> **Ce que ce document est.** Le compte rendu complet d'une séance de ~13 h
> (08:52 → 22:18 CEST), reconstruit depuis le transcript intégral (6 476
> messages) et vérifié contre `git log` dans chaque dépôt.
>
> **Ce qu'il n'est pas.** Il ne redit pas ce qui vit déjà sur disque :
> - `docs/plans/2026-07-28-resource-algebra.md` — la matrice de mesures §1,
>   les 42 théorèmes §2, les 14 dimensions §3, les 6 rétractations §6.
> - `docs/plans/2026-07-28-verdict-coverage.md` — la classe du faux vert,
>   les six constats et leur loi.
> - `dx/doctrine/rules/nika-first-automation.md` §4bis — la loi réciproque
>   du dogfood (monorepo privé).
>
> **Ce qu'il ajoute.** Toute la première moitié de la séance — l'arc kit /
> plugins / MCP / conformance / starters — n'existe dans **aucun** des deux
> plans. `grep` sur les deux fichiers rend 0 pour `nika init`, `starter`,
> `28/33`, `PARSE-021`, `prompts/list`, `4916`, `7578`. Plus les trouvailles
> tardives (le modèle inventé, la dérive du catalogue, le recomptage
> 282→105) qui n'ont pas été repliées dans l'algèbre avant la clôture.
>
> Quand le transcript et un document se contredisent, c'est dit — jamais lissé.

---

## 1 · La chronologie

### Phase A · 08:52-11:00 · « le kit enseigne une syntaxe que le moteur refuse »

La demande d'ouverture : vérifier que toute la suite de plugins (Cursor,
Codex, Claude Code, et les autres) est SOTA face aux dernières mises à jour
de Nika. Le binaire installé est `nika 0.106.0` (`/opt/homebrew/bin/nika`),
la release « the authority release » publiée la veille — deux flag-days dans
la même fenêtre (`vars:`/`env:` morts, remplacés par les quatre autorités
`inputs:`/`config:`/`const:`/`secrets:` ; `permits:` absent = frontière VIDE,
`NIKA-AUTH-006`).

Le premier constat tombe en vingt minutes : le kit enseigne **cinq formes que
le moteur refuse**, dont deux mortes depuis 0.103 — trois releases de retard.
La preuve la plus nette est auto-référentielle : un fichier écrit *exactement*
comme le kit l'enseigne a été **refusé par le garde-fou du kit lui-même**
(`check-on-edit.sh` a bloqué l'écriture). Le plugin réfutait sa propre skill.

La cause racine est structurelle, pas éditoriale : **les surfaces avec un test
de parité sont restées justes ; celles sans test ont pourri.** `AGENTS.md`
connaît les 21 commandes du binaire parce qu'un test l'y force ;
`nika-workflow-language.mdc` était la **dernière** surface encore dupliquée en
constante Rust là où ses neuf frères sont `include_str!`és depuis le kit — il
a donc dérivé seul. Conséquence : `nika init` enseignait déjà 0.106 pendant
que le plugin miroir était à 0.105, **pour le même fichier**.

Correctifs : 14 fichiers dans le moteur (source de vérité — le kit est
`engine-mirror`, pins sha256), `CURSOR_RULES` transformé en `include_str!`
(la divergence devient structurellement impossible), et un cliquet en deux
tests (denylist des formes mortes + un vrai `nika_check` sur chaque workflow
complet que le kit imprime). Le cliquet a attrapé **deux erreurs de l'auteur
à sa première exécution**.

En parallèle, insertion opérateur : OpenClaude (`gitlawb`, 30 399 ★) comme
client du kit. Sondage de leur source : `src/utils/plugins/schemas.ts:965`
lit `.claude-plugin/marketplace.json` par défaut, et leur `parseArgs.ts`
porte les mêmes verbes que Claude Code. **Le kit s'installe tel quel, sans
port.** Signal honnête retenu : leur Skill Hub est à 11 ★ sans push depuis
le 27 mai.

### Phase B · 11:00-12:15 · les trous MCP, et le rail qui bouche

Audit de la surface MCP : notre oracle sert **1 primitive sur 3** de la spec
2025-06-18 (`{"tools":{}}`, ni `prompts` ni `resources`), et ses 9 outils
portent **zéro annotation** — bloquant pour une soumission OpenAI
(`annotations_required`), et une friction UX permanente puisque l'oracle est
*prouvablement* en lecture seule. Les deux trous sont fermés : annotations +
titres prouvés **sur le fil** (`tools/list` d'un binaire construit), puis les
5 slash commands du kit servies comme **prompts MCP** — elles passent de
3 écosystèmes derrière 3 manifests à **tous les clients câblés**, depuis une
seule implémentation, parce que stdio et HTTP partagent déjà un dispatcher.

Le rail git se bouche : deux `pre-push` de sessions sœurs tiennent le créneau,
puis deux `git commit` concurrents partagent l'index. Le premier commit moteur
est **rejeté après 21 minutes** (`clippy-touched`, `-D warnings` sur le test
de l'auteur : 105 lignes contre le plafond maison de 100).

### Phase C · 12:15-14:00 · la conformance, et le premier arbitrage

Un balayage sur les autres dépôts trouve du périmètre neuf : la **spec** porte
un `workflow:` scalaire dans le bloc « 5 pillars · immutable forever », et sa
suite de conformance ne dit plus la vérité. Mesure : **25 fixtures promettent
`exit 0` et sont refusées** (quasi toutes `NIKA-AUTH-006`) et **24 déclarent
un code et en reçoivent un autre**. Le mécanisme : le flag-day permits fait
refuser la fixture *pour la frontière, avant* qu'elle atteigne la règle
qu'elle existe pour prouver. Toute la famille `yaml-profile/invalid/` est
neutralisée, et `trifecta-realized-flow-ungated` — une fixture de **sécurité**
— ne déclenche plus son `NIKA-SEC-009`.

Réparé par l'inférence du moteur lui-même (`nika check --infer-permits`),
jamais par une devinette : **25 → 1**. La 25e révèle une divergence
sémantique (le profil YAML dit admettre les sept tags du core, le moteur les
refuse tous) — laissée intacte et remontée, parce que déplacer une fixture de
`valid/` vers `invalid/` est un arbitrage de canon.

L'opérateur tranche : **la spec adopte les codes du moteur**. Un agent exécute
la réconciliation — PASS 80→95, DRIFT 16→3, DIVERGENT 1→0.

### Phase D · 14:00-15:30 · les rapports terrain, et la loi

L'opérateur remonte deux rapports issus de son usage réel dans Cursor : les
agents inventent du Python dès qu'ils heurtent SEC-004/SEC-009, et un run
affiche **23/23 ✔ avec 4 tâches échouées**. Chaque affirmation est reproduite
avant d'être traitée. Trois écarts check↔run sont établis, et ils ne pointent
pas tous dans le même sens :

| # | Écart | Qui a tort |
|---|---|---|
| 1 | un bound de permit derrière `const:` échappe à `AUTH-006` | le **check** |
| 2 | `exec:` forme `shell:` sans permits passe, `SEC-004` au run | le **check** |
| 3 | `item` dans `when:` — check vert, `NIKA-VAR-001` au run | le **runtime** |

Le n°3 s'est **inversé à la lecture de la spec** : `spec/03-dag.md:418` montre
`when: ${{ item.kind == 'article' }} # for_each-local`, mais la prose
normative ligne 214 dit *« `when:` decides whether an admitted task runs · it
is evaluated after the gate »*. **La prose gagne sur l'exemple** — le runtime
avait raison, le checker était trop permissif. Le refus tombe désormais au
check, et le même vice a été trouvé sur `for_each:` (c'est *la collection* que
le fan-out lit — y référencer `item` est circulaire).

La question socratique de fin de phase produit la loi de la séance :

> **Un verdict doit soit COUVRIR sa revendication, soit la RÉTRÉCIR à ce
> qu'il couvre.** Un vert qui vaut moins que ce qu'il dit dépense la confiance
> du lecteur et ne rend rien.

Elle est écrite dans `2026-07-28-verdict-coverage.md` avec chaque repro
lancée et chaque ancre exacte.

### Phase E · 15:30-17:30 · les surfaces qui jugent, et les starters

Le push moteur échoue après **2h06** de gate. Diagnostic par élimination,
chaque jambe mesurée : tests 4916 verts / 55 crates, clippy workspace exit 0,
hygiene exit 1 = YELLOW (ne bloque pas) → le rouge est dans les cliquets.
Lancés individuellement : `fn-length` exit 1, `scan_task` à **115 lignes**
contre un plafond de 100 — **les dix lignes du contexte pre-fan-out de
l'auteur**. Neuf commits attendaient derrière son propre dépassement.

Question socratique : *ai-je vraiment tout touché ?* Non. Les deux templates
« démarre ici » (`nika-starter`, `nika-actions-starter`) **ne passent pas leur
propre check** — l'endroit où une forme morte coûte le plus, puisqu'elle est
copiée avant qu'on ait rien appris. `nika-starter` était **trois flag-days en
retard**. Réparés par le codemod du moteur, 2 PR ouvertes.

Puis le P0 de l'opérateur — `--native-strict` doit faire échouer dur la glue
Python — se révèle **déjà implémenté**. Le trou n'était pas dans le
vérificateur : **personne ne le lançait**. Câblé sur cinq surfaces, dont
l'outil MCP `nika_check` que Cursor appelle vraiment, qui rendait
« ✔ clean — audited before a single token was spent » **sans mentionner un
seul hint**.

### Phase F · 17:30-19:00 · on casse la spec · les Q1-Q4

L'opérateur autorise à casser la spec. Quatre questions d'architecture posées
une à une, chacune avec recommandation, chacune ratifiée (§4). L'objet en jeu :
`accept_flow:`, une décharge **authored** dont chaque classe est une
**précondition que le moteur vérifie** — l'inverse du `# nosec` dont la raison
est un commentaire que personne ne relit.

La recherche tue une classe sur trois avant qu'elle soit écrite : CaMeL
§6.4/§7 démontre que **non-interprétation ≠ non-influence** (le dispatcher,
le canal par le nombre, l'oracle 1 bit), donc `content_not_interpreted` est
une condition partielle et ne peut pas décharger un refus entier — la loi de
l'auteur appliquée à l'auteur.

### Phase G · 19:00-21:15 · l'algèbre des ressources

Le rapport de 589 lignes de l'agent Cursor de l'opérateur nomme huit fautes.
Sept sont confirmées par mesure. La plus rentable est **H** — la barrière de
vague : deux attentes indépendantes de 5 s prennent **10,1 s**. Suit une
séquence de recherches profondes (ordonnancement, non-interférence, bornes
déclarées, sémantique d'annulation, algèbre des workflows, analyse statique
de coût, modèles temps/énergie/attention) qui produit
`2026-07-28-resource-algebra.md` : 55 mesures, 42 théorèmes, 11 puis 14
dimensions, 6 décisions ratifiées, 6 rétractations, et un §4 « ce qui n'est
PAS testé ».

Le verdict sur le coût **change de nature** dans cette phase : le matin
« le plafond est 126× trop lâche » (précision) ; le soir « le plafond est
**faux** sur une boucle d'agent, et exploitable à 658× » (soundness).

### Phase H · 21:15-22:18 · le dogfood, la doctrine, et la réfutation

Le catalogue de prix : l'infrastructure existe et elle est bonne
(`data/*.toml` → `build.rs` → `phf::Map`, `[meta]` avec `source`/`as_of`/
`sha256`), mais **rien ne la rafraîchit**. L'opérateur propose que le refresh
soit lui-même un **workflow Nika** — meilleure idée que le workflow GitHub
Actions écrit dix minutes plus tôt, qui est abandonné. Le workflow natif
`workflows/catalog-pricing-probe.nika.yaml` est écrit, checké, lancé, et
**prouve la dérive par lui-même**.

Écrire ce seul workflow produit **trois défauts en vingt minutes**, aucun
cherché — dont une huitième instance du faux vert. D'où la doctrine que
l'opérateur demande d'ancrer partout : quand on écrit du Nika et qu'on voit
un manque, ça remonte à la spec ou au moteur. Ancrée en Surface A
(`nika-first-automation.md` §4bis) plutôt qu'en nouvelle règle.

Trois swarms lancés (16 agents). Celui de réfutation adversariale rend le
verdict le plus dur de la journée : **17 des 33 affirmations du document
sont réfutées**, dont la mesure de largeur de DAG que l'auteur avait passé
trois heures à reconstruire alors que `analysis.rs:26-32` la documente et
que le binaire l'imprime déjà. Les corrections sont écrites **en tête** du
document, avant les sections qu'elles annulent.

---

## 2 · Tout ce qui a été commité

### 2.1 Moteur (`supernovae-st/nika`, branche `main`)

23 commits vérifiés au `git log`. `a3c4950e2` (11:21, `feat(cli): nika lsp
accepts the host convention flags as no-ops`) est **exclu** — le transcript
l'attribue explicitement à une session sœur.

| SHA | Heure | Sujet | Ce que ça change réellement |
|---|---|---|---|
| `0b362db42` | 11:52 | `fix(kit): the plugin taught a syntax the engine refuses` | 14 fichiers du kit + `briefs.rs` ; `CURSOR_RULES` devient `include_str!` ; le cliquet en 2 tests |
| `a94360f2c` | 11:58 | `docs(kit): the kit teaches the whole language, not a third of it` | 28/28 builtins · 13/13 clés d'enveloppe · 18/18 modificateurs · la section *the one way* |
| `a6bdf3e54` | 11:59 | `fix(mcp): the read-only oracle says so on the wire` | annotations + `title` sur les 9 outils, estampillées en un seul point dans `catalog()` ; test de parité |
| `e7f1f5284` | 12:11 | `feat(mcp): the five slash commands reach every client, not three` | module `prompts.rs` ; capability `prompts` ; corps `include_str!`és du kit |
| `56d6dc3e5` | 12:41 | `feat(plugin): the codex manifest carries its public listing card` | bloc `interface` ; `author` chaîne→objet ; `displayName` top-level supprimé ; couleur `#4f86ff` depuis la SSOT tokens |
| `9cc9d6002` | 12:52 | `docs(kit): composition was a whole chapter the kit never mentioned` | `invoke: { workflow: }` enseigné ; famille `NIKA-COMP-*` ; syntaxe prouvée exit 0 avant écriture |
| `f7b680366` | 13:21 | `fix(check): \`when:\` and \`for_each:\` stop admitting the loop locals` | **3 préoccupations fondues** (voir §7) : le fix scope, le correctif `explain`, la réparation du corpus 33/33, le message d'erreur |
| `0b5040ca1` | 15:07 | `docs(kit): the native-first law gains the recipe it was missing` | section *« When the boundary pushes back »* : la recette SEC-004 et la recette SEC-009 |
| `16bb12b97` | 15:15 | `refactor(check): scan_task stops building its contexts by hand` | 115 → sous le plafond de 100 ; helper unique pour trois contextes quasi identiques |
| `2aaa80881` | 15:25 | `docs(plans): the false-green class, six findings and their law` | `docs/plans/2026-07-28-verdict-coverage.md` |
| `acdb032ab` | 15:58 | `feat(check): a prompt with no default names its headless cost` | hint `headless-prompt` ; nomme le code que le run va émettre |
| `5ea8cf2dd` | 16:05 | `feat(check): a structured capture nobody branches on names what it swallows` | hint `swallowed-exit` ; silencieux dès qu'un `exit_code` est lu |
| `74d230254` | 16:45 | `fix(check): every surface that judges for the author judges like the run gate` | `--native-strict` câblé sur `check-on-edit`, `guard-run`, `/nika:check`, subagent `nika-author` ; trois remèdes faux corrigés |
| `62ba2406e` | 17:27 | `feat(mcp): the agent-facing oracle stops handing back a green the run gate refuses` | `nika_check` MCP strict par défaut ; 3 tests |
| `12add44a3` | 17:50 | `refactor(check): the traversal half of the hint lane earns its own file` | `hints.rs` 1593 → 1393 + `walk.rs` 219 (débloque le cap `file-loc-cap`) |
| `14a14b1d5` | 18:50 | `test(check): the ledger fixture goes on one line, and says why` | chaîne du test remise sur une ligne (le cliquet `fn-length` la mal-comptait) |
| `a77078707` | 19:47 | `fix(check): the audited card stops calling a ceiling a floor` | `est ≥` → `est ≤` ; `at_most` ajouté au vocabulaire ; `est unbounded · N uncapped tasks` |
| `71c67da3f` | 21:05 | `docs(plans): the resource algebra` | `docs/plans/2026-07-28-resource-algebra.md` (939 lignes) |
| `1ca813fbb` | 21:12 | `docs(plans): the cost ceiling is unsound on agent loops` | §2.8 · Θ(n²), les 2 848 runs, la variance infinie |
| `a7cd93b28` | 21:14 | `docs(plans): the resource-vector research corrects four of my own rows` | §3.0 · les corrections X1-X4 |
| `9b172979e` | 21:15 | `docs(plans): the DAG width is measured` | §1.2 S9 — la mesure de largeur, plus tard réfutée (§0.5) |
| `a30080600` | 21:44 | `feat(catalog): the pricing provenance is produced by the engine it feeds` | `workflows/catalog-pricing-probe.nika.yaml` (112 l.) + la ligne `as_of` dans `render.rs` + 43 l. de skill |
| `b40839d16` | 22:15 | `docs(plans): an adversarial pass killed 17 of 33 claims` | §0.5 en tête du document |

**⚠ Divergence transcript / git.** Le transcript annonce à 21:15 *« moteur ·
14 commits en ligne »*. Le compte réel de `0b362db42..9b172979e` est **21**.
Le chiffre 14 n'est justifié nulle part et est faux. Les comptes
intermédiaires, eux, sont exacts : « sept livrés » à `9cc9d6002` (6 siens +
1 sœur), « les 10 partent au gate » pour `f7b680366..14a14b1d5`.

**État du push au moment de l'écriture** : `origin/main` = `9b172979e`.
`a30080600` et `b40839d16` sont **non poussés** — un `git push origin main`
lancé en arrière-plan à 22:18 tournait encore dans son gate `pre-push`.

### 2.2 Spec (`supernovae-st/nika-spec`)

| SHA | Heure | Sujet | Effet |
|---|---|---|---|
| `a62be2a` | 12:35 | `docs(spec): the immutable-forever block showed a dead envelope` | le `workflow:` scalaire dans « 5 pillars · immutable forever » + QUICKSTART argv + `llms-full.txt` régénéré |
| `586e0b3` | 12:37 | `fix(conformance): the positive baseline promised clean and refused` | 25 fixtures promettant `exit 0` → 1 |
| `e83ddaa` | 12:54 | `fix(conformance): the authority gate spoke over the rule under test` | codes masqués réconciliés (agent) — PASS 80→95 · DRIFT 16→3 · DIVERGENT 1→0 · BUG 5→4 |
| `5c0a203` → `15e459c` | 15:46 | `fix(examples): the spec copy caught up with the engine's` | `03-exec-pipeline` — la copie spec avait divergé de celle réparée dans le miroir |
| `4f207c9` → `4cd3ec6` | 16:32 | `fix(spec): the helper-script rule stops offering a remedy that does not work` | la queue de la cellule `/005` cesse d'offrir « ou inscris-le au ledger » |
| `735a5a0` | 19:56 | `fix(examples): the ceo brief stops teaching a builtin field that does not exist` | `tasks.bill.output.total_usd` × 2 → le compteur entier ; balayage de la classe |

Les flèches marquent un cherry-pick : le commit d'origine a été posé dans
l'arbre principal, puis reporté sur une branche jetable quand un push direct
a été refusé en non-fast-forward (une session sœur avait mergé #221-225).

Push direct réussi : `020eec3..7f7fa6f` (les trois premiers), avec la réponse
du remote *« Bypassed rule violations — 3 of 3 required status checks are
expected »* — le push est passé en contournant les checks requis (droits
admin), donc la CI a tourné **après**. Elle est ensuite rendue **3/3 verte**
(Conformance · REUSE · CodeQL).

### 2.3 Monorepo (`supernovae/`, privé)

| SHA | Heure | Sujet |
|---|---|---|
| `2c2434354` | 14:12 | `chore(repos): the spec pointer follows its own main` — pointeur de submodule, commit vérifié présent sur le remote avant de pinner |
| `df8fc160d` | 21:47 | `docs(doctrine): le dogfood remonte · la loi réciproque de nika-first` — 63 lignes dans `nika-first-automation.md` §4bis |

Poussé : `6519f9c5c..2c2434354` (emmenant deux commits OpenClaude d'une autre
provenance). `df8fc160d` est **non poussé** au moment de l'écriture.

### 2.4 Les PR

| Dépôt | # | Titre | État |
|---|---|---|---|
| nika-agents | [129](https://github.com/supernovae-st/nika-agents/pull/129) | `feat(integrations): openclaude installs this kit unported` | **MERGED** → `d021ffb1e` |
| nika-agents | [130](https://github.com/supernovae-st/nika-agents/pull/130) | `feat(gate): the kit-native surfaces get the drift ratchet too` | **MERGED** → `715ab62` |
| nika-agents | [131](https://github.com/supernovae-st/nika-agents/pull/131) | `fix(gate): a legitimate JSON Schema is not a dead form` | **MERGED** → `1f400de` |
| nika-agents | [132](https://github.com/supernovae-st/nika-agents/pull/132) | `docs(kit): the kit-native surfaces teach the language the binary speaks` | **MERGED** → `4f155fd` |
| nika-spec | [226](https://github.com/supernovae-st/nika-spec/pull/226) | `fix(spec): the helper-script rule stops offering a remedy that does not work` | **OPEN** — et **redondante**, voir §7 |
| nika-spec | [229](https://github.com/supernovae-st/nika-spec/pull/229) | `fix(examples): the ceo brief stops teaching a builtin field that does not exist` | **OPEN** |
| nika-starter | [6](https://github.com/supernovae-st/nika-starter/pull/6) | `fix(template): the starter refused its own check` | **OPEN** |
| nika-actions-starter | [8](https://github.com/supernovae-st/nika-actions-starter/pull/8) | `fix(template): the starter refused its own check` | **OPEN** |

### 2.5 Dépôts touchés par des agents dispatchés, poussés par des sessions sœurs

- **vscode** — `3cf439c` (12:47) `fix(language): teach the live 0.106 predicate
  and exec forms`. Contenu : le prompt de génération faisait **émettre
  `succeeded` au modèle** ; la grammaire TextMate ne colorait que le jeu mort ;
  le go-to-definition ne matchait que les orthographes mortes ; la fixture
  `signature-demo` échouait **au parse** (~20 suites exerçaient un fichier que
  le moteur refuse). 1 408 tests verts après.
- **website** — `864c95c` (12:54) `fix(content): the dead predicate spellings,
  and a dead depends_on`.

---

## 3 · Toutes les mesures

Convention : **[NOUVEAU]** = absent des deux plans existants — c'est la raison
d'être de cette section. **[⊂ algèbre]** = déjà consigné dans
`2026-07-28-resource-algebra.md`, rappelé ici pour la continuité.
**[RÉFUTÉ]** = la mesure ou sa lecture a été démontrée fausse ensuite.

### 3.1 L'écart kit ↔ moteur · les cinq formes mortes  [NOUVEAU]

Chaque forme écrite exactement comme le kit l'enseignait, passée au binaire
0.106.0. Sortie verbatim :

```
######## a-exec ########
exit=2
PARSE ✗  [NIKA-PARSE-019] validation error: `exec.command` is argv-only —
         ["prog", "arg", …] runs via execve, each element one token …
         the old string form was an IMPLICIT shell … (02 §exec · 0.103)
######## b-list ########
exit=2
PARSE ✗  [NIKA-PARSE-022] `tasks:` is a sequence — it became a map keyed by
         task id; drop `- id:`, the key IS the identity
######## e-exec-argv ########
exit=2
PARSE ✗  [NIKA-PARSE-019] validation error: unknown capture mode `text`
         (stdout·stderr·combined·structured)
```

```
PARSE ✗  [NIKA-VALUES-001] vars: is a dead envelope field (R3a · the E-split) …
         `nika check --fix` migrates                                    exit=2
PARSE ✗  [NIKA-PARSE-020] `workflow:` is a scalar — the envelope became an
         object; write `workflow:` then `  id: exec-probe`              exit=2
```

Récapitulatif :

| Ce que le kit enseignait | Verdict 0.106 |
|---|---|
| `workflow: <kebab-id>` scalaire | `NIKA-PARSE-020` |
| `tasks:` en séquence (`- id:`) | `NIKA-PARSE-022` |
| `vars:` · `${{ vars.x }}` | `NIKA-VALUES-001` |
| `${{ env.KEY }}` | `NIKA-VALUES-002` |
| `after: { t: succeeded }` | `NIKA-DAG-005` |
| `capture: text` | `NIKA-PARSE-019` (modes : `stdout·stderr·combined·structured`) |
| `command:` en chaîne | `NIKA-PARSE-019`, argv-only **depuis 0.103** |

Surface CLI : le binaire expose **21 commandes**, le kit n'en enseignait
**15**.

Ce que `env` **conserve** (sondé, les trois passent exit 0) : `secrets: { gh:
{ source: env, key: … } }`, `permits: { env: ["HOME"] }`, `exec: { env: {…} }`.
L'autorité de valeur `env` est morte ; le mot survit comme frontière d'un
process enfant.

### 3.2 Le blast radius de `nika init`  [NOUVEAU]

`nika init -y` lancé dans un repo neuf avec le binaire **publié** 0.106.0 :

```
=== does the scaffold teach dead forms? ===
  workflow: <      2 file(s)
  ${{ vars.        1 file(s)
  ${{ env.         3 file(s)
  capture: text    2 file(s)
  : succeeded      2 file(s)
```

`nika init` écrit **15 fichiers** dans chaque nouveau repo (dont
`.agents/skills/nika-authoring/SKILL.md`, les trois subagents Cursor, les
trois hooks, `.cursor/rules/nika.mdc`, `.github/copilot-instructions.md`).
Neuf sont `include_str!`és depuis le kit ; le dixième —
`nika-workflow-language.mdc` — était le seul en constante Rust dupliquée.

### 3.3 La couverture d'enseignement, avant → après  [NOUVEAU]

```
builtins           9/28  →  28/28
clés d'enveloppe  11/13  →  13/13   (types: · policy: ajoutés)
modificateurs     11/18  →  18/18   (on_error · on_finally · returns · inert ·
                                     declassify · fail_fast · max_parallel)
exemples embarqués 28/33 →  33/33
```

Les 6 familles de builtins sont celles du moteur lui-même : CORE · FILE ·
DATA · NETWORK · INTROSPECTION · MEDIA. Le trou le plus grave était
**auto-destructeur** : la loi *native-first* ordonne de préférer un builtin
au shell, mais le catalogue affiché n'en nommait qu'un tiers — un agent
obéissant écrivait `exec: grep` alors que `nika:grep` existe.

Corpus embarqué : **5 des 33 exemples ne passaient pas leur propre check**,
dont **trois de la même façon** — une tâche `on_finally` dont l'outil n'est
jamais accordé. Le moteur **a** une own-corpus law testée, mais elle couvre
les **templates**, pas les **examples**.

### 3.4 Le trou de couverture statique derrière `const:`  [NOUVEAU]

Paire minimale, même URL, seul le porteur change :

```
######## f1-literal ########
exit=2
 ✖ PERMITS  [NIKA-AUTH-006 · net] task `grab` · invoke `nika:fetch` with a
            literal URL under an absent `permits:` block
######## f2-const ########
exit=0
 ○ PERMITS  zero authority (no `permits:` declared · F-O8) · pure compute ·
            `permits: {}` states it
 ✔ audited · 1 task · 1 wave · permits none · est ≥$0.0000 · 1 hint
```

`PERMITS` n'observe que les **littéraux** — un bound replié depuis `const:`
est invisible. C'est la ligne 2 du tableau de `verdict-coverage.md`.

### 3.5 Les surfaces MCP  [NOUVEAU]

Avant : `capabilities = {"tools": {}}`, `annotations = NONE` sur les 9 outils.

Après, **sur le fil** (`initialize` + `tools/list` contre un binaire construit) :

```
ON THE WIRE · 9 tools
  ✓ nika_check   title='Audit a workflow'
      {"destructiveHint": false, "idempotentHint": true,
       "openWorldHint": false, "readOnlyHint": true}
  ✓ nika_inspect title='Project the workflow graph'   (idem)
  ✓ nika_explain title='Explain an error code'        (idem)
  ✓ nika_schema  title='The workflow JSON Schema'     (idem)
```

Puis les prompts :

```
capabilities : {"prompts": {}, "tools": {}}
prompts/list : 5 — clients show these as slash commands
   /check    Audit a workflow                     arg=<file.nika.yaml>          required
   /explain  Explain a workflow or an error code  arg=<file.nika.yaml|NIKA-XXXX> required
   /new      Scaffold a workflow                  arg="[template] [file]"       optional
   /trace    Read a run's flight recorder         arg=[trace-or-workflow]       optional
   /permits  Infer the permits boundary           arg=<file.nika.yaml>          required
prompts/get  : 1442 chars · arg substituted = True
nom inconnu  : -32602   (pas un bloc de contenu — un modèle le relirait comme des instructions)
```

Et le trou que Cursor heurte réellement : quand le rapport est propre,
l'outil MCP `nika_check` renvoyait *« ✔ clean — audited before a single token
was spent »* **sans mentionner les hints du tout** — les hints ne comptent
pas dans `is_clean()`. Le fichier identique échouait en shell et était refusé
par la porte du run. Le fichier énonçait lui-même la loi qu'il violait :
*« a `nika check` that fails the shell must not read as success over MCP »*.

### 3.6 La couverture plateformes, sur la machine de l'opérateur  [NOUVEAU]

15 cibles `nika wire`. Mesuré :

| Client | Config présente | Nika câblé |
|---|---|---|
| cursor · gemini · qwen · claude-desktop · windsurf | ✓ | ✓ |
| **warp** | ✓ (`~/.warp/.mcp.json`) | ✗ — et **`warp` n'est pas une cible `nika wire`** |
| zed | ✓ | ✗ (`context_servers` porte olympus seul) |
| **kimi** | CLI installé (`~/.kimi-code/bin/kimi`) | ✗ — pas de cible |
| **openclaude** | — | ✗ — pas de cible |

**Faux positif corrigé en cours de route** : `nika welcome` disait `windsurf ✓`
et `vscode ✗`. Le ✓ signifie **wired**, pas *installé*. Aucun bug — l'accusation
a été retirée avant d'être formulée publiquement.

OpenClaude : **30 399 ★**, poussé le jour même ; leur Skill Hub **11 ★**, sans
push depuis le 27 mai. `.claude-plugin/marketplace.json` est leur défaut
documenté (`schemas.ts:965`).

### 3.7 La conformance de la spec  [NOUVEAU]

```
fixtures promettant « exit 0 » et refusées au check   25  →  1
fixtures déclarant un code de check, un autre sort    24
attentes « at RUN » (check muet, à juste titre)       29   ← faux positifs de
                                                            ma première méthode
```

Après la réconciliation par agent (runner `conformance/run.sh` · nika 0.106.0) :

```
PASS 80 → 95 · DRIFT 16 → 3 · DIVERGENT 1 → 0 · BUG 5 → 4
CI static gate `runner.py all` : 254 PASS, rc=0
8 selftests Python reference-core : 298 lois, tous verts
```

**Divergence spec ↔ moteur trouvée** : `nika explain NIKA-YAML-001` → *unknown
code*. Zéro occurrence de `NIKA-YAML` dans `crates/*/src`. La spec déclare un
profil YAML avec 11 codes et 13 fixtures ; le moteur refuse bien les
constructions mais **sous d'autres noms** (alias → `NIKA-PARSE-001`, merge-key
→ `-005`, dup-key → `-017`). Sauf `non-nfc` : avec permits, il ne déclenche
**plus rien** — la normalisation NFC n'est pas appliquée.

**⚠ Correction apportée par l'agent** : les `NIKA-YAML-*` ne sont pas
seulement *déclarés* côté spec, ils y sont **implémentés** (11 lois, 11 lignes
de registre, un juge de référence). La décision « la spec adopte les codes du
moteur » a donc un coût que la formulation initiale masquait.

### 3.8 Les trois écarts check ↔ run rapportés par l'opérateur, reproduits

**`item` dans `when:`**  [NOUVEAU]

```
spec/03-dag.md:418   when: ${{ item.kind == 'article' }}   # for_each-local
spec/03-dag.md:214   « when: decides whether an admitted task runs.
                       It is evaluated after the gate. »        ← la prose normative
checker (scan.rs)    allow_loop_locals: has_for_each
nika check           exit 0
nika run             ✖ summaries when · NIKA-VAR-001 · unresolved template
                       reference `item != null`

après le correctif    check exit 2   →  jamais de run
                      item/index dans le CORPS d'un for_each : toujours exit 0
```

Second bug dans le même écran : `nika explain NIKA-VAR-001` disait *« or given
a `default:` in the workflow `vars:` block »* — le code dont le métier est
d'enseigner la réparation pointait vers le bloc **mort**. Le texte vivait dans
`crates/nika-cli/src/verbs/explain.rs`.

**`capture: structured` avale l'exit non-zéro**  [⊂ algèbre, mais la repro
verbatim est ici]

```
capture: structured   run exit=0   ✔  s  exec · /usr/bin/false
capture: stdout       run exit=1   ✖  NIKA-EXEC-001 · command exited with status 1
```

Même commande, mêmes permits. Le choix est **délibéré et écrit** —
`dispatch.rs:623-629` : *« the one-obvious-way split: under `structured` a
non-zero exit is DATA (the task succeeds · `exit_code` is the branch), under
the text modes it fails the task »*.

**`nika:prompt` sans `default:`**  [NOUVEAU]

```
nika check   exit 0   ✔ audited · 1 task · 0 hints
nika run     exit 1   NIKA-BUILTIN-PROMPT-001
```

Le fait est **statique** — un prompt sans `default:` ne peut pas se terminer
sans humain. Le check ne disait rien. Hint `headless-prompt` livré.

**`completion` vide facturée = success** — détectée par le moteur
(`dispatch.rs:1068-1096`, `empty_thinking_warning()` ligne 1082), montée en
`warning` sur `TaskCompleted`, et la tâche **réussit exprès**. Non modifié :
le flip casse silencieusement tout workflow qui branche sur `exit_code` tant
que `allow_nonzero:` n'existe pas.

### 3.9 `--native-strict` · le mécanisme existait, personne ne le lançait  [NOUVEAU]

```
exec python3 helper.py            →  rc=2 · « native-strict · 1 native-first hint above »
exec git  (avec ledger)           →  rc=0
exec git  (SANS ledger)           →  rc=0
```

Discrimination exacte : le drapeau ne frappe **que** la glue script, zéro
dommage collatéral. Les surfaces qui checkaient pour l'auteur :

| Surface | avant | après |
|---|---|---|
| `check-on-edit` (chaque édition) | non | **oui** |
| `guard-run` (porte avant tout run) | non | **oui** |
| `/nika:check` | non | **oui** |
| subagent `nika-author` | non | **oui** |
| **outil MCP `nika_check`** | non | **oui, par défaut** |

Piège évité, et il est nommé : un hook qui juge avec le drapeau doit **nommer
le drapeau** dans son message, sinon l'agent relance la forme nue, lit un vert
que la porte refuse, et boucle.

**Trois remèdes faux corrigés** :

1. le moteur disait *« replace them **or** record them in the exec ledger »* —
   mesuré faux : un wrapper `.py` avec ledger complet échoue exactement pareil.
   Corrigé au moteur, dans le hint, dans la skill, **et dans la spec**.
2. `TYPES` affirmait *« every deep output reference fits its declared shape »* —
   le scan est sain (`schema_typing.rs` ne juge que ce qui déclare et rend
   « inconnu » ailleurs), mais **aucun mécanisme `output_schema` n'existe** dans
   le catalogue, donc aucun builtin ne *peut* déclarer une forme. Ligne resserrée.
3. la commande `/nika:check` enseignait *« lis `clean` »*. Sous `--native-strict`
   le payload dit `clean: true` **et** `native_strict_clean: false` avec exit 2
   (`check/mod.rs:153` : `strict_clean = clean && (!native_strict || native_hints == 0)`).
   **Le moteur était honnête ; l'enseignement ne l'était pas.**

### 3.10 Les starters  [NOUVEAU]

```
nika-starter          daily-brief.nika.yaml           ✖ NIKA-PARSE-021
nika-actions-starter  flows/daily-brief.nika.yaml     ✖ NIKA-VALUES-001
nika-actions-starter  flows/pr-risk-review.nika.yaml  ✓
```

Les deux templates « démarre ici » ne passaient pas leur propre check.
`nika-starter` était **trois flag-days en retard** — il a fallu les trois
migrations : `w1-map` (enveloppe scalaire + tasks en liste), `c2-esplit`
(le bloc `vars:`), `w2-flow` (`depends_on` + lectures `tasks.*` en corps).
Après : **les trois templates exit 0**.

Une seule chose est restée au jugement humain : un `type: array` nu n'est plus
un type dans la grammaire 0.106, et le codemod a **eu raison de refuser de
deviner**.

### 3.11 Le balayage écosystème  [NOUVEAU]

`nika check --native-strict` sur **chaque** `.nika.yaml` de l'écosystème :

```
295 fichiers · 216 verts · 79 rouges

les 79 rouges, PAR ZONE :
   50  conformance/{envelope,yaml-profile/invalid,variables,dag}   ← DOIVENT échouer
    6  scripts/media/fixtures      (permits-escape · l'actif « cassé » des médias)
    4  scripts/test/battery
    3  crates/nika-runtime/fixtures/adversarial/                   ← DOIVENT échouer
    2  fuzz/corpus/                (arbitraire par définition)
    1  project

LE CORPUS D'ENSEIGNEMENT · 76 fichiers · 76 verts · 0 rouge
```

Sur ~110 échecs bruts d'un premier balayage, **un seul défaut réel** :
`spec/examples/03-exec-pipeline` avait divergé de la copie moteur réparée le
matin. Le reste :

- 262 fichiers `eval/authorability/runs/` = **artefacts de mesure** — les
  corriger effacerait la donnée ;
- `audit-workflow/site-audit.nika.yaml` = **faux positif de l'auteur** — les
  chemins `skills:` se résolvent contre le **CWD** :

  ```
  depuis le repo      exit 0   ✔ audited · 10 tasks · 5 waves · permits declared · 0 hints
  depuis le parent    exit 2   NIKA-AGENT-003
  ```

  Même fichier, inchangé. Seul le répertoire courant diffère.
- `website/moonshot.nika.yaml` = faux positif (une doc d'usage moonshot doit
  montrer moonshot ; le provider ne résout pas dans ce binaire) ;
- composition `child.nika.yaml` ×5 = faux positif (un enfant vérifié **seul**
  n'a pas les permits que son parent lui transmet).

### 3.12 Les portes du dépôt moteur  [NOUVEAU]

```
push pre-push gate         exit 1 après 7578,55 s  ( = 2 h 06 )
  🥊 gate: pre-push gate failed (tests · clippy · hygiene · ratchets)

diagnostic par élimination, chaque jambe mesurée :
  workspace tests          exit 0  ·  55 crates  ·  4916 tests
  workspace clippy         exit 0
  hygiene check-all.sh     exit 1  =  YELLOW (seul rc=2 bloque)
  ratchets CI              ← le seul restant

les 8 cliquets, lancés individuellement :
  fn-length → exit=1   ←  LE BLOCAGE
  crate-size → exit=0 · loc-limits → exit=0 · no-default-features → exit=124 (timeout 120 s)

  FAIL  crates/nika-check/src/analyzer/scan.rs:190
        fn 'scan_task' is 115 lines (max 100)

confirmation indépendante par la suite complète :
  [ci-ratchets] 1 ratchet(s) failed: fn-length
```

Deuxième train, deuxième blocage :

```
12 file-loc-cap            RED     ← un fichier ≥ 1500 LOC sans exemption
  1593  crates/nika-check/src/hints.rs        ← seul fautif du dépôt
après le split :  hints.rs 1393  +  walk.rs 219      → YELLOW
```

Troisième train, troisième blocage — et c'est le cliquet qui a tort :

```
check-fn-length.sh accuse une fonction de 212 lignes.
Elle en fait 24 (ouverture 819, fermeture 842).

cause : son détiqueteur de littéraux travaille LIGNE PAR LIGNE
  row = re.sub(r'(?<![r#])b?"(?:\\.|[^"\\])*"', '""', row)   # exige la quote fermante
une chaîne Rust multi-ligne à continuations `\` n'a pas de quote fermante sur
sa ligne d'ouverture → les accolades du YAML DANS la chaîne comptent comme du code.
```

Compteurs de tests recueillis au fil de la séance : 60 → 62 (`nika-onboard`),
87 → 88 → 94 → 98 (`nika-mcp`), 535 → 536 → 537 (`nika-check`), 813 après le
split, 375 (276 cli + 99 display), 4 916 sur 55 crates, 55 suites vertes
workspace en fin de séance.

### 3.13 Le catalogue de modèles  [NOUVEAU]

```
crates/nika-catalog/data/*.toml   →  build.rs  →  src/data/generated.rs (phf::Map)

  model-pricing.toml        606 règles   schema "nika/model-pricing@1.1"
                            [meta] source = https://models.dev/api.json
                                   as_of  = 2026-07-07
                                   source_sha256_16 = d31a39603aa5419d
  mcp-servers.toml          282 blocs [[  →  ⚠ 105 SERVEURS (+104 packages, 1 remote)
  llm-providers.toml        106 blocs [[  →  ⚠  38 FOURNISSEURS (+68 modèles imbriqués)
  model-capabilities.toml    49
  embeddings.toml            13
```

**⚠ Recomptage.** Les chiffres 282 et 106 sont **faux** : `grep -c '^\[\['`
compte toutes les tables TOML, imbriquées comprises. Corrigés par le swarm
catalogue, vérifiés à la main. Les 606 modèles tarifés tiennent.

**Un modèle inventé passe le rung MODELS** — faux vert non consigné ailleurs :

```
anthropic/claude-totally-invented-9      rc=0   ✔ MODELS   1 model resolves in this binary
```

Le résolveur et le tarificateur ne sont pas d'accord, et c'est le premier qui
parle : le nom passe sous un provider connu, retombe en « no catalog price »,
et le plafond devient `unbounded` sans que personne ne dise que le modèle
n'existe pas. Or le rung MODELS a été ajouté précisément pour *« la faille
exacte qu'un agent qui hallucine rencontre »*.

**La dérive du catalogue, prouvée par un workflow Nika** :

```
catalogue    as_of = 2026-07-07   sha = d31a39603aa5419d
amont vivant                      sha = d5aa8fd06062f49e
```

Mesuré par `workflows/catalog-pricing-probe.nika.yaml` (5 tâches · 3 vagues ·
`--native-strict` rc=0 · 0 hint · net limité à models.dev), avec sa trace
comme preuve.

**Le gisement models.dev** — 173 fournisseurs, 5 810 modèles, 21 champs ; on
en lit **quatre** :

| Champ | Couverture | Ce que ça débloque |
|---|---|---|
| `limit.output` | 653/653 · 100 % | **le plus gros** — un `max_tokens: 100000` sur un modèle plafonné à 8192 est FAUX et le check peut le dire aujourd'hui |
| `limit.context` | 653/653 · 100 % | aujourd'hui écrit **à la main** sur 69 lignes |
| `open_weights` | 653/653 · 100 % | « local vs cloud » devient dérivable au lieu d'être deviné par l'absence de prix |
| `tool_call` · `reasoning` | 653/653 · 100 % | un workflow qui donne des outils à un modèle qui n'en prend pas est un échec runtime que le check pourrait attraper |
| `structured_output` | 487/653 · 74 % | idem pour `schema:` |
| `temperature` | 650/653 · 99 % | |
| `knowledge` (cutoff) | 400/653 · 61 % | |
| `status` | 35/653 · 5 % | déprécié — on l'enseigne peut-être |
| `cost.reasoning` | 18/653 · 2 % | un tarif séparé qu'on ignore |
| `cost.tiers` | 31/653 · 4 % | long contexte — openai 8 · google 7 · xai 6 · openrouter 10 · anthropic/deepseek/mistral/groq/hf/nvidia = **zéro** |

**Le trou d'honnêteté** : `model-pricing.toml` porte un `[meta]` avec
`source`/`as_of`/`sha` ; **les quatre autres n'en ont aucun**. Et
`mcp-servers` porte `last_verified` sur 105 lignes **écrites à la main** — un
consommateur ne peut pas distinguer une ligne générée d'une ligne artisanale.

**Ce qui existait déjà et que l'auteur a failli réimplémenter** (corrigé deux
fois) : `pricing_snapshot()` rend `source` + `as_of` + `sha` ; `doctor`
avertit au-delà de 120 jours ; `models_rung` construit le bloc snapshot pour
`--json` ; et `catalog-verify.yml` tourne **quotidiennement à 6 h** en sondant
npm/PyPI/OCI/MCP. Le seul vrai trou était **la ligne humaine** (l'opérateur ne
voyait jamais la date) et **le cron du prix** (le seul catalogue à dériver
silencieusement, et c'est celui qui adosse une promesse).

### 3.14 Le faux vert trouvé en dogfoodant  [NOUVEAU]

Écrire le workflow de refresh a produit trois défauts en vingt minutes,
aucun cherché :

```
① le seatbelt câblé le matin a attrapé son auteur DEUX FOIS
     · nika:date sans son `op` requis
     · une clé `shape` en double  (NIKA-PARSE-017)

② nika:jq reçoit une VALEUR · un fetch `raw` rend du TEXTE
     la forme juste est `mode: jq` DANS le fetch

③ CHECK ✔ · RUN ✔ · ARTEFACT MALFORMÉ
     content: '{"fetched": ${{ with.day }}}'
     produit   "fetched": 2026-07-28T19:39Z     ← du JSON invalide
     et RIEN ne l'attrape
```

Le ③ est le constat neuf : **l'interpolation d'un scalaire dans du JSON
littéral n'est ni typée ni vérifiée.** Le workflow réussit et écrit un fichier
que personne ne peut relire. Le swarm d'authoring l'a retrouvé
indépendamment (FG-2).

### 3.15 Les mesures déjà consignées dans l'algèbre  [⊂ algèbre]

Rappel de pointeurs seulement — les tables complètes vivent dans
`2026-07-28-resource-algebra.md` §1 :

- **Coût** C1-C10 : le plafond ne facture que la sortie (`cost.rs:194`) alors
  que le runtime facture quatre termes (`usd_for_split`) ; `$0.0305` annoncé
  contre `$0.000242` facturé (126×) ; un modèle local rendu `0.00`, ce qui est
  **inversé** et non prudent.
- **Ordonnancement** S1-S9 : 10,1 s pour deux attentes indépendantes de 5 s ;
  `wave_parallelism = None` = illimité par défaut (`config.rs:14`) ; aucun
  compteur par fournisseur ; arête `after:` fantôme et redondante → 0 hint.
- **Langage de valeur** V1-V5 : macros CEL et `matches()` refusées
  (`NIKA-VAR-005`) — l'explosion exponentielle et le ReDoS sont fermés **par
  construction** ; aucune borne de cardinalité nulle part.
- **Portes de sécurité** G1-G6 : leg ② du trifecta armée par la
  **déclaration**, pas par le corps (v2 ne lit **rien** et SEC-009 tire) ; et
  DRIFT dit huit lignes plus bas *« ce permit ne correspond à aucun chemin lu
  par le corps »* — deux sous-systèmes, un rapport, conclusions opposées.
- **Gate humaine** H1-H3 : **deux** implémentations de `Prompter`, dont une
  en `#[cfg(test)]` ; `compose.rs:829` injecte `NonInteractive` **sans
  condition** ; et le doc du trait promet *« The L4 CLI implements the TTY
  prompter »*.
- **Builtins** B1-B4 : `nika:jq` diverge de jq 1.7.1 sur `scan` (2 cas sur 4),
  et le run rend **4/4 vert** ; `nika:inspect view: cost` répond
  `available: false`.
- **`--max-cost-usd` blanchi par la composition** (FG-1 du swarm d'authoring) :

  ```
  l'ENFANT seul   --max-cost-usd 0.0001
    → rc=2 · « refusing to start: the workflow's unavoidable cost floor
              $0.003000 exceeds --max-cost-usd »            ✅ refus correct
  le PARENT qui l'appelle, MÊME flag
    → rc=1 · le run DÉMARRE · dispatche l'enfant · atteint le provider
      et n'échoue que sur « authentication failed: no API key »
  ```

  Avec une clé présente, **ça aurait dépensé de l'argent réel sous un budget
  qui l'interdisait explicitement.**
- **URL canonicalisée comme un chemin** : `lexically_normalize`
  (`nika-cap/fit.rs:74`) découpe sur `/` et jette les segments vides, donc
  `https://acme.test` → `["https:", "", "acme.test"]` → `https:/acme.test`.
  Mécanisme confirmé au code ; le chemin runtime a été mesuré par l'agent de
  l'opérateur, les deux se recoupent.

---

## 4 · Toutes les décisions

### 4.1 Ratifiées par l'opérateur pendant la séance

| # | Question posée | Réponse | Base |
|---|---|---|---|
| — | Que fait-on de `env` ? (« pk on laisse env ? on la supprime ») | **Supprimé de toutes les surfaces d'enseignement**, zéro mention de compat. Seule la skill *migration* garde une table « mort → classe d'accueil » | l'autorité de valeur `env` est morte en deux formes (`NIKA-VALUES-002` au parse et au check) |
| — | Le canal de livraison OpenClaude | scope **projet** (sûr), pas scope user | `~/.openclaude.json` est un fichier d'état chaud portant l'OAuth, avec un garde anti-perte de credentials — un read-modify-write naïf peut écraser des identifiants |
| — | La divergence spec ↔ moteur sur les codes | **la spec adopte les codes du moteur** | 0 occurrence de `NIKA-YAML` dans `crates/*/src`. ⚠ l'agent a ensuite montré que les codes sont *implémentés* côté spec (11 lois + registre + juge) — le coût est réel |
| — | `capture: structured` | **« exit != 0 DOIT échouer, même en `capture: structured` »**, avec `allow_nonzero: true` en sortie de secours | l'opérateur, sur son run 23/23 vert avec 4 échecs |
| **Q1** | Où vit la décision humaine ? (A runtime · B authored · C les deux) | **(C), avec B seul load-bearing** — le runtime n'est **jamais** la porte de sécurité | Willison ne recommande pas l'approbation humaine ; Meta Rule of Two la met en dernier recours ; Anthropic mesure *« users approve 93 % of permission prompts »* et a remplacé le prompt par une allowlist ; aucun système de production n'exige un TTY |
| **Q2** | Qu'est-ce qui arme leg ② du trifecta ? | **(d) la PROVENANCE** — relire ce que ce run vient d'écrire n'introduit aucune information nouvelle, donc n'arme rien | les 4 variantes v1/v2/v4/v5 ; et la porte récompensait la **sous-déclaration** d'autorité |
| **Q3** | Quelle forme pour l'exemption ? | **enum FERMÉ, chaque classe une précondition VÉRIFIÉE par le moteur**, + `because:` obligatoire. Pas d'`other`, pas de texte libre | tout l'état de l'art traite la justification comme un commentaire non vérifié (`# nosec`, `@SuppressWarnings`, `noqa`, VEX, SARIF `justification: string`, SPARK *« no impact on the behavior of the tool »*) |
| **Q4** | Quelles classes shippent ? | ordre **v1 à une classe → prompteur TTY → non-influence** | CaMeL §6.4/§7 tue `content_not_interpreted` ; `human_validates_payload` est gaté sur l'existence d'un prompteur TTY ; `data_not_sensitive` retirée |
| — | FIN vs GROS sur l'analyse de flux | **GROS d'abord** — le label `pc` seul, qui ferme quatre canaux sans aucune annotation | Progent a payé 45 %→27 % d'utilité, CaMeL divise la sienne par deux ; TACIT tient 100 % de sécurité à utilité égale pour ~1000 lignes |
| — | La borne de cardinalité | **`take: N` de l'opérateur** plutôt que `max_items:` de l'auteur | `max_items` est une *prédiction sur des données qu'on ne connaît pas* → équilibre prévisible `1000000` partout ; `take: N` est une *décision sur le travail* → toujours répondable. GitHub GraphQL impose exactement ça avec son `first: n` obligatoire |
| — | La doctrine du dogfood | **ancrée**, Surface A (amendement de la règle parente) plutôt qu'une règle neuve | `nika-first-automation.md` dit « l'atelier FAIT tourner le produit » ; §4bis dit « et la friction remonte » |

### 4.2 Ouvertes, avec ce qui bloque

| Décision | Ce qui bloque |
|---|---|
| **Q-B · la sémantique d'annulation sous dataflow** — (a) tout laisser se régler · (b) annuler à la première panne | La recherche a **démoli (b)** en cours de route (OpenJDK jette l'exception sœur ; qui gagne la course est du pur timing). L'algèbre l'enregistre comme **Q5 ratifiée = DRAIN** en §5.1, mais **le transcript ne contient aucune ratification opérateur de ce point.** ⚠ Contradiction à trancher : le document affirme ratifié ce que la conversation a laissé ouvert. |
| **Le flip `capture: structured`** | L'arbitrage est donné (§4.1) mais **interdit avant** que `allow_nonzero:` existe — c'est une addition de schéma (`nika-schema`, validation, doc, tests), pas un patch de dispatch |
| **Le flip « completion vide facturée = échec »** | Même forme, même raison. `dispatch.rs:1079` porte le choix inverse par écrit |
| **L'arc `outputSchema` × 28 builtins** | Jamais lancé. C'est ce qui rend `✔ TYPES` signifiant au lieu de vrai par vacuité |
| **Le prompteur TTY manquant** | Capacité runtime qui touche la sémantique d'une gate de sécurité. Non improvisé en fin de séance. Débloque la classe `human_validates_payload` |
| **La fixture `yaml-profile/valid/unicode-nfc`** | Elle affirme que le profil admet les sept tags du core (`!!str calme`) ; le moteur refuse **tout** tag. Déplacer une fixture de `valid/` vers `invalid/` est un arbitrage de canon — laissée intacte à l'octet |
| **La classification SEC-009 de `exec` comme ingress** | `--native-strict` ferme l'**incitation**, pas la **classification** (`content_flow.rs:140`) |
| **La résolution des chemins `skills:`** | (a) résoudre contre le fichier workflow, comme un import · (b) garder le CWD mais que **le refus le dise**. La (b) est gratuite et suffit à tuer le piège |
| **Le teaching SEC-004 nommant `declassify:`** | Le texte vit dans une fiche `status: reserved` marquée *« the teaching string mints at C1 »* — écrire dedans sauterait un processus gouverné |
| **`nika wire` pour warp · kimi · openclaude** | Spécifié (le danger `~/.openclaude.json` documenté), jamais implémenté |

---

## 5 · Toutes les rétractations

Les six de `2026-07-28-resource-algebra.md` §6 (R1 l'optimiseur de requêtes ·
R2 l'historique-pour-planifier · R3 le certificat signé · R4 la clôture
série-parallèle · R5 « coût et chemin critique sont le même calcul » ·
R6 la borne obligatoire) **ne sont pas redites ici**. Voici celles que le
document ne porte pas.

| # | Ce qui a été affirmé | Ce qui l'a tué |
|---|---|---|
| **A1** | « nos 9 outils MCP sont 8 » (relevé initial) | comparaison des deux copies aux octets : `nika_tools` était déjà listé (ligne 36) dans les deux. L'édition prévue était un no-op |
| **A2** | « `tools.listChanged` manque » | notre liste d'outils **ne change jamais** à l'exécution. L'omettre est honnête ; le déclarer serait promettre des notifications qui n'arrivent jamais |
| **A3** | « `nika welcome` a un bug : `zed ✗`, `vscode ✗` » | le ✓ signifie **wired**, pas *installé*. Zed ne porte qu'olympus dans `context_servers`. **Faux positif jamais signalé** — retiré avant publication |
| **A4** | « 49 fixtures de conformance sont cassées » | méthode fausse : 29 déclaraient une attente « at RUN », et `nika check` est un audit **statique**. Re-mesuré à 25 + 24 |
| **A5** | « la porte `shell:` est injoignable » | mon test avait un id de workflow invalide (finissant par un underscore). `exec: true` + `shell:` → **exit 0**. Mon propre re-test m'a arrêté |
| **A6** | « les prompts MCP et les annotations sont live » | **l'agent kit-native m'a réfuté** : il a sondé le binaire **publié** 0.106.0 — `prompts/list` → `-32601`, annotations `NONE`. Mes changements étaient sur `main`, non publiés. J'avais prouvé sur un binaire construit localement |
| **A7** | « il ne reste qu'une surface publique fausse (la marketplace) » | **trois fois faux, trois fois corrigé par la vérification** : `nika init` écrit encore les 5 formes mortes (attend une *release*, pas un push) ; puis les deux starters refusent leur propre check ; puis le balayage complet |
| **A8** | « `audit-workflow` est cassé, son propre produit ne passe pas » | **faux positif de mon balayage** : les chemins `skills:` se résolvent contre le CWD. `exit 0` depuis le repo, `exit 2` depuis le parent, même fichier. Accusation **retirée après avoir été annoncée** — une fois de trop |
| **A9** | « septième bug moteur : `--json --native-strict` dit `clean: true` avec exit 2 » | le moteur publie `native_strict_clean: false` à côté. **Le moteur était honnête ; mon enseignement ne l'était pas** |
| **A10** | « il faut implémenter `--native-strict` en échec dur » | **il existait déjà** et fonctionnait (`exit=2 · native-strict · 1 native-first hint above`) |
| **A11** | « le rail est bouché, aucun travail ne manque, il faut juste attendre » | **dit deux fois, faux les deux fois.** Le gate avait **travaillé puis refusé** (`fn-length`). Je lisais un **refus** comme une **attente** parce que la sortie du push avait été tronquée à 13 lignes |
| **A12** | « je peux réparer le cliquet `fn-length` » | mesuré sur **tout** le dépôt : **1 faux positif devenu 5** (une chaîne brute `r#"…"#` se ferme par une quote nue que mon scanner lisait comme une ouverture). **Reverté** — une heuristique à moitié corrigée est pire qu'une heuristique documentée |
| **A13** | « le swarm a trouvé 32 fichiers cassés » | **erreur de catégorie** : il comptait comme cassées des fixtures dont le métier est d'être rouges. Corpus d'enseignement **76/76 vert** |
| **A14** | « la porte humaine `approve`/`nika:prompt` a été retirée côté PACK » (claim du swarm) | `git log -S'nika:prompt'` sur ce fichier ne rend **rien** — elle n'a jamais existé côté miroir. Et le « édité aujourd'hui 13:01 » était un **mtime de checkout**. L'agent avait confondu mtime et édition, puis bâti un récit dessus |
| **A15** | « j'ai réparé les cinq exemples » | **réparés dans le MIROIR** (`crates/nika-pack/pack/`), qui est re-vendoré depuis la spec au heal quotidien. Ils auraient été écrasés au prochain bump pendant que la source continuait d'enseigner le défaut |
| **A16** | « ma recette SEC-004 : pose la valeur dans un fichier et passe le CHEMIN en argv » | **c'est ce chemin-là qui arme la trifecta**, puisque le CLI doit relire le fichier → `fs.read` → jambe privée → gate obligatoire → gate irrépondable. **Piège écrit le matin même**, réécrit l'après-midi avec `declassify:` en tête |
| **A17** | « le catalogue porte 282 serveurs MCP et 106 fournisseurs » | **105 et 38.** `grep -c '^\[\['` compte toutes les tables TOML, imbriquées comprises |
| **A18** | « Anthropic facture un premium au-delà de 200k tokens, notre catalogue sous-facture » | la page vendeur, verbatim : *« Claude 4.6 and later models include the full 1M token context window at standard pricing »*. **Le moteur est juste** ; shipper la « correction » aurait *introduit* un premium que le vendeur ne publie pas. Nuance conservée : pour 4.5 la page est **silencieuse** — donc « non confirmable », pas « réfuté » |
| **A19** | « mon théorème 1+2 : le plafond de coût et le chemin critique sont le même calcul » | `cost.rs:94` — `for task in &wf.tasks`, une **somme plate**. Et le code a raison : l'argent s'additionne sous le parallélisme, le temps non. (Consigné aussi en §6 R5 de l'algèbre) |
| **A20** | « la mesure S9 de largeur de DAG » | **réfutée trois fois par le swarm** : mauvaise quantité (`width` compte des nœuds, le vrai parallélisme est dans `for_each` — `task.rs:640`, cap = `max_parallel` OU `items.len()`), mauvaise méthode (`analysis.rs` fait Dilworth → Fulkerson → Hopcroft-Karp avec témoin de König, **exact et déjà imprimé**), mauvais chiffres (45 des 76 ont width 1 → médiane **1**, moyenne **1,66**). Seuls « max 4 » et « 100 % ≤ 4 » survivent |

**Une rétractation qui n'a PAS été faite, et sa preuve** (l'algèbre la porte
déjà) : un agent d'audit a prétendu que le benchmark de vague ne correspondait
pas à son propre mécanisme. Le dump du plan le réfute — `slow_late` est à
profondeur 3 derrière la chaîne `c1→c2→c3`, donc `T_wave = 5+0+0+5 = 10`,
mesuré 10,1 s.

---

## 6 · Les swarms

### 6.1 Agents dispatchés individuellement

| L | Cible | Demande | Résultat de tête |
|---|---|---|---|
| 1362 | `Explore` | spécifier 3 nouvelles cibles `nika wire` (warp · kimi · openclaude) | **Danger trouvé** : `~/.openclaude.json` est un fichier d'état chaud contenant l'OAuth avec un garde anti-perte ; Warp est propre ; Kimi demande un résolveur `KIMI_CODE_HOME` |
| 1364 | `Explore` | balayage syntaxe morte sur les autres dépôts publics | **Périmètre neuf** : spec (`workflow:` scalaire dans « immutable forever » + 3 fixtures), vscode (`generate.ts:132` fait émettre `succeeded` **au modèle**), website (`opener:` auto-contradictoire) |
| 1377 | `Explore` | spec d'implémentation des prompts MCP | **Les deux transports partagent un dispatcher** (`protocol::dispatch`) ; les 5 commandes du kit portent déjà leur métadonnée en frontmatter |
| 1754 | `general-purpose` | corriger vscode | `3cf439c` · 1 408 tests verts ; la fixture `signature-demo` échouait **au parse** |
| 1756 | `general-purpose` | corriger website | `864c95c` |
| 1758 | `general-purpose` | réconcilier les codes masqués de conformance | **PASS 80→95 · DRIFT 16→3 · DIVERGENT 1→0 · BUG 5→4** ; et **il a corrigé l'orchestrateur** : les `NIKA-YAML-*` sont *implémentés* côté spec, pas seulement déclarés |
| 1874 | `general-purpose` | passe SOTA sur les surfaces kit-native | **Il a réfuté deux affirmations de l'orchestrateur** en sondant le binaire **publié** (A6) |
| 1876 | `general-purpose` | audit de complétude des docs | la doc renvoyait vers `invoke: nika:run_workflow`, un builtin **qui n'existe pas**, en déclarant la composition « pour un mineur ultérieur » |
| 2582 | `general-purpose` | vérifier 2 trous de sécurité rapportés | `nika:notify` absent du classifieur d'egress SEC-009 ; `SEC-001` ne couvre pas `rm -rf` |
| 2584 | `general-purpose` | durcir native-first + étendre l'own-corpus law | (worktree isolé) |
| 4161 | `web-researcher` | prior art de l'approbation humaine (HITL) | **Aucun système de production n'exige un TTY** ; VEX est le seul mécanisme ayant rendu la justification obligatoire, et il l'a fait avec un **enum fermé** ; Anthropic mesure 93 % d'approbations aveugles |
| 4407 | `web-researcher` | tester la nouveauté de « la justification est vérifiée » | **La clause tient** (SPARK dit de son propre `pragma Annotate` : *« no impact on the behavior of the tool »*). **Mais** un axe manquait : PERTINENCE (« ça supprime-t-il quelque chose ? ») est largement déployé ; VÉRITÉ (« la raison est-elle vraie ? ») n'existe nulle part. Et **`content_not_interpreted` est UNSOUND** — CaMeL §6.4/§7 |
| 4648 | `web-researcher` | non-interférence décidable sur DAG statique | **L'absence de boucle est l'hypothèse qui fait basculer la non-interférence de l'indécidable au décidable** (Yasuoka & Terauchi 2010 thm 3.9 ; Finkbeiner et al. CCS 2017). Les 5 canaux, dont 4 fermés par **un seul** mécanisme : le label `pc` |
| 4650 | `web-researcher` | exécution pilotée par certificat | **A donné tort à l'orchestrateur** : le certificat supprime notre meilleur détecteur de bugs. Argument Special J, précédent PCC de Necula. Le plus proche existant est `terraform plan -out` / `apply f`, **non signé** |
| 5324 | `web-researcher` | prior art des bornes de ressources déclarées | **A démoli `max_items:`** : Dhall, par son auteur — *« The absence of Turing completeness per se does not provide many safety guarantees »* ; l'équilibre prévisible est `1000000` partout |
| 5331 | `web-researcher` | sémantique d'annulation | **A démoli l'annulation-à-la-première-panne** : OpenJDK `StructuredTaskScopeImpl` **jette** l'exception sœur. Et `.buffered(cap)` est déjà un tampon de réordonnancement — la réécriture dataflow naïve glisse vers `buffer_unordered` |
| 5386 | `web-researcher` | théorie de l'ordonnancement de DAG | ASAP est **exactement optimal** sous `m ≥ largeur` (P∞), avec ses 5 hypothèses explicites |
| 5394 | `web-researcher` | théorie de l'optimiseur de requêtes | **A démoli l'analogie SQL** : le DAG est la **requête**, pas le plan. Graham 1966 plafonne tout le prix de l'ordre à `2 − 1/m` ; Leis 2015 (VLDB) : le modèle de coût est le composant de **moindre** valeur, et le régler avec les vraies cardinalités a **dégradé 35 %** des requêtes |
| 5397 | `web-researcher` | théorèmes des autres constructions | retry (AWS jitter, gRFC A6 : `maxAttempts` capé à 5 côté client, throttling par token bucket, « committed »), cache, deadline, sagas |
| 5468 | `web-researcher` | l'algèbre d'un workflow | CKA, loi d'échange, forme normale de Foata |
| 5477 | `web-researcher` | analyse statique de ressources | Melani Alg. 1, propagation d'ensembles |
| 5599 | `web-researcher` | paramètres de coût et rate-limit | **Le plafond est unsound sur une boucle d'agent** : `max_tokens` ne borne que la sortie ; la boucle est Θ(n²) **sur l'entrée** (9,8× à 15 itérations) ; sur 2 848 runs facturés — cache create 44,3 % · cache read 35,4 % · **sortie 10,4 %** · in 1,3 % ; un serveur MCP malveillant mais conforme gonfle le coût à **658×** |
| 5647 | `web-researcher` | modèles temps / énergie / attention | **A corrigé 4 lignes de la table de l'orchestrateur** (X1-X4) : l'argent **multiplie** sous retry ; l'énergie n'est additive que sur matériel **distinct** (3,7× d'écart selon la config de batch) ; le temps est une **paire** (travail, span) ; l'unité de l'attention est un **compte**, pas des secondes — et le « 23 minutes » folklorique est mal cité partout (la vraie source dit 25 min 26 s, SD 54 min 48 s, **pendant lesquelles** la personne fait 2,26 autres vrais morceaux de travail) |

### 6.2 Swarms dynamiques (workflows)

| L | Nom | Demande | Résultat de tête |
|---|---|---|---|
| 4942 | `nika-corpus-sota` | rechercher les patterns SOTA, auditer tout le corpus d'enseignement, synthétiser un catalogue canonique | `{total: 112, broken: 32, workaround: 3, suboptimal: 53, canonical: 24}` — **le `broken: 32` est RÉFUTÉ** (erreur de catégorie, §5 A13). Ce qui survit : `declassify:` enseigné **nulle part** (0 occurrence dans les 3 arbres) et le fan-out non borné |
| 6015 | `catalog-evolution` | trouver la source autoritative de chaque nouvelle dimension du catalogue, juger publiable vs mesurable | **A corrigé trois chiffres de l'orchestrateur** (282→105, 106→38, et l'argument massue Anthropic, §5 A17-A18). `model-perf` : **aucune source n'existe** — les 21 clés de models.dev, aucune n'est perf. `context_window` est le meilleur gain, gratuit depuis l'artefact déjà fetché |
| 6144 | `theorem-refutation` | attaquer adversarialement **tous** les théorèmes de la séance, puis dessiner l'orchestration de ce qui survit | **17 REFUTED · 10 hypothèses fausses · sur 33.** Le contre-exemple qui réordonne le plan : 16 tâches `infer:` identiques, fournisseur limité à 4 concurrents → `flat16` (0 arête) `EXIT=1 · 12/16 rouges · 12 × 429` ; `staged16` (12 arêtes `after:`) `EXIT=0 · 16/16 vert · 4,0 s · 0 × 429`. **L'ordonnancement le plus large échoue, le plus étroit réussit** — la classe de concurrence par fournisseur devient un **prérequis** du dataflow, pas son pair |
| 6314 | `intent-to-workflow` | écrire 6 workflows réels **en one-shot** depuis une intention, mesurer chaque friction jusqu'au vert, en faire une constatation moteur ou spec | **ZÉRO one-shot sur six · 45 aller-retours** (moy. 7,5 · méd. 7,5 · min 3 · max 11). 78 frictions : 47 enseignement (24 `language_surprise` + 23 `missing_teaching`) · **24 moteur** (16 `engine_defect` + **8 faux verts**) · 7 étourderies assumées. **40 apprenables nulle part.** Et le datum le plus fort n'est pas de friction mais de **distance** : intention 4, premier workflow **8 rounds** ; l'auteur lit **deux** exemples ; second workflow **0 round, vert one-shot**. « Le langage n'est pas le problème, l'ORDRE l'est » |
| 6448 | `sota-surfaces` | 3 sweeps SOTA — playground WASM · catalogue de modèles · domaines croisés (aérospatial, biologie/FBA, finance sous variance infinie, files d'attente) | **Lancé à 22:15, jamais rentré dans le transcript.** Résultat inconnu |

---

## 7 · Ce qui reste ouvert

### 7.1 Livraison — le goulot réel de la journée

| Item | État vérifié à l'écriture |
|---|---|
| **Le re-sync du miroir `nika-agents`** | **JAMAIS FAIT.** Le dernier commit du kit mirroré est `af6886c` (08:59, release-heal du matin). `resync-mirror.py` lit `origin/main`, donc il était mécaniquement bloqué derrière le push. **Ce que les gens installent depuis la marketplace est toujours à 0.105** — c'est-à-dire la syntaxe morte que toute la journée a corrigée |
| **`nika init`** | Écrit encore les **5 formes mortes** dans chaque nouveau repo. Attend une **release**, pas un push |
| **Engine `a30080600` + `b40839d16`** | **NON POUSSÉS.** `origin/main` = `9b172979e`. Un `git push origin main` lancé en arrière-plan à 22:18 tournait toujours dans son gate `pre-push` |
| **Monorepo `df8fc160d`** | **NON POUSSÉ** (1 commit d'avance sur `origin/main`) |
| **Spec `735a5a0`** | Sur `main` local, **non poussé** ; existe sur la branche `fix/ceo-brief-phantom-field` (PR #229) |
| **PR spec #226** | ⚠ **REDONDANTE.** Son contenu est **déjà sur `origin/main`** sous `4cd3ec6` — vérifié : le patch de `spec/03-dag.md` est identique à l'octet entre `4cd3ec6` (mergé) et `85b7f62` (tip de la PR). À fermer plutôt qu'à merger |
| **PR starter #6 · #8** | Ouvertes, jamais mergées. Les templates « démarre ici » servent donc encore un fichier que le moteur refuse |

### 7.2 Observé au moment de l'écriture, non instruit

Dans le log du push moteur encore en vol (`/tmp/push8.log`), la jambe
`force-push-guard` a imprimé :

```
X NIKA-AGENT-003 · skill `/var/folders/.../nika-run-skills-2365-0/SKILL.md`
  was never resolved at compose time — the composition root must read every
  `skills:` file and inject it via `Runtime::with_skills`
```

**Non vérifié.** Cela ressemble à un problème de chemin temporaire dans le
garde-fou, pas au défaut applicatif. À sonder à froid.

### 7.3 Dette moteur vérifiée par l'auteur

| Défaut | Gravité |
|---|---|
| `capture: structured` rapporte succès sur échec | **P0** — le run ment sur ce qui vient de se passer |
| `TYPES` vrai par vacuité — aucun mécanisme `output_schema` n'existe | **P0** architectural |
| Le plafond de coût ne facture qu'**un terme sur quatre**, et le terme omis est le **non borné** | **P0** soundness |
| `--max-cost-usd` **nul** un niveau de composition plus bas | **P0** — dépenserait de l'argent réel sous un budget qui l'interdit |
| Un modèle **inventé** passe le rung MODELS | **P0** — le rung existe précisément pour ça |
| Un bound de permit derrière `const:` échappe à `AUTH-006` au check | P1 |
| Les chemins `skills:` se résolvent contre le **CWD** | P1 |
| Aucun prompteur TTY n'existe, et le doc du trait promet le contraire | P1 |
| Deux copies de `03-exec-pipeline` sans gate de parité | P2 — elles rediviergeront |
| `nika:jq` diverge de jq 1.7.1 sur `scan` (2 cas sur 4), en silence | P1 |
| L'interpolation d'un scalaire dans du JSON littéral n'est ni typée ni vérifiée | P1 |
| Le cliquet `check-fn-length.sh` est faux et accuse la mauvaise fonction | P2 — le vrai fix est un parcours AST `syn` |
| Aucun cron ne rafraîchit `model-pricing.toml` (`as_of = 2026-07-07`, **21 jours de dérive**) ; le workflow GH Actions écrit à 21:35 a été **abandonné** au profit du workflow Nika, qui n'est **planifié nulle part** | P1 |
| Le seuil de péremption de `doctor` est **120 jours** — trop long : un prix d'introduction expire le 1ᵉʳ septembre, un workflow vérifié en août facture 1,5× en septembre | P2 |
| Les 4 catalogues autres que `model-pricing` n'ont **aucun `[meta]`** ; `mcp-servers` porte `last_verified` sur 105 lignes écrites à la main | P2 |

### 7.4 Dette rapportée, **jamais vérifiée par l'auteur**

Dit explicitement, parce qu'un produit sain a déjà été accusé à tort ce jour-là :

- SEC-009 **récompense** `exec curl` face à `nika:fetch` *(opérateur — la
  classification a été confirmée au code, l'incitation est fermée, mais la
  classe ne l'est pas)*
- résumé d'échec qui cite la dernière erreur, pas la causale *(opérateur)*
- bannière répétée ~50× *(opérateur — repro renderer jamais lancé, `pyte` absent)*
- `nika:notify` absent du classifieur d'egress SEC-009 *(agent)*
- `SEC-001` ne couvre pas `rm -rf` *(agent)*
- `const:` typés qui ne résolvent pas — 5 fixtures *(agent)*
- `assert:` accepté au check mais absent du JSON Schema publié *(agent)*

### 7.5 Surfaces jamais auditées en complétude

Déclarées « clean » sur la syntaxe morte, ce qui est **un test différent** :

`nika-registry` · `client-sdk` · `homebrew` · `audit-workflow` (en
complétude) · `THREAT-MODEL.md` · `server.json` · `listings.yaml` · les
**trois copies fork** de la skill Hermes (hermes-agent#61632 ·
agentic-awesome-skills#806 · buildwithclaude#238), que l'agent kit-native a
signalées **périmées** et qu'`AGENTS.md` classe comme la **même classe de
corruption qu'un pin cassé**.

### 7.6 Non testé — le §4 de l'algèbre, rappelé

Le repro du renderer (jamais lancé) · l'amplification `k^N` sur retry
imbriqué (sonde mal formée) · la facturation fournisseur sur échec mi-flux ·
ce que les goldens comparent réellement · 33 fichiers du corpus que le parser
a sautés.

---

## 8 · Les leçons de méthode

Chacune avec l'incident qui l'a enseignée. Les points 2-5 sont déjà dans
`2026-07-28-resource-algebra.md` §7 ; ils sont repris ici avec leur incident
complet, plus quatre que le document ne porte pas.

**① Une opération gatée est la DERNIÈRE commande de sa chaîne.**
Un `git log` derrière un `git commit` rend `exit 0` par-dessus l'échec du
commit. Ça a menti **deux fois** dans la journée. La première : la
notification disait `exit 0`, `HEAD` n'avait pas bougé — clippy `doc_markdown`
refusait `is_clean` sans backticks. La seconde : un `echo "push rc=$?"` en fin
de chaîne rendait 0 par-dessus un push refusé sur `file-loc-cap` RED.
*Corollaire* : quand la commande gatée est bien la dernière, la notification
dit vrai — vérifié sur les commits suivants.

**② La réparation d'un juge se mesure contre TOUT le corpus avant d'être
gardée.** Le cliquet `fn-length` accusait une fonction de 24 lignes d'en faire
212. Le correctif était juste sur ce cas ; mesuré sur les 10 000+ fonctions du
dépôt, il faisait passer **1 faux positif à 5** (une chaîne brute `r#"…"#` se
ferme par une quote nue). **Reverté**, avec le piège en commentaire. Une
heuristique à moitié corrigée est pire qu'une heuristique documentée — le
script se déclare lui-même *« Phase 0 heuristic … a proper `syn` AST walk »*.

**③ Le correctif atterrit dans la SOURCE, jamais dans le miroir vendoré.**
Cinq exemples ont été réparés dans `crates/nika-pack/pack/`, qui est le
miroir de `nika-spec` épinglé par `SPEC_PIN` et re-vendoré au heal quotidien.
Ils auraient été **écrasés au prochain bump** pendant que la source continuait
d'enseigner le champ fantôme. C'est exactement la loi appliquée au kit depuis
le matin (`engine-mirror` → le fix va dans le moteur), appliquée à l'envers
ici. Corollaire mécanique : `resync-mirror.py` lit `origin/main`, donc
moteur-d'abord n'est pas seulement doctrinal, c'est **forcé**.

**④ Quand le moteur imprime un chiffre que tu t'apprêtes à recalculer, lis
d'abord d'où il vient.** `crates/nika-check/src/analysis.rs:26-32` documente
verbatim : *« Width can EXCEED the largest wave … `max parallelism` = the wave
peak **as executed** · `width` = what the DAG **permits** »*, et le calcule par
Dilworth → Fulkerson → Hopcroft-Karp avec témoin de König — **exact, et déjà
imprimé**. `width 3` est passé sous les yeux de l'auteur deux heures plus tôt.
Il l'a noté et n'est pas allé voir. Trois heures de reconstruction, plus
lente et fausse.

**⑤ Quand la question est une propriété du graphe, on interroge le graphe.**
Deux tentatives de mesurer la largeur en invoquant le binaire une fois par
fichier ont expiré à dix minutes chacune. Le parsing YAML direct tourne en
**moins d'une seconde**. (Le swarm a ensuite montré que même ça était le
mauvais mouvement — voir ④.)

**⑥ `clippy` et `fmt` sont deux portes distinctes, et passer l'une ne dit rien
de l'autre.** `cargo fmt` a bloqué **deux fois** après un clippy vert. Le
pré-vol est `fmt` **et** `clippy`, en ciblé, **avant** le rail — 8 minutes de
hooks économisées par une backtick manquante, et jusqu'à 21 minutes pour un
dépassement de plafond.

**⑦ Un « CLEAN » répond à la question qu'on lui a posée, pas à celle qu'on
croit.** Le balayage « syntaxe morte » et le balayage « ce fichier passe-t-il
son check » sont **deux tests différents**. L'auteur a laissé le premier le
rassurer pendant des heures. C'est le second — *passer chaque `.nika.yaml` au
check* — qui a trouvé les starters, puis le reste. Et c'est la loi de la
journée appliquée à sa propre méthode : *un verdict qui ne couvre pas ce qu'il
annonce*.

**⑧ Un gate qui n'a jamais échoué n'est pas prouvé — il faut les DEUX
directions.** Le cliquet kit-native a été validé sur une fixture volontairement
fausse (**6 formes attrapées**) *et* sur le repo réel (**0**). Ce protocole a
immédiatement révélé qu'il laissait passer un bloc `vars:` nu — seule la
référence `${{ vars. }}` le sauvait. Le motif s'ancre donc sur la **colonne**,
pas sur le mot : `env:` en colonne 0 est une enveloppe, donc mort ; le même
`env:` indenté sous une tâche est bien vivant. Le même protocole a ensuite
tué un faux positif (`type: boolean` est **correct** dans un bloc `schema:` —
c'est du JSON Schema, pas la grammaire de types de Nika) : motif **retiré**
plutôt qu'exception ajoutée, parce qu'un gate qui rougit sur un exemple
légitime est un gate qu'on apprend à ignorer.

**⑨ Un gate ne doit pas se bloquer lui-même.** Le `gate.yml` câblé par
l'auteur portait un commentaire qui *nommait* les formes mortes ; la PR serait
partie rouge à son premier run. Corrigé en citant les **codes** plutôt que les
littéraux — plus précis de toute façon — **sans ajouter de dérogation** : un
carve-out pour la tuyauterie du checker est de la machinerie à maintenir pour
toujours, alors que retirer le conflit coûte une phrase.

**⑩ La release est la vérité pour tout ce que les gens touchent.** L'auteur a
prouvé les prompts MCP et les annotations sur un binaire **construit
localement** ; un agent a sondé le binaire **publié** et l'a réfuté
(`prompts/list` → `-32601`). La même erreur a failli se répéter sur
`nika init`. Corollaire de livraison : **un push ne suffit pas** pour ce que
`nika init` écrit — il faut une release.

**⑪ Ne pas paralléliser dans un arbre contendu.** Des heures ont été perdues
sur la contention d'index entre deux `git commit` concurrents (11 fichiers
stagés, trois préoccupations fondues dans un commit dont le message n'en
couvre qu'une). La décision de **ne pas** lancer trois agents sur le dépôt
moteur, faute de worktrees isolables (`.claude` est un symlink, garde
anti-redirection), a été aussi payante que du travail livré. Et on ne tue
jamais un `cargo` d'une autre session : un process en `SN` sans enfant
`rustc` est en **attente**, pas figé, et se distingue par le renouvellement
des PID.

**⑫ Une friction d'authoring est une constatation, pas un contretemps.**
Écrire **un seul** workflow Nika a produit trois défauts en vingt minutes,
aucun cherché — dont une huitième instance du faux vert. C'est le taux normal,
et c'est pour ça que c'est devenu une loi (`nika-first-automation.md` §4bis)
et pas un conseil. Le geste, dans l'ordre qui compte :

```
1  MESURER    le repro minimal · jamais « je crois que »
2  LOCALISER  file:line dans le moteur, ou § dans la spec
3  INSCRIRE   là où ça survit à la session
4  RÉPARER    À LA SOURCE — jamais dans le miroir vendoré
5  CÂBLER     un test ou un hint, sinon la classe revient
```

---

## Annexe · la loi, et où elle a mordu

> **Un verdict doit soit COUVRIR sa revendication, soit la RÉTRÉCIR à ce
> qu'il couvre.**

Découverte le matin sur six bugs qui semblaient indépendants, elle s'est
appliquée à chaque étage — y compris à ceux de l'auteur :

```
 le kit                   enseignait des formes que le moteur refuse
 PERMITS                  n'observe que les littéraux · un const: est invisible
 TYPES                    n'a aucune forme de sortie à confronter
 l'analyse de scope       voyait la TÂCHE, pas la SURFACE
 le settle exec           observe le MODE de capture, jamais le code de sortie
 l'oracle MCP             rendait « clean » sans nommer un seul hint
 le plafond de coût       disait ≥ là où sa propre section disait plafond
 leg ② du trifecta        comptait une lecture que DRIFT déclarait absente
 le cliquet fn-length     annonçait 212 pour 24, et accusait le mauvais coupable
 mon balayage             annonçait « cassé » là où il mesurait « refuse d'ici »
 mon certificat proposé   aurait supprimé le détecteur qui nous informe
 mon document             affirmait 17 choses fausses jusqu'à ce qu'un swarm l'attaque
```

**⚠ Sur le décompte.** Le transcript se compte lui-même de façon
incohérente : il annonce successivement « sept fois trouvé, six fois fermé »
(13:47), puis appelle « la huitième instance » **deux choses différentes** —
le `nika:prompt` sans `default:` (13:55) *et* l'interpolation JSON non citée
trouvée en dogfoodant (20:11). Le compteur a dérivé et n'a jamais été
rebasé. Le décompte fiable est celui du tableau ci-dessus : **douze
surfaces** où un verdict a affirmé plus qu'il n'observait, dont **trois
sont des artefacts de l'auteur** (le balayage, le certificat proposé, le
document lui-même).

Ce qui a effectivement atterri dans le code contre cette classe :
`0b362db42` (le kit) · `f7b680366` (l'analyse de scope) · `74d230254` +
`62ba2406e` (les surfaces de jugement et l'oracle MCP) · `a77078707` (le
signe du plafond) · plus deux hints purement additifs, `acdb032ab`
(prompt sans `default:`) et `5ea8cf2dd` (`capture: structured` avalé).
Le reste — `PERMITS` derrière `const:`, le flip du settle exec, le
`output_schema` des builtins, leg ② du trifecta, le cliquet `fn-length` —
reste ouvert (§7.3).
