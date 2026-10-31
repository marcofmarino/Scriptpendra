//! Herramientas de estructura de la historia: tarjetas (escribir, reordenar y ver en una línea de tiempo), el mapa de
//! relaciones entre personajes, locaciones y otras cosas, y la curva dramática.

use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Ui, vec2};

use crate::{
    icon,
    model::{DramaKind, EntityRef, Project},
    theme::{self, Palette},
    ui::{kit, tokens},
};

/// Lo que recuerdan estas pantallas mientras se las usa.
#[derive(Default)]
pub struct StoryState {
    /// 0 tarjetas, 1 línea de tiempo.
    pub card_view: usize,
    pub card_sel: Option<u64>,
    /// La tarjeta que se está arrastrando.
    card_drag: Option<u64>,
    /// Nodo y relación elegidos en el mapa de relaciones.
    pub rel_node: Option<u64>,
    pub rel_edge: Option<u64>,
    /// El nodo del que se está arrastrando una relación nueva.
    connecting: Option<u64>,
    /// Punto elegido de la curva dramática.
    pub drama_sel: Option<u64>,
    /// Una relación soltada en un lugar vacío: de qué nodo viene, dónde va el nodo nuevo (en el lienzo) y dónde se abre el menú
    /// para elegir qué es.
    rel_pending: Option<(u64, (f32, f32), egui::Pos2)>,
}

/// Lo que devuelven las pantallas de documentos.
#[derive(Default)]
pub struct Out {
    pub changed: bool,
    pub delete: bool,
}

fn card_color(p: &Palette, n: u8) -> Color32 {
    [p.accent, p.success, p.warning, p.danger, p.primary, p.dim][n as usize % 6]
}

// ------------------------------------------------------------------------------------------------
// Tarjetas
// ------------------------------------------------------------------------------------------------

const CARD_W: f32 = 250.0;
const CARD_H: f32 = 210.0;
const CARD_GAP: f32 = 14.0;
const STEP: f32 = 190.0;

/// Mueve el elemento `from` a la posición `to`, como al soltarlo en otro lugar de la fila.
fn reorder(order: &mut Vec<usize>, from: usize, to: usize) {
    let item = order.remove(from);
    order.insert(to.min(order.len()), item);
}

pub fn cards(ui: &mut Ui, project: &mut Project, idx: usize, state: &mut StoryState, p: &Palette) -> Out {
    let mut out = Out::default();
    let doc_id = project.documents[idx].id;
    let mut add = false;
    let mut remove: Option<u64> = None;
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::CARDS).size(20.0).color(p.accent));
                out.changed |= ui
                    .add_sized(
                        vec2(260.0, tokens::CONTROL_H),
                        egui::TextEdit::singleline(&mut project.documents[idx].name),
                    )
                    .changed();
                if let Some(i) = kit::segmented(ui, p, &["Tarjetas", "Línea de tiempo"], state.card_view) {
                    state.card_view = i;
                }
                if kit::accent_button(ui, p, icon::PLUS, "Tarjeta").clicked() {
                    add = true;
                }
                if kit::accent_button(ui, p, icon::TRASH, "Eliminar").clicked() {
                    out.delete = true;
                }
            });
            ui.label(
                RichText::new("Arrastrá una tarjeta por su barra de arriba para cambiarla de lugar.")
                    .size(tokens::FONT_SMALL)
                    .color(p.dim),
            );
        });
    let doc = &mut project.documents[idx];
    if doc.cards.is_empty() {
        ui.add_space(tokens::SP_L);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Todavía no hay tarjetas. Sumá la primera para empezar a ordenar la historia.").color(p.dim));
        });
    }
    // Lo que se arrastra y a dónde: el orden en pantalla se acomoda mientras se arrastra, y al soltar queda.
    let mut moved: Option<(usize, usize)> = None;
    if state.card_view == 0 {
        grid(ui, doc, state, p, &mut out, &mut moved, &mut remove);
    } else {
        timeline(ui, doc, state, p, &mut out, &mut moved, &mut remove);
    }
    let doc = &mut project.documents[idx];
    if let Some((from, to)) = moved
        && from < doc.cards.len()
    {
        let mut order: Vec<usize> = (0..doc.cards.len()).collect();
        reorder(&mut order, from, to);
        let old = doc.cards.clone();
        doc.cards = order.iter().map(|i| old[*i].clone()).collect();
        out.changed = true;
    }
    if let Some(id) = remove {
        doc.cards.retain(|c| c.id != id);
        if state.card_sel == Some(id) {
            state.card_sel = None;
        }
        out.changed = true;
    }
    if add
        && let Some(id) = project.add_story_card(doc_id)
    {
        state.card_sel = Some(id);
        out.changed = true;
    }
    out
}

/// El orden en que se ven las tarjetas mientras se arrastra la de `dragging` hacia `target`.
fn visual_order(count: usize, dragging: Option<(usize, usize)>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..count).collect();
    if let Some((from, to)) = dragging
        && from < count
    {
        reorder(&mut order, from, to);
    }
    order
}

