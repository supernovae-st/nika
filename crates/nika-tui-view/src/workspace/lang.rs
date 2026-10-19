// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The view's own words in the language the person writes, as the Session
//! reads it: English by default and as the fallback, French where the table
//! holds a phrase. Only the view's chrome is said here (the review's headings,
//! the faces' first rows and facts, a workflow face's notes and the check's
//! reasons, a proposal's identity on its border); the Session's facts, the
//! engine's findings, the person's words, paths, values and model ids never
//! pass through.
//! Under the ASCII glyph column a French phrase loses its accents
//! ([`super::text::fold`]).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use nika_session::Lang;

use super::text::fold;

/// The view's phrase `en` in `lang`, in the glyph column in use: its French
/// when the person writes French and the table holds it (folded under
/// `ascii`), else `en` itself, unchanged.
pub(crate) fn say(lang: Lang, ascii: bool, en: &str) -> &str {
    if lang != Lang::Fr {
        return en;
    }
    match table().get(en) {
        Some((_, plain)) if ascii => plain.as_str(),
        Some((french, _)) => french,
        None => en,
    }
}

/// `n` of `noun` in `lang`: its French forms when the person writes French and
/// the units hold the noun (one for 0 and 1, folded under `ascii`), else the
/// English (`1 line`, `2 lines`).
pub(crate) fn count(lang: Lang, ascii: bool, n: usize, noun: &str) -> String {
    let french = UNITS.iter().find(|units| units.0 == noun);
    match french {
        Some(&(_, one, many)) if lang == Lang::Fr => {
            let word = if n <= 1 { one } else { many };
            let word = if ascii { fold(word) } else { word.to_owned() };
            format!("{n} {word}")
        }
        _ => crate::cells::count(n, noun),
    }
}

/// The nouns the view counts, by their English: French one, then many.
const UNITS: &[(&str, &str, &str)] = &[
    ("line", "ligne", "lignes"),
    ("task", "tâche", "tâches"),
    ("wave", "vague", "vagues"),
    ("edge", "arête", "arêtes"),
    ("human gate", "validation humaine", "validations humaines"),
    ("cleanup unit", "unité de nettoyage", "unités de nettoyage"),
    ("unit", "unité", "unités"),
    ("finding", "constat", "constats"),
    ("level", "niveau", "niveaux"),
];

/// The French of the view's phrases, keyed by their exact English, each with
/// its folded form.
fn table() -> &'static BTreeMap<&'static str, (&'static str, String)> {
    static TABLE: OnceLock<BTreeMap<&'static str, (&'static str, String)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (FR.iter())
            .map(|&(en, french)| (en, (french, fold(french))))
            .collect()
    })
}

