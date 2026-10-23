// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The TUI's own words in the language the person writes: English by default
//! and as the fallback, French once the Session reads French in the person's
//! lines (the renderer's `Conversation::language`). Only the TUI's chrome is
//! said here (the hints under the composer, the notices a key leaves, the
//! notes beside the status): the Session's words, the person's words, facts,
//! values, paths and model ids never pass through. A phrase the table does
//! not hold stays English. Under the ASCII glyph column a French phrase loses
//! its accents ([`fold`]), so an ASCII screen stays ASCII; the renderer's own
//! marks (`›`, `·`, `…`, `↑↓`) keep taking their twins where they are painted.

use std::collections::BTreeMap;
use std::sync::OnceLock;

pub use crate::workspace::text::fold;
pub use nika_session::Lang;

/// The language and the glyph column the TUI's own words are said in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Voice {
    /// The language the person writes, as the Session reads it.
    pub lang: Lang,
    /// The ASCII glyph column is in use.
    pub ascii: bool,
}

impl Voice {
    /// The TUI's phrase `en` in this voice: its French when the person writes
    /// French and the table holds it (accents folded under ASCII), else `en`
    /// itself, unchanged.
    #[must_use]
    pub fn say(self, en: &str) -> &str {
        if self.lang != Lang::Fr {
            return en;
        }
        match table().get(en) {
            Some((_, plain)) if self.ascii => plain.as_str(),
            Some((french, _)) => french,
            None => en,
        }
    }

    /// `count` of a unit the TUI counts, in this voice: `1 row`, `3 rows`;
    /// in French `1 ligne`, `3 lignes`, the singular below two as French
    /// writes it. A unit the table of units does not hold takes the English rule.
    #[must_use]
    pub fn count(self, count: usize, unit: &str) -> String {
        let found = UNITS.iter().find(|units| units.0 == unit);
        let word = match found {
            Some(&(_, _, one, many)) if self.lang == Lang::Fr => {
                let word = if count <= 1 { one } else { many };
                if self.ascii {
                    fold(word)
                } else {
                    word.to_owned()
                }
            }
            Some(&(one, many, _, _)) => (if count == 1 { one } else { many }).to_owned(),
            None if count == 1 => unit.to_owned(),
            None => format!("{unit}s"),
        };
        format!("{count} {word}")
    }
}

/// `phrase` with each of its `{name}`s set to its value in one pass: a value
/// set in is never read again, so the person's words keep a brace they hold.
#[must_use]
pub fn fill(phrase: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(phrase.len());
    let mut rest = phrase;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some((name, value)) = values.iter().find(|(name, _)| tail.starts_with(name)) {
            out.push_str(value);
            rest = &tail[name.len()..];
        } else {
            out.push('{');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// The units the TUI counts: English one and many, then French one and many.
const UNITS: &[(&str, &str, &str, &str)] = &[
    ("row", "rows", "ligne", "lignes"),
    ("match", "matches", "résultat", "résultats"),
    (
        "author call",
        "author calls",
        "appel auteur",
        "appels auteur",
    ),
    ("call", "calls", "appel", "appels"),
    (
        "model call",
        "model calls",
        "appel de modèle",
        "appels de modèle",
    ),
    ("step", "steps", "étape", "étapes"),
    (
        "more step not listed",
        "more steps not listed",
        "étape de plus non listée",
        "étapes de plus non listées",
    ),
    ("task", "tasks", "tâche", "tâches"),
    (
        "task ran",
        "tasks ran",
        "tâche exécutée",
        "tâches exécutées",
    ),
    ("skipped", "skipped", "ignorée", "ignorées"),
    ("failed", "failed", "échouée", "échouées"),
    ("not run", "not run", "non exécutée", "non exécutées"),
    (
        "run by its trigger",
        "runs by its trigger",
        "run par son déclencheur",
        "runs par son déclencheur",
    ),
    (
        "fired run",
        "fired runs",
        "run déclenché",
        "runs déclenchés",
    ),
    (
        "slot skipped",
        "slots skipped",
        "créneau sauté",
        "créneaux sautés",
    ),
    (
        "missed slot",
        "missed slots",
        "créneau manqué",
        "créneaux manqués",
    ),
    (
        "missed slot caught up",
        "missed slots caught up",
        "créneau manqué rattrapé",
        "créneaux manqués rattrapés",
    ),
    (
        "run paused at a gate",
        "runs paused at a gate",
        "run en pause à une porte",
        "runs en pause à une porte",
    ),
    ("run failed", "runs failed", "run en échec", "runs en échec"),
    ("model", "models", "modèle", "modèles"),
    ("older", "older", "plus ancien", "plus anciens"),
];

/// The French of the TUI's own phrases, keyed by their exact English, each
/// with its ASCII twin as the renderer writes it under ASCII (a notice stored
/// in that column is found too): the accented phrase and its folded form.
fn table() -> &'static BTreeMap<String, (&'static str, String)> {
    static TABLE: OnceLock<BTreeMap<String, (&'static str, String)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = BTreeMap::new();
        for &(en, french) in FR {
            table.insert(en.to_owned(), (french, fold(french)));
        }
        for &(en, french) in FR {
            let twin = crate::workspace::text::own(en, true);
            table.entry(twin).or_insert_with(|| (french, fold(french)));
        }
        table
    })
}

