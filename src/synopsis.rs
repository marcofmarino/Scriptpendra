//! Sinopsis: el texto de la historia, con las herramientas para crear personajes, locaciones… sin salir de él.

use crate::ui::{kit, tokens};
use eframe::egui::{
    self, Align, FontId, Id, Key, Modifiers, RichText, TextEdit, Ui,
    text::{CCursor, CCursorRange},
    text_edit::TextEditState,
    vec2,
};

use crate::{
    create::{self, NewEntity},
    editor::styled_job,
    icon,
    model::{CaseOptions, Project, adjust_links, mention_spans},
    storage::Settings,
    theme::{self, Palette},
    widgets,
};

/// Píxeles por pulgada de la hoja al 100 % de zoom (igual que el tratamiento).
const INCH: f32 = 120.0;

#[derive(Default)]
pub struct SynopsisState {
    entity: Option<NewEntity>,
    /// Teclas de deshacer/rehacer pedidas desde la barra; se entregan al texto en el próximo cuadro.
    pub keys: Vec<(Key, Modifiers)>,
    /// Cambio pedido al zoom de la vista, en pasos; lo aplica la app.
    pub zoom_delta: i32,
    focus: Option<usize>,
    sel: Option<(usize, usize)>,
    cursor: Option<usize>,
    was_focused: bool,
}

impl SynopsisState {
    /// Abre el formulario; con un texto seleccionado, ese texto es el nombre y se convierte en lo que se cree.
    pub fn open_entity(&mut self, project: &Project) {
        let quote = self.sel.map(|(a, b)| {
            project
                .synopsis
                .chars()
                .skip(a)
                .take(b - a)
                .collect::<String>()
        });
        let quote = quote.filter(|q| !q.trim().is_empty());
        let replace = self.sel.filter(|_| quote.is_some());
        let name = quote.map(|q| q.trim().to_string()).unwrap_or_default();
        self.entity = Some(NewEntity {
            fresh: true,
            name,
            replace,
            ..Default::default()
        });
    }
}

fn text_id() -> Id {
    Id::unique("synopsis_text")
}

/// Herramientas de la sinopsis en la barra de herramientas: crear personajes, locaciones… (deshacer y rehacer los dibuja
/// el marco).
pub fn toolbar(ui: &mut Ui, state: &mut SynopsisState, project: &Project, p: &Palette) {
    let mut entity = false;
    crate::ui::kit::toolbar_groups(ui, "synopsis_tools", &[1], |ui, _| {
        entity |= widgets::icon_button(
            ui,
            p,
            icon::USER_PLUS,
            "Crear personaje, locación, objeto… (Ctrl+Mayús+N)",
            true,
        )
        .clicked()
    });
    if entity {
        state.open_entity(project);
    }
}

/// Zoom de la vista, contra el borde derecho de la barra de herramientas.
pub fn toolbar_end(ui: &mut Ui, state: &mut SynopsisState, p: &Palette, settings: &Settings) {
    state.zoom_delta += crate::ui::kit::zoom_control(ui, p, settings.view_zoom);
}

