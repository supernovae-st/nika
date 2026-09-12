// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Callable definitions shared by the static parser, runtime and diagnostics.

macro_rules! callable_kind {
    ($kind:ident, $($variant:ident => $name:literal),+ $(,)?) => {
        /// A callable admitted by the expression parsers.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum $kind {
            $(#[doc = concat!("The `", $name, "` callable.")]
            $variant),+
        }

        impl $kind {
            /// Resolve a source name against the closed callable set.
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

// Names occur once: this declaration generates both admission and teaching.
macro_rules! callables {
    (globals { $gf:ident => $gn:literal($ga:literal)
        $(, $g:ident => $name:literal($arg:literal))* }
     methods { $mf:ident => $mn:literal($ma:literal)
        $(, $m:ident => $method:literal($marg:literal))* }) => {
        callable_kind!(GlobalFunction, $gf => $gn $(, $g => $name)*);
        callable_kind!(Method, $mf => $mn $(, $m => $method)*);

        /// The supported call forms, derived from the parsers' definitions.
        pub const CALLABLE_HELP: &str = concat!(
            "global functions: `", $gn, "(", $ga, ")`",
            $(", `", $name, "(", $arg, ")`",)*
            "; methods: `x.", $mn, "(", $ma, ")`",
            $(", `x.", $method, "(", $marg, ")`",)*
        );
    };
}

callables! {
    globals { Size => "size"("x"), Has => "has"("x") }
    methods {
        Size => "size"(""), Contains => "contains"("s"),
        StartsWith => "startsWith"("s"), EndsWith => "endsWith"("s")
    }
}
