//! Componentes compartidos (reglas-interfaz.md §5). Si una pantalla necesita algo que no está acá, primero se agrega
//! acá y después se usa. `widgets.rs` se va pasando a este módulo pantalla por pantalla.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Align2, FontFamily, FontId, Response, RichText, Sense, Stroke, Ui, vec2};

use super::checks;
use super::tokens::*;
use crate::{
    icon,
    theme::{self, Palette, bold_family, icon_text, mix},
};

static REDUCE_MOTION: AtomicBool = AtomicBool::new(false);

/// Activa o desactiva «Reducir animaciones» (Ajustes). La app lo fija en cada cuadro desde los ajustes.
pub fn set_reduce_motion(reduce: bool) {
    REDUCE_MOTION.store(reduce, Ordering::Relaxed);
}

/// Duración de una animación en segundos, o 0 si están reducidas. Las pruebas corren con las animaciones en 0 para medir
/// el estado final (reglas §6.7).
pub fn anim(seconds: f32) -> f32 {
    if cfg!(test) || REDUCE_MOTION.load(Ordering::Relaxed) {
        0.0
    } else {
        seconds
    }
}

/// Cuánto está «prendido» algo (0 a 1) animado con la curva `cubic_out`: sirve para el fundido de pasar el mouse.
pub fn fade(ui: &Ui, id: egui::Id, on: bool, seconds: f32) -> f32 {
    let t = ui.ctx().animate_bool_with_time(id, on, anim(seconds));
    egui::emath::easing::cubic_out(t)
}

/// El menú de exportar de las pantallas: el mismo botón en todas (icono y «Exportar»).
pub fn export_menu(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.menu_button(format!("{}  Exportar", crate::icon::EXPORT), add);
}

/// Exportar directo, sin menú: el mismo botón que `export_menu`.
pub fn export_button(ui: &mut Ui, tooltip: &str) -> Response {
    ui.button(format!("{}  Exportar", crate::icon::EXPORT))
        .on_hover_text(tooltip)
}

/// Fila con contenido a la izquierda y acciones contra el borde derecho de la zona (reglas §2.4). Envuelve `egui::Sides`.
pub fn sides<L, R>(
    ui: &mut Ui,
    left: impl FnOnce(&mut Ui) -> L,
    right: impl FnOnce(&mut Ui) -> R,
) -> (L, R) {
    egui::Sides::new().shrink_left().show(ui, left, right)
}

/// Ancho para un control con texto: lo disponible, entre `min` y `max`. Reemplaza a los `desired_width(220.0)`.
pub fn fill_width(ui: &Ui, min: f32, max: f32) -> f32 {
    ui.available_width().clamp(min, max)
}

/// Ancho para un control que comparte la fila con `buttons` botones de icono y `extra` puntos de otros controles a su
/// derecha: lo que sobra, con un mínimo. Reemplaza a las restas mágicas del ancho disponible.
pub fn beside(ui: &Ui, buttons: usize, extra: f32, min: f32) -> f32 {
    (ui.available_width() - buttons as f32 * (ICON_BTN + ui.spacing().item_spacing.x) - extra)
        .max(min)
}

/// Botón cuadrado de icono. El tooltip es obligatorio: no puede estar vacío. Trae el fundido de pasar el mouse y apretar.
pub fn icon_button(ui: &mut Ui, p: &Palette, code: &str, tooltip: &str, enabled: bool) -> Response {
    debug_assert!(
        !tooltip.trim().is_empty(),
        "un botón de icono necesita tooltip"
    );
    let (rect, resp) = ui.allocate_exact_size(vec2(ICON_BTN, ICON_BTN), Sense::click());
    let hover = fade(
        ui,
        resp.id.with("hover"),
        enabled && resp.hovered(),
        ANIM_FAST,
    );
    let press = fade(
        ui,
        resp.id.with("press"),
        enabled && resp.is_pointer_button_down_on(),
        ANIM_FAST,
    );
    let fill = mix(
        mix(p.panel, p.container_hi, hover),
        p.container,
        press * 0.5,
    );
    ui.painter()
        .rect_filled(rect, R_CONTROL, fill.gamma_multiply(hover.max(press)));
    let color = if enabled {
        p.text
    } else {
        p.dim.linear_multiply(0.45)
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        code,
        FontId::proportional(18.0),
        color,
    );
    let resp = resp.on_hover_text(tooltip);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Tarjeta con borde fino.
pub fn card(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(p.container)
        .stroke(Stroke::new(1.0, mix(p.container, p.border, 0.6)))
        .corner_radius(R_CARD)
        .inner_margin(SP_L)
        .show(ui, |ui| {
            // Los controles de una tarjeta se comparan entre sí (reglas §7.3).
            checks::push_group();
            add(ui);
            checks::pop_group();
        });
}

/// Bloque del inspector: una tarjeta con borde y su título arriba, para que se vea qué campos van juntos.
pub fn block(ui: &mut Ui, p: &Palette, title: &str, add: impl FnOnce(&mut Ui)) {
    card(ui, p, |ui| {
        ui.set_width(ui.available_width());
        ui.add(
            egui::Label::new(
                RichText::new(title.to_uppercase())
                    .size(FONT_SMALL)
                    .color(p.accent),
            )
            .truncate(),
        );
        // Los campos de un bloque van más juntos que los del resto de la pantalla.
        ui.spacing_mut().item_spacing.y = SP_XS;
        add(ui);
    });
    ui.add_space(SP_S);
}

/// Título de sección del inspector: mayúsculas, tenue; con `open` es colapsable y devuelve si el cuerpo se ve. Deja
/// `SECTION_GAP` antes del título y `SP_S` entre el título y el primer campo (reglas §2.10).
pub fn section(ui: &mut Ui, p: &Palette, title: &str, open: Option<&mut bool>) -> bool {
    checks::new_group();
    // El espacio entre widgets (`item_spacing.y`) ya suma a ambos lados; se descuenta para que quede justo.
    // La primera sección de una zona no deja espacio arriba: el contenido arranca arriba (reglas §2.9).
    if ui.min_rect().height() > 0.5 {
        ui.add_space((SECTION_GAP - ui.spacing().item_spacing.y).max(0.0));
    }
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let shown = open.as_deref().copied().unwrap_or(true);
    let mut x = rect.left();
    if open.is_some() {
        let caret = if shown {
            crate::icon::CARET_DOWN
        } else {
            crate::icon::CARET_RIGHT
        };
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            caret,
            FontId::proportional(FONT_SMALL),
            p.dim,
        );
        x += SP_XL - SP_S;
    }
    let clip = ui.painter().with_clip_rect(rect);
    clip.text(
        egui::pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        title.to_uppercase(),
        FontId::proportional(FONT_SMALL),
        p.dim,
    );
    if let Some(open) = open
        && resp.clicked()
    {
        *open = !*open;
    }
    shown
}

/// Etiqueta de campo: 12, tenue y recortada.
fn field_label(ui: &mut Ui, p: &Palette, label: &str) {
    ui.add(egui::Label::new(RichText::new(label).size(FONT_SMALL).color(p.dim)).truncate())
        .on_hover_text(label);
}

/// Fila de propiedad: la etiqueta a la izquierda, en una columna de ancho `LABEL_W`, y el control en el resto de la fila
/// (que lo dimensiona con `fill_width` o con un `W_*`). Si el ancho es menor que `PROP_STACK_BELOW`, se apilan como en
/// `field`. Solo para interruptores, números, colores, deslizadores y valores de solo lectura (reglas §2.10).
pub fn prop_row(ui: &mut Ui, p: &Palette, label: &str, control: impl FnOnce(&mut Ui)) {
    if ui.available_width() < PROP_STACK_BELOW {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = SP_XS;
            field_label(ui, p, label);
            control(ui);
        });
    } else {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                vec2(LABEL_W, CONTROL_H),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(vec2(LABEL_W, CONTROL_H));
                    ui.set_max_width(LABEL_W);
                    field_label(ui, p, label);
                },
            );
            // Lo que dibuja el control, para los detectores de alineación.
            let zone_right = ui.max_rect().right();
            let start = ui.cursor().min;
            control(ui);
            let used = ui.min_rect();
            checks::control(
                checks::Pattern::Prop,
                egui::Rect::from_min_max(
                    start,
                    egui::pos2(used.right().max(start.x), used.bottom().max(start.y)),
                ),
                zone_right,
            );
        });
    }
}

/// Par de solo lectura: la etiqueta tenue a la izquierda y el valor a la derecha, recortado (reglas §2.10).
pub fn kv_row(ui: &mut Ui, p: &Palette, label: &str, value: &str) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), CONTROL_H),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            sides(
                ui,
                |ui| field_label(ui, p, label),
                |ui| {
                    let shown = if value.is_empty() { "–" } else { value };
                    ui.add(
                        egui::Label::new(RichText::new(shown).color(if value.is_empty() {
                            p.dim
                        } else {
                            p.text
                        }))
                        .truncate(),
                    )
                    .on_hover_text(shown);
                },
            );
        },
    );
}

/// De 2 a 3 campos cortos en una fila, con el ancho repartido en partes iguales (por ejemplo Citación y Fin). `add`
/// dibuja el campo `i`, normalmente con `field`.
pub fn field_group(ui: &mut Ui, n: usize, mut add: impl FnMut(&mut Ui, usize)) {
    debug_assert!((2..=3).contains(&n), "field_group es para 2 o 3 campos");
    ui.columns(n, |columns| {
        for (i, ui) in columns.iter_mut().enumerate() {
            checks::set_column(Some((n, i)));
            add(ui, i);
        }
        checks::set_column(None);
    });
}

/// Etiqueta arriba y, debajo, un control de formato fijo (`date_field`, `time_field`): no ocupa todo el ancho, así que no
/// entra en los detectores de alineación.
pub fn format_field(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    add: impl FnOnce(&mut Ui) -> Response,
) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = SP_XS;
        field_label(ui, p, label);
        add(ui)
    })
    .inner
}

/// Campo de fecha de formato fijo (`AAAA-MM-DD`), de ancho `W_DATE`.
pub fn date_field(ui: &mut Ui, value: &mut String) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(W_DATE)
            .char_limit(10)
            .hint_text("AAAA-MM-DD"),
    )
}

/// Campo de hora de formato fijo (`HH:MM`), de ancho `W_TIME`.
pub fn time_field(ui: &mut Ui, value: &mut String) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(W_TIME)
            .char_limit(5)
            .hint_text("HH:MM"),
    )
}

