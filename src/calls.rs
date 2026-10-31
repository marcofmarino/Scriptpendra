//! Hojas de llamado: se elige un día del plan y se completa lo que no sale solo (citaciones, almuerzo, pronóstico,
//! notas por área). Debajo se ve lo que la hoja trae del plan: lugar, escenas, equipo y lo que piden las escenas.

use eframe::egui::{self, RichText, Ui};

use crate::{
    callsheet::{self, CallSheet},
    icon, reports,
    storage::Paper,
    theme::{self, Palette},
    ui::{kit, tokens},
    widgets::{self, card},
};

#[derive(Default)]
pub struct CallsState {
    selected: Option<u64>,
}

#[derive(Default)]
pub struct Out {
    pub changed: bool,
    pub status: Option<String>,
}

use crate::model::Project;

fn save_pdf(
    project: &Project,
    paper: Paper,
    name: &str,
    pages: Vec<Vec<crate::pdf::Line>>,
) -> Option<String> {
    let sheet = paper.sheet();
    let bytes = match crate::pdf::render(
        &pages,
        sheet.width * 72.0,
        sheet.height * 72.0,
        &format!("{} — {name}", project.title),
        &project.author,
    ) {
        Ok(bytes) => bytes,
        Err(e) => return Some(format!("Error al exportar: {e}")),
    };
    let path = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name(format!("{} - {name}.pdf", project.title))
        .save_file()?;
    Some(match std::fs::write(&path, bytes) {
        Ok(()) => "Exportado".to_string(),
        Err(e) => format!("Error al exportar: {e}"),
    })
}

/// Un campo apilado de una línea con texto de ayuda. Devuelve si cambió.
fn time_field(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    text: &mut String,
    hint: &str,
    _width: f32,
) -> bool {
    kit::field_full(ui, p, label, text, 1, false, hint).0
}

/// Elige un día si no hay uno válido.
fn ensure_selected(state: &mut CallsState, days: &[crate::schedule::DayView]) {
    if state
        .selected
        .is_none_or(|id| !days.iter().any(|d| d.id == id))
    {
        state.selected = days.first().map(|d| d.id);
    }
}

fn day_label(project: &Project, d: &crate::schedule::DayView) -> String {
    let date = project
        .shoot_days
        .iter()
        .find(|s| s.id == d.id)
        .map(|s| s.date.clone())
        .unwrap_or_default();
    if date.is_empty() {
        format!("Jornada {}", d.number)
    } else {
        format!("Jornada {} · {date}", d.number)
    }
}

/// Herramientas de la barra: exportar la hoja completa o la versión reducida para el elenco.
pub fn toolbar(ui: &mut Ui, state: &mut CallsState, project: &mut Project, paper: Paper) -> Out {
    let mut out = Out::default();
    let days = project.days_view();
    ensure_selected(state, &days);
    let sheet = state.selected.and_then(|id| callsheet::build(project, id));
    kit::export_menu(ui, |ui| {
        if ui
            .add_enabled(sheet.is_some(), egui::Button::new("Hoja completa (PDF)"))
            .clicked()
        {
            if let Some(sheet) = &sheet {
                out.status = save_pdf(
                    project,
                    paper,
                    &format!("llamado jornada {}", sheet.day_number),
                    reports::call_sheet_pages(sheet, paper),
                );
            }
            ui.close();
        }
        if ui
            .add_enabled(
                sheet.as_ref().is_some_and(|s| !s.cast.is_empty()),
                egui::Button::new("Versión reducida para cada persona del elenco (PDF)"),
            )
            .clicked()
        {
            if let Some(sheet) = &sheet {
                out.status = save_pdf(
                    project,
                    paper,
                    &format!("llamado elenco jornada {}", sheet.day_number),
                    reports::all_actor_sheets(sheet, paper),
                );
            }
            ui.close();
        }
    });
    out
}

/// Barra de contexto: la jornada, con flechas.
pub fn context_bar(ui: &mut Ui, state: &mut CallsState, project: &mut Project, p: &Palette) {
    let days = project.days_view();
    if days.is_empty() {
        return;
    }
    ensure_selected(state, &days);
    crate::ui::shell::context_bar(ui, p, |ui| {
        let labels: Vec<String> = days.iter().map(|d| day_label(project, d)).collect();
        let active = days
            .iter()
            .position(|d| Some(d.id) == state.selected)
            .unwrap_or(0);
        if let Some(i) = kit::stepper_select(ui, p, "call_day", &labels, active) {
            state.selected = Some(days[i].id);
        }
    });
}

