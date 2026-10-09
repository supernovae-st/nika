// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The object in view, relocated to [`nika_tui_view::workspace::object`]
//! (ADR-143): this path keeps the renderer's callers unchanged. The
//! welcome of the renderer's own project view is proved here, where that
//! view lives.

pub use nika_tui_view::workspace::object::{
    Object, Paint, content_rows, length, lines, lines_from, render, render_from, welcome_mark,
};

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::text::Line;
    use unicode_width::UnicodeWidthStr;

    use super::{Paint, lines};
    use crate::visual::logomark::REVEAL_ENDS;

    fn paint(ascii: bool) -> Paint {
        Paint {
            ascii,
            color: false,
            elapsed: REVEAL_ENDS,
            reduced_motion: false,
        }
    }

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn the_real_project_welcome_keeps_the_path_without_repeating_other_regions() {
        use crate::workspace::{
            geometry::Geometry,
            project::{self, ProjectView},
        };
        for ascii in [false, true] {
            for (width, height) in [(60, 18), (80, 24)] {
                let view = ProjectView::new("local", "first project", "/project")
                    .listing(Vec::new(), true)
                    .seated("deepseek/chosen - deepseek API, metered");
                let object = project::welcome(Some(&view), ascii);
                let area = Geometry::of(Rect::new(0, 0, width, height), false)
                    .expect("fits")
                    .object;
                let rows = text(&lines(&object, area.width, area.height, paint(ascii)));
                let all = rows.join("\n");
                for expected in [
                    "1 Describe",
                    "2 Answer",
                    "3  Save, then Run with the workflow's models.",
                ] {
                    assert!(
                        all.replace("  ", " ")
                            .contains(&expected.replace("  ", " ")),
                        "{width}x{height}: missing {expected}: {all}"
                    );
                }
                for repeated in [
                    "To prepare",
                    "deepseek/chosen",
                    "/intelligence:",
                    "Click/F6:",
                ] {
                    assert!(!all.contains(repeated), "{all}");
                }
                assert!(rows.len() <= usize::from(area.height));
                if ascii {
                    assert!(all.is_ascii(), "{all}");
                }
                let missing =
                    project::welcome(Some(&ProjectView::new("local", "first", "/project")), ascii);
                let all = text(&lines(&missing, area.width, area.height, paint(ascii))).join("\n");
                assert!(
                    all.contains("Describe") && !all.contains("not chosen yet"),
                    "{all}"
                );
            }
        }
    }

    #[test]
    fn the_compact_guide_leaves_selected_intelligence_in_the_conversation() {
        use crate::workspace::project::{self, ProjectView};
        for (seat, expected) in [
            ("deepseek/chosen - deepseek API, metered", "deepseek/chosen"),
            (
                "claude-code/chosen - through your account",
                "claude-code/chosen",
            ),
            ("ollama/chosen - on this machine", "ollama/chosen"),
            (
                "none, the engine facts answer",
                "none, the engine facts answer",
            ),
        ] {
            let view = ProjectView::new("local", "first", "/project").seated(seat);
            let object = project::welcome(Some(&view), true);
            let rows = text(&lines(&object, 60, 8, paint(true)));
            let all = rows.join("\n");
            for required in ["Describe", "Answer", "workflow's models"] {
                assert!(all.contains(required), "missing {required}: {all}");
            }
            assert!(!all.contains(expected), "{all}");
            assert!(!all.contains("To prepare"), "{all}");
            assert!(rows.len() <= 8 && rows.iter().all(|row| row.width() <= 60));
            assert!(all.is_ascii());
        }
    }
}