/// Campo numérico de ancho `W_NUM`; la unidad va dentro del control (`suffix`, por ejemplo « mm»), nunca al lado.
pub fn number_field<N: egui::emath::Numeric>(
    ui: &mut Ui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    suffix: &str,
) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.x = W_NUM;
        ui.add(egui::DragValue::new(value).range(range).suffix(suffix))
    })
    .inner
}

/// Control de arrastre de formato fijo, de ancho `W_NUM`; la unidad va dentro (`suffix`) y el prefijo también (`prefix`).
pub fn drag_field(ui: &mut Ui, drag: egui::DragValue<'_>) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.x = W_NUM;
        ui.add(drag)
    })
    .inner
}

/// Fila de propiedad con un número (`DragValue`): la etiqueta a la izquierda y el control de ancho `W_NUM`. Devuelve si cambió.
pub fn drag_prop(ui: &mut Ui, p: &Palette, label: &str, drag: egui::DragValue<'_>) -> bool {
    let mut changed = false;
    prop_row(ui, p, label, |ui| changed = drag_field(ui, drag).changed());
    changed
}

/// Fila de propiedad con una casilla de verificación (sin texto propio: la etiqueta es la de la fila).
pub fn check_prop(ui: &mut Ui, p: &Palette, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    prop_row(ui, p, label, |ui| {
        changed = ui.checkbox(value, "").changed()
    });
    changed
}

/// Desplegable apilado: la etiqueta arriba y, debajo, el desplegable a todo el ancho de la zona con su valor recortado.
/// `add` dibuja las opciones; la lista mide como mínimo lo mismo que el campo.
pub fn combo_field(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected: &str,
    add: impl FnOnce(&mut Ui),
) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = SP_XS;
        field_label(ui, p, label);
        let zone_right = ui.available_rect_before_wrap().right();
        let combo = egui::ComboBox::from_id_salt(id_salt)
            .width(ui.available_width())
            .truncate()
            .selected_text(selected)
            .show_ui(ui, add);
        checks::control(checks::Pattern::Stacked, combo.response.rect, zone_right);
    });
}

/// Dato de la barra de estado: tenue, de una línea y recortado. Deja `SP_L` de separación con el dato anterior de la
/// misma fila.
pub fn status_item(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    if ui.min_rect().width() > 0.0 {
        ui.add_space((SP_L - ui.spacing().item_spacing.x).max(0.0));
    }
    ui.add(egui::Label::new(RichText::new(text).size(FONT_SMALL).color(p.dim)).truncate())
        .on_hover_text(text)
}

/// Columna de ancho máximo `max_width`, centrada en horizontal y pegada arriba (reglas §2.9). No pone márgenes: los da la
/// zona que la contiene.
pub fn centered_column(ui: &mut Ui, max_width: f32, add: impl FnOnce(&mut Ui)) {
    let avail = ui.available_width();
    let width = avail.min(max_width);
    let side = ((avail - width) / 2.0).floor().max(0.0);
    let spacing = ui.spacing().item_spacing;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = spacing;
            ui.set_width(width);
            add(ui);
        });
    });
}

/// Con qué color se dibuja una hoja.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SheetTone {
    /// Lo impreso: `paper` con tinta `paper_ink`, igual en todos los temas.
    Paper,
    /// Las hojas de escritura: los colores del tema.
    Theme,
}

/// Hoja de papel de `size` (puntos, sin zoom) con su proporción, escalada por `zoom` y reducida al ancho disponible si no
/// entra. Va centrada en horizontal y pegada arriba; `add` dibuja adentro, recortado a la hoja.
pub fn sheet(
    ui: &mut Ui,
    p: &Palette,
    tone: SheetTone,
    size: egui::Vec2,
    zoom: f32,
    add: impl FnOnce(&mut Ui),
) -> Response {
    let avail = ui.available_width();
    let width = (size.x * zoom).min(avail).max(1.0);
    let height = width * size.y / size.x.max(1.0);
    let (outer, resp) = ui.allocate_exact_size(vec2(avail, height), Sense::hover());
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            outer.left() + ((avail - width) / 2.0).floor().max(0.0),
            outer.top(),
        ),
        vec2(width, height),
    );
    let (fill, ink) = match tone {
        SheetTone::Paper => (p.paper, p.paper_ink),
        SheetTone::Theme => (p.container, p.text),
    };
    ui.painter().rect(
        rect,
        2,
        fill,
        Stroke::new(1.0, mix(fill, p.border, 0.6)),
        egui::StrokeKind::Inside,
    );
    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    inner.set_clip_rect(rect.intersect(ui.clip_rect()));
    if tone == SheetTone::Paper {
        inner.visuals_mut().override_text_color = Some(ink);
    }
    add(&mut inner);
    resp
}

/// Etiqueta de color con texto contrastado (estado, categoría).
pub fn chip(ui: &mut Ui, p: &Palette, text: &str, fill: egui::Color32) -> Response {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(R_CONTROL)
        .inner_margin(egui::Margin::symmetric(SP_S as i8, 2))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .size(FONT_SMALL)
                        .color(theme::ink_on(fill, p)),
                )
                .truncate(),
            )
        })
        .response
        .on_hover_text(text)
}

/// Icono, una frase de qué falta y el botón que lo resuelve. Devuelve si se apretó la acción.
pub fn empty_state(
    ui: &mut Ui,
    p: &Palette,
    code: &str,
    message: &str,
    action: Option<&str>,
) -> bool {
    let mut clicked = false;
    checks::note_empty_state();
    ui.vertical_centered(|ui| {
        ui.add_space(SP_XXL);
        ui.label(RichText::new(code).size(ICON_XL).color(p.dim));
        ui.add_space(SP_S);
        ui.add(egui::Label::new(RichText::new(message).color(p.dim)).wrap());
        if let Some(action) = action {
            ui.add_space(SP_M);
            clicked = ui.button(action).clicked();
        }
    });
    clicked
}

/// Control segmentado (Tabla | Storyboard). El marcador de lo elegido se desliza a la nueva posición. Devuelve el índice
/// elegido, si cambió.
pub fn segmented(ui: &mut Ui, p: &Palette, labels: &[&str], active: usize) -> Option<usize> {
    let mut picked = None;
    let mut rects = Vec::new();
    egui::Frame::new()
        .fill(p.field)
        .corner_radius(R_CONTROL)
        .inner_margin(2)
        .show(ui, |ui| {
            let marker = ui.painter().add(egui::Shape::Noop);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, label) in labels.iter().enumerate() {
                    let color = if i == active { p.accent } else { p.text };
                    let button =
                        egui::Button::new(RichText::new(*label).size(FONT_SMALL).color(color))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(Stroke::NONE);
                    let resp = ui.add(button);
                    rects.push(resp.rect);
                    if resp.clicked() && i != active {
                        picked = Some(i);
                    }
                }
            });
            if let Some(target) = rects.get(active) {
                let id = ui.scope_id().with(("segmented_marker", labels));
                let x0 = ui
                    .ctx()
                    .animate_value_with_time(id.with("x0"), target.left(), anim(ANIM));
                let x1 =
                    ui.ctx()
                        .animate_value_with_time(id.with("x1"), target.right(), anim(ANIM));
                let rect = egui::Rect::from_min_max(
                    egui::pos2(x0, target.top()),
                    egui::pos2(x1, target.bottom()),
                );
                ui.painter().set(
                    marker,
                    egui::Shape::rect_filled(rect, R_CONTROL, p.selection),
                );
            }
        });
    picked
}

/// Desplegable de ancho `fill_width` y texto recortado. Devuelve el índice elegido, si cambió.
pub fn dropdown(ui: &mut Ui, id: &str, options: &[String], active: usize) -> Option<usize> {
    let mut picked = None;
    let width = fill_width(ui, 120.0, 320.0);
    let current = options.get(active).map_or("", String::as_str);
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .truncate()
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (i, option) in options.iter().enumerate() {
                if ui
                    .add(egui::Button::selectable(i == active, option.as_str()).truncate())
                    .on_hover_text(option)
                    .clicked()
                {
                    picked = Some(i);
                }
            }
        });
    picked
}

/// Selector con flechas `‹ ▾ ›` para escena, jornada, etc. Devuelve el nuevo índice si cambió.
pub fn stepper_select(
    ui: &mut Ui,
    p: &Palette,
    id: &str,
    options: &[String],
    active: usize,
) -> Option<usize> {
    let mut picked = None;
    ui.horizontal(|ui| {
        if icon_button(ui, p, crate::icon::CARET_LEFT, "Anterior", active > 0).clicked()
            && active > 0
        {
            picked = Some(active - 1);
        }
        if let Some(i) = dropdown(ui, id, options, active) {
            picked = Some(i);
        }
        if icon_button(
            ui,
            p,
            crate::icon::CARET_RIGHT,
            "Siguiente",
            active + 1 < options.len(),
        )
        .clicked()
            && active + 1 < options.len()
        {
            picked = Some(active + 1);
        }
    });
    picked
}

/// Grupos de una barra que se pliegan a `⋯` cuando no entran: `priorities[i]` es la prioridad del grupo `i` (los de
/// menor prioridad se pliegan primero) y `draw(ui, i)` lo dibuja. Una sola función dibuja todos los grupos, así que
/// ellos pueden compartir el estado de la pantalla. Los anchos se miden en el cuadro anterior.
pub fn toolbar_groups(
    ui: &mut Ui,
    id: &str,
    priorities: &[u8],
    mut draw: impl FnMut(&mut Ui, usize),
) {
    let key = ui.make_persistent_id(("toolbar_widths", id));
    let widths: Vec<f32> = ui
        .data(|d| d.get_temp(key))
        .filter(|w: &Vec<f32>| w.len() == priorities.len())
        .unwrap_or_else(|| vec![0.0; priorities.len()]);
    let gap = ui.spacing().item_spacing.x * 2.0 + SP_S;
    let budget = ui.available_width() - ICON_BTN - SP_S;
    let mut hidden = vec![false; priorities.len()];
    let mut used: f32 = widths.iter().map(|w| w + gap).sum();
    while used > budget {
        // El visible de menor prioridad (el último entre iguales) es el primero en plegarse.
        let Some(next) = (0..priorities.len())
            .filter(|i| !hidden[*i])
            .min_by_key(|i| (priorities[*i], std::cmp::Reverse(*i)))
        else {
            break;
        };
        hidden[next] = true;
        used -= widths[next] + gap;
    }
    let mut measured = widths.clone();
    for i in 0..priorities.len() {
        if hidden[i] {
            continue;
        }
        let width = ui.horizontal(|ui| draw(ui, i)).response.rect.width();
        measured[i] = width;
        // Un grupo vacío no dibuja su divisor (reglas §2.3).
        if i + 1 < priorities.len() && width > 1.0 {
            ui.separator();
        }
    }
    if hidden.iter().any(|h| *h) {
        ui.menu_button(crate::icon::DOTS_THREE_VERTICAL, |ui| {
            for i in (0..priorities.len()).filter(|i| hidden[*i]) {
                ui.horizontal(|ui| draw(ui, i));
                ui.separator();
            }
        })
        .response
        .on_hover_text("Más herramientas");
    }
    // Si algún ancho cambió (primer cuadro, zoom, contenido), se vuelve a calcular enseguida.
    if measured
        .iter()
        .zip(&widths)
        .any(|(a, b)| (a - b).abs() > 0.5)
    {
        ui.ctx().request_repaint();
    }
    ui.data_mut(|d| d.insert_temp(key, measured));
}

