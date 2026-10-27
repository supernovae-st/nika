// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where the images a seat delivers land under `images: { save: <path> }` (spec 02 §Harness
//! images): the first at the path, image *n* (*n* ≥ 2) at the path with `-n` before the
//! extension of its last component. One table for the writer (the agent verb), the static
//! judgments of `nika check` (the boundary fit, the inferred boundary, the same-path law) and
//! the client's media bound, so every judge sees every path the writer can take.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The most images one harness turn delivers with their bytes (the ACP client's media bound):
/// a `save:` path can name at most this many files.
pub const MAX_HARNESS_IMAGES: usize = 4;

/// Where image `index` (0-based) lands under `save`: the path itself for the first, then the
/// path with `-n` (n = index + 1) inserted before the extension of its last component.
#[must_use]
pub fn harness_image_path(save: &str, index: usize) -> String {
    if index == 0 {
        return save.to_string();
    }
    let n = index + 1;
    let (dir, file) = save
        .rfind('/')
        .map_or(("", save), |cut| save.split_at(cut + 1));
    match file.rfind('.').filter(|&dot| dot > 0) {
        Some(dot) => format!("{dir}{}-{n}{}", &file[..dot], &file[dot..]),
        None => format!("{dir}{file}-{n}"),
    }
}

/// Every path a `save:` can write, in delivery order ([`MAX_HARNESS_IMAGES`] of them).
#[must_use]
pub fn harness_image_paths(save: &str) -> Vec<String> {
    (0..MAX_HARNESS_IMAGES)
        .map(|index| harness_image_path(save, index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn further_images_take_a_numbered_sibling_of_the_declared_path() {
        assert_eq!(
            harness_image_path("./out/banner.png", 0),
            "./out/banner.png"
        );
        assert_eq!(
            harness_image_path("./out/banner.png", 1),
            "./out/banner-2.png"
        );
        assert_eq!(
            harness_image_path("./out/banner.png", 3),
            "./out/banner-4.png"
        );
        assert_eq!(
            harness_image_path("out/v1.2/banner", 1),
            "out/v1.2/banner-2"
        );
        assert_eq!(harness_image_path(".hidden", 1), ".hidden-2");
        assert_eq!(
            harness_image_path("banner.final.png", 1),
            "banner.final-2.png"
        );
    }

    #[test]
    fn a_save_names_one_path_per_image_the_client_can_deliver() {
        assert_eq!(
            harness_image_paths("./out/a.png"),
            [
                "./out/a.png",
                "./out/a-2.png",
                "./out/a-3.png",
                "./out/a-4.png"
            ]
        );
    }
}