fn grid(
    ui: &mut Ui,
    doc: &mut crate::model::Document,
    state: &mut StoryState,
    p: &Palette,
    out: &mut Out,
    moved: &mut Option<(usize, usize)>,
    remove: &mut Option<u64>,
) {
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        let avail = ui.available_width() - 2.0 * tokens::SP_L;
        let cols = (((avail + CARD_GAP) / (CARD_W + CARD_GAP)).floor() as usize).max(1);
        let count = doc.cards.len();
        let rows = count.div_ceil(cols).max(1);
        let (area, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), rows as f32 * (CARD_H + CARD_GAP) + tokens::SP_XXL),
            Sense::hover(),
        );
        let origin = area.min + vec2(tokens::SP_L, 0.0);
        let slot = |n: usize| {
            egui::Rect::from_min_size(
                origin + vec2((n % cols) as f32 * (CARD_W + CARD_GAP), (n / cols) as f32 * (CARD_H + CARD_GAP)),
                vec2(CARD_W, CARD_H),
            )
        };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        // Dónde caería la tarjeta que se arrastra: el lugar cuyo centro está más cerca del mouse.
        let dragging = state.card_drag.and_then(|id| doc.cards.iter().position(|c| c.id == id));
        let target = dragging.zip(pointer).map(|(from, pos)| {
            let to = (0..count)
                .min_by(|a, b| slot(*a).center().distance(pos).total_cmp(&slot(*b).center().distance(pos)))
                .unwrap_or(from);
            (from, to)
        });
        let order = visual_order(count, target);
        let mut ghost: Option<(usize, egui::Rect)> = None;
        let mut started: Option<u64> = None;
        let mut stopped = false;
        for (place, &i) in order.iter().enumerate() {
            let r = slot(place);
            let card = &mut doc.cards[i];
            let color = card_color(p, card.color);
            let is_dragged = state.card_drag == Some(card.id);
            let painter = ui.painter_at(area);
            painter.rect(r, tokens::R_CARD, p.container, Stroke::new(1.5, if is_dragged { p.accent } else { color }), egui::StrokeKind::Inside);
            // La barra de arriba: el número, y de ahí se agarra para arrastrar.
            let bar = egui::Rect::from_min_size(r.min, vec2(CARD_W, 26.0));
            painter.rect_filled(bar, egui::CornerRadius { nw: tokens::R_CARD, ne: tokens::R_CARD, sw: 0, se: 0 }, color.gamma_multiply(0.35));
            painter.text(bar.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, format!("{}", i + 1), FontId::new(tokens::FONT_BODY, theme::bold_family()), p.text);
            painter.text(bar.center(), egui::Align2::CENTER_CENTER, "· · ·", FontId::proportional(tokens::FONT_BODY), p.dim);
            let grip = ui.interact(bar, ui.scope_id().with(("card_grip", card.id)), Sense::click_and_drag());
            if grip.drag_started() {
                started = Some(card.id);
            }
            if grip.drag_stopped() {
                stopped = true;
            }
            if grip.dragged() {
                ghost = Some((i, r.translate(pointer.map_or(vec2(0.0, 0.0), |pos| pos - r.center()))));
            }
            let _ = grip.on_hover_cursor(egui::CursorIcon::Grab);
            if is_dragged {
                continue;
            }
            // Quitar, con el mouse encima.
            let hovered = ui.rect_contains_pointer(r);
            if hovered {
                let x = egui::Rect::from_center_size(bar.right_center() - vec2(14.0, 0.0), vec2(20.0, 20.0));
                let resp = ui.interact(x, ui.scope_id().with(("card_x", card.id)), Sense::click()).on_hover_text("Quitar la tarjeta");
                painter.text(x.center(), egui::Align2::CENTER_CENTER, icon::TRASH, FontId::proportional(tokens::ICON_S), if resp.hovered() { p.danger } else { p.dim });
                if resp.clicked() {
                    *remove = Some(card.id);
                }
            }
            // Los campos, en rectángulos fijos: la tarjeta no cambia de tamaño con lo que se escribe.
            let inner = r.shrink2(vec2(tokens::SP_S, 0.0));
            let title = egui::Rect::from_min_size(egui::pos2(inner.left(), r.top() + 32.0), vec2(inner.width(), 24.0));
            let moment = egui::Rect::from_min_size(egui::pos2(inner.left(), title.bottom() + 4.0), vec2(inner.width(), 22.0));
            let body = egui::Rect::from_min_max(egui::pos2(inner.left(), moment.bottom() + 4.0), egui::pos2(inner.right(), r.bottom() - 28.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(title), |ui| {
                out.changed |= ui.add_sized(title.size(), egui::TextEdit::singleline(&mut card.title).hint_text("Título")).changed();
            });
            ui.scope_builder(egui::UiBuilder::new().max_rect(moment), |ui| {
                out.changed |= ui.add_sized(moment.size(), egui::TextEdit::singleline(&mut card.moment).hint_text("Momento (Acto I, 0:15…)")).changed();
            });
            ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                egui::ScrollArea::vertical().id_salt(("card_body", card.id)).auto_shrink([false; 2]).show(ui, |ui| {
                    out.changed |= ui.add_sized(vec2(body.width(), body.height()), egui::TextEdit::multiline(&mut card.text).hint_text("Qué pasa")).changed();
                });
            });
            // Los colores.
            for c in 0..6u8 {
                let center = egui::pos2(inner.left() + 10.0 + c as f32 * 22.0, r.bottom() - 14.0);
                let hit = egui::Rect::from_center_size(center, vec2(18.0, 18.0));
                let resp = ui.interact(hit, ui.scope_id().with(("card_color", card.id, c)), Sense::click());
                painter.circle_filled(center, 6.0, card_color(p, c));
                if card.color == c {
                    painter.circle_stroke(center, 8.5, Stroke::new(1.5, p.text));
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    card.color = c;
                    out.changed = true;
                }
            }
        }
        if let Some(id) = started {
            state.card_drag = Some(id);
            state.card_sel = Some(id);
        }
        // La tarjeta que se arrastra va flotando bajo el mouse.
        if let Some((i, r)) = ghost {
            let card = &doc.cards[i];
            let layer = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("card_ghost")));
            layer.rect(r, tokens::R_CARD, p.container_hi, Stroke::new(2.0, p.accent), egui::StrokeKind::Inside);
            layer.text(r.left_top() + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, format!("{}  {}", i + 1, card.title), FontId::new(tokens::FONT_BODY, theme::bold_family()), p.text);
        }
        if stopped {
            if let Some(t) = target {
                *moved = Some(t).filter(|(from, to)| from != to);
            }
            state.card_drag = None;
        }
    });
}