/// Una columna de `table`: su rótulo, el ancho mínimo y cómo se reparte el resto.
pub struct TableCol {
    pub header: std::borrow::Cow<'static, str>,
    pub min: f32,
    pub kind: ColKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColKind {
    /// Ancho inicial `min` y redimensionable.
    Sized,
    /// Absorbe el ancho que sobra.
    Remainder,
}

impl TableCol {
    pub const fn new(header: &'static str, min: f32) -> Self {
        Self {
            header: std::borrow::Cow::Borrowed(header),
            min,
            kind: ColKind::Sized,
        }
    }

    pub const fn rest(header: &'static str, min: f32) -> Self {
        Self {
            header: std::borrow::Cow::Borrowed(header),
            min,
            kind: ColKind::Remainder,
        }
    }

    /// Una columna con rótulo que se arma en tiempo de ejecución (las columnas que el usuario nombra).
    pub fn named(header: &str, min: f32, kind: ColKind) -> Self {
        Self {
            header: std::borrow::Cow::Owned(header.to_string()),
            min,
            kind,
        }
    }
}

/// Tabla con las reglas de §2.5: `TableBuilder` con encabezado fijo, filas de alto `ROW_H`, columnas redimensionables y
/// recortadas, y desplazamiento horizontal solo si la suma de los mínimos no entra. `header(ui, i)` dibuja el
/// encabezado de la columna `i` (por defecto, usar `table_header_text`); `row` dibuja cada fila con `row.col(..)`.
pub fn table(
    ui: &mut Ui,
    id: &str,
    cols: &[TableCol],
    rows: usize,
    header: impl FnMut(&mut Ui, usize),
    row: impl FnMut(&mut egui_extras::TableRow),
) {
    table_with(ui, id, cols, rows, true, header, row);
}

/// Como `table`, para una tabla dentro de una zona que ya se desplaza (varias tablas una debajo de otra): la tabla
/// no tiene desplazamiento propio y crece con sus filas.
pub fn table_in_scroll(
    ui: &mut Ui,
    id: &str,
    cols: &[TableCol],
    rows: usize,
    header: impl FnMut(&mut Ui, usize),
    row: impl FnMut(&mut egui_extras::TableRow),
) {
    table_with(ui, id, cols, rows, false, header, row);
}

fn table_with(
    ui: &mut Ui,
    id: &str,
    cols: &[TableCol],
    rows: usize,
    vscroll: bool,
    mut header: impl FnMut(&mut Ui, usize),
    mut row: impl FnMut(&mut egui_extras::TableRow),
) {
    use egui_extras::{Column, TableBuilder};
    let total_min: f32 = cols.iter().map(|c| c.min).sum();
    let mark = checks::paint_mark(ui);
    let mut build = |ui: &mut Ui| {
        // Las filas se apilan sin huecos: cada una mide exactamente `ROW_H`.
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut builder = TableBuilder::new(ui)
            .id_salt(id)
            .striped(true)
            .resizable(true)
            .vscroll(vscroll)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for c in cols {
            builder = builder.column(match c.kind {
                ColKind::Sized => Column::initial(c.min).at_least(c.min.min(60.0)).clip(true),
                ColKind::Remainder => Column::remainder()
                    .at_least(c.min)
                    .clip(true)
                    .resizable(false),
            });
        }
        builder
            .header(ROW_H, |mut h| {
                for i in 0..cols.len() {
                    h.col(|ui| header(ui, i));
                }
            })
            .body(|body| body.rows(ROW_H, rows, |r| row(&mut { r })));
    };
    let table = if total_min > ui.available_width() {
        egui::ScrollArea::horizontal()
            .id_salt((id, "h"))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.set_width(total_min);
                ui.scope(|ui| build(ui)).response.rect
            })
            .inner
    } else {
        ui.scope(|ui| build(ui)).response.rect
    };
    // Ninguna celda es más alta que `ROW_H` ni tiene texto en más de una línea (reglas §2.6).
    checks::check_cells(ui, mark, table, ROW_H);
}

/// Texto de encabezado de tabla: mayúsculas, tenue y recortado.
pub fn table_header_text(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(text.to_uppercase())
                .size(FONT_SMALL)
                .color(p.dim),
        )
        .truncate(),
    )
    .on_hover_text(text);
}

/// Alineación del contenido de una columna (reglas §2.6): texto a la izquierda, números a la derecha, códigos e iconos
/// centrados.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColAlign {
    Left,
    Right,
    Center,
}

/// Una columna de `data_table`, declarada una sola vez en el módulo de la pantalla: el dibujo de las filas y la pestaña
/// COLUMNAS leen el mismo arreglo (reglas §2.6).
#[derive(Clone, Copy)]
pub struct ColumnDef {
    pub id: &'static str,
    /// Título completo: nunca se recorta. Si no entra, se le da un nombre más corto en el código.
    pub title: &'static str,
    pub min: f32,
    /// Ancho inicial; se ignora si `rest`.
    pub initial: f32,
    /// Absorbe el ancho que sobra (`Column::remainder`).
    pub rest: bool,
    pub in_compact: bool,
    pub in_full: bool,
    pub align: ColAlign,
    /// Nombre completo para el tooltip, cuando el título es una forma corta («Rodaje» por «Orden de rodaje»).
    pub tooltip: Option<&'static str>,
    /// Un clic en el título ordena por esta columna (`TableOut::header_clicked`).
    pub sortable: bool,
}

impl ColumnDef {
    /// Columna presente en las dos configuraciones, a la izquierda, de ancho inicial `min`.
    pub const fn new(id: &'static str, title: &'static str, min: f32) -> Self {
        Self {
            id,
            title,
            min,
            initial: min,
            rest: false,
            in_compact: true,
            in_full: true,
            align: ColAlign::Left,
            tooltip: None,
            sortable: false,
        }
    }

    /// Se puede ordenar por esta columna con un clic en su título.
    pub const fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    /// El nombre completo de una columna de título corto.
    pub const fn tooltip(mut self, full: &'static str) -> Self {
        self.tooltip = Some(full);
        self
    }

    /// La que absorbe el resto del ancho.
    pub const fn rest(mut self) -> Self {
        self.rest = true;
        self
    }

    pub const fn initial(mut self, initial: f32) -> Self {
        self.initial = initial;
        self
    }

    pub const fn align(mut self, align: ColAlign) -> Self {
        self.align = align;
        self
    }

    /// Solo en la configuración completa.
    pub const fn full_only(mut self) -> Self {
        self.in_compact = false;
        self
    }
}

/// Qué es cada fila de `data_table`. El que llama deja afuera las filas de un grupo cerrado.
#[derive(Clone, Copy)]
pub enum TableRowKind<'a> {
    /// Una fila de datos: `cell` dibuja sus celdas.
    Item,
    /// Encabezado de grupo (por ejemplo una escena): ocupa todo el ancho, con chevron, texto en negrita y cantidad.
    Group {
        title: &'a str,
        count: usize,
        open: bool,
    },
    /// Fila «Añadir…» al final de un grupo: todo el ancho es clicable.
    Add(&'a str),
}

/// Lo que pasó en una tabla en este cuadro, por índice de fila.
#[derive(Default)]
pub struct TableOut {
    pub clicked: Option<usize>,
    pub double_clicked: Option<usize>,
    /// Se pulsó el encabezado de un grupo (hay que abrirlo o cerrarlo).
    pub toggled: Option<usize>,
    /// Se pulsó una fila «Añadir…».
    pub added: Option<usize>,
    /// Se pulsó el título de una columna ordenable (su `id`).
    pub header_clicked: Option<&'static str>,
}

/// Texto de una celda: una línea, recortado, con el texto completo en el tooltip; un valor vacío se muestra como «–» tenue.
pub fn cell_text(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    if text.trim().is_empty() {
        return ui.add(egui::Label::new(RichText::new("–").color(p.dim)).truncate());
    }
    ui.add(egui::Label::new(text).truncate())
        .on_hover_text(text)
}

/// Tabla de las reglas de §2.6 sobre `TableBuilder`: encabezado fijo con divisores entre columnas, filas de alto
/// `ROW_H`, columnas `ColumnDef` redimensionables y recortadas, franjas, y desplazamiento horizontal solo si la suma de los
/// mínimos no entra. La fila `selected` se pinta con `selection` sin cambiar el color del texto; al pasar el mouse,
/// `container_hi`. `cell(ui, fila, columna)` dibuja una celda de una fila `Item`, de una sola línea y alineada según la
/// columna.
#[allow(clippy::too_many_arguments)]
pub fn data_table(
    ui: &mut Ui,
    p: &Palette,
    id: &str,
    cols: &[ColumnDef],
    compact: bool,
    rows: &[TableRowKind],
    selected: Option<usize>,
    cell: impl FnMut(&mut Ui, usize, &ColumnDef),
) -> TableOut {
    data_table_with(
        ui,
        p,
        id,
        cols,
        compact,
        ROW_H,
        true,
        rows,
        selected,
        cell,
        |_, _| {},
    )
}

