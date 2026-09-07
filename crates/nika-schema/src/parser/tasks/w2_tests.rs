//! W2 · `NIKA-PARSE-024` per SHAPE — what the finding may PROMISE.
//!
//! Carved out of `tasks.rs` at the 1500-line file cap (the parser body
//! had 3 lines of headroom left). The table pinned here is one half of a
//! pair: this side reads `provable` off the finding, and nika-cli-host's
//! fix ladder runs the REAL `nika_migrate::w2()` on the same sources and
//! asserts the two agree shape by shape — a promise nobody executes is
//! how the two drifted apart in the first place.

use super::tests::parse_strict;
use crate::error::SchemaError;

/// W2 · NIKA-PARSE-024: the `--fix` clause is spoken only for the
/// shape the migrator's scanner reads (no entry anything but a bare
/// task id · quotes stripped · the empty list drops). A scalar, a map
/// entry or any other string names itself as the author's to rewrite,
/// and the `after:` example never splices a non-id (wave 3 · persona
/// 02 · the promise fired, then `--fix` said « rewrite by hand » on
/// the same screen). The shape-by-shape agreement with the real
/// `nika_migrate::w2()` is pinned in nika-cli-host's fix ladder.
#[test]
fn depends_on_promises_fix_only_for_the_shape_it_proves() {
    let ids = parse_strict(
        "tasks:\n  a:\n    exec: { command: [ls] }\n  b:\n    depends_on: [a]\n    exec: { command: [ls] }\n",
    )
    .expect_err("dead form");
    assert!(
        matches!(&ids, SchemaError::W2DependsOnField { task, task_hint, provable: true, .. }
            if task == "b" && task_hint == "a"),
        "{ids:?}"
    );
    let text = ids.to_string();
    assert!(text.contains("`after: {a: success}`"), "{text}");
    assert!(text.contains("can migrate this shape"), "{text}");
    assert!(!text.contains("leaves this shape"), "{text}");
    // NECESSARY, not sufficient — the accepted shape says the
    // whole-file stops out loud (S1 skip · S3 status-only).
    assert!(text.contains("still stops the file"), "{text}");

    // A QUOTED bare id is the same sequence to the parser and (since
    // the scanner dequotes) to the fixer: the clause is spoken.
    let quoted = parse_strict(
        "tasks:\n  a:\n    exec: { command: [ls] }\n  b:\n    depends_on: [\"a\"]\n    exec: { command: [ls] }\n",
    )
    .expect_err("dead form");
    assert!(
        matches!(&quoted, SchemaError::W2DependsOnField { task_hint, provable: true, .. }
            if task_hint == "a"),
        "{quoted:?}"
    );

    // An EMPTY list declares no edge — `--fix` drops the dead line, so
    // the finding must not hand it back to the author.
    let empty = parse_strict("tasks:\n  b:\n    depends_on: []\n    exec: { command: [ls] }\n")
        .expect_err("dead form");
    assert!(
        matches!(&empty, SchemaError::W2DependsOnField { provable: true, .. }),
        "{empty:?}"
    );

    for (shape, yaml) in [
        ("scalar", "    depends_on: a\n"),
        ("map entry", "    depends_on: [{ task: a }]\n"),
        ("arrow string", "    depends_on: [\"a >> b\"]\n"),
        ("nested list", "    depends_on: [[a]]\n"),
    ] {
        let err = parse_strict(&format!(
            "tasks:\n  a:\n    exec: {{ command: [ls] }}\n  b:\n{yaml}    exec: {{ command: [ls] }}\n"
        ))
        .expect_err(shape);
        assert!(
            matches!(&err, SchemaError::W2DependsOnField { task, task_hint, provable: false, .. }
                if task == "b" && task_hint == "producer"),
            "{shape}: {err:?}"
        );
        let text = err.to_string();
        assert!(text.contains("leaves this shape to you"), "{shape}: {text}");
        assert!(!text.contains("migrates this shape"), "{shape}: {text}");
        assert!(
            text.contains("`after: {producer: success}`"),
            "{shape}: {text}"
        );
        assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-024", "{shape}");
    }
}
