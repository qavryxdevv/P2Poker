//! `docs/SPEC_CS.md` section 22. eframe/egui, per `docs/research/GUI_STACK.md`.

pub mod album;
/// `D-073`: the installer's window. Windows only, as the installation is.
#[cfg(windows)]
pub mod installer;
pub mod lobby;
pub mod render;
pub mod rewards;
pub mod table;
pub mod theme;
