//! Medidas, espacios, radios y tamaños de letra (reglas-interfaz.md §3). Todo en puntos de egui, antes del zoom.
//! Ningún número de medida se escribe suelto en una pantalla: se usa una constante de acá.

#![allow(dead_code)]

// ---- Espacios y márgenes ----
pub const SP_XS: f32 = 4.0;
pub const SP_S: f32 = 8.0;
pub const SP_M: f32 = 12.0;
pub const SP_L: f32 = 16.0;
pub const SP_XL: f32 = 24.0;
pub const SP_XXL: f32 = 32.0;

// ---- Alturas ----
/// Fila del título de la pantalla.
pub const TITLE_H: f32 = 36.0;
/// Barra de herramientas (y la fila del historial en la barra lateral).
pub const TOOLBAR_H: f32 = 36.0;
pub const CONTEXT_H: f32 = 32.0;
pub const STATUS_H: f32 = 24.0;
/// Alto de botones, campos, desplegables, segmentos y deslizadores (`interact_size.y` en `theme::apply`).
pub const CONTROL_H: f32 = 24.0;
/// Botón de icono cuadrado.
pub const ICON_BTN: f32 = 24.0;
/// Pestañas.
pub const TAB_H: f32 = 30.0;
/// Filas de tablas y listas.
pub const ROW_H: f32 = 24.0;
/// Filas con miniatura.
pub const ROW_H_TALL: f32 = 40.0;
/// Tiras del plan.
pub const STRIP_H: f32 = 28.0;
/// Etiquetas de color.
pub const CHIP_H: f32 = 22.0;
/// Muestra de color (una fila de colores).
pub const SWATCH: f32 = 20.0;
/// Mosaico de la ventana «Añadir» (icono grande y rótulo).
pub const TILE_W: f32 = 124.0;
pub const TILE_H: f32 = 92.0;
/// Iconos en filas y en botones.
pub const ICON_S: f32 = 16.0;
pub const ICON_M: f32 = 20.0;

// ---- Anchos ----
/// Columna de la etiqueta en una fila de propiedad.
pub const LABEL_W: f32 = 96.0;
/// Campos de formato fijo: fecha, hora y número.
pub const W_DATE: f32 = 128.0;
pub const W_TIME: f32 = 80.0;
pub const W_NUM: f32 = 88.0;
/// Barra lateral en riel.
pub const RAIL_W: f32 = 48.0;
/// Ancho inicial de la barra lateral y del inspector (el ancho que se recuerda sale de `storage.rs`).
pub const SIDEBAR_W: f32 = 340.0;
pub const INSPECTOR_W: f32 = 360.0;
/// Ancho mínimo del contenido antes de pasar el inspector a cajón.
pub const CONTENT_MIN: f32 = 560.0;
/// Ancho de ventana (con el zoom) debajo del cual la barra lateral pasa a riel.
pub const SIDEBAR_RAIL_BELOW: f32 = 1100.0;
/// Ancho máximo de la columna de una ficha.
pub const FICHA_MAX: f32 = 960.0;
/// Ancho de contenido debajo del cual la columna lateral de una ficha pasa abajo.
pub const FICHA_SIDE_BREAK: f32 = 760.0;
/// Ancho de la tarjeta Portada.
pub const COVER_W: f32 = 320.0;
pub const SETTINGS_MAX: f32 = 640.0;
/// Ancho máximo de un diálogo, y de uno con dos columnas.
pub const DIALOG_MAX: f32 = 520.0;
pub const DIALOG_WIDE: f32 = 880.0;
/// Alto mínimo de una zona para soltar.
pub const DROP_MIN_H: f32 = 64.0;
/// Ancho debajo del cual `prop_row` apila etiqueta y control.
pub const PROP_STACK_BELOW: f32 = 260.0;
/// Ancho mínimo de cada segmento de un control segmentado.
pub const SEGMENT_MIN_W: f32 = 64.0;

// ---- Radios ----
pub const R_CONTROL: u8 = 4;
pub const R_CARD: u8 = 6;
pub const R_FLOAT: u8 = 8;

/// Tamaño mínimo de la ventana (reglas §2.7).
pub const WINDOW_MIN: [f32; 2] = [1024.0, 640.0];

/// Duraciones de las animaciones (reglas §6.7), en segundos. Pasan por `kit::anim`, que las pone en 0 si se reducen.
pub const ANIM_FAST: f32 = 0.10;
pub const ANIM: f32 = 0.16;
pub const ANIM_SLOW: f32 = 0.24;

// ---- Márgenes internos de cada zona del marco (reglas §3.1) ----
pub const SIDEBAR_PAD_X: f32 = SP_L;
pub const SIDEBAR_PAD_Y: f32 = SP_M;
pub const BAR_PAD_X: f32 = SP_M;
pub const CONTENT_PAD: f32 = SP_XL;
/// Margen interno de una tarjeta y de un diálogo.
pub const CARD_PAD: f32 = SP_L;
pub const DIALOG_PAD: f32 = SP_XL;
/// Separación entre bloques o tarjetas del contenido (tipos C, E y F).
pub const BLOCK_GAP: f32 = SP_L;
/// Separación entre secciones del inspector y entre secciones de la barra lateral.
pub const SECTION_GAP: f32 = SP_XL;
/// Separación entre filas de una lista de la barra lateral.
pub const LIST_GAP: f32 = 2.0;
/// Margen a los lados de una fila de lista, de una celda de tabla y de una pestaña.
pub const ROW_PAD_X: f32 = SP_S;

// ---- Letra ----
pub const FONT_TITLE: f32 = 19.0;
/// Título de tarjeta o de diálogo.
pub const FONT_CARD_TITLE: f32 = 16.0;
pub const FONT_BODY: f32 = 13.0;
pub const FONT_SMALL: f32 = 11.0;
/// Número grande de un indicador.
pub const FONT_INDICATOR: f32 = 22.0;
/// Icono grande de un estado vacío.
pub const ICON_XL: f32 = 32.0;
