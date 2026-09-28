// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bounded pre-decode validation of an encoded snapshot's wire envelope
//! (descended from Serve's job door, C6). The envelope is probed with a
//! capped unit count and borrowed strings: its root and unit paths and its
//! JSON metadata are bounded, its digests must be canonical lowercase
//! SHA-256 and its unit bytes even-length lowercase hex, all before the full
//! decode allocates what the probe would refuse. A transport maps each typed
//! refusal to its own wire answer.

/// The most units an encoded envelope may carry.
pub const WIRE_UNIT_CEILING: usize = 256;
const UNIT_COUNT_PROBE_MARKER: &str = "nika snapshot wire unit count exceeded";

/// The envelope bounds a transport enforces before decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WireLimits {
    /// The longest root or unit logical path, in bytes.
    pub path_bytes: usize,
    /// The most encoded bytes outside the units' hex payloads.
    pub metadata_bytes: usize,
}

impl WireLimits {
    /// Bounds for logical paths and for the JSON metadata around the payloads.
    #[must_use]
    pub const fn new(path_bytes: usize, metadata_bytes: usize) -> Self {
        Self {
            path_bytes,
            metadata_bytes,
        }
    }
}

/// Why an encoded envelope refuses before decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireRefusal {
    /// More than [`WIRE_UNIT_CEILING`] units.
    UnitCount,
    /// Not a snapshot envelope.
    Malformed,
    /// A root or unit path beyond [`WireLimits::path_bytes`].
    PathLimit,
    /// A digest that is not canonical lowercase SHA-256.
    Digest,
    /// Unit bytes that are not even-length lowercase hexadecimal.
    Hex,
    /// Metadata beyond [`WireLimits::metadata_bytes`].
    MetadataLimit,
}

#[derive(serde::Deserialize)]
struct Probe<'a> {
    root: &'a str,
    #[serde(default)]
    digest: Option<&'a str>,
    #[serde(borrow)]
    units: BoundedUnits<'a>,
}

struct BoundedUnits<'a>(Vec<Unit<'a>>);

#[derive(serde::Deserialize)]
struct Unit<'a> {
    path: &'a str,
    #[serde(default)]
    digest: Option<&'a str>,
    bytes_hex: &'a str,
}

impl<'de: 'a, 'a> serde::Deserialize<'de> for BoundedUnits<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct UnitsVisitor;

        impl<'de> serde::de::Visitor<'de> for UnitsVisitor {
            type Value = BoundedUnits<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a bounded execution snapshot unit array")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut units = Vec::new();
                while let Some(unit) = sequence.next_element()? {
                    if units.len() == WIRE_UNIT_CEILING {
                        return Err(serde::de::Error::custom(UNIT_COUNT_PROBE_MARKER));
                    }
                    units.push(unit);
                }
                Ok(BoundedUnits(units))
            }
        }

        deserializer.deserialize_seq(UnitsVisitor)
    }
}

/// Probe `encoded` against `limits` before any full decode.
///
/// # Errors
/// The first refusal, in the order: unit count, envelope, paths, digests,
/// hex, metadata.
pub fn check_wire(encoded: &str, limits: WireLimits) -> Result<(), WireRefusal> {
    let probe = match serde_json::from_str::<Probe<'_>>(encoded) {
        Ok(probe) => probe,
        Err(error) if error.to_string().contains(UNIT_COUNT_PROBE_MARKER) => {
            return Err(WireRefusal::UnitCount);
        }
        Err(_) => return Err(WireRefusal::Malformed),
    };
    let units = &probe.units.0;
    if probe.root.len() > limits.path_bytes
        || units.iter().any(|unit| unit.path.len() > limits.path_bytes)
    {
        return Err(WireRefusal::PathLimit);
    }
    let bad_digest = |digest: Option<&str>| digest.is_some_and(|value| !canonical_digest(value));
    if bad_digest(probe.digest) || units.iter().any(|unit| bad_digest(unit.digest)) {
        return Err(WireRefusal::Digest);
    }
    if units.iter().any(|unit| malformed_hex(unit.bytes_hex)) {
        return Err(WireRefusal::Hex);
    }
    let hex_bytes = units
        .iter()
        .try_fold(0usize, |total, unit| {
            total.checked_add(unit.bytes_hex.len())
        })
        .ok_or(WireRefusal::MetadataLimit)?;
    if encoded.len().saturating_sub(hex_bytes) > limits.metadata_bytes {
        return Err(WireRefusal::MetadataLimit);
    }
    Ok(())
}

fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn malformed_hex(value: &str) -> bool {
    !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