/// Como `data_table`, con el alto de fila (`ROW_H_TALL` si hay miniaturas), con `vscroll` en falso para una tabla dentro de
/// una zona que ya se desplaza y con el menú contextual de las filas `Item`: `menu(ui, fila)` dibuja sus opciones.
#[allow(clippy::too_many_arguments)]
pub fn data_table_with(
    ui: &mut Ui,
    p: &Palette,
    id: &str,
    cols: &[ColumnDef],
    compact: bool,
    row_h: f32,
    vscroll: bool,
    rows: &[TableRowKind],
    selected: Option<usize>,
    mut cell: impl FnMut(&mut Ui, usize, &ColumnDef),
    mut menu: impl FnMut(&mut Ui, usize),
) -> TableOut {
    use egui_extras::{Column, TableBuilder};
    let visible: Vec<&ColumnDef> = cols
        .iter()
        .filter(|c| if compact { c.in_compact } else { c.in_full })
        .collect();
    let mut out = TableOut::default();
    if visible.is_empty() {
        return out;
    }
    let total_min: f32 = visible.iter().map(|c| c.min).sum();
    let hover_key = ui.make_persistent_id(("data_table_hover", id));
    let hovered_before: Option<usize> = ui.data(|d| d.get_temp(hover_key));
    let mut hovered_now = None;
    let mark = checks::paint_mark(ui);
    let table_clip = ui.clip_rect();
    let mut build = |ui: &mut Ui| {
        // Las filas se apilan sin huecos: cada una mide exactamente su alto.
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.visuals_mut().widgets.hovered.weak_bg_fill = p.container_hi;
        // Solo el encabezado lleva divisores entre columnas (los dibuja `data_table`); el cuerpo, solo las franjas.
        ui.visuals_mut().widgets.noninteractive.bg_stroke = Stroke::NONE;
        let mut builder = TableBuilder::new(ui)
            .id_salt(id)
            .striped(true)
            .resizable(true)
            .vscroll(vscroll)
            .sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for c in &visible {
            builder = builder.column(if c.rest {
                Column::remainder()
                    .at_least(c.min)
                    .clip(true)
                    .resizable(false)
            } else {
                Column::initial(c.initial).at_least(c.min).clip(true)
            });
        }
        let last = visible.len() - 1;
        builder
            .header(ROW_H, |mut h| {
                for (i, c) in visible.iter().enumerate() {
                    h.col(|ui| {
                        ui.painter().rect_filled(ui.max_rect(), 0, p.container);
                        ui.add_space(ROW_PAD_X);
                        // El título nunca se recorta; la celda sí, así que un título que no entra se ve en la prueba.
                        let title = ui
                            .add(
                                egui::Label::new(
                                    RichText::new(c.title.to_uppercase())
                                        .size(FONT_SMALL)
                                        .color(p.dim),
                                )
                                .wrap_mode(egui::TextWrapMode::Extend)
                                .sense(if c.sortable {
                                    Sense::click()
                                } else {
                                    Sense::hover()
                                }),
                            )
                            .on_hover_text(c.tooltip.unwrap_or(c.title));
                        if c.sortable && title.clicked() {
                            out.header_clicked = Some(c.id);
                        }
                        if i < last {
                            let r = ui.max_rect();
                            ui.painter().vline(
                                r.right() - 0.5,
                                r.y_range(),
                                Stroke::new(1.0, mix(p.panel, p.border, 0.6)),
                            );
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, rows.len(), |mut row| {
                    let n = row.index();
                    let kind = rows[n];
                    let is_item = matches!(kind, TableRowKind::Item);
                    row.set_selected(is_item && selected == Some(n));
                    row.set_hovered(hovered_before == Some(n));
                    let mut left = f32::NAN;
                    for (i, c) in visible.iter().enumerate() {
                        row.col(|ui| {
                            let r = ui.max_rect();
                            if i == 0 {
                                left = r.left();
                            }
                            match kind {
                                TableRowKind::Item => {
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                    let layout = match c.align {
                                        ColAlign::Left => {
                                            egui::Layout::left_to_right(egui::Align::Center)
                                        }
                                        ColAlign::Right => {
                                            egui::Layout::right_to_left(egui::Align::Center)
                                        }
                                        ColAlign::Center => egui::Layout::centered_and_justified(
                                            egui::Direction::LeftToRight,
                                        ),
                                    };
                                    egui::Frame::NONE
                                        .inner_margin(egui::Margin::symmetric(ROW_PAD_X as i8, 0))
                                        .show(ui, |ui| {
                                            ui.with_layout(layout, |ui| cell(ui, n, c));
                                        });
                                }
                                TableRowKind::Group { title, count, open } if i == last => {
                                    // Se dibuja con el rectángulo completo de la fila, no con el de una columna.
                                    let full = egui::Rect::from_min_max(
                                        egui::pos2(left, r.top()),
                                        egui::pos2(r.right(), r.bottom()),
                                    );
                                    let clip = egui::Rect::from_min_max(
                                        egui::pos2(table_clip.left(), ui.clip_rect().top()),
                                        egui::pos2(table_clip.right(), ui.clip_rect().bottom()),
                                    );
                                    let painter =
                                        ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(clip);
                                    // Una sola celda de lado a lado, de otro color que las filas de datos.
                                    let fill = mix(
                                        p.container,
                                        p.accent,
                                        if hovered_before == Some(n) {
                                            0.34
                                        } else {
                                            0.24
                                        },
                                    );
                                    painter.rect_filled(full, 0, fill);
                                    let cy = full.center().y;
                                    let caret = if open {
                                        icon::CARET_DOWN
                                    } else {
                                        icon::CARET_RIGHT
                                    };
                                    painter.text(
                                        egui::pos2(full.left() + ROW_PAD_X, cy),
                                        Align2::LEFT_CENTER,
                                        caret,
                                        FontId::proportional(FONT_SMALL),
                                        p.accent,
                                    );
                                    let text = painter.text(
                                        egui::pos2(full.left() + ROW_PAD_X + SP_L + SP_XS, cy),
                                        Align2::LEFT_CENTER,
                                        title,
                                        FontId::new(FONT_BODY, bold_family()),
                                        p.text,
                                    );
                                    painter.text(
                                        egui::pos2(text.right() + SP_S, cy),
                                        Align2::LEFT_CENTER,
                                        count.to_string(),
                                        FontId::proportional(FONT_SMALL),
                                        p.dim,
                                    );
                                }
                                TableRowKind::Add(label) if i == last => {
                                    let full = egui::Rect::from_min_max(
                                        egui::pos2(left, r.top()),
                                        egui::pos2(r.right(), r.bottom()),
                                    );
                                    let clip = egui::Rect::from_min_max(
                                        egui::pos2(table_clip.left(), ui.clip_rect().top()),
                                        egui::pos2(table_clip.right(), ui.clip_rect().bottom()),
                                    );
                                    let painter =
                                        ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(clip);
                                    if hovered_before == Some(n) {
                                        painter.rect_filled(full, 0, p.container_hi);
                                    }
                                    painter.text(
                                        egui::pos2(full.left() + ROW_PAD_X, full.center().y),
                                        Align2::LEFT_CENTER,
                                        format!("{}  {label}", icon::PLUS),
                                        FontId::proportional(FONT_BODY),
                                        p.dim,
                                    );
                                }
                                TableRowKind::Group { .. } | TableRowKind::Add(_) => {}
                            }
                        });
                    }
                    let resp = row.response();
                    if resp.hovered() {
                        hovered_now = Some(n);
                    }
                    match kind {
                        TableRowKind::Item => {
                            resp.context_menu(|ui| menu(ui, n));
                            if resp.clicked() {
                                out.clicked = Some(n);
                            }
                            if resp.double_clicked() {
                                out.double_clicked = Some(n);
                            }
                        }
                        TableRowKind::Group { .. } => {
                            if resp.clicked() {
                                out.toggled = Some(n);
                            }
                        }
                        TableRowKind::Add(_) => {
                            if resp.clicked() {
                                out.added = Some(n);
                            }
                        }
                    }
                });
            });
    };
    let table = if total_min > ui.available_width() {
        egui::ScrollArea::horizontal()
            .id_salt((id, "h"))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.set_width(total_min);
                ui.scope(|ui| build(ui)).response.rect
            })
            .inner
    } else {
        ui.scope(|ui| build(ui)).response.rect
    };
    // Ninguna celda es más alta que `ROW_H` ni tiene texto en más de una línea (reglas §2.6).
    checks::check_cells(ui, mark, table, row_h);
    ui.data_mut(|d| d.insert_temp(hover_key, hovered_now));
    out
}

/// Deslizador con su valor y unidad a la derecha, y una nota opcional debajo (por ejemplo «Eq. full frame: 80 mm»).
/// `decimals` son los decimales del valor, que se muestra con coma. Devuelve si cambió.
pub fn slider_value(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    unit: &str,
    decimals: usize,
    note: Option<&str>,
) -> bool {
    let mut changed = false;
    sides(
        ui,
        |ui| {
            ui.add(egui::Label::new(RichText::new(label).size(FONT_SMALL).color(p.dim)).truncate());
        },
        |ui| {
            // El valor se puede escribir (o arrastrar) además de usar el deslizador.
            let suffix = format!(
                "{}{unit}",
                if unit.starts_with('°') || unit.is_empty() {
                    ""
                } else {
                    " "
                }
            );
            let (shown_suffix, parsed_suffix) = (suffix.clone(), suffix.trim().to_string());
            let r = ui.add(
                egui::DragValue::new(value)
                    .range(range.clone())
                    .speed(((range.end() - range.start()) / 400.0).max(0.01))
                    .custom_formatter(move |n, _| {
                        format!("{n:.decimals$}{shown_suffix}").replace('.', ",")
                    })
                    .custom_parser(move |s| {
                        s.trim()
                            .trim_end_matches(parsed_suffix.as_str())
                            .trim()
                            .replace(',', ".")
                            .parse()
                            .ok()
                    }),
            );
            changed |= r.changed();
        },
    );
    ui.spacing_mut().slider_width = fill_width(ui, 80.0, 600.0);
    let resp = ui.add(
        egui::Slider::new(value, range)
            .show_value(false)
            .step_by(10f64.powi(-(decimals as i32))),
    );
    // Solo cuenta lo que hace quien lo toca: el redondeo de un valor que cambia solo (una lente que se interpola mientras
    // se reproduce) no es una edición.
    changed |=
        resp.changed() && (resp.is_pointer_button_down_on() || resp.dragged() || resp.has_focus());
    if let Some(note) = note {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.label(RichText::new(note).size(FONT_SMALL).color(p.dim));
        });
    }
    changed
}

/// Ancho de un diálogo (reglas §2.11).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DialogWidth {
    /// `DIALOG_MAX`.
    Normal,
    /// `DIALOG_WIDE`: un diálogo con dos columnas reales (editor y datos).
    Wide,
}