/// Las tarjetas en orden sobre una línea: arriba el momento, en la línea un círculo numerado y abajo el título y el texto. Se
/// arrastran por el círculo, a izquierda o derecha.
fn timeline(
    ui: &mut Ui,
    doc: &mut crate::model::Document,
    state: &mut StoryState,
    p: &Palette,
    out: &mut Out,
    moved: &mut Option<(usize, usize)>,
    remove: &mut Option<u64>,
) {
    let selected_height = 190.0;
    let area = (ui.available_height() - selected_height).max(220.0);
    egui::ScrollArea::horizontal()
        .id_salt("story_timeline")
        .max_height(area)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let count = doc.cards.len();
            let width = (count as f32 * STEP + 120.0).max(ui.available_width());
            let (rect, _) = ui.allocate_exact_size(vec2(width, 210.0), Sense::hover());
            let y = rect.top() + 56.0;
            let painter = ui.painter_at(rect);
            painter.line_segment([egui::pos2(rect.left() + 24.0, y), egui::pos2(rect.right() - 24.0, y)], Stroke::new(2.0, p.border));
            let slot_x = |n: usize| rect.left() + 60.0 + n as f32 * STEP;
            let pointer = ui.input(|i| i.pointer.interact_pos());
            let dragging = state.card_drag.and_then(|id| doc.cards.iter().position(|c| c.id == id));
            let target = dragging.zip(pointer).map(|(from, pos)| {
                let to = (((pos.x - rect.left() - 60.0) / STEP).round().max(0.0) as usize).min(count.saturating_sub(1));
                (from, to)
            });
            let order = visual_order(count, target);
            let mut started = None;
            let mut stopped = false;
            for (place, &i) in order.iter().enumerate() {
                let card = &doc.cards[i];
                let cx = slot_x(place);
                let color = card_color(p, card.color);
                let sel = state.card_sel == Some(card.id);
                let is_dragged = state.card_drag == Some(card.id);
                let draw_x = if is_dragged { pointer.map_or(cx, |pos| pos.x) } else { cx };
                let dot = egui::Rect::from_center_size(egui::pos2(draw_x, y), vec2(28.0, 28.0));
                let resp = ui.interact(dot.expand(6.0), ui.scope_id().with(("timeline_dot", card.id)), Sense::click_and_drag());
                if resp.drag_started() {
                    started = Some(card.id);
                }
                if resp.drag_stopped() {
                    stopped = true;
                }
                if resp.clicked() {
                    state.card_sel = Some(card.id);
                }
                painter.circle_filled(dot.center(), 14.0, color);
                if sel || resp.hovered() || is_dragged {
                    painter.circle_stroke(dot.center(), 17.0, Stroke::new(2.0, p.text));
                }
                painter.text(dot.center(), egui::Align2::CENTER_CENTER, (i + 1).to_string(), FontId::new(tokens::FONT_SMALL, theme::bold_family()), theme::ink_on(color, p));
                painter.text(egui::pos2(draw_x, rect.top() + 20.0), egui::Align2::CENTER_CENTER, &card.moment, FontId::proportional(tokens::FONT_SMALL), p.dim);
                let title = painter.layout(card.title.clone(), FontId::new(tokens::FONT_BODY, theme::bold_family()), p.text, STEP - 24.0);
                let title_h = title.size().y;
                painter.galley(egui::pos2(draw_x - title.size().x / 2.0, y + 26.0), title, p.text);
                let body = painter.layout(card.text.clone(), FontId::proportional(tokens::FONT_SMALL), p.dim, STEP - 24.0);
                let clip = egui::Rect::from_min_size(egui::pos2(draw_x - STEP / 2.0 + 12.0, y + 30.0 + title_h), vec2(STEP - 24.0, (130.0 - title_h).max(10.0)));
                painter.with_clip_rect(clip).galley(egui::pos2(draw_x - body.size().x / 2.0, clip.top()), body, p.dim);
                resp.on_hover_cursor(egui::CursorIcon::Grab);
            }
            if let Some(id) = started {
                state.card_drag = Some(id);
                state.card_sel = Some(id);
            }
            if stopped {
                if let Some(t) = target {
                    *moved = Some(t).filter(|(from, to)| from != to);
                }
                state.card_drag = None;
            }
        });
    // La tarjeta elegida se edita debajo.
    let Some(n) = state.card_sel.and_then(|id| doc.cards.iter().position(|c| c.id == id)) else {
        ui.label(RichText::new("Elegí una tarjeta de la línea para editarla.").color(p.dim));
        return;
    };
    let card = &mut doc.cards[n];
    ui.horizontal(|ui| {
        out.changed |= ui.add(egui::TextEdit::singleline(&mut card.title).desired_width(260.0).hint_text("Título")).changed();
        out.changed |= ui.add(egui::TextEdit::singleline(&mut card.moment).desired_width(160.0).hint_text("Momento")).changed();
        if kit::icon_button(ui, p, icon::TRASH, "Quitar la tarjeta", true).clicked() {
            *remove = Some(card.id);
        }
    });
    out.changed |= ui
        .add_sized(vec2(ui.available_width(), 90.0), egui::TextEdit::multiline(&mut card.text).hint_text("Qué pasa"))
        .changed();
}

