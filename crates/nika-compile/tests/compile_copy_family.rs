// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The copy family at the deterministic door: a copy of one file to another, or a read
//! followed by « write it as is to … » in six languages, compiles READY as a read and a
//! write of what was read, with no language step and no model (measured on sealed-v3:
//! sv3-06 « copie … tel quel, octet pour octet » had been read as a draft).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use serde_json::Value;

mod common;
use common::keys;

fn ready_copy(intent: &str) {
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    assert!(!keys(&out).contains(&"model"), "{intent}: {out:#?}");
    let source = out.candidate.as_deref().unwrap();
    assert!(!source.contains("infer:"), "{intent}: {source}");
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    assert_eq!(
        doc["const"]["source_path"], "./fete/consignes.txt",
        "{intent}: {source}"
    );
    let write = &doc["tasks"]["write_output"];
    assert_eq!(write["invoke"]["tool"], "nika:write", "{intent}: {source}");
    assert!(
        write["with"]["content"]
            .as_str()
            .unwrap_or_default()
            .contains("tasks.read_source.output"),
        "{intent}: the destination receives what was read: {source}"
    );
}

#[test]
fn a_copy_compiles_as_a_read_and_a_write_of_what_was_read() {
    for intent in [
        "Copy ./fete/consignes.txt as is, byte for byte, to ./out/consignes-copie.txt",
        "copie ./fete/consignes.txt tel quel, octet pour octet, dans ./out/consignes-copie.txt",
        "Copia ./fete/consignes.txt tal cual en ./out/consignes-copie.txt",
        "Kopiere ./fete/consignes.txt unverändert nach ./out/consignes-copie.txt",
    ] {
        ready_copy(intent);
    }
}

#[test]
fn a_written_object_clitic_refers_back_to_the_material_read() {
    for intent in [
        "Read ./fete/consignes.txt and write it as is to ./out/consignes-copie.txt",
        "Lis ./fete/consignes.txt et écris-le tel quel dans ./out/consignes-copie.txt",
        "Lee ./fete/consignes.txt y escríbelo tal cual en ./out/consignes-copie.txt",
    ] {
        ready_copy(intent);
    }
}
