// Chunk: docs/chunks/treesitter_gotodef - Locals query files for go-to-definition
//!
//! Tree-sitter locals queries for scope-aware go-to-definition.
//!
//! This module provides `locals.scm` query content for languages that don't
//! include them in their tree-sitter crates. The queries define:
//!
//! - `@local.scope`: Nodes that create a new scope (functions, blocks, etc.)
//! - `@local.definition`: Nodes that define a name (parameters, let bindings, etc.)
//! - `@local.reference`: Nodes that reference a name (identifiers)
//!
//! These captures enable the go-to-definition resolver to:
//! 1. Find the reference under the cursor
//! 2. Walk enclosing scopes to find a matching definition
//! 3. Jump to that definition

pub mod python;
pub mod rust;
pub mod typescript;
