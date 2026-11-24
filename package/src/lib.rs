//! Gitmoji Triangle: a vocabulary for commit messages, a checker and a guide.
//!
//! A first line reads `<area><behaviour or intent>[modifier] <ticket> (<scope>): <summary>`.
//! The vocabulary lives in `vocabulary.toml` and is compiled into the binary, so the
//! checker, the guide, the agent skill and the website all say the same thing.

pub mod attribution;
pub mod caption;
pub mod check;
pub mod config;
pub mod fix;
pub mod guide;
pub mod staged;
pub mod subject;
pub mod trailers;
pub mod vocabulary;

/// Where the list of changes between releases can be read. Messages about a version
/// that does not match point here, so that the reader can see what changed.
pub const CHANGELOG: &str = "https://github.com/D1no/gitmoji/blob/trunk/CHANGELOG.md";
