// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The image / tts rows' local-backend clause (#1581): doctor never
//! probes the media planes, so an unset URL is named as UNSET (the
//! engine's silent `:8080` fallback disclosed, never printed as a
//! listener) and a set one is « configured », never « reachable ».

use super::*;

#[test]
fn an_unset_local_media_url_is_never_printed_as_a_listener() {
    let image = image_finding(&ImageProbe::default());
    assert_eq!(image.level, Level::Ok, "mock is always ready");
    assert!(image.detail.contains("mock ready"), "{}", image.detail);
    assert!(
        image.detail.contains("local backend unset")
            && image.detail.contains("NIKA_IMAGE_LOCAL_URL"),
        "{}",
        image.detail
    );
    assert!(
        !image.detail.contains("local → http://localhost:8080"),
        "the fallback port is not a wired path: {}",
        image.detail
    );
    assert!(image.detail.contains("unprobed"), "{}", image.detail);
    let tts = tts_finding(&TtsProbe::default());
    assert!(
        tts.detail.contains("local backend unset") && tts.detail.contains("NIKA_TTS_LOCAL_URL"),
        "{}",
        tts.detail
    );
    assert!(
        !tts.detail.contains("local → http://localhost:8080"),
        "{}",
        tts.detail
    );
}

#[test]
fn a_set_local_media_url_is_configured_never_reachable() {
    let image = image_finding(&ImageProbe {
        local_url: Some("http://user:s3cret@gpu.lan:8080".to_owned()),
        ..ImageProbe::default()
    });
    assert!(
        image
            .detail
            .contains("local → http://***@gpu.lan:8080 (configured · unprobed)"),
        "{}",
        image.detail
    );
    assert!(!image.detail.contains("s3cret"), "userinfo is redacted");
    let tts = tts_finding(&TtsProbe {
        local_url: Some("http://tts.lan:8880/v1".to_owned()),
        ..TtsProbe::default()
    });
    assert!(
        tts.detail
            .contains("local → http://tts.lan:8880/v1 (configured · unprobed)"),
        "{}",
        tts.detail
    );
}