/// Diálogo modal centrado en la ventana, sin redimensionar. Ancho `min(DIALOG_MAX, ventana − 2 × SP_XL)` y alto de hasta el
/// 85 % de la ventana. Tiene tres partes: la cabecera (título a la izquierda y `X` contra el borde derecho), el cuerpo con
/// su desplazamiento y el pie fijo con los botones contra el borde derecho. `footer` dibuja los botones de derecha a
/// izquierda: primero la acción principal (queda en el extremo, en color de acento) y después «Cancelar». Entra con un
/// fundido y una escala de 0,97 a 1 (reglas §6.7). Devuelve si hay que cerrarlo (`X`, Esc o clic afuera).
pub fn dialog(
    ctx: &egui::Context,
    p: &Palette,
    id: &str,
    title: &str,
    width: DialogWidth,
    body: impl FnOnce(&mut Ui),
    footer: impl FnOnce(&mut Ui),
) -> bool {
    let key = egui::Id::unique("kit_dialog").with(id);
    let t = egui::emath::easing::cubic_out(ctx.animate_bool_with_time(key, true, anim(ANIM)));
    let screen = ctx.content_rect();
    let max = match width {
        DialogWidth::Normal => DIALOG_MAX,
        DialogWidth::Wide => DIALOG_WIDE,
    };
    let outer_w = max.min(screen.width() - 2.0 * SP_XL).max(0.0);
    let inner_w = (outer_w - 2.0 * DIALOG_PAD).max(0.0);
    // Lo que ocupan la cabecera, el pie y sus separaciones; el cuerpo se queda con el resto del 85 %.
    let chrome = 2.0 * DIALOG_PAD + CONTROL_H + SP_M + SP_XL + CONTROL_H;
    let body_h = (screen.height() * 0.85 - chrome).max(ROW_H);
    let mut close_button = false;
    let modal = egui::Modal::new(key.with("modal"))
        .backdrop_color(p.shadow.gamma_multiply(t))
        .frame(
            egui::Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(R_FLOAT)
                .shadow(egui::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: p.shadow,
                })
                .inner_margin(DIALOG_PAD as i8),
        )
        .show(ctx, |ui| {
            ui.set_opacity(t);
            ui.set_width(inner_w);
            ui.spacing_mut().item_spacing.y = 0.0;
            sides(
                ui,
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(title)
                                .font(FontId::new(FONT_CARD_TITLE, bold_family()))
                                .color(p.text),
                        )
                        .truncate(),
                    )
                    .on_hover_text(title);
                },
                |ui| {
                    close_button = icon_button(ui, p, icon::X, "Cerrar (Esc)", true).clicked();
                },
            );
            ui.add_space(SP_M);
            egui::ScrollArea::vertical()
                .id_salt(("kit_dialog_body", id))
                .auto_shrink([false, true])
                .max_height(body_h)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = SP_M;
                    body(ui)
                });
            ui.add_space(SP_XL);
            sides(ui, |_| {}, footer);
        });
    // Escala de 0,97 a 1 alrededor del centro del diálogo.
    let scale = 0.97 + 0.03 * t;
    if scale < 1.0 {
        let center = modal.response.rect.center();
        ctx.set_transform_layer(
            modal.response.layer_id,
            egui::emath::TSTransform::new(center.to_vec2() * (1.0 - scale), scale),
        );
    }
    modal.should_close() || close_button
}

/// Botón de la acción principal de un diálogo, en color de acento. Deshabilitado, su tooltip dice por qué.
pub fn primary_button(ui: &mut Ui, p: &Palette, label: &str, enabled: Option<&str>) -> Response {
    let button = egui::Button::new(RichText::new(label).color(p.on_accent)).fill(p.accent);
    let resp = ui.add_enabled(enabled.is_none(), button);
    match enabled {
        Some(why) => resp.on_disabled_hover_text(why),
        None => resp,
    }
}

/// Diálogo de confirmación: dice qué se pierde y ofrece «Cancelar» y la acción con su verbo, en color de peligro.
/// Devuelve `Some(true)` si se confirmó, `Some(false)` si se canceló.
pub fn confirm(
    ctx: &egui::Context,
    p: &Palette,
    id: &str,
    title: &str,
    text: &str,
    action: &str,
) -> Option<bool> {
    let mut answer = None;
    let close = dialog(
        ctx,
        p,
        id,
        title,
        DialogWidth::Normal,
        |ui| {
            ui.add(egui::Label::new(RichText::new(text).color(p.text)).wrap());
        },
        |ui| {
            if ui.button(RichText::new(action).color(p.danger)).clicked() {
                answer = Some(true);
            }
            if ui.button("Cancelar").clicked() {
                answer = Some(false);
            }
        },
    );
    answer.or(close.then_some(false))
}

/// Mensaje breve abajo al centro: entra con un fundido subiendo 8 y sale con un fundido (reglas §6.7). `until` es el
/// instante (segundos de la app) en que deja de mostrarse. Devuelve si todavía se muestra.
pub fn toast(ctx: &egui::Context, p: &Palette, text: &str, until: f64) -> bool {
    let now = ctx.input(|i| i.time);
    if now > until + anim(ANIM) as f64 {
        return false;
    }
    let key = egui::Id::unique("kit_toast");
    let t = ctx.animate_bool_with_time(key, now <= until, anim(ANIM));
    let rise = (1.0 - egui::emath::easing::cubic_out(t.min(1.0))) * 8.0;
    egui::Area::new(egui::Id::unique("kit_toast_area"))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(STATUS_H + SP_XL) + rise))
        .show(ctx, |ui| {
            ui.set_opacity(t);
            egui::Frame::new()
                .fill(p.container_hi)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(R_CARD)
                .shadow(egui::Shadow {
                    offset: [0, 2],
                    blur: 8,
                    spread: 0,
                    color: p.shadow,
                })
                .inner_margin(egui::Margin::symmetric(SP_L as i8, SP_S as i8))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text).color(p.text)).truncate());
                });
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    true
}

/// La vista de un lienzo: dónde está el origen respecto de la esquina y cuántos puntos mide un metro.
#[derive(Clone, Copy, Debug)]
pub struct CanvasView {
    pub offset: egui::Vec2,
    pub scale: f32,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self {
            offset: vec2(SP_XXL + SP_XL, SP_XXL + SP_XL),
            scale: 60.0,
        }
    }
}

/// Lienzo con zoom alrededor del cursor (rueda), paneo con el botón del medio o con Espacio, grilla de 1 m y
/// `painter_at`: nada se pinta fuera del rectángulo. `add` recibe el rectángulo, el painter y la respuesta.
pub fn canvas(
    ui: &mut Ui,
    p: &Palette,
    view: &mut CanvasView,
    scale_range: (f32, f32),
    add: impl FnOnce(&egui::Painter, egui::Rect, &Response, &CanvasView),
) -> Response {
    let size = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(
        vec2(size.x.max(60.0), size.y.max(60.0)),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0, p.canvas);
    let space = ui.input(|i| i.key_down(egui::Key::Space));
    if resp.dragged_by(egui::PointerButton::Middle)
        || (space && resp.dragged_by(egui::PointerButton::Primary))
    {
        view.offset += resp.drag_delta();
    }
    let hover = resp.hover_pos();
    let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
    if let (Some(pos), true) = (hover, scroll != 0.0 || pinch != 1.0) {
        let world = (
            (pos.x - rect.min.x - view.offset.x) / view.scale,
            (pos.y - rect.min.y - view.offset.y) / view.scale,
        );
        view.scale =
            (view.scale * (scroll * 0.0015).exp() * pinch).clamp(scale_range.0, scale_range.1);
        view.offset = vec2(
            pos.x - rect.min.x - world.0 * view.scale,
            pos.y - rect.min.y - world.1 * view.scale,
        );
    }
    let to_screen = |x: f32, y: f32| {
        egui::pos2(
            rect.min.x + view.offset.x + x * view.scale,
            rect.min.y + view.offset.y + y * view.scale,
        )
    };
    if view.scale >= 24.0 {
        let faint = Stroke::new(1.0, p.grid.gamma_multiply(0.5));
        let firm = Stroke::new(1.0, p.grid);
        let (x0, y0) = (-view.offset.x / view.scale, -view.offset.y / view.scale);
        let (x1, y1) = (
            (rect.width() - view.offset.x) / view.scale,
            (rect.height() - view.offset.y) / view.scale,
        );
        for gx in x0.floor() as i32..=x1.ceil() as i32 {
            let x = to_screen(gx as f32, 0.0).x;
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                if gx % 5 == 0 { firm } else { faint },
            );
        }
        for gy in y0.floor() as i32..=y1.ceil() as i32 {
            let y = to_screen(0.0, gy as f32).y;
            painter.line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                if gy % 5 == 0 { firm } else { faint },
            );
        }
    }
    add(&painter, rect, &resp, view);
    resp
}

/// Título de pantalla: 19 pt en negrita, recortado.
pub fn screen_title(ui: &mut Ui, p: &Palette, title: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(title)
                .font(FontId::new(FONT_TITLE, FontFamily::Proportional))
                .color(p.text),
        )
        .truncate(),
    )
    .on_hover_text(title);
}

// ---- Lo que venía de widgets.rs ----

pub struct Row<'a> {
    pub depth: u8,
    /// `Some(abierto)` si la fila despliega hijos.
    pub caret: Option<bool>,
    pub icon: Option<&'a str>,
    pub label: &'a str,
    pub selected: bool,
}

/// Fila de la barra lateral: la seleccionada lleva fondo marrón, texto naranja y una flecha a la derecha.
pub fn tree_row(ui: &mut Ui, p: &Palette, row: Row) -> Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), crate::ui::tokens::ROW_H),
        Sense::click(),
    );
    let hover = fade(ui, resp.id.with("hover"), resp.hovered(), ANIM_FAST);
    if row.selected {
        // La fila activa lleva el fondo de selección completo, sin líneas ni marcas sueltas.
        ui.painter().rect_filled(rect, R_CONTROL, p.selection);
    } else {
        ui.painter().rect_filled(
            rect,
            R_CONTROL,
            mix(p.panel, p.container, 0.7).gamma_multiply(hover),
        );
    }
    let (icon_color, text_color) = if row.selected {
        (p.accent, p.accent)
    } else {
        (p.dim, p.text)
    };
    let cy = rect.center().y;
    let base = rect.left() + 12.0 + row.depth as f32 * 18.0;
    if let Some(open) = row.caret {
        ui.painter().text(
            egui::pos2(base, cy),
            Align2::LEFT_CENTER,
            if open {
                icon::CARET_DOWN
            } else {
                icon::CARET_RIGHT
            },
            FontId::proportional(12.0),
            p.dim,
        );
    }
    let mut x = base + 20.0;
    if let Some(code) = row.icon {
        ui.painter().text(
            egui::pos2(x, cy),
            Align2::LEFT_CENTER,
            code,
            FontId::proportional(16.0),
            icon_color,
        );
        x += 26.0;
    }
    let font = if row.selected {
        FontId::new(14.0, bold_family())
    } else {
        FontId::proportional(14.0)
    };
    let clip = ui.painter().with_clip_rect(egui::Rect::from_min_max(
        egui::pos2(x, rect.top()),
        egui::pos2(rect.right() - 28.0, rect.bottom()),
    ));
    clip.text(
        egui::pos2(x, cy),
        Align2::LEFT_CENTER,
        row.label,
        font,
        text_color,
    );
    resp.on_hover_text(row.label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Texto en mayúsculas del color de acento con un icono, como «+ AÑADIR DOCUMENTO».
pub fn accent_button(ui: &mut Ui, p: &Palette, icon_code: &str, label: &str) -> Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let r1 = ui.add(
            egui::Label::new(icon_text(icon_code, 18.0).color(p.accent)).sense(Sense::click()),
        );
        let r2 = ui.add(
            egui::Label::new(
                RichText::new(label.to_uppercase())
                    .font(FontId::new(13.0, bold_family()))
                    .color(p.accent),
            )
            .sense(Sense::click()),
        );
        (r1 | r2).on_hover_cursor(egui::CursorIcon::PointingHand)
    })
    .inner
}