/// La hoja de llamado como se va a imprimir (lo que trae del plan); los datos manuales se completan en el inspector.
pub fn show(ui: &mut Ui, state: &mut CallsState, project: &mut Project, p: &Palette) -> Out {
    let out = Out::default();
    project.prune_schedule();
    let days = project.days_view();
    if days.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::SCROLL,
            "Todavía no hay días de rodaje. Armá el plan en Producción → Plan de rodaje y cada día tiene su hoja de llamado.",
            None,
        );
        return out;
    }
    ensure_selected(state, &days);
    let Some(sheet) = callsheet::build(project, state.selected.unwrap_or(0)) else {
        return out;
    };
    egui::ScrollArea::vertical()
        .id_salt("calls_preview")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            widgets::column(ui, tokens::FICHA_MAX, |ui| {
                ui.add_space(tokens::SP_M);
                preview_card(ui, p, &sheet);
                ui.add_space(tokens::SP_XXL);
            });
        });
    out
}

/// El inspector: HORARIOS, ELENCO y NOTAS del día elegido.
pub fn inspector(
    ui: &mut Ui,
    state: &mut CallsState,
    project: &mut Project,
    p: &Palette,
    paper: Paper,
    tab: usize,
) -> Out {
    let mut out = Out::default();
    let days = project.days_view();
    ensure_selected(state, &days);
    (|ui: &mut Ui| {
        let Some(day_id) = state.selected.filter(|_| !days.is_empty()) else {
            kit::empty_state(
                ui,
                p,
                icon::SCROLL,
                "Todavía no hay días de rodaje. Armá el plan y cada día tiene su hoja.",
                None,
            );
            return;
        };
        let Some(sheet) = callsheet::build(project, day_id) else {
            return;
        };
        match tab {
            0 => schedule_card(ui, p, project, day_id, &sheet, &mut out),
            1 => {
                if let Some(character) = cast_card(ui, p, project, day_id, &sheet, &mut out) {
                    if let Some(line) = sheet.cast.iter().find(|c| c.character == character) {
                        out.status = save_pdf(
                            project,
                            paper,
                            &format!("llamado {}", line.name),
                            reports::actor_sheet_pages(&sheet, character, paper),
                        );
                    }
                }
            }
            _ => {
                kit::section(ui, p, "Notas generales del día", None);
                if let Some(day) = project.shoot_days.iter_mut().find(|d| d.id == day_id) {
                    out.changed |= kit::field(ui, p, "Notas", &mut day.notes, 3);
                }
                crew_card(ui, p, project, day_id, &sheet, &mut out);
            }
        }
    })(ui);
    out
}

fn schedule_card(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    day_id: u64,
    sheet: &CallSheet,
    out: &mut Out,
) {
    card(ui, p, |ui| {
        ui.label(
            RichText::new(format!(
                "JORNADA {} DE {}",
                sheet.day_number, sheet.days_total
            ))
            .size(11.0)
            .color(p.dim),
        );
        ui.add_space(tokens::SP_S);
        if let Some(day) = project.shoot_days.iter_mut().find(|d| d.id == day_id) {
            out.changed |= time_field(ui, p, "Fecha", &mut day.date, "AAAA-MM-DD", 0.0);
            out.changed |= time_field(ui, p, "Citación general", &mut day.call_time, "07:00", 0.0);
            out.changed |= time_field(ui, p, "Almuerzo", &mut day.lunch, "13:00", 0.0);
            out.changed |= time_field(ui, p, "Fin estimado", &mut day.wrap_time, "19:00", 0.0);
            out.changed |= time_field(
                ui,
                p,
                "Pronóstico",
                &mut day.forecast,
                "Soleado, 24 °C, sin viento",
                0.0,
            );
        }
        ui.add_space(tokens::SP_S);
        match (&sheet.sunrise, &sheet.sunset) {
            (Some(rise), Some(set)) => {
                ui.label(RichText::new(format!("Sale el sol {rise} · se pone {set} (calculado sin internet con las coordenadas del lugar)")).size(12.0).color(p.dim));
            }
            _ => {
                let why = if sheet.date.is_empty() {
                    "Poné la fecha del día"
                } else if sheet.places.is_empty() {
                    "Asigná un lugar de rodaje a las escenas"
                } else {
                    "Cargá las coordenadas del lugar de rodaje"
                };
                ui.label(
                    RichText::new(format!(
                        "{why} para calcular la salida y la puesta del sol."
                    ))
                    .size(12.0)
                    .color(p.dim),
                );
            }
        }
    });
}