// ------------------------------------------------------------------------------------------------
// Mapa de relaciones
// ------------------------------------------------------------------------------------------------

const NODE_H: f32 = 34.0;

fn node_label(project: &Project, node: &crate::model::RelNode) -> String {
    match node.target {
        Some(t) => project.label_for(t),
        None => node.text.clone(),
    }
}

fn node_icon(node: &crate::model::RelNode) -> &'static str {
    match node.target {
        Some(EntityRef::Character(_)) => icon::USER,
        Some(EntityRef::Location(_)) => icon::HOUSE,
        Some(EntityRef::Object(_)) => icon::CUBE,
        Some(EntityRef::Element(_)) => icon::BOOKS,
        Some(EntityRef::World(_)) => icon::GLOBE,
        None => icon::NOTE,
    }
}

fn node_size(ui: &Ui, text: &str) -> egui::Vec2 {
    let w = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(tokens::FONT_BODY), Color32::WHITE)
        .size()
        .x;
    vec2((w + 56.0).clamp(110.0, 260.0), NODE_H)
}

fn distance_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() <= f32::EPSILON { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
    (a + ab * t).distance(p)
}

pub fn relations(ui: &mut Ui, project: &mut Project, idx: usize, state: &mut StoryState, p: &Palette) -> Out {
    let mut out = Out::default();
    let doc_id = project.documents[idx].id;
    let mut add: Option<Option<EntityRef>> = None;
    let mut remove_node = None;
    let mut remove_edge = None;
    let people: Vec<(EntityRef, String)> = project.characters.iter().map(|c| (EntityRef::Character(c.id), c.name.clone())).collect();
    let places: Vec<(EntityRef, String)> = project.location_profiles.iter().map(|l| (EntityRef::Location(l.id), project.label_for(EntityRef::Location(l.id)))).collect();
    let things: Vec<(EntityRef, String)> = project.objects.iter().map(|o| (EntityRef::Object(o.id), project.label_for(EntityRef::Object(o.id)))).collect();
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::FLOW_ARROW).size(20.0).color(p.accent));
                out.changed |= ui
                    .add_sized(vec2(260.0, tokens::CONTROL_H), egui::TextEdit::singleline(&mut project.documents[idx].name))
                    .changed();
                ui.menu_button(format!("{}  Añadir nodo", icon::PLUS), |ui| {
                    if ui.button("Nota libre").clicked() {
                        add = Some(None);
                        ui.close();
                    }
                    for (title, list) in [("Personajes", &people), ("Locaciones", &places), ("Objetos", &things)] {
                        ui.add_enabled_ui(!list.is_empty(), |ui| {
                            ui.menu_button(title, |ui| {
                                for (target, name) in list {
                                    if ui.button(name).clicked() {
                                        add = Some(Some(*target));
                                        ui.close();
                                    }
                                }
                            });
                        });
                    }
                });
                if kit::accent_button(ui, p, icon::TRASH, "Eliminar mapa").clicked() {
                    out.delete = true;
                }
            });
            ui.add_space(tokens::SP_XS);
            let doc = &mut project.documents[idx];
            if let Some(edge) = state.rel_edge.and_then(|id| doc.rel_edges.iter_mut().find(|e| e.id == id)) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Relación").size(tokens::FONT_SMALL).color(p.dim));
                    out.changed |= ui
                        .add(egui::TextEdit::singleline(&mut edge.label).desired_width(240.0).hint_text("Cómo se relacionan (hermana de, trabaja en…)"))
                        .changed();
                    if kit::icon_button(ui, p, icon::ARROWS_LEFT_RIGHT, "Invertir el sentido", true).clicked() {
                        std::mem::swap(&mut edge.from, &mut edge.to);
                        out.changed = true;
                    }
                    if kit::icon_button(ui, p, icon::TRASH, "Quitar la relación", true).clicked() {
                        remove_edge = Some(edge.id);
                    }
                });
            } else if let Some(node) = state.rel_node.and_then(|id| doc.rel_nodes.iter_mut().find(|n| n.id == id)) {
                ui.horizontal(|ui| {
                    if node.target.is_none() {
                        out.changed |= ui
                            .add(egui::TextEdit::singleline(&mut node.text).desired_width(240.0).hint_text("Texto de la nota"))
                            .changed();
                    }
                    if kit::icon_button(ui, p, icon::TRASH, "Quitar el nodo y sus relaciones", true).clicked() {
                        remove_node = Some(node.id);
                    }
                });
            } else {
                ui.label(
                    RichText::new("Arrastrá desde el círculo de un nodo hasta otro para relacionarlos. Clic en una línea para rotularla.")
                        .size(tokens::FONT_SMALL)
                        .color(p.dim),
                );
            }
        });

    // Un nodo se mueve, se elige o empieza una relación; las líneas se eligen con un clic.
    let doc = &project.documents[idx];
    let labels: Vec<(u64, String)> = doc.rel_nodes.iter().map(|n| (n.id, node_label(project, n))).collect();
    let mut moves: Vec<(u64, egui::Vec2)> = Vec::new();
    let mut new_edge: Option<(u64, u64)> = None;
    let mut clicked_node = None;
    let mut clicked_edge = None;
    let mut canvas_click = false;
    egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
        let extent = doc
            .rel_nodes
            .iter()
            .fold(vec2(0.0, 0.0), |m, n| vec2(m.x.max(n.x + 300.0), m.y.max(n.y + 140.0)));
        let size = vec2(extent.x.max(ui.available_width()), extent.y.max(ui.available_height()));
        let (rect, bg) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        let rect_of = |ui: &Ui, id: u64| -> Option<egui::Rect> {
            let n = doc.rel_nodes.iter().find(|n| n.id == id)?;
            let label = &labels.iter().find(|(i, _)| *i == id)?.1;
            Some(egui::Rect::from_min_size(rect.min + vec2(n.x, n.y), node_size(ui, label)))
        };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        // Relaciones.
        for e in &doc.rel_edges {
            let (Some(a), Some(b)) = (rect_of(ui, e.from), rect_of(ui, e.to)) else {
                continue;
            };
            let (pa, pb) = (a.center(), b.center());
            let start = clip_to_rect(a, pb);
            let end = clip_to_rect(b, pa);
            let selected = state.rel_edge == Some(e.id);
            let color = if selected { p.accent } else { p.dim };
            painter.line_segment([start, end], Stroke::new(if selected { 2.5 } else { 1.5 }, color));
            let dir = (end - start).normalized();
            let normal = vec2(-dir.y, dir.x);
            painter.add(egui::Shape::convex_polygon(
                vec![end, end - dir * 12.0 + normal * 5.0, end - dir * 12.0 - normal * 5.0],
                color,
                Stroke::NONE,
            ));
            let mid = start + (end - start) * 0.5;
            if !e.label.is_empty() {
                let g = painter.layout_no_wrap(e.label.clone(), FontId::proportional(tokens::FONT_SMALL), p.text);
                let r = egui::Rect::from_center_size(mid, g.size() + vec2(10.0, 4.0));
                painter.rect_filled(r, 4, p.canvas);
                painter.galley(r.min + vec2(5.0, 2.0), g, p.text);
            }
            if bg.clicked()
                && let Some(pos) = pointer
                && distance_to_segment(pos, start, end) < 7.0
            {
                clicked_edge = Some(e.id);
            }
        }
        // Nodos.
        for n in &doc.rel_nodes {
            let Some(r) = rect_of(ui, n.id) else {
                continue;
            };
            let resp = ui.interact(r, ui.scope_id().with(("rel_node", n.id)), Sense::click_and_drag());
            if resp.dragged() && state.connecting.is_none() {
                moves.push((n.id, resp.drag_delta()));
            }
            if resp.clicked() {
                clicked_node = Some(n.id);
            }
            let sel = state.rel_node == Some(n.id);
            let label = &labels.iter().find(|(i, _)| *i == n.id).map_or("", |(_, l)| l.as_str());
            let accent = match n.target {
                Some(_) => theme::character_color(label),
                None => p.dim,
            };
            painter.rect(r, 17, if sel { p.selection } else { p.container }, Stroke::new(if sel { 2.0 } else { 1.5 }, accent), egui::StrokeKind::Inside);
            painter.text(r.left_center() + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, node_icon(n), FontId::proportional(tokens::ICON_S), accent);
            painter.with_clip_rect(r.shrink2(vec2(30.0, 0.0))).text(r.left_center() + vec2(32.0, 0.0), egui::Align2::LEFT_CENTER, label, FontId::proportional(tokens::FONT_BODY), p.text);
            // El tirador: un círculo a la derecha, para sacar una relación hacia otro nodo.
            let near = ui.rect_contains_pointer(r.expand2(vec2(18.0, 4.0)));
            if near || sel || state.connecting == Some(n.id) {
                let handle = egui::Rect::from_center_size(r.right_center() + vec2(10.0, 0.0), vec2(18.0, 18.0));
                let hr = ui.interact(handle, ui.scope_id().with(("rel_handle", n.id)), Sense::drag());
                painter.circle(handle.center(), 7.0, if hr.hovered() || hr.dragged() { p.accent } else { p.container }, Stroke::new(1.5, p.accent));
                painter.text(handle.center(), egui::Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(10.0), if hr.hovered() || hr.dragged() { p.on_accent } else { p.accent });
                if hr.drag_started() {
                    state.connecting = Some(n.id);
                }
                if hr.drag_stopped() {
                    if let Some(pos) = pointer {
                        let target = doc.rel_nodes.iter().map(|m| m.id).find(|id| *id != n.id && rect_of(ui, *id).is_some_and(|rr| rr.contains(pos)));
                        match target {
                            Some(to) => new_edge = Some((n.id, to)),
                            None => {
                                let at = (pos.x - rect.min.x - 20.0, pos.y - rect.min.y - NODE_H / 2.0);
                                state.rel_pending = Some((n.id, (at.0.max(0.0), at.1.max(0.0)), pos));
                            }
                        }
                    }
                    state.connecting = None;
                }
                hr.on_hover_text("Arrastrá hasta otro nodo para relacionarlos");
            }
        }
        // La relación que se está arrastrando.
        if let (Some(from), Some(pos)) = (state.connecting, pointer)
            && let Some(a) = rect_of(ui, from)
        {
            painter.line_segment([a.right_center(), pos], Stroke::new(2.0, p.accent));
        }
        if bg.clicked() && clicked_edge.is_none() {
            canvas_click = true;
        }
    });
    // El menú de lo que se suelta en un lugar vacío: elegir qué nodo crear ahí, ya relacionado con el de origen.
    if let Some((from, at, screen)) = state.rel_pending {
        let mut choice: Option<Option<EntityRef>> = None;
        let mut cancel = false;
        let area = egui::Area::new(egui::Id::unique("rel_pending_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(220.0);
                    ui.label(RichText::new("Crear y relacionar").size(tokens::FONT_SMALL).color(p.dim));
                    if ui.button(format!("{}  Nota libre", icon::NOTE)).clicked() {
                        choice = Some(None);
                    }
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (title, glyph, list) in [("Personajes", icon::USER, &people), ("Locaciones", icon::HOUSE, &places), ("Objetos", icon::CUBE, &things)] {
                            if list.is_empty() {
                                continue;
                            }
                            ui.label(RichText::new(title).size(tokens::FONT_SMALL).color(p.dim));
                            for (target, name) in list {
                                if ui.button(format!("{glyph}  {name}")).clicked() {
                                    choice = Some(Some(*target));
                                }
                            }
                        }
                    });
                });
            });
        let outside = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|pos| !area.response.rect.contains(pos)));
        if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if let Some(target) = choice {
            if let Some(new) = project.add_rel_node(doc_id, target, at) {
                project.add_rel_edge(doc_id, from, new);
                state.rel_node = Some(new);
                state.rel_edge = None;
                out.changed = true;
            }
            state.rel_pending = None;
        } else if cancel {
            state.rel_pending = None;
        }
    }
    for (id, delta) in moves {
        if let Some(n) = project.documents[idx].rel_nodes.iter_mut().find(|n| n.id == id) {
            n.x = (n.x + delta.x).max(0.0);
            n.y = (n.y + delta.y).max(0.0);
            out.changed = true;
        }
    }
    if let Some(id) = clicked_node {
        state.rel_node = Some(id);
        state.rel_edge = None;
    } else if let Some(id) = clicked_edge {
        state.rel_edge = Some(id);
        state.rel_node = None;
    } else if canvas_click {
        state.rel_node = None;
        state.rel_edge = None;
    }
    if let Some((from, to)) = new_edge
        && let Some(id) = project.add_rel_edge(doc_id, from, to)
    {
        state.rel_edge = Some(id);
        state.rel_node = None;
        out.changed = true;
    }
    if let Some(target) = add {
        let n = project.documents[idx].rel_nodes.len();
        let at = (40.0 + (n % 5) as f32 * 190.0, 30.0 + (n / 5) as f32 * 80.0);
        if let Some(id) = project.add_rel_node(doc_id, target, at) {
            state.rel_node = Some(id);
            state.rel_edge = None;
            out.changed = true;
        }
    }
    let doc = &mut project.documents[idx];
    if let Some(id) = remove_node {
        doc.rel_nodes.retain(|n| n.id != id);
        doc.rel_edges.retain(|e| e.from != id && e.to != id);
        state.rel_node = None;
        out.changed = true;
    }
    if let Some(id) = remove_edge {
        doc.rel_edges.retain(|e| e.id != id);
        state.rel_edge = None;
        out.changed = true;
    }
    out
}

