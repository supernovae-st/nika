// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `&str → registry`, and the law pass. This file NEVER opens a file:
//! the L4 edge reads the bytes, this layer reads the text.
//!
//! Two-stage by design: [`parse_registry`] turns YAML into the raw
//! types (syntax) · [`validate`] applies the grammar LAWS (every
//! refusal named, every remedy taught). A registry that parses but
//! breaks a law is a findings list, never a panic and never a guess.

use crate::cron::parse_cron_fields;
use crate::error::{CadenceError, CadenceErrorKind};
use crate::next::bundled_tz;
use crate::phrase::parse_readable;
use crate::registry::{ArmRegistry, Beat, Cadence, Overlap};

/// Parse the registry text (syntax only — the laws live in
/// [`validate`]). A closed grammar: an unknown key fails here
/// (`deny_unknown_fields`), and the message names what is known.
///
/// # Errors
/// A [`CadenceErrorKind::Grammar`] refusal carrying the YAML diagnostic.
pub fn parse_registry(text: &str) -> Result<ArmRegistry, CadenceError> {
    serde_yaml_bw::from_str(text).map_err(|e| {
        CadenceError::file(
            CadenceErrorKind::Grammar,
            format!("le registre ne se lit pas · {e}"),
            "la grammaire est fermée · clés connues : nika · ceiling · arm (workflow · cadence · où · plafond · manqué · chevauchement · après_saut · actif · raison · jusqu_au · tolérance · décalage · par)",
        )
    })
}

/// Apply every round-1 law. An empty walk IS the green verdict.
/// (FCI-014: an iterator, not a Vec.)
pub fn validate(registry: &ArmRegistry) -> impl Iterator<Item = CadenceError> {
    let mut faults = Vec::new();
    if registry.nika != ArmRegistry::SCHEMA {
        faults.push(CadenceError::file(
            CadenceErrorKind::TagFrozen,
            format!("nika: {} · le tag est gelé à `v1`", registry.nika),
            "le tag de l enveloppe projet est `nika: v1` — le toucher est le geste le plus lourd du projet",
        ));
    }
    if let Some(ceiling) = registry.ceiling
        && !(ceiling > 0.0 && ceiling.is_finite())
    {
        faults.push(CadenceError::file(
            CadenceErrorKind::CeilingNonPositive,
            format!("ceiling: {ceiling} · un plafond se borne au réel positif"),
            "ceiling: 0.50 — le défaut de tout run de ce projet",
        ));
    }
    if registry.traces.is_some() {
        faults.push(CadenceError::file(
            CadenceErrorKind::DeferredKey,
            "traces: · clé du round 2 — elle attend un manque mesuré",
            "retire-la · la rétention demeure aux 3 variables d env pour l instant",
        ));
    }
    if registry.registry.is_some() {
        faults.push(CadenceError::file(
            CadenceErrorKind::DeferredKey,
            "registry: · clé du round 2 — elle attend un manque mesuré",
            "retire-la · le plancher de provenance demeure par machine pour l instant",
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    for beat in registry.beats() {
        validate_beat(beat, &mut faults);
        if seen.contains(&beat.workflow.as_str()) {
            faults.push(CadenceError::beat(
                &beat.workflow,
                CadenceErrorKind::DuplicateWorkflow,
                "workflow armé deux fois — quelle politique gouverne serait une devinette",
                "garde une seule ligne par workflow",
            ));
        }
        seen.push(&beat.workflow);
    }
    faults.into_iter()
}

/// One beat against the laws — the two REQUIRED fields refuse by name,
/// the conditional fields refuse their broken condition.
fn validate_beat(beat: &Beat, faults: &mut Vec<CadenceError>) {
    let w = beat.workflow.as_str();
    // The path's SHAPE is judged here — the law says « relatif au
    // registre »: no absolute prefix, no `..` segment, a real basename
    // before the `.nika.yaml` suffix. Existence is the L4 edge's.
    let shape_bad = w.is_empty()
        || !w.ends_with(".nika.yaml")
        || w.starts_with('/')
        || w.split(':').nth(1).is_some()
        || w.split('/').any(|seg| seg == "..")
        || w.rsplit('/')
            .next()
            .is_some_and(|base| base == ".nika.yaml");
    if shape_bad {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::WorkflowPath,
            "workflow: · un chemin `*.nika.yaml` relatif au registre (ni absolu, ni `..`, un vrai basename)",
            "workflow: workflows/mon-beat.nika.yaml",
        ));
    }
    if let Err(fault) = Cadence::parse(&beat.cadence) {
        faults.push(fault.on_beat(w));
    }
    match beat.plafond {
        None => faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::PlafondMissing,
            "plafond: absent — OBLIGATOIRE, sans défaut · choisir pour toi, c est choisir qui paie",
            "plafond: 0.35 — le plafond PAR TICK · il refuse AVANT de dépenser",
        )),
        Some(p) if !(p > 0.0 && p.is_finite()) => faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::PlafondNonPositive,
            format!("plafond: {p} · un plafond se borne au réel positif"),
            "plafond: 0.35",
        )),
        Some(_) => {}
    }
    if beat.manque.is_none() {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::ManqueMissing,
            "manqué: absent — OBLIGATOIRE, sans défaut · un défaut `rattraper` dépense ce que personne n a demandé, un défaut `sauter` perd un livrable en silence",
            "manqué: rattraper · rattraper-une-fois · sauter — l opérateur dit ce que « raté » veut dire",
        ));
    }
    validate_beat_guards(beat, faults);
}