/// El elenco del día con sus citaciones. Devuelve el personaje cuya hoja se pidió exportar.
fn cast_card(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    day_id: u64,
    sheet: &CallSheet,
    out: &mut Out,
) -> Option<u64> {
    let mut export: Option<u64> = None;
    card(ui, p, |ui| {
        ui.label(RichText::new("ELENCO DEL DÍA").size(12.0).color(p.dim));
        ui.add_space(tokens::SP_S);
        if sheet.cast.is_empty() {
            ui.label(RichText::new("Ningún personaje en las escenas de este día.").color(p.dim));
        }
        let Some(day) = project.shoot_days.iter_mut().find(|d| d.id == day_id) else {
            return;
        };
        for c in &sheet.cast {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(c.number.map_or(String::new(), |n| format!("{n}.")))
                        .color(p.accent),
                );
                ui.add(egui::Label::new(RichText::new(&c.name).color(p.text)).truncate());
                if kit::icon_button(
                    ui,
                    p,
                    icon::EXPORT,
                    "Exportar la hoja de este personaje (PDF)",
                    true,
                )
                .clicked()
                {
                    export = Some(c.character);
                }
            });
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{} {}", c.actor, c.phone))
                        .size(12.0)
                        .color(p.dim),
                )
                .truncate(),
            );
            let call = day.cast_calls.entry(c.character).or_default();
            for (label, value, hint) in [
                ("Citación", &mut call.call, "08:00"),
                ("Maquillaje", &mut call.makeup, "07:30"),
                ("Al set", &mut call.set, "09:00"),
            ] {
                out.changed |= time_field(ui, p, label, value, hint, 0.0);
            }
            ui.add_space(tokens::SP_S);
        }
        // Lo que quedó vacío no se guarda.
        day.cast_calls
            .retain(|_, c| !(c.call.is_empty() && c.makeup.is_empty() && c.set.is_empty()));
    });
    export
}