/// Dónde la recta del centro del rectángulo hacia `toward` sale del rectángulo.
fn clip_to_rect(r: egui::Rect, toward: egui::Pos2) -> egui::Pos2 {
    let c = r.center();
    let d = toward - c;
    if d.length_sq() <= f32::EPSILON {
        return c;
    }
    let (hx, hy) = (r.width() / 2.0, r.height() / 2.0);
    let t = (hx / d.x.abs().max(f32::EPSILON)).min(hy / d.y.abs().max(f32::EPSILON));
    c + d * t
}

// ------------------------------------------------------------------------------------------------
// Curva dramática
// ------------------------------------------------------------------------------------------------

/// Puntos de la curva suave que pasa por `pts` (ordenados por x): Catmull-Rom sobre la tensión.
fn curve(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut out = Vec::new();
    for i in 0..pts.len() - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let (p1, p2) = (pts[i], pts[i + 1]);
        let p3 = pts[(i + 2).min(pts.len() - 1)];
        for s in 0..16 {
            let t = s as f32 / 16.0;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
            out.push((f(p0.0, p1.0, p2.0, p3.0).clamp(p1.0, p2.0), f(p0.1, p1.1, p2.1, p3.1)));
        }
    }
    out.push(*pts.last().unwrap());
    out
}

