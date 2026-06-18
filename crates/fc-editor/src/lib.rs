//! `fc-editor` — GUI-free editor cores for the FlatCAM Rust port.
//!
//! Each editor is a pure, unit-testable model + edit-operation layer (no egui,
//! no I/O): an editable collection of primitives, hit-testing, mutation
//! operations, and conversion to/from `geo` geometry. The interactive egui
//! panels in `fc-gui` drive these cores. This mirrors FlatCAM's editors
//! (`appEditors/`) but separates the editable model from the GUI so it can be
//! tested headlessly.
//!
//! Modules are wired in as each editor core lands.

pub mod geo_editor;
pub use geo_editor::{GeoEditor, Shape};
pub mod gerber_editor;
pub use gerber_editor::{GbrPrim, GerberEditor};
pub mod exc_editor;
pub use exc_editor::{EditTool, ExcEditor};
pub mod gcode_editor;
pub use gcode_editor::GCodeEditor;