/// Pestaña de un panel (FORMATO / COMENTARIOS); la activa va en el color de acento. El subrayado lo dibuja `tab_row`,
/// que lo desliza de una pestaña a otra.
pub fn tab(ui: &mut Ui, p: &Palette, label: &str, width: f32, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, TAB_H), Sense::click());
    let hover = fade(ui, resp.id.with("hover"), resp.hovered(), ANIM_FAST);
    ui.painter()
        .rect_filled(rect, 0, p.container_hi.gamma_multiply(hover * 0.6));
    let color = if active {
        p.accent
    } else {
        mix(p.dim, p.text, hover)
    };
    let font = FontId::new(FONT_SMALL, FontFamily::Proportional);
    let clip = ui.painter().with_clip_rect(rect);
    clip.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label.to_uppercase(),
        font,
        color,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Fila de pestañas que reparte el ancho según el largo de cada rótulo, con el subrayado de la activa pegado al
/// divisor y deslizándose a la nueva posición. Devuelve la que se pulsó.
pub fn tab_row(ui: &mut Ui, p: &Palette, labels: &[&str], active: usize) -> Option<usize> {
    let font = FontId::new(FONT_SMALL, FontFamily::Proportional);
    let natural: Vec<f32> = labels
        .iter()
        .map(|l| {
            ui.fonts_mut(|f| {
                f.layout_no_wrap(l.to_uppercase(), font.clone(), p.text)
                    .size()
                    .x
            }) + SP_L
                + SP_XS
        })
        .collect();
    let available = ui.available_width().floor();
    let total: f32 = natural.iter().sum();
    // Si no entran, se encogen todas por igual en vez de ensanchar el panel.
    let (scale, spare) = if total > available {
        (available / total, 0.0)
    } else {
        (1.0, (available - total) / labels.len() as f32)
    };
    let widths: Vec<f32> = natural
        .iter()
        .map(|w| (w * scale).floor() + spare)
        .collect();
    let mut clicked = None;
    let mut rects = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, label) in labels.iter().enumerate() {
            let resp = tab(ui, p, label, widths[i], i == active);
            rects.push(resp.rect);
            if resp.clicked() {
                clicked = Some(i);
            }
        }
    });
    if let Some(target) = rects.get(active) {
        let id = ui.scope_id().with(("tab_underline", labels));
        let x0 = ui
            .ctx()
            .animate_value_with_time(id.with("x0"), target.left(), anim(ANIM));
        let x1 = ui
            .ctx()
            .animate_value_with_time(id.with("x1"), target.right(), anim(ANIM));
        ui.painter().line_segment(
            [
                egui::pos2(x0, target.bottom() - 1.0),
                egui::pos2(x1, target.bottom() - 1.0),
            ],
            Stroke::new(2.0, p.accent),
        );
    }
    clicked
}

/// Campo apilado: la etiqueta (12, tenue) arriba, con `SP_XS` de separación, y el control debajo a todo el ancho de la
/// zona; el borde se vuelve de acento al enfocar. Con `rows` de 1 es de una línea y mide `CONTROL_H`; con más, es un área
/// de texto (para eso, `textarea`). Entre un campo y el siguiente quedan `SP_S` (el `item_spacing` del tema).
pub fn field(ui: &mut Ui, p: &Palette, label: &str, text: &mut String, rows: usize) -> bool {
    field_with(ui, p, label, text, rows, false)
}

/// Área de texto con etiqueta: se define por filas (`desired_rows`), con un mínimo de 3 (reglas §3).
pub fn textarea(ui: &mut Ui, p: &Palette, label: &str, text: &mut String, rows: usize) -> bool {
    field_with(ui, p, label, text, rows.max(3), false)
}

/// Como `field`; con `autofocus` el cursor queda en el campo al abrirse.
pub fn field_with(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    text: &mut String,
    rows: usize,
    autofocus: bool,
) -> bool {
    field_state(ui, p, label, text, rows, autofocus).0
}

/// Como `field_with`; devuelve además si el campo tiene el foco (para las pantallas que importan texto al campo activo).
pub fn field_state(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    text: &mut String,
    rows: usize,
    autofocus: bool,
) -> (bool, bool) {
    field_full(ui, p, label, text, rows, autofocus, "")
}

/// Como `field_state`, con un texto de ayuda (`hint`) que se ve mientras el campo está vacío.
pub fn field_full(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    text: &mut String,
    rows: usize,
    autofocus: bool,
    hint: &str,
) -> (bool, bool) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = SP_XS;
        field_label(ui, p, label);
        let id = ui.next_auto_id();
        if autofocus {
            ui.memory_mut(|m| m.request_focus(id.with("edit")));
        }
        let focused = ui.memory(|m| m.has_focus(id.with("edit")));
        let single = rows <= 1;
        let pad_x = ui.spacing().button_padding.x;
        let frame = egui::Frame::new()
            .fill(p.field)
            .stroke(Stroke::new(1.0, if focused { p.accent } else { p.border }))
            .corner_radius(R_CONTROL)
            .inner_margin(egui::Margin::symmetric(
                pad_x as i8,
                if single { 0 } else { SP_S as i8 - 1 },
            ));
        let margin = frame.total_margin().sum();
        let inner = vec2(
            (ui.available_width() - margin.x).max(0.0),
            CONTROL_H - margin.y,
        );
        let zone_right = ui.available_rect_before_wrap().right();
        let shown = frame.show(ui, |ui| {
            let edit = if single {
                egui::TextEdit::singleline(text)
            } else {
                egui::TextEdit::multiline(text).desired_rows(rows)
            };
            let edit = edit
                .id(id.with("edit"))
                .desired_width(f32::INFINITY)
                .hint_text(hint)
                .frame(egui::Frame::NONE);
            if single {
                ui.allocate_ui_with_layout(
                    inner,
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let resp = ui.add_sized(vec2(inner.x, 0.0), edit);
                        (resp.changed(), resp.has_focus())
                    },
                )
                .inner
            } else {
                ui.set_width(inner.x);
                let resp = ui.add(edit);
                (resp.changed(), resp.has_focus())
            }
        });
        // El rectángulo del campo, para los detectores de alineación (reglas §7.3).
        checks::control(checks::Pattern::Stacked, shown.response.rect, zone_right);
        shown.inner
    })
    .inner
}

/// Campo apilado cuyo control lo dibuja `add` dentro de una fila de `CONTROL_H` a todo el ancho de la zona (casillas con
/// sugerencias, desplegables): la etiqueta arriba, igual que `field`, y el rectángulo entra en los detectores de alineación.
pub fn cell_field(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    add: impl FnOnce(&mut Ui, egui::Rect) -> bool,
) -> bool {
    cell_field_with(ui, p, label, CellLook::Text, add)
}

/// Cómo se ve el campo de `cell_field_with`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CellLook {
    /// Un campo de texto con borde y fondo.
    Text,
    /// Un campo con borde y la flecha de los desplegables: se elige de una lista o se escribe.
    Dropdown,
    /// Sin marco propio: el control que va adentro ya se dibuja con el suyo (un `ComboBox`).
    Bare,
}

/// Como `cell_field`, con otro aspecto. El campo se dibuja con borde y fondo, como los demás, para que se vea que se puede
/// completar.
pub fn cell_field_with(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    look: CellLook,
    add: impl FnOnce(&mut Ui, egui::Rect) -> bool,
) -> bool {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = SP_XS;
        field_label(ui, p, label);
        let zone_right = ui.available_rect_before_wrap().right();
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), CONTROL_H), Sense::hover());
        checks::control(checks::Pattern::Stacked, rect, zone_right);
        if look != CellLook::Bare {
            let focused = ui.memory(|m| m.focused()).is_some() && ui.rect_contains_pointer(rect);
            ui.painter().rect(
                rect,
                R_CONTROL,
                p.field,
                Stroke::new(1.0, if focused { p.accent } else { p.border }),
                egui::StrokeKind::Inside,
            );
        }
        if look == CellLook::Dropdown {
            ui.painter().text(
                rect.right_center() - vec2(SP_S, 0.0),
                Align2::RIGHT_CENTER,
                icon::CARET_DOWN,
                FontId::proportional(FONT_SMALL),
                p.dim,
            );
        }
        add(ui, rect)
    })
    .inner
}

/// Una opción de una lista de sugerencias. Se elige al apretar el botón, no al soltarlo: al apretar fuera del campo
/// de texto este pierde el foco y la lista desaparece, así que al soltar ya no habría nada que recibir el clic.
pub fn suggestion_item(ui: &mut Ui, selected: bool, label: &str) -> bool {
    suggestion_item_resp(ui, selected, label).0
}

/// Como `suggestion_item`, y devuelve también la respuesta (para llevar el desplazamiento a la opción elegida con el teclado).
pub fn suggestion_item_resp(ui: &mut Ui, selected: bool, label: &str) -> (bool, Response) {
    // Solo al apretar: si también contara el soltar, la lista que queda abierta recibiría un segundo clic.
    let resp = ui.selectable_label(selected, label);
    (resp.hovered() && ui.input(|i| i.pointer.primary_pressed()), resp)
}

/// Contenedor centrado de ancho limitado con margen a los lados; nada se corta aunque la ventana sea angosta.
pub fn column(ui: &mut Ui, max_width: f32, add: impl FnOnce(&mut Ui)) {
    let avail = ui.available_width();
    let width = (avail - SP_XL * 2.0).clamp(240.0, max_width);
    let side = ((avail - width) / 2.0).max(SP_XL);
    ui.horizontal_top(|ui| {
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(width);
            add(ui);
        });
    });
}

