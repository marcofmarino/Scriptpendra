//! Los componentes compartidos viven en `ui/kit.rs`; este módulo solo los reexporta mientras las pantallas se migran.

#[allow(unused_imports)]
pub use crate::ui::kit::{
    Row, accent_button, card, clean_pasted, column, draft_cell, field, field_with, icon_button,
    stepper, suggestion_item, tab, tab_row, tile, tree_row, wide_button,
};
