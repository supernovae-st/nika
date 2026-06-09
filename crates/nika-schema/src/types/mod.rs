// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow configuration types — the vocabulary of `.nika.yaml` files.
//!
//! These types represent the configuration options available in workflow
//! definitions. They are used by both the raw AST (parser output) and
//! the analyzed AST (post-validation).

pub mod agent_def;
pub mod artifact;
pub mod budget;
pub mod capture;
pub mod completion;
pub mod content;
pub mod context;
pub mod decompose;
pub mod duration;
pub mod extract;
pub mod include;
pub mod limits;
pub mod logging;
pub mod on_error;
pub mod orchestrate;
pub mod output;
pub mod output_decl;
pub mod record;
pub mod retry;
pub mod routing;
pub mod schedule;
pub mod schema_version;
pub mod secret;
pub mod structured;
pub mod templatable;
pub mod var_decl;

// Re-exports for convenience.
pub use agent_def::AgentDef;
pub use artifact::{ArtifactSpec, ArtifactsConfig};
pub use budget::{Budget, BudgetConfig};
pub use capture::CaptureMode;
pub use completion::{CompletionConfig, CompletionMode};
pub use content::ContentPart;
pub use context::ContextConfig;
pub use decompose::{DecomposeSpec, DecomposeStrategy};
pub use duration::{GoDurationError, parse_go_duration};
pub use extract::{ExtractMode, ResponseMode};
pub use include::IncludeSpec;
pub use limits::{LimitAction, LimitStatus, LimitType, LimitsConfig};
pub use logging::{LogConfig, LogFormat, LogLevel};
pub use on_error::OnError;
pub use orchestrate::OrchestrateConfig;
pub use output::{OutputFormat, OutputPolicy, SchemaRef};
pub use output_decl::OutputDecl;
pub use record::RecordSpec;
pub use retry::{BackoffStrategy, RetryConfig, is_valid_error_code};
pub use routing::RoutingConfig;
pub use schedule::ScheduleConfig;
pub use schema_version::SchemaVersion;
pub use secret::{SecretRef, SecretSource};
pub use structured::StructuredOutputSpec;
pub use templatable::Templatable;
pub use var_decl::{VarDecl, VarType};