/// The conditional laws of one beat — each refuses its own broken
/// condition (the ratchet's 100-line cap splits them from the
/// required-fields pass above, and the split reads better anyway).
fn validate_beat_guards(beat: &Beat, faults: &mut Vec<CadenceError>) {
    let w = beat.workflow.as_str();
    if beat.apres_saut.is_some() && beat.chevauchement.is_some_and(|o| o != Overlap::Sauter) {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::ApresSautMisplaced,
            "après_saut: n a de sens que sous `chevauchement: sauter`",
            "retire-le, ou remets `chevauchement: sauter`",
        ));
    }
    if beat.actif == Some(false) {
        if beat.raison.is_none() {
            faults.push(CadenceError::beat(
                w,
                CadenceErrorKind::SuspensionSansRaison,
                "actif: false sans raison: — une suspension se raconte (flux suspend --reason, réclamé depuis 2023)",
                "raison: \"…\" — pourquoi ce beat dort",
            ));
        }
        if beat.jusqu_au.is_none() {
            faults.push(CadenceError::beat(
                w,
                CadenceErrorKind::SuspensionSansEcheance,
                "actif: false sans jusqu_au: — une suspension sans échéance est un oubli",
                "jusqu_au: 2026-09-01 — la date où ce beat se réveille ou se supprime",
            ));
        }
    }
    if let Some(date) = &beat.jusqu_au
        && date.parse::<jiff::civil::Date>().is_err()
    {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::EcheanceSyntaxe,
            format!("jusqu_au: `{date}` · une date ISO, rien d autre"),
            "jusqu_au: 2026-09-01",
        ));
    }
    if let Some(tol) = &beat.tolerance
        && !valid_tolerance(tol)
    {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::ToleranceSyntaxe,
            format!("tolérance: `{tol}` · la forme (m,k)-firm est `m/k` avec m ≤ k"),
            "tolérance: 3/4 — au-delà, ce n est plus un saut, c est une panne",
        ));
    }
    if let Some(dec) = &beat.decalage
        && dec != "hash"
    {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::DecalageInconnu,
            format!("décalage: `{dec}` · seul `hash` existe — déterministe, dérivé du chemin"),
            "décalage: hash — immunisé au portable qui dort (RandomizedOffsetSec, v258)",
        ));
    }
    if beat.signature.is_some() {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::DeferredKey,
            "signature: · la vérification est à ② (serve) — le round 1 ne revendique pas ce qu il ne peut pas prouver",
            "retire-la pour l instant · armer signera quand serve vérifiera",
        ));
    }
    if beat.budget.is_some() {
        faults.push(CadenceError::beat(
            w,
            CadenceErrorKind::DeferredKey,
            "budget: · le quota par fenêtre attend un manque mesuré",
            "retire-le · plafond: (par tick) est le seul garde-fou du round 1",
        ));
    }
}

fn valid_tolerance(text: &str) -> bool {
    match text.split_once('/') {
        Some((m, k)) => match (m.parse::<u8>(), k.parse::<u8>()) {
            (Ok(m), Ok(k)) => m >= 1 && m <= k,
            _ => false,
        },
        None => false,
    }
}

impl Cadence {
    /// Parse a `cadence:` expression. The zone lives IN the expression
    /// (`TZ=…` prefix) for every scheduled form — a TZ-less cadence is
    /// refused by name (scar #1 is closed by the form, not vigilance).
    ///
    /// # Errors
    /// A [`CadenceError`] naming the law and the fix — and, when the
    /// refusal was born on authored text, carrying the byte span of the
    /// faulty token (the painter's data, the `CelError` precedent).
    pub fn parse(raw: &str) -> Result<Self, CadenceError> {
        let text = raw.trim();
        let lead = raw.len() - raw.trim_start().len(); // spans rebase on RAW input
        if text.eq_ignore_ascii_case("on-webhook") {
            return Ok(Self::Webhook);
        }
        let (tz, rest) = split_tz(text, lead)?;
        let base = lead + text.len() - rest.len();
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        if let Some(spec) = parse_readable(&tokens) {
            return Ok(Self::Cron { tz, spec });
        }
        // An attempted readable form earns its OWN refusal — preaching
        // the cron syntax at a French sentence is the refusal-trompeur
        // class this grammar exists to kill.
        if tokens.len() == 2 && looks_readable(tokens[0], tokens[1]) {
            let spans = token_spans(rest, base);
            let faulty = if weekday_fr_lenient(tokens[0]) {
                spans.get(1).copied()
            } else {
                spans.first().copied()
            };
            return Err(CadenceError::file(
                CadenceErrorKind::PhraseSyntax,
                format!(
                    "cadence `{text}` · la forme lisible est `<jour-fr> <H>h<MM>` (jour en minuscules · minute à 2 chiffres)"
                ),
                "ex. `TZ=Europe/Paris lundi 9h07` — sinon, c est du cron",
            )
            .with_span(faulty.unwrap_or((base, lead + text.len()))));
        }
        // The field count is the TYPE — scar #6 belongs to the compiler,
        // not to a discipline: five tokens ride one array, and a sixth
        // field cannot call the field parser at all.
        let spans = token_spans(rest, base);
        let fields: &[&str; 5] = match tokens.as_slice().try_into() {
            Ok(f) => f,
            Err(_) => {
                return Err(CadenceError::file(
                    CadenceErrorKind::FieldCount,
                    format!(
                        "cadence `{text}` · {} champs au lieu de 5 — le compte est le TYPE, la cicatrice n°6 est le compilateur",
                        tokens.len()
                    ),
                    "cron = minute heure jour-du-mois mois jour-de-semaine · ex. `TZ=Europe/Paris 0 9 * * 1`",
                )
                .with_span((base, lead + text.len())));
            }
        };
        let spec = parse_cron_fields(fields).map_err(|e| attach_field_span(e, &spans))?;
        Ok(Self::Cron { tz, spec })
    }
}

