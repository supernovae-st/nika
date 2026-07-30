# Checkpoint · 2026-07-30 · cold-start de la prochaine session

> État poussé et vérifié sur le binaire installé. Ce fichier est la
> rampe : une session neuve lit ÇA, puis le record run-5
> (`2026-07-29-audit-run-5-decay-verdict-gate.md`) pour le détail.

## Où on en est (tout est poussé)

- **engine** `main @ 85a0adad5` · **spec** `main @ f37d28e` (nika-spec).
- **binaire installé** `/opt/homebrew/bin/nika` = build release de
  `85a0adad5` (swap Cellar · restauration : `brew reinstall nika`).
- **L'arc d'audit est clos** : runs 1–5 + DECAY terminés. Les 4
  régressions du run DECAY (F2 · F3 · F4 · F14) sont TOUTES fixées après
  les décisions opérateur du 2026-07-30, plus D1 (exec net-fit) et la
  queue de cohérence notify (default channel = net).
- **Vérifié sur l'installé** : D1 → 1 escape net · F2 exec-curl → 2
  SEC-009 (0 avant) · native → 1 (inchangé) · F3 → ligne TYPES rétrécie
  nommant la ref invérifiable · F4 → run frais `--answer` 2/2 done ·
  fixture corpus `trifecta-realized-flow-ungated` → SEC-009.

## Les 4 décisions opérateur (le mandat de la prochaine session)

1. **Run #5 (surfaces jamais auditées) · domaine = runtime exec/sandbox.**
2. **F3-B** (shapes `returns:` des builtins) **= APRÈS run #5.**
3. **Le WIP spec-04 de la lane parallèle = strictement hors scope.**
4. **La couture wave-order = sonde déclarée DANS run #5** (mesurée, pas
   réparée — la réparation est une décision de loi séparée).

## Run #5 · squelette de déclaration (à instancier par la session)

```
DOMAIN      runtime exec/sandbox — ce que la frontière OS applique
            vraiment vs ce que check/run prétendent (la classe SQLite
            WAL : sidecars, dispatch, capture, profil sandbox)
ORACLE      /opt/homebrew/bin/nika (0.106.1 @ 85a0adad5) + rebuild
            target/debug par fix · les sondes discriminates du run
SURFACES    profil sandbox exec (fs/net au niveau OS) · sidecars/locks
            (SQLite WAL · le finding banké du début de l'arc) ·
            dispatch (re-gate argv/cwd · NEP-0004 loi 2) · modes de
            capture (stdout · structured · raw) · flux env/stdin ·
            LA SONDE wave-order (canal fichier · ordre intra-wave —
            déclarée, mesurée, pas réparée)
DENOMINATOR session solo · 8 sondes planifiées (déclarées up front) · ~3h
EXCLUDED    les 5 domaines déjà balayés (jamais poolés · §1) · F3-B
            (son propre arc, après) · le lock spec-04 de la lane ∥
```

Le finding SQLite WAL banké au tout début de l'arc (« SQLite (WAL) sous
globs fins = SQLITE_CANTOPEN, même dir granted — un SEUL côté (read OU
write) débloque, clean hors sandbox · le profil ne mappe pas une op
SQLite (lock/canonicalize) pour les globs étroits · 3 lanes db portent
le hatch documenté inline, à resserrer le jour du fix ») est la SONDE 1
de ce run — c'est sa fenêtre.

## Pièges connus (ne pas re-découvrir)

- **Tree principal = WIP d'une lane parallèle** (`schema_paths.rs` +
  `tests/conformance_core.rs` non commités · 2 tests rouges · le « lock
  spec-04 »). JAMAIS `git add -A` — toujours des adds scopés. Si le gate
  pré-push est rouge sur leur WIP : pousser depuis un worktree propre
  (`git worktree add /tmp/nika-push HEAD` → commit → `git push origin
  HEAD:main` → `git worktree remove`).
- **Le gate pré-push prend ~3-4 min** ; `force-push-guard` = le remote a
  avancé → `git fetch && git merge origin/main --no-edit` puis re-push.
- **Cap 1500 LOC/fichier** — la réponse maison est la DESCENTE
  (`run/mod.rs` → `resume_setup.rs` · `check_render.rs` → `claims.rs`),
  jamais une exemption.
- **Swap Cellar** : `rm $CELLAR && cp target/release/nika-cli $CELLAR &&
  chmod 555 $CELLAR && codesign --force --sign - $CELLAR`.
- **Spec repo** : la lane opérateur committe elle-même mes changements
  non poussés (arrivé avec `05-errors.md`) — vérifier `git log` avant de
  re-committer là-bas.
- **Fixtures scratch** : `engine/target/audit5/` (git-ignoré ·
  re-créable depuis les records).

## Après run #5 (la file)

- **F3-B** — les shapes `returns:` des builtins (la surface stdlib) :
  rend le verdict TYPES capable, pas seulement honnête. L'arc est nommé,
  sa place est après le sweep.
