// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Local trace adapter for the L3 snapshot-backed child runner.
//!
//! Child resolution, admission, closure hashing, capability intersection, and
//! production composition live in
//! `nika_service_execution::ServiceExecutionDriver`. The CLI retains only the
//! L4 journal implementation it owns.

use nika_dap::journal::TraceFileSink;
use nika_runtime::EventSink;
use nika_service_execution::{ChildTrace, ChildTraceFactory, ChildTraceMetadata};

pub(super) struct CliChildTraceFactory {
    enabled: bool,
}

impl CliChildTraceFactory {
    pub(super) const fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

impl ChildTraceFactory for CliChildTraceFactory {
    fn create(&self) -> Box<dyn ChildTrace> {
        let sink = if self.enabled {
            TraceFileSink::new(nika_dap::store::TRACE_DIR)
        } else {
            TraceFileSink::disabled()
        };
        Box::new(CliChildTrace { sink })
    }
}

struct CliChildTrace {
    sink: TraceFileSink,
}

impl ChildTrace for CliChildTrace {
    fn sink(&mut self) -> &mut dyn EventSink {
        &mut self.sink
    }

    fn metadata(&self) -> ChildTraceMetadata {
        ChildTraceMetadata::new(
            self.sink
                .path()
                .and_then(std::path::Path::file_name)
                .map(|name| name.to_string_lossy().into_owned()),
            Some(self.sink.chain_head().to_owned()),
        )
    }
}