/// Does this pair of tokens look like an ATTEMPTED readable form (a
/// French weekday — case forgiven — or an `HhMM` shape) ?
fn looks_readable(jour: &str, heure: &str) -> bool {
    weekday_fr_lenient(jour) || heure.contains('h')
}

/// The French weekday, case-insensitive (an attempted form is named even
/// when the case betrays it — the refusal teaches the exact form).
fn weekday_fr_lenient(jour: &str) -> bool {
    [
        "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
    ]
    .iter()
    .any(|name| jour.eq_ignore_ascii_case(name))
}

/// Byte spans of the whitespace-separated tokens of `rest`, rebased on
/// the full cadence expression (`base`).
fn token_spans(rest: &str, base: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in rest.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push((base + s, base + i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push((base + s, base + rest.len()));
    }
    out
}

/// Attach the faulty token's byte span to a field-born error (the label
/// rides the error from its birth in `parse_field`). A `dom`+`dow`
/// refusal paints the pair.
fn attach_field_span(err: CadenceError, spans: &[(usize, usize)]) -> CadenceError {
    const FIELDS: [&str; 5] = ["minute", "heure", "jour-du-mois", "mois", "jour-de-semaine"];
    if err.kind() == CadenceErrorKind::DomDowOr && spans.len() == 5 {
        return err.with_span((spans[2].0, spans[4].1));
    }
    let Some(label) = err.field() else {
        return err;
    };
    match FIELDS
        .iter()
        .position(|f| f == &label)
        .and_then(|i| spans.get(i))
    {
        Some(&(a, b)) => err.with_span((a, b)),
        None => err,
    }
}

/// Detach the `TZ=<iana>` prefix — REQUIRED for every scheduled form.
/// `lead` is the raw-input trim offset so spans paint the raw text.
fn split_tz(text: &str, lead: usize) -> Result<(String, &str), CadenceError> {
    match text.split_once(char::is_whitespace) {
        Some((head, rest)) if head.starts_with("TZ=") => {
            let name = &head[3..];
            if bundled_tz(name).is_none() {
                return Err(CadenceError::file(
                    CadenceErrorKind::TzUnknown,
                    format!("cadence `{text}` · fuseau `{name}` inconnu de la base IANA embarquée"),
                    "un nom IANA · ex. `TZ=Europe/Paris …` (la base est figée au build, zéro lecture système)",
                )
                .with_span((lead + 3, lead + head.len())));
            }
            let rest = rest.trim_start();
            if rest.is_empty() {
                return Err(CadenceError::file(
                    CadenceErrorKind::FieldCount,
                    format!("cadence `{text}` · le fuseau est là — ce sont les 5 champs qui manquent"),
                    "cron = minute heure jour-du-mois mois jour-de-semaine · ex. `TZ=Europe/Paris 0 9 * * 1`",
                )
                .with_span((lead + head.len(), lead + text.len())));
            }
            Ok((name.to_owned(), rest))
        }
        _ if text.starts_with("TZ=") => Err(CadenceError::file(
            CadenceErrorKind::FieldCount,
            format!("cadence `{text}` · le fuseau est là — ce sont les 5 champs qui manquent"),
            "cron = minute heure jour-du-mois mois jour-de-semaine · ex. `TZ=Europe/Paris 0 9 * * 1`",
        )
        .with_span((lead, lead + text.len()))),
        _ => Err(CadenceError::file(
            CadenceErrorKind::TzMissing,
            format!(
                "cadence `{text}` · pas de fuseau — il ne peut pas être oublié par le calculateur, donc il vit DANS l expression"
            ),
            "préfixe la cadence de `TZ=<iana>` · ex. `TZ=Europe/Paris 0 9 * * 1` (seul `on-webhook` n en porte pas)",
        )
        .with_span((lead, lead + text.len()))),
    }
}
