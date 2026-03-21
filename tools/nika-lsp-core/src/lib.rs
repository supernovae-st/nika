//! Protocol-agnostic LSP intelligence for Nika workflows.
//!
//! Provides [`WorldDatabase`], [`LineIndex`], [`PositionIndex`], [`FileSnapshot`] --
//! all the infrastructure that LSP handlers need, without depending
//! on the heavy nika runtime.

pub mod analysis;
pub mod db;
pub mod document;
pub mod handler;
pub mod handlers;
pub mod parse;
pub mod position;