/// Control de valor por pasos: `−  valor  +`. Devuelve -1, 0 o +1 según el botón pulsado. A diferencia de
/// un deslizador, no cambia nada mientras se mueve el mouse, así que se puede ajustar con precisión.
pub fn stepper(ui: &mut Ui, p: &Palette, value: &str, can_dec: bool, can_inc: bool) -> i32 {
    let mut delta = 0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if icon_button(ui, p, icon::MINUS, "Menos", can_dec).clicked() && can_dec {
            delta = -1;
        }
        ui.add_sized(
            vec2(64.0, 30.0),
            egui::Label::new(
                RichText::new(value)
                    .font(FontId::new(15.0, bold_family()))
                    .color(p.text),
            ),
        );
        if icon_button(ui, p, icon::PLUS, "Más", can_inc).clicked() && can_inc {
            delta = 1;
        }
    });
    delta
}

/// Zoom de la vista: `−  100 %  +`. Devuelve -1, 0 o +1 según el botón pulsado; la app aplica el paso.
pub fn zoom_control(ui: &mut Ui, p: &Palette, zoom: u32) -> i32 {
    let (min, max) = crate::storage::VIEW_ZOOM_RANGE;
    stepper(ui, p, &format!("{zoom} %"), zoom > min, zoom < max)
}

/// Mosaico cuadrado con un icono grande y un rótulo, para el diálogo «Añadir».
pub fn tile(ui: &mut Ui, p: &Palette, code: &str, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H), Sense::click());
    let fill = if resp.hovered() {
        mix(p.container, p.primary, 0.18)
    } else {
        p.container
    };
    let border = if resp.hovered() { p.primary } else { p.border };
    ui.painter().rect(
        rect,
        6,
        fill,
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 34.0),
        Align2::CENTER_CENTER,
        code,
        FontId::proportional(30.0),
        p.accent,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 20.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(13.0),
        p.text,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Botón a un ancho dado con icono y texto centrados.