fn crew_card(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    day_id: u64,
    sheet: &CallSheet,
    out: &mut Out,
) {
    card(ui, p, |ui| {
        ui.label(RichText::new("EQUIPO TÉCNICO").size(11.0).color(p.dim));
        ui.add_space(tokens::SP_S);
        if sheet.crew.is_empty() {
            ui.label(RichText::new("Todavía no hay contactos del equipo. Cargalos en Producción → Contactos, con su departamento.").size(12.0).color(p.dim));
        }
        let Some(day) = project.shoot_days.iter_mut().find(|d| d.id == day_id) else {
            return;
        };
        for area in &sheet.crew {
            ui.add(
                egui::Label::new(
                    RichText::new(&area.department)
                        .font(egui::FontId::new(14.0, theme::bold_family()))
                        .color(p.text),
                )
                .truncate(),
            );
            let call = day.area_calls.entry(area.department.clone()).or_default();
            out.changed |= time_field(ui, p, "Citación", call, "06:30", 0.0);
            ui.label(
                RichText::new(
                    area.people
                        .iter()
                        .map(|(n, r, t)| {
                            format!(
                                "{n}{} {t}",
                                if r.is_empty() {
                                    String::new()
                                } else {
                                    format!(" ({r})")
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" · "),
                )
                .size(12.0)
                .color(p.dim),
            );
            let note = day.area_notes.entry(area.department.clone()).or_default();
            out.changed |= kit::field_full(
                ui,
                p,
                &format!("Notas para {}", area.department),
                note,
                2,
                false,
                "",
            )
            .0;
            ui.add_space(tokens::SP_S);
        }
        day.area_calls.retain(|_, c| !c.is_empty());
        day.area_notes.retain(|_, c| !c.trim().is_empty());
    });
}

/// Lo que la hoja trae solo del plan, para revisarlo antes de imprimirla.
fn preview_card(ui: &mut Ui, p: &Palette, sheet: &CallSheet) {
    card(ui, p, |ui| {
        ui.label(
            RichText::new("LO QUE TRAE DEL PLAN")
                .size(11.0)
                .color(p.dim),
        );
        ui.add_space(tokens::SP_S);
        let section = |ui: &mut Ui, title: &str| {
            ui.add_space(tokens::SP_XS);
            ui.label(
                RichText::new(title)
                    .font(egui::FontId::new(13.0, theme::bold_family()))
                    .color(p.text),
            );
        };
        section(ui, "Locación");
        if sheet.places.is_empty() {
            ui.label(
                RichText::new(
                    "Sin lugar de rodaje: asignalo a las locaciones del guion o a las escenas.",
                )
                .size(12.0)
                .color(p.dim),
            );
        }
        for place in &sheet.places {
            let contact = place
                .contact
                .as_ref()
                .map_or(String::new(), |(n, t)| format!(" · {n} {t}"));
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{} — {}{contact}",
                        place.name,
                        if place.address.is_empty() {
                            "sin dirección"
                        } else {
                            &place.address
                        }
                    ))
                    .color(p.text),
                )
                .wrap(),
            );
            if !place.hospital.is_empty() {
                ui.label(
                    RichText::new(format!("Hospital más cercano: {}", place.hospital))
                        .size(12.0)
                        .color(p.dim),
                );
            }
        }
        section(ui, "Escenas");
        for s in &sheet.scenes {
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{}  {} · {} págs.", s.number, s.heading, s.eighths))
                        .color(p.text),
                )
                .wrap(),
            );
        }
        if !sheet.requirements.is_empty() {
            section(ui, "Requerimientos");
            for (category, items) in &sheet.requirements {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{category}: {}", items.join(", ")))
                            .size(12.0)
                            .color(p.dim),
                    )
                    .wrap(),
                );
            }
        }
        if let Some((number, date, scenes)) = &sheet.next {
            section(
                ui,
                &format!(
                    "Próxima jornada ({number}{})",
                    if date.is_empty() {
                        String::new()
                    } else {
                        format!(" · {date}")
                    }
                ),
            );
            for s in scenes {
                ui.label(
                    RichText::new(format!("{}  {}", s.number, s.heading))
                        .size(12.0)
                        .color(p.dim),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind::*;

    #[test]
    fn the_screen_draws_with_and_without_days_and_saves_what_is_typed_in_the_day() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let palette = crate::storage::Settings::default().scheme.palette();
        let mut project = Project::default();
        project.paragraphs = [
            (SceneHeading, "INT. A - DÍA"),
            (Character, "ANA"),
            (Dialogue, "Hola."),
        ]
        .iter()
        .map(|(k, t)| project.make_paragraph(*k, t))
        .collect();
        project.auto_link_all();
        let mut state = CallsState::default();
        let run = |state: &mut CallsState, project: &mut Project| {
            for n in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1300.0, 900.0),
                    )),
                    time: Some(n as f64 * 0.1),
                    focused: true,
                    ..Default::default()
                };
                let mut o = ctx.run_ui(input, |ui| {
                    toolbar(ui, state, project, Paper::A4);
                    context_bar(ui, state, project, &palette);
                    show(ui, state, project, &palette);
                    inspector(ui, state, project, &palette, Paper::A4, 1);
                });
                o.textures_delta.clear();
            }
        };
        run(&mut state, &mut project);
        let scene = project.script_scenes()[0].0;
        let day = project.add_shoot_day();
        project.place_scenes(&[scene], None);
        let contact = project.add_contact("Marta");
        project.contacts[0].department = "Sonido".into();
        let _ = contact;
        run(&mut state, &mut project);
        assert_eq!(state.selected, Some(day));
        // Lo vacío no deja basura en el día.
        let shoot = project.shoot_days.iter().find(|d| d.id == day).unwrap();
        assert!(
            shoot.cast_calls.is_empty()
                && shoot.area_calls.is_empty()
                && shoot.area_notes.is_empty()
        );
    }
}
