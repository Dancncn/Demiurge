//! Compatibility facade for code still migrating to the workspace crates.
//! New code should import contracts from common/core and persistence from framework directly.

pub use demiurge_common::settings::*;
pub use demiurge_core::session::*;
pub use demiurge_framework::persistence::{
    atomic_write, atomic_write_text, backup_path, load_sessions, load_settings, redacted_settings,
    save_sessions, save_settings, session_store_value,
};