/// Dibuja la sinopsis y la idea corta. Devuelve si el proyecto cambió.
pub fn show(
    ui: &mut Ui,
    state: &mut SynopsisState,
    project: &mut Project,
    settings: &Settings,
    p: &Palette,
) -> bool {
    let ctx = ui.ctx().clone();
    let id = text_id();
    let mut changed = false;

    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::N)) {
        state.open_entity(project);
    }
    if !state.keys.is_empty() {
        ctx.memory_mut(|m| m.request_focus(id));
        for (key, modifiers) in state.keys.drain(..) {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers,
                })
            });
        }
    }
    if let Some(at) = state.focus.take() {
        let len = project.synopsis.chars().count();
        ctx.memory_mut(|m| m.request_focus(id));
        let mut edit = TextEditState::load(&ctx, id).unwrap_or_default();
        edit.cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(at.min(len)))));
        edit.store(&ctx, id);
    }

    let mentions = project.mention_names();
    let case = CaseOptions::as_written();
    let links = project.synopsis_links.clone();
    let color = p.text;
    let before = project.synopsis.clone();
    let paper = settings.paper.sheet();
    let zoom = settings.view_zoom as f32 / 100.0;
    let mut out_text: Option<egui::text_edit::TextEditOutput> = None;

    egui::ScrollArea::vertical()
        .id_salt("synopsis_sheet")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let size = vec2(paper.width * INCH, paper.height * INCH);
            kit::centered_column(ui, size.x * zoom, |ui| {
                changed |= kit::field(
                    ui,
                    p,
                    "Descripción corta (logline)",
                    &mut project.logline,
                    1,
                );
                // Los márgenes y la letra escalan con el ancho real de la hoja, que se achica si no entra.
                let width = (size.x * zoom).min(ui.available_width()).max(1.0);
                let k = width / size.x;
                let inch = INCH * k;
                let pt = crate::storage::TREATMENT_PT as f32 * inch / 72.0;
                let font = FontId::new(pt, theme::sans_family());
                let bold = FontId::new(pt, theme::sans_bold_family());
                let line_h = pt * 1.15;
                let (margin_l, margin_t) = (paper.left * inch, paper.top * inch);
                let text_w = (width - margin_l - paper.right * inch).max(40.0);
                let layout = |ui: &Ui, text: &str, wrap: f32| {
                    let spans = mention_spans(text, &links, &mentions, &case);
                    let job = styled_job(
                        text,
                        &font,
                        &bold,
                        color,
                        wrap,
                        line_h,
                        &spans,
                        false,
                        false,
                        Align::LEFT,
                    );
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                // La hoja crece con el texto: nunca queda más corta que el papel.
                let text_h = layout(ui, &project.synopsis, text_w).size().y;
                let needed = margin_t + text_h + paper.bottom * inch;
                let size = vec2(size.x, size.y.max(needed / k));
                kit::sheet(ui, p, kit::SheetTone::Theme, size, zoom, |ui| {
                    let rect = egui::Rect::from_min_size(
                        ui.max_rect().min + vec2(margin_l, margin_t),
                        vec2(text_w, size.y * k - margin_t - paper.bottom * inch),
                    );
                    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                        layout(ui, buf.as_str(), wrap)
                    };
                    out_text = Some(
                        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                            TextEdit::multiline(&mut project.synopsis)
                                .id(id)
                                .layouter(&mut layouter)
                                .desired_width(text_w)
                                .min_size(rect.size())
                                .margin(egui::Margin::ZERO)
                                .frame(egui::Frame::NONE)
                                .hint_text(RichText::new("Escribí la sinopsis…").color(p.dim))
                                .show(ui)
                        })
                        .inner,
                    );
                });
                ui.add_space(tokens::SP_XL);
            });
        });
    if let Some(out) = out_text {
        if out.response.changed() {
            changed = true;
            adjust_links(&mut project.synopsis_links, &before, &project.synopsis);
        }
        if out.response.has_focus() {
            let range = out
                .cursor_range
                .map(|r| (usize::from(r.primary.index), usize::from(r.secondary.index)));
            state.cursor = range.map(|(a, _)| a);
            state.sel = range.and_then(|(a, b)| (a != b).then_some((a.min(b), a.max(b))));
        }
        // Al soltar el foco, los nombres que coinciden con algo del proyecto se vinculan.
        if state.was_focused && !out.response.has_focus() {
            let links_before = project.synopsis_links.clone();
            project.auto_link_synopsis();
            changed |= links_before != project.synopsis_links;
        }
        state.was_focused = out.response.has_focus();
    }

    if let Some(form) = create::dialog(&ctx, &mut state.entity, project, p) {
        if let Some(target) = create::create_entity(project, &form) {
            let len = project.synopsis.chars().count();
            let (range, typed) = match form.replace {
                Some((a, b)) => (
                    (a, b),
                    project
                        .synopsis
                        .chars()
                        .skip(a)
                        .take(b - a)
                        .collect::<String>()
                        .trim()
                        .to_string(),
                ),
                None => {
                    let at = state.cursor.unwrap_or(len).min(len);
                    ((at, at), "A".to_string())
                }
            };
            let mut text = std::mem::take(&mut project.synopsis);
            let mut links = std::mem::take(&mut project.synopsis_links);
            let at = create::splice(&mut text, &mut links, project, target, range, &typed);
            project.synopsis = text;
            project.synopsis_links = links;
            state.focus = Some(at);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selected_word_in_the_synopsis_becomes_a_character_and_follows_renames() {
        let mut project = Project::default();
        project.synopsis = "Lucas busca a Miriam.".into();
        let mut state = SynopsisState::default();
        state.sel = Some((0, 5));
        state.open_entity(&project);
        let mut form = state.entity.take().unwrap();
        assert_eq!(form.name, "Lucas");
        form.kind = crate::create::Kind::Character;
        let target = create::create_entity(&mut project, &form).unwrap();
        let mut text = std::mem::take(&mut project.synopsis);
        let mut links = std::mem::take(&mut project.synopsis_links);
        let at = create::splice(&mut text, &mut links, &project, target, (0, 5), "Lucas");
        project.synopsis = text;
        project.synopsis_links = links;
        assert_eq!(
            (project.synopsis.as_str(), at),
            ("Lucas busca a Miriam.", 5)
        );
        assert_eq!(project.synopsis_links.len(), 1);
        // Renombrar al personaje cambia el nombre en la sinopsis.
        project.characters[0].name = "LUCIANO".into();
        project.sync_links();
        assert!(
            project.synopsis.starts_with("Luciano busca")
                || project.synopsis.starts_with("LUCIANO busca"),
            "{}",
            project.synopsis
        );
    }
}