/// The view's own phrases in French, keyed by their exact English. A `{…}`
/// stands for a value set in after, `{sep}` for the separator of the glyph
/// column in use; key names and typed words (`/show`, `F2`) keep their form.
const FR: &[(&str, &str)] = &[
    // The facts a proposal's review and its faces read, around the Session's own.
    ("when it runs", "à l'exécution"),
    ("schedule", "planification"),
    (
        "not a workflow the Session audited: it runs nothing",
        "pas un workflow audité par la Session : il n'exécute rien",
    ),
    ("nothing outside the process", "rien hors du processus"),
    ("reaches, as declared", "atteint, comme déclaré"),
    ("rehearsal", "répétition"),
    (
        "none bound to this identity",
        "aucune liée à cette identité",
    ),
    (
        "{n} more change(s) whose bytes these faces do not show{sep}`/show` prints every byte",
        "{n} changement(s) de plus dont ces faces ne montrent pas les octets{sep}`/show` affiche chaque octet",
    ),
    (
        "what a yes answers{sep}not saved{sep}nothing has run on your files",
        "ce à quoi un oui répond{sep}non enregistré{sep}rien n'a tourné sur vos fichiers",
    ),
    (
        "draft{sep}the compiler's question waits{sep}not a proposal yet",
        "brouillon{sep}la question du compilateur attend{sep}pas encore une proposition",
    ),
    (
        "set aside while the revision's question waits{sep}not consentable now",
        "mis de côté pendant que la question de la révision attend{sep}pas de consentement possible maintenant",
    ),
    (
        "proposal {path}{sep}{standing}",
        "proposition {path}{sep}{standing}",
    ),
    (
        "Left/Right change the face",
        "Gauche/Droite changent la face",
    ),
    (
        "these bytes, the proposal's own",
        "ces octets, ceux de la proposition",
    ),
    (
        "these bytes, the draft's own",
        "ces octets, ceux du brouillon",
    ),
    ("draft{sep}{title}", "brouillon{sep}{title}"),
    // The proposal a consent answers, by where it lands, on the review's border.
    (
        "proposal {path} · F2: whole words",
        "proposition {path} · F2 : tous les mots",
    ),
    ("{path} · F2: whole words", "{path} · F2 : tous les mots"),
    // The candidate's name in the aside and the conversation panel.
    ("proposal {path}", "proposition {path}"),
    ("draft (unsaved)", "brouillon (non enregistré)"),
    ("draft {title}", "brouillon {title}"),
    // The welcome: what the work is, its three steps and the example it offers.
    (
        "Turn an intention into a workflow.",
        "Transformez une intention en workflow.",
    ),
    (
        "1  Describe the outcome in the conversation.",
        "1  Décrivez le résultat voulu dans la conversation.",
    ),
    (
        "2  Answer questions; inspect the proposed plan.",
        "2  Répondez aux questions ; examinez le plan proposé.",
    ),
    (
        "3  Save, then Run with the workflow's models.",
        "3  Enregistrez, puis lancez avec les modèles du workflow.",
    ),
    ("Try: {demo}", "Essayez : {demo}"),
    (
        "a demo with its own records: totals per",
        "une démo avec ses propres données :",
    ),
    (
        "region, you name the currency.",
        "des totaux par région, dans la devise de votre choix.",
    ),
    (
        "For your own data, name the files to read.",
        "Pour vos propres données, nommez les fichiers à lire.",
    ),
    (
        "no project is known to this conversation",
        "aucun projet n'est connu de cette conversation",
    ),
    // The aside: its region, projections, headings, counts, footer and verdicts.
    ("Project", "Projet"),
    ("Files", "Fichiers"),
    ("FILES", "FICHIERS"),
    ("MEMORY", "MÉMOIRE"),
    ("CONNECTIONS", "CONNEXIONS"),
    ("SETTINGS", "RÉGLAGES"),
    ("  +{hidden} above", "  +{hidden} au-dessus"),
    ("  +{hidden} below", "  +{hidden} en dessous"),
    ("partial listing", "liste partielle"),
    ("not clean", "non conforme"),
    ("1 finding", "1 constat"),
    ("{n} findings", "{n} constats"),
    ("run failed", "run échoué"),
    // How a run's row says it ended or stands.
    ("pending", "en attente"),
    ("running", "en cours"),
    ("succeeded", "réussi"),
    ("failed", "échoué"),
    ("retrying", "nouvel essai"),
    ("skipped", "ignoré"),
    ("stopped", "arrêté"),
    ("paused", "en pause"),
    // The header: what a location is, its facts, and the selected intelligence's label.
    ("no project", "aucun projet"),
    ("  project", "  projet"),
    ("  {host} project", "  projet {host}"),
    ("no git", "pas de git"),
    ("no nika.yaml", "pas de nika.yaml"),
    ("nika.yaml refused", "nika.yaml refusé"),
    ("  nika.yaml refused", "  nika.yaml refusé"),
    ("Prepare with: ", "Préparer avec : "),
    ("not selected", "non choisie"),
    // A workflow's faces: the facts above each, the faces' names, the sizes.
    ("Nika workflow", "workflow Nika"),
    ("graph", "graphe"),
    ("check", "vérification"),
    ("1 byte", "1 octet"),
    ("{n} bytes", "{n} octets"),
    ("{n} KB", "{n} Ko"),
    ("{n} MB", "{n} Mo"),
    ("{n} GB", "{n} Go"),
    // What a viewer cut, fell back to or found disagreeing.
    (
        "read the first {read} of {total}; the rest was not read",
        "lu les premiers {read} sur {total} ; le reste n'a pas été lu",
    ),
    (
        "the first 1 line shown; more follow",
        "la première ligne affichée ; d'autres suivent",
    ),
    (
        "the first {n} lines shown; more follow",
        "les {n} premières lignes affichées ; d'autres suivent",
    ),
    (
        "1 line wider than {width} columns, cut at the edge",
        "1 ligne plus large que {width} colonnes, coupée au bord",
    ),
    (
        "{n} lines wider than {width} columns, cut at the edge",
        "{n} lignes plus larges que {width} colonnes, coupées au bord",
    ),
    (
        "1 byte sequence not UTF-8, one {mark} per sequence",
        "1 séquence d'octets hors UTF-8, un {mark} par séquence",
    ),
    (
        "{n} byte sequences not UTF-8, one {mark} per sequence",
        "{n} séquences d'octets hors UTF-8, un {mark} par séquence",
    ),
    (
        "1 control character made visible",
        "1 caractère de contrôle rendu visible",
    ),
    (
        "{n} control characters made visible",
        "{n} caractères de contrôle rendus visibles",
    ),
    (
        "1 value masked: secret-looking in a protected object",
        "1 valeur masquée : d'apparence secrète dans un objet protégé",
    ),
    (
        "{n} values masked: secret-looking in a protected object",
        "{n} valeurs masquées : d'apparence secrète dans un objet protégé",
    ),
    (
        "declared {declared}, but the bytes do not carry its signature",
        "déclaré {declared}, mais les octets n'en portent pas la signature",
    ),
    (
        "declared {declared}, but the bytes are {found}",
        "déclaré {declared}, mais les octets sont {found}",
    ),
    ("shown as {to}: {why}", "affiché comme {to} : {why}"),
    (
        "the object changed since it was recorded",
        "l'objet a changé depuis son enregistrement",
    ),
    (
        "nesting deeper than {levels} keeps the last indent",
        "une imbrication au-delà de {levels} garde le dernier retrait",
    ),
    (
        "the width holds {shown} of {total} columns",
        "la largeur tient {shown} colonnes sur {total}",
    ),
    (
        "rows drawn: the first {shown} of {total}",
        "lignes dessinées : les {shown} premières sur {total}",
    ),
    // The plan face: its waves, its gates, its order.
    ("wave {k}", "vague {k}"),
    ("{size} side by side", "{size} côte à côte"),
    ("human gate", "validation humaine"),
    ("cleanup", "nettoyage"),
    (
        "run order from the check",
        "ordre d'exécution donné par la vérification",
    ),
    (
        "file order: the check found no run order",
        "ordre du fichier : la vérification n'a trouvé aucun ordre d'exécution",
    ),
    (
        "file order: no check ran",
        "ordre du fichier : aucune vérification n'a tourné",
    ),
    // The graph face: why there is none, its counts, its cards.
    (
        "no graph: the parser refused the bytes (see the check face)",
        "pas de graphe : l'analyseur a refusé les octets (voir la face vérification)",
    ),
    (
        "no graph yet: it needs the projection and the check's run order",
        "pas encore de graphe : il lui faut la projection et l'ordre d'exécution de la vérification",
    ),
    (
        "{units} outside task waves",
        "{units} hors des vagues de tâches",
    ),
    ("outside task waves", "hors des vagues de tâches"),
    (
        "cards by wave; exact dependencies listed, no inferred wires",
        "cartes par vague ; dépendances exactes listées, aucun fil déduit",
    ),
    (
        "definition{sep}no task state observed",
        "définition{sep}aucun état de tâche observé",
    ),
    ("definition", "définition"),
    (
        "graph exceeds the view's item bound; no partial topology drawn",
        "le graphe dépasse la limite d'éléments de la vue ; aucune topologie partielle dessinée",
    ),
    (
        "no graph: the check found no valid run order",
        "pas de graphe : la vérification n'a trouvé aucun ordre d'exécution valide",
    ),
    (
        "no graph: the projection and the waves disagree",
        "pas de graphe : la projection et les vagues divergent",
    ),
    (
        "graph exceeds the view's byte bound; no partial topology drawn",
        "le graphe dépasse la limite d'octets de la vue ; aucune topologie partielle dessinée",
    ),
    (
        "no graph: the projection contains duplicate or missing node identities",
        "pas de graphe : la projection contient des identités de nœud en double ou manquantes",
    ),
    (
        "card labels shortened to fit; source and plan retain details",
        "libellés des cartes raccourcis pour tenir ; la source et le plan gardent les détails",
    ),
    // The check face: each layer's reason, the lists, the reminder.
    ("the definition is legal", "la définition est conforme"),
    (
        "the definition breaks a rule: the findings below say which",
        "la définition enfreint une règle : les constats ci-dessous disent laquelle",
    ),
    (
        "every model has a ready path on this machine",
        "chaque modèle a un accès prêt sur cette machine",
    ),
    (
        "a model has no ready path on this machine",
        "un modèle n'a aucun accès prêt sur cette machine",
    ),
    (
        "n/a: no infer or agent task, nothing dials",
        "sans objet : aucune tâche infer ou agent, rien n'appelle",
    ),
    (
        "not judged here: the model arrives at run time and admission judges it",
        "non jugé ici : le modèle arrive à l'exécution et l'admission le juge",
    ),
    (
        "the seats can satisfy the declarations",
        "les sièges peuvent satisfaire les déclarations",
    ),
    (
        "a seat cannot satisfy a declaration: the findings below say which",
        "un siège ne peut satisfaire une déclaration : les constats ci-dessous disent laquelle",
    ),
    (
        "unknown: what this file imports was not captured",
        "inconnu : ce que ce fichier importe n'a pas été capturé",
    ),
    (
        "not captured (UNKNOWN): child workflows, skills, registry references; judged: this file alone",
        "non capturé (UNKNOWN) : workflows enfants, skills, références de registre ; jugé : ce fichier seul",
    ),
    ("; it names {children}", " ; il nomme {children}"),
    (
        "nothing known blocks a run",
        "rien de connu ne bloque un run",
    ),
    (
        "not ready: a layer above says why",
        "pas prêt : une couche ci-dessus dit pourquoi",
    ),
    ("blocked by", "bloqué par"),
    (
        "unknown until admission judges the model",
        "inconnu tant que l'admission n'a pas jugé le modèle",
    ),
    (
        "judged when the workflow is saved",
        "jugé quand le workflow est enregistré",
    ),
    (
        "the parser refused the bytes",
        "l'analyseur a refusé les octets",
    ),
    (
        "not judged: the bytes do not parse",
        "non jugé : les octets ne s'analysent pas",
    ),
    (
        "not judged: nothing checked these bytes yet",
        "non jugé : rien n'a encore vérifié ces octets",
    ),
    ("findings", "constats"),
    ("hints", "conseils"),
    (
        " advisory, never a refusal",
        " à titre indicatif, jamais un refus",
    ),
    ("risk {risk}", "risque {risk}"),
    (
        "a static check{sep}nothing ran, and a green check grants no run",
        "une vérification statique{sep}rien n'a tourné, et une vérification verte n'accorde aucun run",
    ),
    // An opened file's own rows around its faces.
    ("r reads it again", "r le relit"),
    ("not read: {why}", "non lu : {why}"),
    (
        "nothing was judged: no face is drawn over bytes nobody read",
        "rien n'a été jugé : aucune face n'est dessinée sur des octets que personne n'a lus",
    ),
    (
        "This file as last read{sep}Check for details",
        "Ce fichier tel que lu en dernier{sep}détails dans la face vérification",
    ),
    (
        "this file's bytes, as last read",
        "les octets de ce fichier, tels que lus en dernier",
    ),
    (
        "Compact graph{sep}exact dependencies",
        "Graphe compact{sep}dépendances exactes",
    ),
    ("Definition", "Définition"),
    ("Structure checked", "Structure vérifiée"),
    ("Structure needs attention", "Structure à revoir"),
    ("this file only", "ce fichier seul"),
    ("1 finding in Check", "1 constat dans la vérification"),
    ("{n} findings in Check", "{n} constats dans la vérification"),
    // The key cues of a face while the box holds the keys: the key that brings them.
    (
        "F6 then Left/Right change the face",
        "F6 puis Gauche/Droite changent la face",
    ),
    ("F6 then r reads it again", "F6 puis r le relit"),
    // The conversation panel: who the next message goes to, what it carries.
    ("Message to {name}", "Message pour {name}"),
    (
        "Message to {project} / {name}",
        "Message pour {project} / {name}",
    ),
    ("Attached: ", "Joint : "),
    ("viewing {object}", "à l'écran {object}"),
    ("{n} attached", "{n} joint(s)"),
    // The compile record a proposal's check face binds to its bytes.
    (
        "compile record · {mode} · binds these bytes, sha256 {these}",
        "relevé de compilation · {mode} · lie ces octets, sha256 {these}",
    ),
    ("over the base, sha256 {base}", "sur la base, sha256 {base}"),
    ("changed · {parts}", "modifié · {parts}"),
    ("preservation · {how}", "préservation · {how}"),
    ("unversioned", "sans version"),
    ("release {sha}", "publication {sha}"),
    ("file {sha}", "fichier {sha}"),
];

#[cfg(test)]
mod tests;