pub fn wide_button(ui: &mut Ui, p: &Palette, code: &str, label: &str, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let fill = if resp.hovered() {
        mix(p.container, p.primary, 0.18)
    } else {
        p.container
    };
    ui.painter().rect(
        rect,
        4,
        fill,
        Stroke::new(1.0, if resp.hovered() { p.primary } else { p.border }),
        egui::StrokeKind::Inside,
    );
    let font = FontId::proportional(14.0);
    let icon_w = 22.0;
    let text_w = ui.fonts_mut(|f| {
        f.layout_no_wrap(label.to_string(), font.clone(), p.text)
            .size()
            .x
    });
    let start = rect.center().x - (icon_w + text_w) / 2.0;
    ui.painter().text(
        egui::pos2(start, rect.center().y),
        Align2::LEFT_CENTER,
        code,
        FontId::proportional(17.0),
        p.accent,
    );
    ui.painter().text(
        egui::pos2(start + icon_w, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        p.text,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Limpia lo que llega al pegar desde otros programas: finales de línea de Windows o Mac, espacios raros (no
/// separables, de ancho cero), tabuladores, espacios al final de cada línea y filas en blanco de más.
pub fn clean_pasted(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text: String = text
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{feff}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{ad}' | '\u{2060}'
            )
        })
        .map(|c| {
            if c == '\t' || c == '\u{a0}' || c == '\u{2007}' || c == '\u{202f}' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        let last = lines.peek().is_none();
        let line = if last { line } else { line.trim_end() };
        if !last && line.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        if !last {
            out.push('\n');
        }
    }
    out
}

/// Una casilla de texto: mientras se escribe guarda el borrador, y al soltar el foco devuelve lo escrito. Las de texto
/// largo (resumen, notas) se parten en renglones y crecen para que se lea todo; las cortas son de una línea.
pub fn draft_cell(
    ui: &mut Ui,
    drafts: &mut std::collections::HashMap<(u64, u8), String>,
    key: (u64, u8),
    shown: &str,
    hint: &str,
    width: f32,
    wrapped: bool,
) -> Option<String> {
    let draft = drafts.entry(key).or_insert_with(|| shown.to_string());
    let edit = if wrapped {
        egui::TextEdit::multiline(draft).desired_rows(1)
    } else {
        egui::TextEdit::singleline(draft)
    };
    let resp = ui.add(edit.desired_width(width).hint_text(hint));
    // Enter confirma; el texto no lleva saltos de línea.
    let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if resp.lost_focus() || enter {
        let mut text = drafts.remove(&key)?;
        if wrapped {
            text = text.replace('\n', " ");
        }
        if enter {
            resp.surrender_focus();
        }
        return Some(text);
    }
    if !resp.has_focus() && *draft != shown {
        // Cambió desde afuera (otra pantalla, deshacer…): se muestra lo nuevo.
        drafts.remove(&key);
    }
    None
}

/// Campo apilado con borrador: mientras se escribe guarda el borrador y, al soltar el foco, devuelve lo escrito. Como
/// `draft_cell`, pero con el aspecto y las medidas de `field`.
#[allow(clippy::too_many_arguments)]
pub fn draft_field(
    ui: &mut Ui,
    p: &Palette,
    drafts: &mut std::collections::HashMap<(u64, u8), String>,
    key: (u64, u8),
    label: &str,
    shown: &str,
    hint: &str,
    rows: usize,
) -> Option<String> {
    let draft = drafts.entry(key).or_insert_with(|| shown.to_string());
    let (_, focused) = field_full(ui, p, label, draft, rows, false, hint);
    let focus_key = ui.make_persistent_id(("draft_field_focus", key));
    let was_focused = ui.data(|d| d.get_temp::<bool>(focus_key)).unwrap_or(false);
    ui.data_mut(|d| d.insert_temp(focus_key, focused));
    if was_focused && !focused {
        let text = drafts.remove(&key)?;
        return Some(if rows > 1 {
            text.replace('\n', " ")
        } else {
            text
        });
    }
    if !focused && drafts.get(&key).is_some_and(|d| d != shown) {
        // Cambió desde afuera (otra pantalla, deshacer…): se muestra lo nuevo.
        drafts.remove(&key);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_text_is_cleaned_of_foreign_whitespace_and_extra_blank_rows() {
        assert_eq!(
            clean_pasted("a \r\n\u{a0}b\t c\r\n\r\n\r\n\r\nd \u{200b}e "),
            "a\n b  c\n\nd e "
        );
        assert_eq!(clean_pasted("una línea "), "una línea ");
    }

    fn run(add: impl FnMut(&mut Ui)) {
        let ctx = egui::Context::default();
        let mut add = add;
        for n in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(300.0, 400.0),
                )),
                time: Some(n as f64 * 0.1),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| add(ui));
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn the_kit_components_draw_in_a_narrow_column() {
        let p = theme::Scheme::Tierra.palette();
        let options: Vec<String> = [
            "1. INT. CASA DE JUANA – LIVING – NOCHE",
            "2. EXT. CALLE – DÍA",
        ]
        .map(String::from)
        .to_vec();
        let mut open = true;
        run(|ui| {
            icon_button(ui, &p, crate::icon::PLUS, "Añadir", true);
            section(ui, &p, "Cámaras activas", Some(&mut open));
            prop_row(ui, &p, "Lente", |ui| {
                ui.label("50 mm");
            });
            chip(ui, &p, "Aprobado", p.success);
            segmented(ui, &p, &["Tabla", "Storyboard"], 0);
            stepper_select(ui, &p, "escena", &options, 0);
            empty_state(
                ui,
                &p,
                crate::icon::FILM_SLATE,
                "Todavía no hay escenas.",
                Some("Ir al guion"),
            );
            card(ui, &p, |ui| {
                ui.label("tarjeta");
            });
        });
    }

    #[test]
    fn toolbar_groups_fold_the_lowest_priority_first_and_never_wrap() {
        let p = theme::Scheme::Tierra.palette();
        for (width, expected) in [(2000.0, vec![0, 1, 2]), (260.0, vec![0])] {
            let ctx = egui::Context::default();
            let mut drawn = Vec::new();
            for n in 0..4 {
                drawn.clear();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(width, 100.0),
                    )),
                    time: Some(n as f64 * 0.1),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            toolbar_groups(ui, "t", &[3, 1, 2], |ui, g| {
                                drawn.push(g);
                                for _ in 0..3 {
                                    icon_button(ui, &p, crate::icon::PLUS, "Más", true);
                                }
                            });
                        });
                    });
                });
                out.textures_delta.clear();
            }
            drawn.sort();
            drawn.dedup();
            assert_eq!(drawn, expected, "ancho {width}");
        }
    }

    #[test]
    fn tables_draw_every_row_with_and_without_room_for_the_minimum_widths() {
        let cols = [
            TableCol::new("A", 300.0),
            TableCol::new("B", 300.0),
            TableCol::rest("C", 300.0),
        ];
        for width in [1500.0, 500.0] {
            let ctx = egui::Context::default();
            let mut tops = Vec::new();
            for n in 0..3 {
                tops.clear();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(width, 600.0),
                    )),
                    time: Some(n as f64 * 0.1),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        table(ui, "t", &cols, 5, |ui, i| {
                            ui.label(&*cols[i].header);
                        }, |row| {
                            tops.push(row.index());
                            row.col(|ui| {
                                ui.label("una celda con un texto bastante más largo que la columna donde está");
                            });
                            row.col(|_| {});
                            row.col(|_| {});
                        });
                    });
                });
                out.textures_delta.clear();
            }
            assert_eq!(tops, vec![0, 1, 2, 3, 4], "ancho {width}");
        }
    }

    #[test]
    fn sides_pushes_the_actions_to_the_right_edge_and_the_new_components_draw() {
        let p = theme::Scheme::Tierra.palette();
        let ctx = egui::Context::default();
        let mut right_edge = 0.0;
        let mut view = CanvasView::default();
        let mut value = 50.0;
        for n in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(400.0, 500.0),
                )),
                time: Some(n as f64 * 0.1),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    sides(
                        ui,
                        |ui| {
                            ui.label("Scriptpendra");
                        },
                        |ui| {
                            right_edge =
                                icon_button(ui, &p, crate::icon::DOTS_THREE_VERTICAL, "Más", true)
                                    .rect
                                    .right();
                        },
                    );
                    slider_value(
                        ui,
                        &p,
                        "Lente",
                        &mut value,
                        10.0..=200.0,
                        "mm",
                        0,
                        Some("Eq. full frame: 80 mm"),
                    );
                    canvas(ui, &p, &mut view, (10.0, 200.0), |_, _, _, _| {});
                });
                dialog(
                    ui.ctx(),
                    &p,
                    "prueba",
                    "Título",
                    DialogWidth::Normal,
                    |ui| {
                        ui.label("cuerpo");
                    },
                    |ui| {
                        primary_button(ui, &p, "Aceptar", None);
                    },
                );
                toast(ui.ctx(), &p, "Guardado", 100.0);
            });
            out.textures_delta.clear();
        }
        // El botón queda contra el borde derecho (con el margen del panel), no pegado al texto.
        assert!(right_edge > 350.0, "el botón quedó en x = {right_edge}");
        assert_eq!(
            anim(1.0),
            0.0,
            "las pruebas corren con las animaciones en 0"
        );
    }

    #[test]
    fn fill_width_stays_between_its_bounds() {
        run(|ui| {
            assert_eq!(fill_width(ui, 400.0, 500.0), 400.0);
            assert_eq!(fill_width(ui, 10.0, 20.0), 20.0);
        });
    }

    /// Corre `add` dos cuadros en un panel de `size` y devuelve el último cuadro.
    fn run_sized(size: egui::Vec2, mut add: impl FnMut(&mut Ui)) -> egui::FullOutput {
        let ctx = egui::Context::default();
        theme::apply(&ctx, &theme::Scheme::Tierra.palette(), 100);
        let mut last = None;
        for n in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(n as f64 * 0.1),
                ..Default::default()
            };
            if let Some(prev) = last.as_mut() {
                let prev: &mut egui::FullOutput = prev;
                prev.textures_delta.clear();
            }
            last = Some(ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| add(ui));
            }));
        }
        let mut last = last.unwrap();
        last.textures_delta.clear();
        last
    }

    #[test]
    fn every_one_line_control_measures_control_h() {
        let p = theme::Scheme::Tierra.palette();
        let (mut text, mut date, mut time, mut num) =
            (String::from("hola"), String::new(), String::new(), 5.0_f64);
        let mut heights = Vec::new();
        let out = run_sized(vec2(400.0, 400.0), |ui| {
            heights.clear();
            ui.horizontal(|ui| {
                heights.push(("botón", ui.button("Aceptar").rect.height()));
                heights.push(("fecha", date_field(ui, &mut date).rect.height()));
                heights.push(("hora", time_field(ui, &mut time).rect.height()));
                heights.push((
                    "número",
                    number_field(ui, &mut num, 0.0..=10.0, " mm").rect.height(),
                ));
            });
            field(ui, &p, "Título", &mut text, 1);
        });
        // El fondo del campo apilado es el único rectángulo con el color `field`.
        let field_box = out.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.fill == p.field => Some(r.rect.height()),
            _ => None,
        });
        heights.push((
            "campo apilado",
            field_box.expect("el campo no dibujó su fondo"),
        ));
        for (name, h) in heights {
            assert!(
                (h - CONTROL_H).abs() < 1.0,
                "{name} mide {h} y no {CONTROL_H}"
            );
        }
    }

    #[test]
    fn fixed_format_fields_fit_their_longest_value_and_placeholder() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let font = FontId::proportional(FONT_BODY);
            for (sample, width) in [
                ("0000-00-00", W_DATE),
                ("00:00", W_TIME),
                ("AAAA-MM-DD", W_DATE),
                ("HH:MM", W_TIME),
                ("-9999 mm", W_NUM),
            ] {
                let w = ui.fonts_mut(|f| {
                    f.layout_no_wrap(sample.to_string(), font.clone(), egui::Color32::WHITE)
                        .size()
                        .x
                });
                assert!(
                    w + 2.0 * ui.spacing().button_padding.x <= width,
                    "«{sample}» mide {w} y W = {width}"
                );
            }
        });
        out.textures_delta.clear();
    }

    #[test]
    fn prop_rows_start_and_end_their_controls_at_the_same_x() {
        let p = theme::Scheme::Tierra.palette();
        let mut rects = Vec::new();
        run_sized(vec2(420.0, 300.0), |ui| {
            rects.clear();
            for label in ["Fecha", "Una etiqueta bastante más larga que la columna"] {
                prop_row(ui, &p, label, |ui| {
                    let w = fill_width(ui, 40.0, 1000.0);
                    rects.push(
                        ui.add_sized(vec2(w, CONTROL_H), egui::Button::new("x"))
                            .rect,
                    );
                });
            }
        });
        assert_eq!(rects.len(), 2);
        assert!(
            (rects[0].left() - rects[1].left()).abs() < 0.5
                && (rects[0].right() - rects[1].right()).abs() < 0.5,
            "{rects:?}"
        );
    }

    #[test]
    fn kv_row_and_field_group_use_the_whole_width() {
        let p = theme::Scheme::Tierra.palette();
        let mut cols = Vec::new();
        let mut kv_right = 0.0;
        run_sized(vec2(400.0, 300.0), |ui| {
            cols.clear();
            kv_row(ui, &p, "Duración", "1 h 10 min");
            kv_right = ui.min_rect().right();
            field_group(ui, 3, |ui, i| {
                cols.push(ui.label(format!("campo {i}")).rect);
            });
        });
        assert!(kv_right > 350.0, "{kv_right}");
        assert_eq!(cols.len(), 3);
    }

    #[test]
    fn centered_column_is_centered_and_starts_at_the_top() {
        let mut rect = egui::Rect::NOTHING;
        run_sized(vec2(1000.0, 600.0), |ui| {
            let top = ui.cursor().top();
            centered_column(ui, 400.0, |ui| {
                rect = ui.label("contenido").rect;
                assert!(rect.top() - top < 1.0, "no arranca arriba");
            });
        });
        assert!(
            (rect.left() - 300.0).abs() < 12.0,
            "no está centrada: {rect:?}"
        );
    }

    #[test]
    fn a_sheet_keeps_its_proportion_and_shrinks_to_the_width() {
        let p = theme::Scheme::Tierra.palette();
        let size = vec2(595.0, 842.0);
        let mut seen = Vec::new();
        run_sized(vec2(400.0, 900.0), |ui| {
            seen.clear();
            for zoom in [0.5, 1.0] {
                seen.push(
                    sheet(ui, &p, SheetTone::Paper, size, zoom, |ui| {
                        ui.label("hola");
                    })
                    .rect,
                );
            }
        });
        assert!(
            (seen[0].height() - 842.0 * 0.5).abs() < 1.0,
            "zoom 0,5: {:?}",
            seen[0]
        );
        assert!(seen[1].height() > 0.0);
    }

    #[test]
    fn a_dialog_never_exceeds_its_width_and_keeps_its_footer_inside_the_window() {
        let p = theme::Scheme::Tierra.palette();
        for (screen_w, wide, expected) in [
            (1600.0, DialogWidth::Normal, DIALOG_MAX),
            (1600.0, DialogWidth::Wide, DIALOG_WIDE),
            (400.0, DialogWidth::Normal, 400.0 - 2.0 * SP_XL),
        ] {
            let (mut body_w, mut footer_bottom, mut footer_right) = (0.0, 0.0, 0.0);
            run_sized(vec2(screen_w, 700.0), |ui| {
                dialog(
                    ui.ctx(),
                    &p,
                    "t",
                    "Título",
                    wide,
                    |ui| {
                        body_w = ui.max_rect().width();
                        for i in 0..80 {
                            ui.label(format!("fila {i}"));
                        }
                    },
                    |ui| {
                        let r = primary_button(ui, &p, "Aceptar", None).rect;
                        footer_bottom = r.bottom();
                        footer_right = r.right();
                        let _ = ui.button("Cancelar");
                    },
                );
            });
            assert!(
                body_w <= expected - 2.0 * DIALOG_PAD + 1.0,
                "ancho {body_w} para {expected}"
            );
            assert!(
                footer_bottom <= 700.0 * 0.85 + 350.0,
                "el pie quedó fuera de la ventana: {footer_bottom}"
            );
            assert!(
                footer_right <= screen_w / 2.0 + expected / 2.0 + 1.0,
                "la acción principal no está contra el borde derecho: {footer_right}"
            );
        }
    }

    #[test]
    fn data_table_draws_group_and_add_rows_across_the_whole_width() {
        let p = theme::Scheme::Tierra.palette();
        let cols = [
            ColumnDef::new("n", "Nº", 64.0).align(ColAlign::Center),
            ColumnDef::new("t", "Tipo", 96.0),
            ColumnDef::new("d", "Descripción", 240.0).rest(),
            ColumnDef::new("s", "Sonido", 120.0).full_only(),
        ];
        let rows = [
            TableRowKind::Group {
                title: "ESCENA 1 — INT. CASA",
                count: 2,
                open: true,
            },
            TableRowKind::Item,
            TableRowKind::Item,
            TableRowKind::Add("Añadir plano"),
        ];
        let mut drawn = Vec::new();
        let out = run_sized(vec2(900.0, 400.0), |ui| {
            drawn.clear();
            data_table(ui, &p, "t", &cols, true, &rows, Some(1), |ui, row, col| {
                drawn.push((row, col.id));
                cell_text(ui, &p, if col.id == "d" { "" } else { "texto" });
            });
        });
        assert!(
            !drawn.iter().any(|(_, id)| *id == "s"),
            "la columna solo-completa no debe verse en la compacta"
        );
        assert_eq!(drawn.iter().filter(|(r, _)| *r == 1).count(), 3);
        assert!(
            !drawn.iter().any(|(r, _)| *r == 0 || *r == 3),
            "las filas de grupo y de añadir no dibujan celdas"
        );
        let total_fill = |color: egui::Color32| {
            out.shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r) if r.fill == color => Some(r.rect.width()),
                    _ => None,
                })
                .sum::<f32>()
        };
        let widest_fill = |color: egui::Color32| {
            out.shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r) if r.fill == color => Some(r.rect.width()),
                    _ => None,
                })
                .fold(0.0, f32::max)
        };
        // El encabezado de grupo es una sola celda de lado a lado, de otro color que las filas de datos.
        let group_fill = mix(p.container, p.accent, 0.24);
        assert!(
            widest_fill(group_fill) > 800.0,
            "el encabezado de grupo no ocupa todo el ancho: {}",
            widest_fill(group_fill)
        );
        assert!(
            total_fill(p.selection) > 800.0,
            "la fila seleccionada no usa selection a todo el ancho"
        );
    }

    #[test]
    fn status_items_are_separated_and_the_new_palette_roles_exist() {
        let p = theme::Scheme::Nord.palette();
        assert!(p.dark && p.paper != p.bg);
        let mut xs = Vec::new();
        run_sized(vec2(600.0, 100.0), |ui| {
            xs.clear();
            ui.horizontal(|ui| {
                xs.push(status_item(ui, &p, "Página 12 de 34").rect);
                xs.push(status_item(ui, &p, "33 min").rect);
            });
        });
        assert!(xs[1].left() - xs[0].right() >= SP_L - 0.5, "{xs:?}");
    }
}