/// The TUI's own phrases in French, keyed by their exact English. Key names
/// a French keyboard writes otherwise take its words (`Entrée`, `Échap`,
/// `Espace`); every other key keeps its name (`Ctrl+O`, `F2`, `↑↓`, `PgUp`,
/// `End`), and so does every word the Session reads when typed (`oui`, `non`
/// and `annuler` are its own; `details`, `save & run` and the slash commands
/// stay as they are typed). A `{…}` stands for a value set in after.
const FR: &[(&str, &str)] = &[
    // The request a typed choice keeps beside it (`retained_rows`).
    ("Request · ", "Demande · "),
    ("Open · ", "Encore ouvert · "),
    // What waits: its whole hint (`Waiting::hint`).
    (
        "describe work · /help · Run: run <file>.nika",
        "décrivez un travail · /help · lancer : run <fichier>.nika",
    ),
    (
        "connect an AI · /providers: connections · /intelligence: choose · /help",
        "connectez une IA · /providers : connexions · /intelligence : choisir · /help",
    ),
    (
        "↑↓ choose · Enter applies · type to filter · Esc back",
        "↑↓ choisir · Entrée applique · tapez pour filtrer · Échap revient",
    ),
    (
        "↑↓ choose · Enter applies · type to filter",
        "↑↓ choisir · Entrée applique · tapez pour filtrer",
    ),
    (
        "↑↓ choose · Enter applies · type to filter · Esc keeps current",
        "↑↓ choisir · Entrée applique · tapez pour filtrer · Échap garde l'actuel",
    ),
    (
        "↑↓ choose · Enter answers · or type your answer",
        "↑↓ choisir · Entrée répond · ou tapez votre réponse",
    ),
    (
        "↑↓ choose · Enter acts · or describe a change",
        "↑↓ choisir · Entrée valide · ou décrivez un changement",
    ),
    (
        "↑↓ choose or type yes / no · Enter acts · F2: full context",
        "↑↓ choisir ou tapez oui / non · Entrée valide · F2 : tout le contexte",
    ),
    (
        "↑↓ choose · Enter acts · F2: full context",
        "↑↓ choisir · Entrée valide · F2 : tout le contexte",
    ),
    (
        "yes approves once · no or Ctrl+C cancels · details shows the full evidence",
        "oui approuve une fois · non ou Ctrl+C annule · details montre toute la preuve",
    ),
    (
        "↑↓ move · Space toggles · Ctrl+A all · Enter validates all checked",
        "↑↓ déplacer · Espace coche · Ctrl+A tout · Entrée valide les réponses cochées",
    ),
    (
        "↑↓ choose · Enter answers · or type your reply · cancel discards the proposal",
        "↑↓ choisir · Entrée répond · ou tapez votre réponse · annuler écarte la proposition",
    ),
    (
        "↑↓ choose · Enter answers · or type your reply · cancel drops it",
        "↑↓ choisir · Entrée répond · ou tapez votre réponse · annuler l'abandonne",
    ),
    (
        "Enter answers · cancel discards the proposal",
        "Entrée répond · annuler écarte la proposition",
    ),
    (
        "Enter answers · cancel drops it",
        "Entrée répond · annuler l'abandonne",
    ),
    (
        "↑↓ choose · Enter acts · cancel drops your message",
        "↑↓ choisir · Entrée valide · annuler abandonne votre message",
    ),
    (
        "answer the question above · cancel to stop",
        "répondez à la question ci-dessus · annuler pour arrêter",
    ),
    (
        "yes + Enter: Save · no: cancel · /show: inspect",
        "oui + Entrée : enregistrer · non : annuler · /show : examiner",
    ),
    (
        "approve or refuse · nothing else answers a gate",
        "approuvez ou refusez · rien d'autre ne répond à une validation",
    ),
    // Its narrower forms, a whole cue shorter each.
    (
        "yes approves once · no or Ctrl+C cancels",
        "oui approuve une fois · non ou Ctrl+C annule",
    ),
    (
        "yes approves once · no cancels",
        "oui approuve une fois · non annule",
    ),
    ("↑↓ choose · Enter acts", "↑↓ choisir · Entrée valide"),
    ("Enter acts", "Entrée valide"),
    (
        "↑↓ choose · Enter applies · Esc back",
        "↑↓ choisir · Entrée applique · Échap revient",
    ),
    (
        "Enter applies · Esc back",
        "Entrée applique · Échap revient",
    ),
    ("↑↓ choose · Enter applies", "↑↓ choisir · Entrée applique"),
    ("Enter applies", "Entrée applique"),
    (
        "↑↓ choose · Enter applies · Esc keeps current",
        "↑↓ choisir · Entrée applique · Échap garde l'actuel",
    ),
    (
        "Enter applies · Esc keeps current",
        "Entrée applique · Échap garde l'actuel",
    ),
    (
        "Space toggles · Enter validates all checked",
        "Espace coche · Entrée valide les réponses cochées",
    ),
    (
        "Space toggles · Enter validates",
        "Espace coche · Entrée valide",
    ),
    (
        "↑↓ choose · Enter answers · cancel discards it",
        "↑↓ choisir · Entrée répond · annuler l'écarte",
    ),
    (
        "Enter answers · cancel discards it",
        "Entrée répond · annuler l'écarte",
    ),
    ("Enter answers", "Entrée répond"),
    (
        "↑↓ choose · Enter answers · cancel drops it",
        "↑↓ choisir · Entrée répond · annuler l'abandonne",
    ),
    (
        "Enter acts · cancel drops it",
        "Entrée valide · annuler l'abandonne",
    ),
    (
        "answer the question above",
        "répondez à la question ci-dessus",
    ),
    (
        "yes + Enter: Save · no: cancel",
        "oui + Entrée : enregistrer · non : annuler",
    ),
    ("approve or refuse", "approuvez ou refusez"),
    (
        "connect an AI · /providers: connections",
        "connectez une IA · /providers : connexions",
    ),
    ("/providers: connections", "/providers : connexions"),
    // The reviewed proposal's every consent word, longest first.
    (
        "save & run + Enter: Save, then Run once · yes: Save only · no: discard · /show: bytes",
        "save & run + Entrée : enregistrer, puis lancer une fois · oui : enregistrer seulement · non : écarter · /show : octets",
    ),
    (
        "save & run: Save, then Run once · yes: Save only · no: discard · /show: bytes",
        "save & run : enregistrer, puis lancer une fois · oui : enregistrer seulement · non : écarter · /show : octets",
    ),
    (
        "save & run · yes: Save only · no: discard · /show: bytes",
        "save & run · oui : enregistrer seulement · non : écarter · /show : octets",
    ),
    (
        "save & run · yes: Save only · no: discard",
        "save & run · oui : enregistrer seulement · non : écarter",
    ),
    (
        "save & run · yes: save · no: discard",
        "save & run · oui : enregistrer · non : écarter",
    ),
    // What `Enter` does at a decision, its act set in.
    (
        "Enter acts: {act} · ↑↓ choose · Esc clears",
        "Entrée : {act} · ↑↓ choisir · Échap efface",
    ),
    ("Enter acts: {act}", "Entrée : {act}"),
    // The free prompt, the workspace's composer and the work under way.
    (
        "Preparing: Ctrl+C requests Stop; correction + Enter. Run: typing waits.",
        "Préparation : Ctrl+C demande l'arrêt ; correction + Entrée. Run : la saisie attend.",
    ),
    ("Ctrl+O: commands", "Ctrl+O : commandes"),
    (
        "click chat; End: latest · /intelligence · Ctrl+O: commands",
        "cliquez dans la conversation ; End : le plus récent · /intelligence · Ctrl+O : commandes",
    ),
    (
        "click chat; End: latest · /intelligence",
        "cliquez dans la conversation ; End : le plus récent · /intelligence",
    ),
    (
        "End: latest · /intelligence",
        "End : le plus récent · /intelligence",
    ),
    (
        "Enter send · Alt+Enter new line · Ctrl+O: commands",
        "Entrée envoie · Alt+Entrée nouvelle ligne · Ctrl+O : commandes",
    ),
    (
        "Enter send · Ctrl+O: commands",
        "Entrée envoie · Ctrl+O : commandes",
    ),
    (
        "Ask, change, or run… / commands",
        "Demandez, modifiez ou lancez… / commandes",
    ),
    ("Your message", "Votre message"),
    ("Your answer", "Votre réponse"),
    // An exit armed, and where the keys are.
    (
        "Ctrl+C again leaves · any key stays",
        "Ctrl+C encore quitte · autre touche reste",
    ),
    (
        "keys: {place} · typing writes in the conversation · Esc returns",
        "touches : {place} · la saisie écrit dans la conversation · Échap revient",
    ),
    (
        "keys: {place} · Esc returns",
        "touches : {place} · Échap revient",
    ),
    ("keys: {place}", "touches : {place}"),
    ("the project list", "la liste du projet"),
    ("the object in view", "l'objet affiché"),
    // The presentation's own note beside the status.
    (
        "focus · Esc returns inline · PgUp/PgDn scroll",
        "focus · Échap revient en mode inline · PgUp/PgDn défile",
    ),
    (
        "workspace needs 60x16 · Esc inline · PgUp/PgDn scroll",
        "l'espace de travail demande 60x16 · Échap : inline · PgUp/PgDn défile",
    ),
    (
        "workspace · F6 panel · Esc back",
        "espace de travail · F6 panneau · Échap revient",
    ),
    (
        " · reading earlier messages · End: latest",
        " · lecture des messages précédents · End : le plus récent",
    ),
    // A proposal's standing while it waits.
    (
        "Not saved yet · yes means Save only",
        "Pas encore enregistré · oui veut dire enregistrer seulement",
    ),
    (
        "Not saved yet · this proposal has not run",
        "Pas encore enregistré · cette proposition n'a pas été lancée",
    ),
    ("Not saved yet", "Pas encore enregistré"),
    // The names a card and a bubble wear.
    ("You", "Vous"),
    ("Review before saving", "À relire avant d'enregistrer"),
    ("Approval needed", "Approbation requise"),
    ("Could not continue", "Impossible de continuer"),
    ("Result", "Résultat"),
    ("Report", "Rapport"),
    ("Activity", "Activité"),
    ("Proposal", "Proposition"),
    // A band's title and facts, a question's header and the cue to its whole words.
    ("Decision", "Décision"),
    ("Knowledge", "Connaissances"),
    ("Proposal choice", "Choix de la proposition"),
    (" · required", " · obligatoire"),
    (" · your message waits", " · votre message attend"),
    (
        " · {checked} of {all} selected",
        " · sélection : {checked} sur {all}",
    ),
    (
        " · {listed} of {all} match",
        " · résultats : {listed} sur {all}",
    ),
    (" · {from}-{to} of {listed}", " · {from}-{to} sur {listed}"),
    ("Offered answers", "Réponses proposées"),
    ("Several answers", "Plusieurs réponses"),
    ("A text answer", "Une réponse en texte"),
    ("An exact value", "Une valeur exacte"),
    ("… the whole question: F2", "… toute la question : F2"),
    // A band's own rows.
    ("Submit", "Valider"),
    (
        "Space checks answers, Enter validates them",
        "Espace coche les réponses, Entrée les valide",
    ),
    (
        "Enter validates the checked answers",
        "Entrée valide les réponses cochées",
    ),
    ("Let Nika choose", "Laisser Nika choisir"),
    (
        "Nika takes the recommended answer",
        "Nika prend la réponse recommandée",
    ),
    (
        "Nika chooses and says what it chose",
        "Nika choisit et dit ce qu'il a choisi",
    ),
    ("recommended", "recommandé"),
    (
        "no row matches {open}{query}{close} · {then}",
        "aucune ligne ne correspond à {open}{query}{close} · {then}",
    ),
    ("Enter sends it as typed", "Entrée l'envoie tel quel"),
    (
        "Enter keeps it as your message until you choose",
        "Entrée le garde comme votre message jusqu'à votre choix",
    ),
    (
        "Enter sends it as a message · intelligence unchanged",
        "Entrée l'envoie comme message · intelligence inchangée",
    ),
    // The keys overlay (`?`): its headings, then each key it names.
    ("While Nika works", "Pendant que Nika travaille"),
    ("Intelligence list", "Liste des intelligences"),
    ("Always", "Toujours"),
    (
        "Enter · send what you wrote",
        "Entrée · envoyer ce que vous avez écrit",
    ),
    ("Alt+Enter · a new line", "Alt+Entrée · une nouvelle ligne"),
    (
        "↑↓ · earlier lines, at the edges of the box",
        "↑↓ · les lignes précédentes, aux bords de la zone de saisie",
    ),
    (
        "/ · the commands, as you type one",
        "/ · les commandes, pendant que vous en tapez une",
    ),
    (
        "Tab · the commands, in an empty box",
        "Tab · les commandes, dans une zone de saisie vide",
    ),
    (
        "Home End · the first and the latest messages (Ctrl+Home over words typed)",
        "Home End · le premier et le dernier message (Ctrl+Home sur des mots tapés)",
    ),
    (
        "↑↓ PgUp PgDn Home End · choose a row",
        "↑↓ PgUp PgDn Home End · choisir une ligne",
    ),
    (
        "Enter · apply the selected row",
        "Entrée · appliquer la ligne sélectionnée",
    ),
    (
        "type · filter the rows; a sentence no row matches is sent as a message",
        "tapez · filtrer les lignes ; une phrase sans ligne correspondante part comme message",
    ),
    (
        "Esc · clear the filter, then go back or keep the current choice",
        "Échap · effacer le filtre, puis revenir ou garder le choix actuel",
    ),
    (
        "1, 2 or 3 with a name, 4, /intelligence … · the expert route, typed",
        "1, 2 ou 3 avec un nom, 4, /intelligence … · la route experte, tapée",
    ),
    (
        "↑↓ PgUp PgDn Home End · choose an answer",
        "↑↓ PgUp PgDn Home End · choisir une réponse",
    ),
    (
        "1-9 · select that answer, while nothing is typed",
        "1-9 · sélectionner cette réponse, tant que rien n'est tapé",
    ),
    (
        "Enter · answer with your own words, else the selection",
        "Entrée · répondre avec vos mots, sinon la sélection",
    ),
    (
        "Esc · clear your words, then the selection; it never answers",
        "Échap · effacer vos mots, puis la sélection ; ne répond jamais",
    ),
    (
        "Space · check or uncheck the selected answer",
        "Espace · cocher ou décocher la réponse sélectionnée",
    ),
    (
        "Ctrl+A · check every answer, or none when all are",
        "Ctrl+A · tout cocher, ou rien quand tout l'est",
    ),
    (
        "Enter · validate every checked answer",
        "Entrée · valider chaque réponse cochée",
    ),
    (
        "↑↓ · choose an act; Enter does it, once",
        "↑↓ · choisir un acte ; Entrée le fait, une fois",
    ),
    (
        "Esc · never answers a decision",
        "Échap · ne répond jamais à une décision",
    ),
    (
        "F2 · the decision's whole words",
        "F2 · tous les mots de la décision",
    ),
    (
        "1-9 · select that act, while nothing is typed; Enter does it",
        "1-9 · sélectionner cet acte, tant que rien n'est tapé ; Entrée le fait",
    ),
    (
        "Ctrl+C · stop or interrupt; twice leaves",
        "Ctrl+C · arrêter ou interrompre ; deux fois quitte",
    ),
    (
        "Ctrl+O · every command and view key",
        "Ctrl+O · toutes les commandes et touches de vue",
    ),
    (
        "Ctrl+C · stop the preparation; again leaves",
        "Ctrl+C · arrêter la préparation ; encore quitte",
    ),
    (
        "Ctrl+C · a Run is never stopped here; twice leaves",
        "Ctrl+C · un Run ne s'arrête jamais ici ; deux fois quitte",
    ),
    (
        "type · your words wait in the box",
        "tapez · vos mots attendent dans la zone de saisie",
    ),
    (
        "Enter · words as a correction to the preparation",
        "Entrée · les mots comme correction de la préparation",
    ),
    (
        "PgUp PgDn · scroll, on a full screen",
        "PgUp PgDn · défiler, en plein écran",
    ),
    (
        "Let Nika choose · leave the choice to Nika, in words",
        "Laisser Nika choisir · laisser le choix à Nika, en mots",
    ),
    // The view keys the overlay names, and the full-words reader's frame.
    ("Expand the object", "Agrandir l'objet"),
    ("Restore the object", "Rétablir l'objet"),
    ("Next panel", "Panneau suivant"),
    ("Earlier messages", "Messages précédents"),
    ("Back to the latest message", "Retour au dernier message"),
    ("Full question", "Question complète"),
    ("Full proposal", "Proposition complète"),
    ("Full diagnostic", "Diagnostic complet"),
    ("Full run story", "Histoire complète du run"),
    ("Back inline", "Retour en mode inline"),
    ("Full screen", "Plein écran"),
    ("Redraw the screen", "Redessiner l'écran"),
    ("New conversation", "Nouvelle conversation"),
    (
        "Begin a new conversation",
        "Commencer une nouvelle conversation",
    ),
    (
        "no decision waiting · this project",
        "aucune décision en attente · ce projet",
    ),
    (
        "Keeps this conversation and begins an empty one; workflows, schedules and runs carry over.",
        "Garde cette conversation et en commence une vide ; workflows, planifications et runs continuent.",
    ),
    ("Show the welcome again", "Revoir l'accueil"),
    ("any time · this screen", "à tout moment · cet écran"),
    (
        "Its steps and the example it offers return as the object in view; nothing is sent.",
        "Ses étapes et l'exemple qu'il propose reviennent à l'écran ; rien n'est envoyé.",
    ),
    ("The keys of this screen", "Les touches de cet écran"),
    ("Keys", "Touches"),
    ("for what is on screen now", "pour ce qui est à l'écran"),
    (
        "the Session's words, read only",
        "les mots de la Session, en lecture seule",
    ),
    (
        " ↑↓ PgUp/PgDn scroll · Esc/Enter returns ",
        " ↑↓ PgUp/PgDn défiler · Échap/Entrée revient ",
    ),
    // The intelligence picker: its marks, rows, lists and guides.
    ("current", "en cours"),
    ("default", "par défaut"),
    ("requested", "demandé"),
    ("legacy", "ancien"),
    ("{tokens} context", "{tokens} de contexte"),
    ("key required", "clé requise"),
    (
        "{in} in / {out} out per MTok",
        "{in} en entrée / {out} en sortie par MTok",
    ),
    ("{out} out per MTok", "{out} en sortie par MTok"),
    ("{in} in per MTok", "{in} en entrée par MTok"),
    (
        "from {prices} (≤{tokens} tokens)",
        "dès {prices} (≤{tokens} tokens)",
    ),
    ("in your plan", "dans votre forfait"),
    ("no metered cost", "aucun coût au compteur"),
    ("price unknown", "prix inconnu"),
    ("{via} API", "API {via}"),
    ("{via} agent account", "compte agent {via}"),
    ("{via} on this machine", "{via} sur cette machine"),
    ("cannot write workflows here", "n'écrit pas de workflow ici"),
    ("Agent account", "Compte agent"),
    ("newest {family}", "{family} le plus récent"),
    ("key: {var}", "clé : {var}"),
    ("Older models ({older})…", "Modèles plus anciens ({older})…"),
    (
        "earlier generations, dated snapshots and legacy models",
        "générations précédentes, instantanés datés et modèles anciens",
    ),
    ("Configure API", "Configurer l'API"),
    ("Choose a model", "Choisir un modèle"),
    ("All connections", "Toutes les connexions"),
    ("Test the key", "Tester la clé"),
    // What the person's bubble says when they leave a choice to Nika.
    ("I leave the choice to Nika.", "Je laisse le choix à Nika."),
    // What the status row says before the Session's own status, beside a retained run.
    ("Latest reply above", "Dernière réponse ci-dessus"),
    ("Latest turn refused", "Dernière demande refusée"),
    ("Preparation stopped", "Préparation arrêtée"),
    // What a key check the person asked for observed, in one plain line.
    ("valid", "valide"),
    ("refused ({status})", "refusée ({status})"),
    ("unreachable", "injoignable"),
    (
        "key accepted by {name} · it listed its models, no model ran",
        "clé acceptée par {name} · il a listé ses modèles, aucun modèle n'a tourné",
    ),
    (
        "key refused by {name} ({status}) · check the key or choose another connection",
        "clé refusée par {name} ({status}) · vérifiez la clé ou choisissez une autre connexion",
    ),
    (
        "{name} could not be reached · {why}",
        "{name} injoignable · {why}",
    ),
    (
        "key not judged by {name} · {why}",
        "clé non jugée par {name} · {why}",
    ),
    ("key not judged by {name}", "clé non jugée par {name}"),
    (
        "key refused by {name} ({status}) · this choice is not applied · check the key or choose another connection",
        "clé refusée par {name} ({status}) · ce choix n'est pas appliqué · vérifiez la clé ou choisissez une autre connexion",
    ),
    (
        "the key check of {name} could not run · {why}",
        "la vérification de la clé de {name} n'a pas pu se faire · {why}",
    ),
    (
        "one free request, no model runs",
        "une requête gratuite, aucun modèle ne tourne",
    ),
    ("Older models · {name}", "Modèles plus anciens · {name}"),
    ("Current models", "Modèles actuels"),
    ("Choose an AI connection", "Choisir une connexion IA"),
    (
        "Recommended for you · {name}",
        "Recommandé pour vous · {name}",
    ),
    ("this conversation's", "celui de cette conversation"),
    ("Keep current · {name}", "Garder l'actuel · {name}"),
    (
        "closes this list, nothing changed",
        "ferme cette liste, rien ne change",
    ),
    ("Other choice…", "Autre choix…"),
    (
        "every connection and model",
        "chaque connexion et chaque modèle",
    ),
    ("Author: {name}", "Auteur : {name}"),
    (
        "change · writes and repairs your workflows",
        "changer · écrit et répare vos workflows",
    ),
    ("Decision: {name}", "Décision : {name}"),
    ("Run: {name}", "Run : {name}"),
    ("the provider's model", "le modèle du fournisseur"),
    ("unavailable here", "indisponible ici"),
    ("none yet", "aucun pour l'instant"),
    ("change · typed decisions", "changer · décisions typées"),
    (
        "change · unavailable: {why}",
        "changer · indisponible : {why}",
    ),
    ("the author decides alone", "l'auteur décide seul"),
    (
        "change · no decision seat is consulted",
        "changer · aucun siège de décision n'est consulté",
    ),
    (
        "change · requested for the next workflows",
        "changer · demandé pour les prochains workflows",
    ),
    (
        "change · this workflow's model",
        "changer · le modèle de ce workflow",
    ),
    ("set by each workflow", "fixé par chaque workflow"),
    (
        "change · the model of the AI steps",
        "changer · le modèle des étapes IA",
    ),
    ("already the default", "déjà le réglage par défaut"),
    (
        "new conversations start with {name}",
        "les nouvelles conversations commencent avec {name}",
    ),
    ("Set as default", "Définir par défaut"),
    ("Test the key · {name}", "Tester la clé · {name}"),
    (
        "Intelligence for this conversation",
        "Intelligence de cette conversation",
    ),
    ("unavailable: {why}", "indisponible : {why}"),
    (
        "unavailable: no decision seat is configured here",
        "indisponible : aucun siège de décision n'est configuré ici",
    ),
    ("Jev (typed decisions)", "Jev (décisions typées)"),
    ("Author decides alone", "L'auteur décide seul"),
    (
        "no decision seat is consulted",
        "aucun siège de décision n'est consulté",
    ),
    (
        "Who makes the typed decisions",
        "Qui prend les décisions typées",
    ),
    ("No ready model here", "Aucun modèle prêt ici"),
    ("back home", "retour à l'accueil"),
    (
        "Model for the AI steps of the next workflows",
        "Modèle des étapes IA des prochains workflows",
    ),
    (
        "Choose an intelligence below.",
        "Choisissez une intelligence ci-dessous.",
    ),
    (
        "Choose an intelligence below. A message typed here waits for your choice.",
        "Choisissez une intelligence ci-dessous. Un message tapé ici attend votre choix.",
    ),
    (
        "This conversation needs an AI connection. Connect an account or configure a provider, then reopen Nika. /providers shows the current connections.",
        "Cette conversation a besoin d'une connexion IA. Connectez un compte ou configurez un fournisseur, puis rouvrez Nika. /providers montre les connexions actuelles.",
    ),
    (
        "Choose a connection below. No selection has changed.",
        "Choisissez une connexion ci-dessous. Aucune sélection n'a changé.",
    ),
    (
        "Choose who makes this conversation's typed decisions. Nothing changes until you choose.",
        "Choisissez qui prend les décisions typées de cette conversation. Rien ne change avant votre choix.",
    ),
    (
        "Choose the model the author uses for the AI steps of the workflows written from now on. Workflows already written do not change.",
        "Choisissez le modèle que l'auteur utilise pour les étapes IA des workflows écrits désormais. Les workflows déjà écrits ne changent pas.",
    ),
    (
        "Older models of this connection: earlier generations, dated snapshots and legacy models. No selection has changed.",
        "Modèles plus anciens de cette connexion : générations précédentes, instantanés datés et modèles anciens. Aucune sélection n'a changé.",
    ),
    (
        "Configure the API key named below, then reopen Nika. No selection has changed.",
        "Configurez la clé d'API nommée ci-dessous, puis rouvrez Nika. Aucune sélection n'a changé.",
    ),
    (
        "Choose a model below. No selection has changed.",
        "Choisissez un modèle ci-dessous. Aucune sélection n'a changé.",
    ),
    // A run card's name, and the facts beside the stage line (its stages are the Session's).
    ("Run", "Exécution"),
    (
        "~{amount} spent since opening",
        "~{amount} dépensés depuis l'ouverture",
    ),
    ("? for shortcuts", "? pour les raccourcis"),
    // The live line while a turn works, and how a run or a preparation ended.
    ("Preparing", "Préparation"),
    ("Working", "Travail en cours"),
    ("working", "travail en cours"),
    ("Ctrl+C stop", "Ctrl+C arrête"),
    ("Ctrl+C twice leaves", "Ctrl+C deux fois quitte"),
    (
        "● reading what the run left",
        "● lecture de ce que le run a laissé",
    ),
    ("Running {file}", "Exécution de {file}"),
    (
        "paused · a gate asks you",
        "en pause · une validation vous attend",
    ),
    ("~{spent} + {n} unpriced", "~{spent} + {n} sans prix"),
    ("{done}/{all} tasks", "{done}/{all} tâches"),
    ("now: {id} ({model})", "en cours : {id} ({model})"),
    ("now: {id}", "en cours : {id}"),
    ("{spent} of {ceiling}", "{spent} sur {ceiling}"),
    (
        "Ctrl+C again leaves now · the call cannot be recalled · {base} · {secs}s",
        "Ctrl+C encore quitte maintenant · l'appel ne peut pas être rappelé · {base} · {secs}s",
    ),
    ("Ran", "Exécuté"),
    ("Ran in", "Exécuté en"),
    ("Failed", "Échoué"),
    ("Failed after", "Échoué après"),
    ("Cancelled", "Annulé"),
    ("Cancelled after", "Annulé après"),
    ("Settled", "Terminé"),
    ("Settled after", "Terminé après"),
    ("({cached} from cache)", "({cached} depuis le cache)"),
    ("Prepared in", "Préparé en"),
    ("Stopped after", "Arrêté après"),
    ("failed", "échouée"),
    ("failed: {why}", "échouée : {why}"),
    ("skipped", "ignorée"),
    ("not run", "non exécutée"),
    // A decision's question and acts, as this host builds them beside the Session's.
    (
        "What should happen to this reviewed proposal?",
        "Que faire de cette proposition relue ?",
    ),
    ("Save workflow", "Enregistrer le workflow"),
    ("Save these changes", "Enregistrer ces changements"),
    ("Save and run once", "Enregistrer et lancer une fois"),
    ("Discard this proposal", "Écarter cette proposition"),
    (
        "Inspect the exact changes",
        "Examiner les changements exacts",
    ),
    ("Continue this run once?", "Continuer ce run une fois ?"),
    ("Run once", "Lancer une fois"),
    ("Cancel this run", "Annuler ce run"),
    (
        "Continue this unpriced request once?",
        "Continuer cette demande sans prix une fois ?",
    ),
    ("Continue once", "Continuer une fois"),
    ("Cancel this request", "Annuler cette demande"),
    (
        "Read the full cost evidence",
        "Lire toute la preuve de coût",
    ),
    (
        "Answer this workflow confirmation",
        "Répondez à cette confirmation du workflow",
    ),
    ("Approve this step", "Approuver cette étape"),
    ("Refuse this step", "Refuser cette étape"),
    ("Explain this step", "Expliquer cette étape"),
    (
        "Choose the answer for this workflow step",
        "Choisissez la réponse de cette étape du workflow",
    ),
    (
        "Choose a value for this input",
        "Choisissez une valeur pour cette entrée",
    ),
    (
        "Choose a value for {name} ({kind})",
        "Choisissez une valeur pour {name} ({kind})",
    ),
    (
        "This candidate still needs qualification",
        "Ce candidat doit encore être qualifié",
    ),
    ("Retry qualification", "Réessayer la qualification"),
    (
        "Discard the unqualified candidate",
        "Écarter le candidat non qualifié",
    ),
    (
        "The saved workflow or the files it reads changed since its rehearsal: it was not run",
        "Le workflow enregistré ou les fichiers qu'il lit ont changé depuis sa répétition : il n'a pas été lancé",
    ),
    (
        "Adapt the workflow to the changed files",
        "Adapter le workflow aux fichiers modifiés",
    ),
    (
        "Answer this schedule question",
        "Répondez à cette question de planification",
    ),
    ("Cancel schedule setup", "Annuler la planification"),
    (
        "Choose this run's ceiling",
        "Choisissez le plafond de ce run",
    ),
    (
        "Expired: a fresh question is needed before approval.",
        "Expirée : une nouvelle question est nécessaire avant l'approbation.",
    ),
    // A candidate's changes, how it was revised and the run it asked, as this host words them.
    ("line", "ligne"),
    ("lines", "lignes"),
    ("creates {path} ({count})", "crée {path} ({count})"),
    (
        "replaces {path} ({count}) · over its bytes as read",
        "remplace {path} ({count}) · sur ses octets tels que lus",
    ),
    (
        "revised in place · no node changed",
        "révisé sur place · aucun nœud changé",
    ),
    (
        "revised in place · {changed}",
        "révisé sur place · {changed}",
    ),
    ("created · written whole", "créé · écrit en entier"),
    (
        "created · composed from components",
        "créé · composé à partir de composants",
    ),
    // What a revision changed and what the bytes show of a component, in words.
    ("step {name}", "étape {name}"),
    ("value {name}", "valeur {name}"),
    ("input {name}", "entrée {name}"),
    ("output {name}", "sortie {name}"),
    ("component {name}", "composant {name}"),
    ("the permissions", "les autorisations"),
    ("the model", "le modèle"),
    ("the run settings", "les réglages du run"),
    ("reused as bound", "réutilisé tel que lié"),
    ("called as bound", "appelé tel que lié"),
    ("reused, then changed", "réutilisé, puis modifié"),
    ("no longer in these bytes", "absent de ces octets"),
    ("unreadable in these bytes", "illisible dans ces octets"),
    ("not checked in these bytes", "non vérifié dans ces octets"),
    (
        "rewritten whole · no preservation of the earlier bytes is claimed",
        "réécrit en entier · aucune préservation des octets précédents n'est revendiquée",
    ),
    (
        "document record · mode not described",
        "enregistrement du document · mode non décrit",
    ),
    ("component", "composant"),
    (
        "asked run · {workflow} once",
        "run demandé · {workflow} une fois",
    ),
    (" · ceiling ${stated}", " · plafond ${stated}"),
    (" · inputs {names}", " · entrées {names}"),
    ("nothing outside the process", "rien hors du processus"),
    // The prompt beside the line, and the placeholder of an own reply's field.
    ("choice › ", "choix › "),
    ("reply › ", "réponse › "),
    ("Save? › ", "Enregistrer ? › "),
    ("answer › ", "réponse › "),
    ("or type your own reply", "ou tapez votre propre réponse"),
    ("or type yes or no", "ou tapez oui ou non"),
    (
        "choose an offered answer",
        "choisissez une réponse proposée",
    ),
    ("type to filter", "tapez pour filtrer"),
    // The palette and the slash list: each command's and view key's words (names as typed).
    (
        "What you can ask, and every command",
        "Ce que vous pouvez demander, et chaque commande",
    ),
    ("any time · reads only", "à tout moment · lecture seule"),
    (
        "The Session's help card: work to describe, questions answered without AI, each command.",
        "La carte d'aide de la Session : le travail à décrire, les questions répondues sans IA, chaque commande.",
    ),
    ("Where you are", "Où vous en êtes"),
    (
        "Project root, intelligence and where your context goes, authoring seat, any reasoning effort.",
        "Racine du projet, intelligence et destination de votre contexte, siège d'écriture, effort de raisonnement éventuel.",
    ),
    (
        "What the waiting answer is for",
        "À quoi sert la réponse attendue",
    ),
    (
        "beside a question or a gate · reads only",
        "à côté d'une question ou d'une validation · lecture seule",
    ),
    (
        "What the question or gate on screen decides and what it lets happen; it answers nothing.",
        "Ce que décide la question ou la validation à l'écran et ce qu'elle permet ; elle ne répond à rien.",
    ),
    (
        "What Nika kept of your request",
        "Ce que Nika a retenu de votre demande",
    ),
    (
        "after a request · reads only",
        "après une demande · lecture seule",
    ),
    (
        "Your request clause by clause, from the compiler's own ledger; a proposal keeps waiting.",
        "Votre demande clause par clause, d'après le registre du compilateur ; une proposition continue d'attendre.",
    ),
    (
        "What the last run's trace proves",
        "Ce que prouve la trace du dernier run",
    ),
    ("after a run · reads only", "après un run · lecture seule"),
    (
        "Chain, seal, boundary and task hashes as nika trace verify judges them, and their limits.",
        "Chaîne, sceau, limite et empreintes des tâches tels que nika trace verify les juge, et leurs limites.",
    ),
    (
        "How the last workflow was built",
        "Comment le dernier workflow a été construit",
    ),
    (
        "after a build · reads only",
        "après une construction · lecture seule",
    ),
    (
        "Authoring model, calls, tokens and time, any explicit reasoning effort, decision seat, engine.",
        "Modèle d'écriture, appels, jetons et durée, effort de raisonnement explicite éventuel, siège de décision, moteur.",
    ),
    (
        "The proposal's exact bytes",
        "Les octets exacts de la proposition",
    ),
    (
        "while a proposal waits · reads only",
        "pendant qu'une proposition attend · lecture seule",
    ),
    (
        "The exact bytes a yes would save; the proposal keeps waiting.",
        "Les octets exacts qu'un oui enregistrerait ; la proposition continue d'attendre.",
    ),
    (
        "Choose the AI this session reasons with",
        "Choisir l'IA avec laquelle cette session raisonne",
    ),
    (
        "this session · choose from the list",
        "cette session · choisir dans la liste",
    ),
    (
        "Browse the AI connections and models available to this session.",
        "Parcourir les connexions et les modèles d'IA disponibles pour cette session.",
    ),
    (
        "Bring back what your last session kept",
        "Rappeler ce que votre dernière session a gardé",
    ),
    (
        "kept from your last session",
        "gardé de votre dernière session",
    ),
    (
        "The kept preparation or proposal, for a fresh review; no AI asked, nothing written until yes.",
        "La préparation ou la proposition gardée, pour un nouvel examen ; aucune IA sollicitée, rien n'est écrit avant un oui.",
    ),
    ("Close the session", "Fermer la session"),
    ("this session", "cette session"),
    (
        "Leaves Nika; nothing waiting for your answer is applied.",
        "Quitte Nika ; rien de ce qui attend votre réponse n'est appliqué.",
    ),
    (
        "A command this conversation answers",
        "Une commande à laquelle cette conversation répond",
    ),
    ("/help describes it", "/help la décrit"),
    (
        "Send it alone on a line; the Session's help card says what it does.",
        "Envoyez-la seule sur une ligne ; la carte d'aide de la Session dit ce qu'elle fait.",
    ),
    (
        "current question · read only",
        "question actuelle · lecture seule",
    ),
    (
        "The question at your answer line, in the Session's own words; nothing is sent.",
        "La question de votre ligne de réponse, dans les mots de la Session ; rien n'est envoyé.",
    ),
    (
        "current proposal · read only",
        "proposition actuelle · lecture seule",
    ),
    (
        "The current proposal in the Session's own words; nothing is sent, saved or run.",
        "La proposition actuelle dans les mots de la Session ; rien n'est envoyé, enregistré ni lancé.",
    ),
    (
        "summarized refusal · read only",
        "refus résumé · lecture seule",
    ),
    (
        "The Session's own words of the summarized refusal at your reading position; nothing is sent.",
        "Les mots de la Session pour le refus résumé à votre position de lecture ; rien n'est envoyé.",
    ),
    ("folded run · read only", "run replié · lecture seule"),
    (
        "The whole story of the folded run at your reading position; nothing is sent or run.",
        "Toute l'histoire du run replié à votre position de lecture ; rien n'est envoyé ni lancé.",
    ),
    (
        "Gives the object more room; the conversation keeps its composer and your draft; nothing is sent.",
        "Donne plus de place à l'objet ; la conversation garde sa zone de saisie et votre brouillon ; rien n'est envoyé.",
    ),
    (
        "Gives the conversation its room back; the object and your draft stay; nothing is sent.",
        "Rend sa place à la conversation ; l'objet et votre brouillon restent ; rien n'est envoyé.",
    ),
    ("workspace · view only", "espace de travail · vue seule"),
    (
        "Moves the keys to the project list, the conversation or the preview; Shift+F6 goes back.",
        "Déplace les touches vers la liste du projet, la conversation ou l'aperçu ; Shift+F6 revient.",
    ),
    ("full screen · view only", "plein écran · vue seule"),
    (
        "Moves the conversation a page back, PgDn a page forward; new messages keep your place.",
        "Recule la conversation d'une page, PgDn avance d'une page ; les nouveaux messages gardent votre place.",
    ),
    ("when scrolled back · view only", "en remontant · vue seule"),
    (
        "Returns the conversation to its newest row.",
        "Ramène la conversation à sa ligne la plus récente.",
    ),
    (
        "the draft stays · view only",
        "le brouillon reste · vue seule",
    ),
    (
        "Returns to the inline view, whose finished lines stay in your scrollback; the draft stays.",
        "Revient à la vue inline, dont les lignes finies restent dans votre historique ; le brouillon reste.",
    ),
    (
        "Opens the workspace, or the focus view on a small terminal; the draft stays.",
        "Ouvre l'espace de travail, ou la vue focus sur un petit terminal ; le brouillon reste.",
    ),
    ("view only", "vue seule"),
    (
        "Paints every cell again, for a terminal that lost part of the screen.",
        "Repeint chaque cellule, pour un terminal qui a perdu une partie de l'écran.",
    ),
    (
        "with nothing typed · read only",
        "sans rien de tapé · lecture seule",
    ),
    (
        "Lists the keys of what is on screen now and the ones that always hold; Esc returns.",
        "Liste les touches de ce qui est à l'écran et celles qui valent toujours ; Échap revient.",
    ),
    // The chooser's own words: what waits, its keys, its search and a draft set aside.
    (
        "Nika is working · a command waits in the box until your turn",
        "Nika travaille · une commande attend dans la zone de saisie jusqu'à votre tour",
    ),
    (
        "A decision waits · choose an action with the arrow keys",
        "Une décision attend · choisissez une action avec les flèches",
    ),
    (
        "A proposal waits · no command answers it · yes + Enter: Save",
        "Une proposition attend · aucune commande n'y répond · oui + Entrée : enregistrer",
    ),
    (
        "A gate waits · no command answers it · approve or refuse",
        "Une validation attend · aucune commande n'y répond · approuvez ou refusez",
    ),
    (
        "The intelligence choice waits · no command answers it",
        "Le choix de l'intelligence attend · aucune commande n'y répond",
    ),
    (
        "Your message waits · /knowledge embedded resumes it · cancel drops it",
        "Votre message attend · /knowledge embedded le reprend · annuler l'abandonne",
    ),
    (
        "A cost decision waits · no command approves it · only your yes does",
        "Une décision de coût attend · aucune commande ne l'approuve · seul votre oui le fait",
    ),
    (
        "A question waits · no command answers it · your reply does",
        "Une question attend · aucune commande n'y répond · votre réponse le fait",
    ),
    (
        "↑↓ choose · Enter keeps it for your turn · Esc: back to your draft",
        "↑↓ choisir · Entrée la garde pour votre tour · Échap : retour à votre brouillon",
    ),
    (
        "Enter keeps it for your turn · Esc: your draft",
        "Entrée la garde pour votre tour · Échap : votre brouillon",
    ),
    (
        "↑↓ choose · Enter runs · Tab inserts · Esc: back to your draft",
        "↑↓ choisir · Entrée lance · Tab insère · Échap : retour à votre brouillon",
    ),
    (
        "↑↓ choose · Enter runs · Tab inserts · Esc",
        "↑↓ choisir · Entrée lance · Tab insère · Échap",
    ),
    (
        "Enter runs · Tab inserts · Esc",
        "Entrée lance · Tab insère · Échap",
    ),
    (
        "{name} waits for your turn · ↑↓ choose · Esc hides the list",
        "{name} attend votre tour · ↑↓ choisir · Échap masque la liste",
    ),
    (
        "{name} waits for your turn · Esc hides",
        "{name} attend votre tour · Échap masque",
    ),
    (
        "Enter sends {name} · ↑↓ choose · Esc hides the list",
        "Entrée envoie {name} · ↑↓ choisir · Échap masque la liste",
    ),
    (
        "Enter sends {name} · Esc hides",
        "Entrée envoie {name} · Échap masque",
    ),
    (
        "↑↓ choose · Enter keeps it for your turn · Esc hides the list",
        "↑↓ choisir · Entrée la garde pour votre tour · Échap masque la liste",
    ),
    (
        "Enter keeps it for your turn · Esc hides",
        "Entrée la garde pour votre tour · Échap masque",
    ),
    (
        "↑↓ choose · Enter runs · Tab inserts · Esc hides the list",
        "↑↓ choisir · Entrée lance · Tab insère · Échap masque la liste",
    ),
    (
        "Enter runs · Tab inserts · Esc hides",
        "Entrée lance · Tab insère · Échap masque",
    ),
    ("commands › ", "commandes › "),
    ("Commands", "Commandes"),
    (" · nothing matches", " · aucun résultat"),
    (" · choose an action", " · choisissez une action"),
    (
        " · beginning with what you typed",
        " · commençant par ce que vous avez tapé",
    ),
    (
        "nothing matches {open}{query}{close} · Backspace edits the search",
        "rien ne correspond à {open}{query}{close} · Retour arrière corrige la recherche",
    ),
    (
        "set aside: {open}{start}{close} · back once this line is sent · Esc: now",
        "mis de côté : {open}{start}{close} · revient une fois cette ligne envoyée · Échap : maintenant",
    ),
    // The workspace's aside, as this host lists it: this conversation, earlier runs, notes.
    ("this conversation", "cette conversation"),
    (
        "{label} {workflow} · earlier",
        "{label} {workflow} · précédent",
    ),
    (
        "The file listing needs a Session contract: the workspace reads no file itself.",
        "La liste des fichiers demande un contrat de la Session : l'espace de travail ne lit aucun fichier lui-même.",
    ),
    (
        "no project is known to this conversation",
        "aucun projet n'est connu de cette conversation",
    ),
    (
        "No workflow in {name} yet",
        "Pas encore de workflow dans {name}",
    ),
    (
        "none found in the listed part",
        "aucun trouvé dans la partie listée",
    ),
    ("Files", "Fichiers"),
    // The run in view: its standing, the facts above its graph, its tabs,
    // the chips its graph's tasks wear and the list of its tasks.
    (
        "earlier session · exit {exit}",
        "session antérieure · sortie {exit}",
    ),
    ("settled · succeeded", "terminé · réussi"),
    ("settled · failed", "terminé · échoué"),
    ("settled · cancelled", "terminé · annulé"),
    ("settled · {state}", "terminé · {state}"),
    ("refused · {why}", "refusé · {why}"),
    (
        "refused before its start{sep}{why}{sep}nothing ran",
        "refusé avant son début{sep}{why}{sep}rien n'a tourné",
    ),
    (
        "stream{sep}its refusal alone, before any frame",
        "flux{sep}son refus seul, avant toute trame",
    ),
    ("ended · no frame received", "fini · aucune trame reçue"),
    (
        "ended · no settlement received",
        "fini · aucune clôture reçue",
    ),
    (
        "asked · waiting for its first frame",
        "demandé · en attente de sa première trame",
    ),
    ("a fresh run", "un nouveau run"),
    ("a resumed leg", "une reprise"),
    ("reaches, as declared", "atteint, comme déclaré"),
    (
        "again{sep}after {label} of this workflow ({words}){sep}a run does every effect it declares again",
        "encore{sep}après {label} de ce workflow ({words}){sep}un run refait chaque effet qu'il déclare",
    ),
    (
        "graph{sep}the bytes {witness} shown when it was asked, until the run names its own",
        "graphe{sep}les octets {witness} affichés à la demande, jusqu'à ce que le run nomme les siens",
    ),
    (
        "graph{sep}the bytes {witness} it was asked over, as the run names them",
        "graphe{sep}les octets {witness} sur lesquels il a été demandé, tels que le run les nomme",
    ),
    (
        "graph not bound{sep}the run names other bytes than {witness} (the file changed since)",
        "graphe non lié{sep}le run nomme d'autres octets que {witness} (le fichier a changé depuis)",
    ),
    (
        "graph not bound{sep}the run named no source hash, or nothing was read when it was asked",
        "graphe non lié{sep}le run n'a nommé aucune empreinte de source, ou rien n'a été lu à la demande",
    ),
    (
        "graph not bound{sep}the run's start was not observed",
        "graphe non lié{sep}le début du run n'a pas été observé",
    ),
    (
        "story only{sep}this runner tells the run's story, never its frames: the run is not followed here, the transcript keeps the story",
        "récit seul{sep}ce lanceur raconte le run, jamais ses trames : le run n'est pas suivi ici, la transcription garde le récit",
    ),
    ("sealed", "scellée"),
    ("unsealed", "non scellée"),
    ("lost", "perdue"),
    ("no journal", "aucun journal"),
    ("not declared yet", "pas encore déclarée"),
    (", as the run declared it", ", comme le run l'a déclarée"),
    ("reason not reported", "raison non signalée"),
    (
        "evidence{sep}{evidence}{declared}{why}{sep}the proof is the trace's own verification",
        "attestation{sep}{evidence}{declared}{why}{sep}la preuve est la vérification propre de la trace",
    ),
    (
        "kept from an earlier session{sep}HOME history recorded it{sep}nothing was replayed",
        "gardé d'une session antérieure{sep}l'historique HOME l'a enregistré{sep}rien n'a été rejoué",
    ),
    (
        "asked{sep}no frame received before its turn ended",
        "demandé{sep}aucune trame reçue avant la fin de son tour",
    ),
    (
        "asked{sep}no frame yet{sep}no run identity yet",
        "demandé{sep}aucune trame encore{sep}aucune identité de run encore",
    ),
    (
        "no settlement received{sep}the runtime's own end frame is not a settlement",
        "aucune clôture reçue{sep}la trame de fin du runtime n'est pas une clôture",
    ),
    (
        "no settlement received{sep}its turn ended{sep}{n} task(s) seen",
        "aucune clôture reçue{sep}son tour est fini{sep}{n} tâche(s) vue(s)",
    ),
    (
        "running{sep}{n} task(s) seen",
        "en cours{sep}{n} tâche(s) vue(s)",
    ),
    ("settlement{sep}{cause}", "clôture{sep}{cause}"),
    ("{ok}/{total} tasks ok", "{ok}/{total} tâches ok"),
    (
        "stream{sep}not followed in this session",
        "flux{sep}non suivi dans cette session",
    ),
    (
        "stream{sep}{n} events and the settlement, whole",
        "flux{sep}{n} événements et la clôture, entiers",
    ),
    ("no settlement", "aucune clôture"),
    ("{n} lost on the way", "{n} perdus en route"),
    ("{n} unread", "{n} non lus"),
    (
        "{n} of another run, set aside",
        "{n} d'un autre run, mis de côté",
    ),
    (
        "{n} past the fold's bound",
        "{n} au-delà de la limite du repli",
    ),
    (
        "{n} after the settlement, set aside",
        "{n} après la clôture, mis de côté",
    ),
    ("{n} out of order", "{n} hors d'ordre"),
    (
        "stream incomplete{sep}{missing}{sep}{n} events folded",
        "flux incomplet{sep}{missing}{sep}{n} événements repliés",
    ),
    (
        "journal {trace}{sep}captured bytes {short}{sep}its tasks as it recorded them",
        "journal {trace}{sep}octets capturés {short}{sep}ses tâches telles qu'il les a enregistrées",
    ),
    (
        "Click this run in the project list to read and verify its saved journal.",
        "Cliquez sur ce run dans la liste du projet pour lire et vérifier son journal enregistré.",
    ),
    ("outputs", "sorties"),
    ("files", "fichiers"),
    ("proof", "preuve"),
    ("scheduled", "planifiée"),
    ("cache hit (resume)", "depuis le cache (reprise)"),
    ("succeeded after recovery", "réussie après récupération"),
    ("succeeded", "réussie"),
    ("retrying", "nouvel essai"),
    ("cancelled", "annulée"),
    ("paused", "en pause"),
    ("no end received", "aucune fin reçue"),
    ("settled ok", "terminée ok"),
    (
        "tasks · ↑↓ pick · Enter details",
        "tâches · ↑↓ choisir · Entrée détails",
    ),
    (
        "tasks · F6 then ↑↓ pick · Enter details",
        "tâches · F6 puis ↑↓ choisir · Entrée détails",
    ),
    ("cleanup (finally)", "nettoyage (finally)"),
    ("not observed in this run", "non observée dans ce run"),
    // A declared schedule's ◐ on the rail: declared only, or a firer standing, not fired yet.
    ("declared", "déclaré"),
    ("waiting", "en attente"),
    // What a declared schedule's trigger decided, in the conversation and the list.
    (
        "`{workflow}` ran on its own: {fired}",
        "`{workflow}` a tourné tout seul : {fired}",
    ),
    (
        "last slot {slot} · exit {exit}",
        "dernier créneau {slot} · sortie {exit}",
    ),
    ("last slot {slot} skipped", "dernier créneau {slot} sauté"),
    (
        "last slot {slot} skipped: {why}",
        "dernier créneau {slot} sauté : {why}",
    ),
    (
        "last slot {slot} paused: a gate asks you",
        "dernier créneau {slot} en pause : une porte vous demande",
    ),
    (
        "last slot {slot} failed · exit {exit}",
        "dernier créneau {slot} en échec · sortie {exit}",
    ),
    (" · trace {trace}", " · sa trace {trace}"),
    ("{caught} at {decided}", "{caught} à {decided}"),
    // Why a slot was skipped, as the ledger's reason says it.
    ("a run was still going", "un run tournait encore"),
    ("its queue waited too long", "sa file a trop attendu"),
    ("suspended in nika.yaml", "suspendu dans nika.yaml"),
    ("past its end date", "après sa date de fin"),
    ("run in the cloud", "exécuté dans le cloud"),
    ("it fires on its event", "il se déclenche sur son événement"),
    ("nika serve stopped", "nika serve s'est arrêté"),
    ("already decided", "déjà décidé"),
    ("not due", "pas encore dû"),
    (
        "its trigger was removed from this Mac",
        "son déclencheur a été retiré de ce Mac",
    ),
    (
        "`{workflow}`: its trigger's ledger cannot be read, so its runs cannot be proved: {why}",
        "`{workflow}` : le journal de son déclencheur est illisible, ses runs ne peuvent pas être prouvés : {why}",
    ),
    // The task detail and the run's outputs, files and proof faces.
    (
        "task {id}{sep}Backspace: back to the tasks",
        "tâche {id}{sep}Retour arrière : retour aux tâches",
    ),
    (
        "{id}{sep}not observed in this run: the bound graph declares it, no frame of this run named it",
        "{id}{sep}non observée dans ce run : le graphe lié la déclare, aucune trame de ce run ne l'a nommée",
    ),
    ("scheduled, not started", "planifiée, pas commencée"),
    ("running, not finished", "en cours, pas terminée"),
    (
        "succeeded from the cache of an earlier leg (resume)",
        "réussie depuis le cache d'un tronçon antérieur (reprise)",
    ),
    (
        "an attempt failed, a retry is scheduled",
        "une tentative a échoué, un nouvel essai est prévu",
    ),
    (
        "skipped: its condition was false",
        "ignorée : sa condition était fausse",
    ),
    (
        "cancelled: it never ran to its end",
        "annulée : elle n'est jamais allée au bout",
    ),
    (
        "paused: a gate asks a human",
        "en pause : une porte demande à un humain",
    ),
    (
        "started, no end of it received before its run ended",
        "commencée, sa fin n'a pas été reçue avant la fin de son run",
    ),
    (
        "succeeded, as the run's settlement counts every task ok; its own end frame was not received",
        "réussie, la clôture du run compte chaque tâche ok ; sa propre trame de fin n'a pas été reçue",
    ),
    (
        "repaired by its on_error recovery",
        "réparée par sa reprise on_error",
    ),
    (
        "as its journal recorded it",
        "telle que son journal l'a enregistrée",
    ),
    ("as the stream folded it", "telle que le flux l'a repliée"),
    (
        "static facts unknown{sep}the graph is not bound to this run",
        "faits statiques inconnus{sep}le graphe n'est pas lié à ce run",
    ),
    (
        "static facts unknown{sep}the bound graph does not declare it",
        "faits statiques inconnus{sep}le graphe lié ne la déclare pas",
    ),
    (
        "cleanup (finally){sep}runs when {parents} unwinds, never as a main task",
        "nettoyage (finally){sep}s'exécute quand {parents} se termine, jamais comme tâche principale",
    ),
    ("for_each {kind} of {n}", "for_each {kind} sur {n}"),
    ("retry {n} attempts", "retry {n} tentatives"),
    (
        "declared{sep}{facts}{sep}the bytes the run names",
        "déclarée{sep}{facts}{sep}les octets que le run nomme",
    ),
    (
        "not finished: no output yet",
        "pas terminée : pas encore de sortie",
    ),
    (
        "no output {medium}: the task failed",
        "aucune sortie {medium} : la tâche a échoué",
    ),
    (
        "no output {medium}: it never ran to its end",
        "aucune sortie {medium} : elle n'est jamais allée au bout",
    ),
    (
        "no output {medium}: its frame carried none (a value withheld as secret, or an older engine)",
        "aucune sortie {medium} : sa trame n'en portait pas (une valeur retenue comme secrète, ou un moteur plus ancien)",
    ),
    ("output{sep}{why}", "sortie{sep}{why}"),
    ("in its journal", "dans son journal"),
    ("on the stream", "sur le flux"),
    ("output", "sortie"),
    ("items", "éléments"),
    ("input", "entrée"),
    ("cache_read", "lecture du cache"),
    ("cache_write", "écriture du cache"),
    ("reasoning", "raisonnement"),
    ("{label}{sep}task {id}", "{label}{sep}tâche {id}"),
    (
        "{name}{sep}{n} bytes {medium}",
        "{name}{sep}{n} octets {medium}",
    ),
    (
        "spend not recorded on its frames",
        "dépense non enregistrée sur ses trames",
    ),
    ("spend {usd}", "dépense {usd}"),
    ("model {m}", "modèle {m}"),
    ("{t} tokens", "{t} jetons"),
    ("{n} wire attempts", "{n} tentatives réseau"),
    ("unknown", "inconnu"),
    ("waited {ms} ms after {on}", "attente de {ms} ms après {on}"),
    ("measured{sep}{parts}", "mesuré{sep}{parts}"),
    ("why{sep}{value}", "pourquoi{sep}{value}"),
    ("warning{sep}{value}", "avertissement{sep}{value}"),
    (
        "untrusted value{sep}born at {value}",
        "valeur non fiable{sep}née à {value}",
    ),
    ("started as{sep}{value}", "commencée ainsi{sep}{value}"),
    ("note{sep}{value}", "remarque{sep}{value}"),
    (
        "definition {def}{sep}inputs {input}",
        "définition {def}{sep}entrées {input}",
    ),
    (
        "Image report unreadable; no file verified",
        "Rapport d'images illisible ; aucun fichier vérifié",
    ),
    (
        "Showing {shown} of {observed} image observations; the rest are in the trace",
        "{shown} observations d'images affichées sur {observed} ; le reste est dans la trace",
    ),
    ("no count", "aucun compte"),
    (
        "Image evidence incomplete: observed {observed}, terminal reports {expected}",
        "Preuves d'images incomplètes : {observed} observées, la fin en annonce {expected}",
    ),
    ("MIME unknown", "MIME inconnu"),
    (
        "{size} bytes received · stored blob {hash}",
        "{size} octets reçus · blob stocké {hash}",
    ),
    (
        "{size} bytes received · storage failed",
        "{size} octets reçus · stockage en échec",
    ),
    (
        "{size} bytes received · storage unconfirmed (no store answer observed)",
        "{size} octets reçus · stockage non confirmé (aucune réponse du stockage observée)",
    ),
    (
        "reported path only; no image data received",
        "chemin annoncé seulement ; aucune donnée d'image reçue",
    ),
    (" · reported path {path}", " · chemin annoncé {path}"),
    (
        "Image reported · {mime} · harness source · {received}{path} · file not verified; permission evidence is separate",
        "Image annoncée · {mime} · source du harnais · {received}{path} · fichier non vérifié ; la preuve d'autorisation est à part",
    ),
    (
        "child run{sep}no relation on its frames: nothing to open (the producer keeps none when the child fails)",
        "run enfant{sep}aucune relation sur ses trames : rien à ouvrir (le producteur n'en garde aucune quand l'enfant échoue)",
    ),
    ("success", "succès"),
    ("failure", "échec"),
    ("none recognized", "aucune reconnue"),
    (
        "child run{sep}{target}{sep}as this task's settle frame names it",
        "run enfant{sep}{target}{sep}tel que la trame de clôture de cette tâche le nomme",
    ),
    ("none", "aucun"),
    (
        "named{sep}journal {trace}{sep}head {head}{sep}source {source}{sep}outcome {named}",
        "nommé{sep}journal {trace}{sep}tête {head}{sep}source {source}{sep}issue {named}",
    ),
    ("Enter: open its journal", "Entrée : ouvrir son journal"),
    (
        "the frame names no journal: nothing to open",
        "la trame ne nomme aucun journal : rien à ouvrir",
    ),
    (
        "outcome not recognized: nothing is opened on it",
        "issue non reconnue : rien n'est ouvert dessus",
    ),
    ("its journal lends nothing", "son journal ne prête rien"),
    (
        "nothing it records is shown: {why}",
        "rien de ce qu'il enregistre n'est montré : {why}",
    ),
    ("its journal {trace}", "son journal {trace}"),
    (
        "its journal holds no terminal frame: its outputs are unknown",
        "son journal ne contient aucune trame de fin : ses sorties sont inconnues",
    ),
    ("its settlement", "sa clôture"),
    (
        "no settlement yet: the outputs are unknown, never empty",
        "pas encore de clôture : les sorties sont inconnues, jamais vides",
    ),
    (
        "the outputs map held {bytes} bytes: only its size was recorded, not its payload",
        "la table des sorties pesait {bytes} octets : seule sa taille a été enregistrée, pas son contenu",
    ),
    (
        "the outputs map was withheld whole when the run closed: neither its keys nor its values were recorded",
        "la table des sorties a été retenue en entier à la fermeture du run : ni ses clés ni ses valeurs n'ont été enregistrées",
    ),
    (
        "the outputs map recorded here cannot be read: it is not shown",
        "la table des sorties enregistrée ici est illisible : elle n'est pas montrée",
    ),
    (
        "this journal records no workflow outputs map (an older engine, or a close that kept none): unknown, never empty",
        "ce journal n'enregistre aucune table des sorties du workflow (un moteur plus ancien, ou une fermeture qui n'en a gardé aucune) : inconnue, jamais vide",
    ),
    (
        "the settlement carried no outputs",
        "la clôture ne portait aucune sortie",
    ),
    (
        "{whence} records an empty outputs map",
        "{whence} enregistre une table des sorties vide",
    ),
    (
        "its journal reports writing no file (nika:write)",
        "son journal n'annonce l'écriture d'aucun fichier (nika:write)",
    ),
    (
        "the run reported writing no file (nika:write); the trace holds its other effects",
        "le run n'a annoncé l'écriture d'aucun fichier (nika:write) ; la trace garde ses autres effets",
    ),
    (
        "{path}{sep}reported written by `{task}`",
        "{path}{sep}annoncé écrit par `{task}`",
    ),
    ("not read yet", "pas encore lu"),
    (
        "read now · the run left no digest of the bytes it wrote",
        "lu maintenant · le run n'a laissé aucune empreinte des octets qu'il a écrits",
    ),
    ("not read{sep}{why}", "non lu{sep}{why}"),
    ("these bytes, read now", "ces octets, lus maintenant"),
    (
        "the journal is captured and verified when this face opens",
        "le journal est capturé et vérifié quand cette face s'ouvre",
    ),
    (
        "no settlement yet: there is no journal to verify",
        "pas encore de clôture : il n'y a aucun journal à vérifier",
    ),
    (
        "journal {trace}{sep}as the settlement named it",
        "journal {trace}{sep}tel que la clôture l'a nommé",
    ),
    ("not verified{sep}{why}", "non vérifié{sep}{why}"),
    (
        "this run's journal{sep}its execution, source and receipt match",
        "le journal de ce run{sep}son exécution, sa source et son reçu concordent",
    ),
    (
        "not bound to this run{sep}no badge",
        "non lié à ce run{sep}aucun badge",
    ),
    (
        "chain {shape}{sep}{events} events{sep}head {head}",
        "chaîne {shape}{sep}{events} événements{sep}tête {head}",
    ),
    (
        "none: the journal holds no terminal frame",
        "aucune : le journal ne contient aucune trame de fin",
    ),
    (
        "terminal frame{sep}{terminal}",
        "trame de fin{sep}{terminal}",
    ),
    (
        "a verified journal records what happened; it never proves the work was right",
        "un journal vérifié enregistre ce qui s'est passé ; il ne prouve jamais que le travail était juste",
    ),
    (
        "captured bytes {short}{sep}the verdict below is theirs",
        "octets capturés {short}{sep}le verdict ci-dessous est le leur",
    ),
    (
        "verdict{sep}{tier}{sep}exit {exit}{sep}the verifier's, over these bytes",
        "verdict{sep}{tier}{sep}sortie {exit}{sep}celui du vérificateur, sur ces octets",
    ),
    // The run's story, as the busy row and the activity card hear it.
    ("running · {workflow}", "en cours · {workflow}"),
    ("↺ {task} · from the cache", "↺ {task} · depuis le cache"),
    ("· {task} skipped", "· {task} ignorée"),
    ("· {task} cancelled", "· {task} annulée"),
    (
        "◇ paused · `{task}` asks you",
        "◇ en pause · `{task}` vous attend",
    ),
    (
        "{status} · {ok}/{total} tasks · {ms} ms",
        "{status} · {ok}/{total} tâches · {ms} ms",
    ),
    // The notices that quote the person's draft or correction.
    (
        "your draft {draft}{sep}it is in the box, not sent",
        "votre brouillon {draft}{sep}il est dans la zone de saisie, non envoyé",
    ),
    (
        "your draft {draft}{sep}cleared: this cost question takes only an answer typed after it{sep}kept in this conversation, whole:\n{text}",
        "votre brouillon {draft}{sep}effacé : cette question de coût ne prend qu'une réponse tapée après elle{sep}gardé dans cette conversation, en entier :\n{text}",
    ),
    (
        "Nika keeps working · {command} waits for your turn",
        "Nika continue · {command} attend votre tour",
    ),
    (
        "correction queued {draft}{sep}the preparation stops, then it is sent",
        "correction en attente {draft}{sep}la préparation s'arrête, puis elle est envoyée",
    ),
    (
        "correction replaced (the earlier one stays in history) {draft}{sep}the preparation stops, then it is sent",
        "correction remplacée (la précédente reste dans l'historique) {draft}{sep}la préparation s'arrête, puis elle est envoyée",
    ),
    (
        "your correction was not sent{sep}the question above takes only an answer typed after it{sep}Up recalls it, whole:\n{draft}",
        "votre correction n'a pas été envoyée{sep}la question ci-dessus ne prend qu'une réponse tapée après elle{sep}Haut la rappelle, en entier :\n{draft}",
    ),
    (
        "the answer above came before your correction {draft}{sep}it is in the box, not sent",
        "la réponse ci-dessus est arrivée avant votre correction {draft}{sep}elle est dans la zone de saisie, non envoyée",
    ),
    ("stop requested by you", "arrêt demandé par vous"),
    (
        "this proposal is unchanged since your last message",
        "cette proposition n'a pas changé depuis votre dernier message",
    ),
    // What an earlier session kept, told apart from this one.
    (
        "earlier in this conversation · kept in your history · nothing is replayed",
        "plus tôt dans cette conversation · gardé dans votre historique · rien n'est rejoué",
    ),
    ("earlier context · {said}", "contexte antérieur · {said}"),
    (
        "last run, observed in an earlier session · {id} of `{workflow}` · exit {exit} · its proof is read again from its journal when you open it · nothing replays",
        "dernier run, observé dans une session antérieure · {id} de `{workflow}` · sortie {exit} · sa preuve est relue dans son journal quand vous l'ouvrez · rien n'est rejoué",
    ),
    (
        "the last run's record is unreadable ({why}) · kept unchanged · nothing replays",
        "l'enregistrement du dernier run est illisible ({why}) · gardé tel quel · rien n'est rejoué",
    ),
    (
        "last run {glyph} exit {exit} in an earlier session",
        "dernier run {glyph} sortie {exit} dans une session antérieure",
    ),
    (
        "last run of `{workflow}` {glyph} exit {exit} in an earlier session",
        "dernier run de `{workflow}` {glyph} sortie {exit} dans une session antérieure",
    ),
    ("{rail} (earlier {glyph})", "{rail} (précédent {glyph})"),
    // The object's own action at the end of its first row.
    ("[+] Expand · F4", "[+] Agrandir · F4"),
    ("[-] Restore · F4", "[-] Restaurer · F4"),
    // The activity card a turn keeps: its heading, its phases and its call row.
    ("Understanding", "Compréhension"),
    ("Choosing knowledge", "Choix des connaissances"),
    ("Generating", "Génération"),
    ("Checking", "Vérification"),
    ("Repairing", "Réparation"),
    ("Stopped by you", "Arrêté par vous"),
    ("Not completed", "Non terminé"),
    ("took {took}", "en {took}"),
    ("under 1 s", "moins d'1 s"),
    ("1 author call", "1 appel auteur"),
    ("{n} author calls", "{n} appels auteur"),
    (", {n} for repair", ", {n} pour réparation"),
    (", 1 stopped", ", 1 arrêté"),
    (", {n} stopped", ", {n} arrêtés"),
    (
        "conversation routing and decision-service calls not counted",
        "routage de la conversation et appels du service de décision non comptés",
    ),
    (
        "1 earlier updates omitted",
        "1 mise à jour antérieure omise",
    ),
    (
        "{n} earlier updates omitted",
        "{n} mises à jour antérieures omises",
    ),
    ("reported {n} times", "signalé {n} fois"),
    ("call {n}", "appel {n}"),
    ("requested {model}", "demandé {model}"),
    ("running", "en cours"),
    ("returned after {took}", "revenu après {took}"),
    ("returned", "revenu"),
    (
        "stopped, a charge may be unknown",
        "arrêté, un coût peut rester inconnu",
    ),
    ("plan the workflow", "planifier le workflow"),
    ("revise the workflow", "réviser le workflow"),
    ("write a step", "écrire une étape"),
    (
        "write a data transformation",
        "écrire une transformation de données",
    ),
    ("review your request", "relire votre demande"),
    (
        "review a part of your request",
        "relire une partie de votre demande",
    ),
    (
        "find the step a missing part points to",
        "trouver l'étape que désigne une partie manquante",
    ),
    (
        "check one part against the trial run",
        "vérifier une partie sur l'essai",
    ),
    ("review a trial run", "relire un essai"),
    ("review the workflow", "relire le workflow"),
    ("repair a step", "réparer une étape"),
    (
        "repair a data transformation",
        "réparer une transformation de données",
    ),
    ("repair the workflow", "réparer le workflow"),
    ("answer you", "vous répondre"),
    ("read your request", "lire votre demande"),
    (
        "check its reading of your request",
        "vérifier sa lecture de votre demande",
    ),
    ("write the workflow", "écrire le workflow"),
    // A conversation tool's step, in words rather than its name.
    ("reading a project file", "lecture d'un fichier du projet"),
    ("reading the draft", "lecture du brouillon"),
    ("writing the draft", "écriture du brouillon"),
    ("editing the draft", "modification du brouillon"),
    ("checking the draft", "vérification du brouillon"),
    (
        "reading the draft's steps",
        "lecture des étapes du brouillon",
    ),
    ("explaining a finding", "explication d'un constat"),
    ("looking up the language", "consultation du langage"),
    ("listing the models", "liste des modèles"),
    (
        "searching Nika's knowledge",
        "recherche dans les connaissances de Nika",
    ),
    ("fetching a web address", "lecture d'une adresse web"),
    (
        "composing from components",
        "composition à partir de composants",
    ),
    ("verifying the draft", "vérification complète du brouillon"),
    ("trying the draft", "essai du brouillon"),
    ("asking you", "question pour vous"),
    ("proposing the workflow", "proposition du workflow"),
    (
        "taking up your new request",
        "reprise de votre nouvelle demande",
    ),
    ("{tool} failed", "{tool} : échec"),
    // A run the person stopped at a gate with their « no »: never a success.
    (
        "settled · stopped at your no",
        "terminé · arrêté à votre « non »",
    ),
    ("Stopped at your no", "Arrêté à votre « non »"),
    ("Stopped at your no after", "Arrêté à votre « non » après"),
    ("answered no", "répondu non"),
    (
        "answered no: the run stopped there",
        "répondu non : le run s'est arrêté là",
    ),
    // A tool step's failure a later attempt of the same tool recovered.
    ("retried", "réessayé"),
    // The notices a key leaves on the hint row.
    (
        "nothing sent · ↑↓ choose an answer, or type your own reply",
        "rien n'est envoyé · ↑↓ choisissez une réponse, ou tapez la vôtre",
    ),
    (
        "not taken · answer text restored for editing",
        "non retenu · le texte de la réponse revient pour être modifié",
    ),
    (
        "not taken · your choice is still selected",
        "non retenu · votre choix reste sélectionné",
    ),
    (
        "not taken · original text stays in the conversation",
        "non retenu · le texte d'origine reste dans la conversation",
    ),
    (
        "Discard this proposal? Enter again · Esc keeps it",
        "Écarter cette proposition ? Entrée encore · Échap la garde",
    ),
    (
        "Discard the unqualified candidate? Enter again · Esc keeps it",
        "Écarter le candidat non qualifié ? Entrée encore · Échap le garde",
    ),
    (
        "kept · nothing was discarded",
        "gardé · rien n'a été écarté",
    ),
    (
        "nothing sent · Esc never answers · ↑↓ choose, then Enter",
        "rien n'est envoyé · Échap ne répond jamais · ↑↓ choisissez, puis Entrée",
    ),
    // Reading the conversation behind a card that covers it.
    ("PgUp: history", "PgUp : historique"),
    (
        "PgUp/PgDn scroll · any other key: back to the card",
        "PgUp/PgDn défilent · autre touche : retour à la carte",
    ),
    (
        "nothing sent · Esc never answers · PgUp: history",
        "rien n'est envoyé · Échap ne répond jamais · PgUp : historique",
    ),
    // A decision's card left with Esc (NIK-227): what stays undecided.
    (
        "set aside · `{workflow}` stays not run: its files changed since its rehearsal · say what you want next",
        "mise de côté · `{workflow}` reste non lancé : ses fichiers ont changé depuis sa répétition · dites ce que vous voulez ensuite",
    ),
    (
        "set aside · nothing was answered · the run stays paused until you answer \"{label}\"",
        "mise de côté · rien n'a été répondu · le run reste en pause jusqu'à votre réponse à « {label} »",
    ),
    (
        "set aside · nothing was answered · \"{label}\" still waits for you",
        "mise de côté · rien n'a été répondu · « {label} » vous attend toujours",
    ),
    (
        "an intelligence is required · ↑↓ choose, then Enter",
        "une intelligence est requise · ↑↓ choisissez, puis Entrée",
    ),
    (
        "nothing sent · Space checks an answer, then Enter validates",
        "rien n'est envoyé · Espace coche une réponse, puis Entrée valide",
    ),
    (
        "nothing sent · type yes or no, or ↑↓ choose, then Enter",
        "rien n'est envoyé · tapez oui ou non, ou ↑↓ choisissez, puis Entrée",
    ),
    (
        "Enter kept · your request follows this read · Ctrl+C cancels",
        "Entrée gardée · votre demande suit cette lecture · Ctrl+C annule",
    ),
    (
        "Nika is working · Enter sends when it is your turn",
        "Nika travaille · Entrée envoie quand c'est votre tour",
    ),
    (
        "Nika is working · Esc leaves when it is your turn",
        "Nika travaille · Échap quitte quand c'est votre tour",
    ),
    (
        "stopping the preparation · your conversation stays · Ctrl+C again leaves",
        "arrêt de la préparation · votre conversation reste · Ctrl+C encore quitte",
    ),
    (
        "a Run is under way and is not stopped here · Ctrl+C again leaves",
        "un Run est en cours et ne s'arrête pas ici · Ctrl+C encore quitte",
    ),
    (
        "a Run is under way · Enter sends when it is your turn",
        "un Run est en cours · Entrée envoie quand c'est votre tour",
    ),
];

#[cfg(test)]
mod tests;