fn drama_color(p: &Palette, kind: DramaKind) -> Color32 {
    match kind {
        DramaKind::Climax => p.danger,
        DramaKind::TurnOne | DramaKind::TurnTwo => p.warning,
        DramaKind::Peak | DramaKind::Midpoint => p.accent,
        DramaKind::Valley => p.primary,
        DramaKind::Start | DramaKind::Resolution => p.success,
        _ => p.dim,
    }
}

/// La curva dramática: el gráfico con sus puntos arrastrables y, debajo, un panel como el de capas de un programa de dibujo:
/// arriba Añadir y Borrar, a la izquierda la lista de puntos y a la derecha lo que se anota del elegido. Pasando el mouse
/// por la curva aparece un «+» para sumar un punto ahí. Devuelve si el proyecto cambió.
pub fn drama(ui: &mut Ui, project: &mut Project, state: &mut StoryState, p: &Palette) -> bool {
    let mut changed = project.ensure_drama();
    if state.drama_sel.is_some_and(|id| !project.drama.iter().any(|d| d.id == id)) {
        state.drama_sel = None;
    }
    let mut add: Option<(DramaKind, Option<(f32, f32)>)> = None;
    let mut remove = false;
    kit::column(ui, 1000.0, |ui| {
        // El gráfico.
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 300.0), Sense::hover());
        let plot = rect.shrink2(vec2(28.0, 28.0));
        let to_screen = |x: f32, y: f32| egui::pos2(plot.left() + x * plot.width(), plot.bottom() - y * plot.height());
        // Un clic en un hueco de la curva suma un punto: va primero para que los puntos queden encima.
        let background = ui.interact(rect, ui.scope_id().with("drama_plot"), Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, tokens::R_CARD, p.canvas);
        for (x, name) in [(0.0, "Acto I"), (0.25, "Acto II"), (0.75, "Acto III")] {
            let s = to_screen(x, 0.0);
            if x > 0.0 {
                painter.line_segment([egui::pos2(s.x, plot.top()), egui::pos2(s.x, plot.bottom())], Stroke::new(1.0, p.grid));
            }
            painter.text(egui::pos2(s.x + 6.0, rect.top() + 8.0), egui::Align2::LEFT_TOP, name, FontId::proportional(tokens::FONT_SMALL), p.dim);
        }
        for y in [0.0, 0.5, 1.0] {
            let a = to_screen(0.0, y);
            painter.line_segment([a, egui::pos2(plot.right(), a.y)], Stroke::new(1.0, p.grid));
        }
        painter.text(egui::pos2(rect.left() + 6.0, plot.top()), egui::Align2::LEFT_CENTER, "Tensión", FontId::proportional(tokens::FONT_SMALL), p.dim);
        let mut sorted: Vec<(f32, f32)> = project.drama.iter().map(|d| (d.x, d.y)).collect();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
        let samples = curve(&sorted);
        let line: Vec<egui::Pos2> = samples.iter().map(|(x, y)| to_screen(*x, y.clamp(0.0, 1.0))).collect();
        painter.add(egui::Shape::line(line.clone(), Stroke::new(2.5, p.accent)));
        let point_at: Vec<egui::Pos2> = project.drama.iter().map(|d| to_screen(d.x, d.y)).collect();

        // El «+» sobre la curva, donde no hay un punto.
        let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|pos| rect.contains(*pos));
        let mut hover: Option<(egui::Pos2, (f32, f32))> = None;
        if let Some(pos) = pointer {
            let near_point = point_at.iter().any(|c| c.distance(pos) < 16.0);
            if !near_point {
                let best = samples
                    .iter()
                    .zip(&line)
                    .map(|(s, l)| (l.distance(pos), *l, *s))
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((dist, at, s)) = best
                    && dist < 10.0
                    && !point_at.iter().any(|c| c.distance(at) < 16.0)
                {
                    hover = Some((at, (s.0.clamp(0.0, 1.0), s.1.clamp(0.0, 1.0))));
                }
            }
        }
        if let Some((at, spot)) = hover {
            painter.circle(at, 9.0, p.accent, Stroke::new(2.0, p.canvas));
            painter.text(at, egui::Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(12.0), p.on_accent);
            if background.clicked() {
                add = Some((DramaKind::Custom, Some(spot)));
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
        }
        // Los puntos.
        let mut drag: Option<(u64, egui::Vec2)> = None;
        for (d, c) in project.drama.iter().zip(&point_at) {
            let hit = egui::Rect::from_center_size(*c, vec2(22.0, 22.0));
            let resp = ui.interact(hit, ui.scope_id().with(("drama_pt", d.id)), Sense::click_and_drag());
            if resp.dragged() {
                drag = Some((d.id, resp.drag_delta()));
            }
            if resp.clicked() || resp.drag_started() {
                state.drama_sel = Some(d.id);
            }
            let sel = state.drama_sel == Some(d.id);
            let color = drama_color(p, d.kind);
            painter.circle(*c, if sel { 9.0 } else { 7.0 }, color, Stroke::new(2.0, if sel { p.text } else { p.canvas }));
            let g = painter.layout_no_wrap(d.label.clone(), FontId::proportional(tokens::FONT_SMALL), if sel { p.text } else { p.dim });
            let y = if c.y - 26.0 > rect.top() + 24.0 { c.y - 26.0 } else { c.y + 12.0 };
            painter.galley(egui::pos2((c.x - g.size().x / 2.0).clamp(rect.left() + 4.0, rect.right() - g.size().x - 4.0), y), g, p.dim);
            resp.on_hover_cursor(egui::CursorIcon::Grab);
        }
        if let Some((id, delta)) = drag
            && let Some(d) = project.drama.iter_mut().find(|d| d.id == id)
        {
            d.x = (d.x + delta.x / plot.width()).clamp(0.0, 1.0);
            d.y = (d.y - delta.y / plot.height()).clamp(0.0, 1.0);
            changed = true;
        }
        ui.add_space(tokens::SP_M);

        // El panel de puntos, con su propio desplazamiento.
        let panel_h = ui.available_height().max(240.0);
        let (panel, _) = ui.allocate_exact_size(vec2(ui.available_width(), panel_h), Sense::hover());
        ui.painter().rect_filled(panel, tokens::R_CARD, p.container);
        ui.scope_builder(egui::UiBuilder::new().max_rect(panel.shrink(tokens::SP_S)), |ui| {
            ui.set_clip_rect(panel.intersect(ui.clip_rect()));
            ui.horizontal(|ui| {
                ui.menu_button(format!("{}  Añadir punto", icon::PLUS), |ui| {
                    for kind in [DramaKind::TurnOne, DramaKind::Peak, DramaKind::Valley, DramaKind::Custom] {
                        let name = if kind == DramaKind::TurnOne { "Punto de giro" } else { kind.label() };
                        if ui.button(name).clicked() {
                            add = Some((kind, None));
                            ui.close();
                        }
                    }
                });
                if ui
                    .add_enabled(state.drama_sel.is_some(), egui::Button::new(format!("{}  Borrar", icon::TRASH)))
                    .on_hover_text("Borrar el punto elegido (Supr)")
                    .clicked()
                {
                    remove = true;
                }
            });
            ui.add_space(tokens::SP_XS);
            let body = ui.available_rect_before_wrap();
            let list = egui::Rect::from_min_size(body.min, vec2(260.0_f32.min(body.width() * 0.45), body.height()));
            let detail = egui::Rect::from_min_max(egui::pos2(list.right() + tokens::SP_M, body.top()), body.max);
            // La lista, en el orden de la historia.
            ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
                egui::ScrollArea::vertical().id_salt("drama_list").auto_shrink([false; 2]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let mut order: Vec<usize> = (0..project.drama.len()).collect();
                    order.sort_by(|a, b| project.drama[*a].x.total_cmp(&project.drama[*b].x));
                    for i in order {
                        let d = &project.drama[i];
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::click());
                        let sel = state.drama_sel == Some(d.id);
                        if sel || resp.hovered() {
                            ui.painter().rect_filled(r, tokens::R_CONTROL, if sel { p.selection } else { p.container_hi });
                        }
                        ui.painter().circle_filled(r.left_center() + vec2(12.0, 0.0), 5.0, drama_color(p, d.kind));
                        ui.painter().with_clip_rect(r.shrink2(vec2(24.0, 0.0))).text(r.left_center() + vec2(26.0, 0.0), egui::Align2::LEFT_CENTER, &d.label, FontId::proportional(tokens::FONT_BODY), p.text);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            state.drama_sel = Some(d.id);
                        }
                    }
                });
            });
            // Lo que se anota del punto elegido.
            ui.scope_builder(egui::UiBuilder::new().max_rect(detail), |ui| {
                let Some(d) = state.drama_sel.and_then(|id| project.drama.iter_mut().find(|d| d.id == id)) else {
                    ui.label(RichText::new("Elegí un punto de la curva o de la lista para anotarlo.").color(p.dim));
                    return;
                };
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut d.label).desired_width(220.0).hint_text("Nombre del punto")).changed();
                    egui::ComboBox::from_id_salt(("drama_kind", d.id)).width(170.0).selected_text(d.kind.label()).show_ui(ui, |ui| {
                        for k in DramaKind::ALL {
                            if ui.selectable_label(d.kind == k, k.label()).clicked() {
                                d.kind = k;
                                changed = true;
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Lugar").size(tokens::FONT_SMALL).color(p.dim));
                    let (mut x, mut y) = (d.x * 100.0, d.y * 100.0);
                    changed |= ui.add(egui::DragValue::new(&mut x).range(0.0..=100.0).suffix(" %")).changed();
                    ui.label(RichText::new("Tensión").size(tokens::FONT_SMALL).color(p.dim));
                    changed |= ui.add(egui::DragValue::new(&mut y).range(0.0..=100.0).suffix(" %")).changed();
                    (d.x, d.y) = (x / 100.0, y / 100.0);
                });
                changed |= ui
                    .add_sized(
                        vec2(ui.available_width(), ui.available_height().max(40.0)),
                        egui::TextEdit::multiline(&mut d.text).hint_text("Qué pasa en este punto"),
                    )
                    .changed();
            });
        });
        // Supr borra el punto elegido, si no se está escribiendo.
        if state.drama_sel.is_some()
            && ui.memory(|m| m.focused()).is_none()
            && ui.input(|i| i.key_pressed(egui::Key::Delete))
        {
            remove = true;
        }
    });
    if let Some((kind, spot)) = add {
        let id = project.add_drama_point(kind);
        if let (Some((x, y)), Some(d)) = (spot, project.drama.iter_mut().find(|d| d.id == id)) {
            (d.x, d.y) = (x, y);
        }
        state.drama_sel = Some(id);
        changed = true;
    }
    if remove
        && let Some(id) = state.drama_sel.take()
    {
        project.drama.retain(|d| d.id != id);
        changed = true;
    }
    changed
}
