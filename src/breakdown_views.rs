//! Las dos vistas nuevas del desglose: la hoja de cada escena (la hoja clásica de desglose, una por escena) y la lista
//! de compras y alquileres, con costos, responsables y un presupuesto simple. Las dos se imprimen.

use crate::ui::tokens;
use eframe::egui::{self, Color32, RichText, Ui};

use crate::{
    icon,
    model::{Acquisition, CategorySource, Project, fmt_eighths, heading_parts},
    reports::{self, Group},
    storage::Paper,
    theme::{self, Palette},
    ui::kit::{self, TableCol},
    widgets,
};

pub struct ViewState {
    sheet_scene: Option<u64>,
    /// Qué estados entran en la lista de compras (en el orden de `Acquisition::ALL`).
    statuses: [bool; 5],
    group: Group,
}

impl ViewState {
    /// Muestra la hoja de desglose de una escena.
    pub fn show_scene(&mut self, scene: u64) {
        self.sheet_scene = Some(scene);
    }
}

impl Default for ViewState {
    fn default() -> Self {
        // Pendiente, alquilar, comprar y fabricar; lo ya conseguido no se compra.
        Self {
            sheet_scene: None,
            statuses: [true, false, true, true, true],
            group: Group::Category,
        }
    }
}

impl ViewState {
    fn selected(&self) -> Vec<Acquisition> {
        Acquisition::ALL
            .iter()
            .zip(self.statuses)
            .filter(|(_, on)| *on)
            .map(|(s, _)| *s)
            .collect()
    }
}

#[derive(Default)]
pub struct Out {
    pub changed: bool,
    /// Abrir esta escena en el guion.
    pub open_scene: Option<u64>,
    /// Un mensaje breve para el aviso de abajo.
    pub status: Option<String>,
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn save(path_name: &str, filter: (&str, &[&str]), bytes: Vec<u8>) -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter(filter.0, filter.1)
        .set_file_name(path_name)
        .save_file()?;
    Some(match std::fs::write(&path, bytes) {
        Ok(()) => "Exportado".to_string(),
        Err(e) => format!("Error al exportar: {e}"),
    })
}

// ------------------------------------------------------------------------------------------------
// Hoja por escena
// ------------------------------------------------------------------------------------------------

pub fn sheet(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut ViewState,
    p: &Palette,
    paper: Paper,
) -> Out {
    let mut out = Out::default();
    let scenes = project.script_scenes();
    if scenes.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::CLIPBOARD_TEXT,
            "Todavía no hay escenas. Escribí un encabezado en el Guion y acá aparece su hoja de desglose.",
            None,
        );
        return out;
    }
    let labels = project.scene_labels();
    if state
        .sheet_scene
        .is_none_or(|id| !scenes.iter().any(|(s, _)| *s == id))
    {
        state.sheet_scene = scenes.first().map(|(id, _)| *id);
    }
    let at = scenes
        .iter()
        .position(|(s, _)| Some(*s) == state.sheet_scene)
        .unwrap_or(0);
    let (id, heading) = scenes[at].clone();

    crate::ui::kit::sides(
        ui,
        |ui| {
            if widgets::icon_button(
                ui,
                p,
                icon::CARET_RIGHT,
                "Escena siguiente",
                at + 1 < scenes.len(),
            )
            .clicked()
            {
                state.sheet_scene = Some(scenes[at + 1].0);
            }
            egui::ComboBox::from_id_salt("sheet_scene")
                .width(crate::ui::kit::fill_width(ui, 80.0, 380.0))
                .selected_text(format!("{}. {}", labels[at].1, heading.trim()))
                .show_ui(ui, |ui| {
                    for (n, (sid, text)) in scenes.iter().enumerate() {
                        if ui
                            .selectable_label(
                                *sid == id,
                                format!("{}. {}", labels[n].1, text.trim()),
                            )
                            .clicked()
                        {
                            state.sheet_scene = Some(*sid);
                        }
                    }
                });
            if widgets::icon_button(ui, p, icon::CARET_DOWN, "Escena anterior", at > 0).clicked() {
                state.sheet_scene = Some(scenes[at - 1].0);
            }
        },
        |ui| {
            if ui.add_enabled(true, egui::Button::new(format!("{}  Exportar", icon::EXPORT))).on_hover_text("Exportar las hojas de todas las escenas a PDF").clicked()
            {
                let pages = reports::scene_sheets(project, paper);
                let sheet = paper.sheet();
                out.status = crate::pdf::render(
                    &pages,
                    sheet.width * 72.0,
                    sheet.height * 72.0,
                    &format!("{} — desglose", project.title),
                    &project.author,
                )
                .map_err(|e| format!("Error al exportar: {e}"))
                .map(|bytes| {
                    save(
                        &format!("{} - desglose.pdf", project.title),
                        ("PDF", &["pdf"]),
                        bytes,
                    )
                })
                .unwrap_or_else(Some);
            }
            if widgets::icon_button(ui, p, icon::SCROLL, "Ir a la escena en el guion", true)
                .clicked()
            {
                out.open_scene = Some(id);
            }
        },
    );
    ui.add_space(tokens::SP_M);

    let meta = project.scenes.get(&id).cloned().unwrap_or_default();
    let (kind, location, time) = heading_parts(&heading);
    let place = project
        .scene_shoot_location(id)
        .and_then(|s| project.shoot_locations.iter().find(|l| l.id == s))
        .map(|l| l.name.clone());
    let eighths = project.scene_eighths(id);
    let cast = project.scene_cast_detail(id);
    let items = project.scene_sheet_items(id);
    let categories = project.breakdown_categories.clone();

    // Datos de la ficha de la escena.
    widgets::card(ui, p, |ui| {
        ui.label(
            RichText::new(format!("ESCENA {}", labels[at].1))
                .size(11.0)
                .color(p.dim),
        );
        ui.label(
            RichText::new(heading.trim())
                .font(egui::FontId::new(18.0, theme::bold_family()))
                .color(p.text),
        );
        ui.add_space(tokens::SP_S);
        let day = meta
            .story_day
            .map_or("—".to_string(), |d| format!("Día {d}"));
        let facts = [
            (
                "INT/EXT",
                if kind.is_empty() {
                    "—".to_string()
                } else {
                    kind
                },
            ),
            (
                "Momento",
                if time.is_empty() {
                    "—".to_string()
                } else {
                    time
                },
            ),
            ("Octavos", fmt_eighths(eighths)),
            ("Día de la historia", day),
            (
                "Locación del guion",
                if location.is_empty() {
                    "—".to_string()
                } else {
                    location
                },
            ),
            ("Se filma en", place.unwrap_or_else(|| "—".to_string())),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, value) in facts {
                ui.label(RichText::new(label).size(12.0).color(p.dim));
                ui.label(RichText::new(value).color(p.text));
                ui.add_space(tokens::SP_M);
            }
        });
        if !meta.summary.is_empty() {
            ui.add_space(tokens::SP_XS);
            ui.add(egui::Label::new(RichText::new(&meta.summary).color(p.text)).wrap());
        }
    });
    ui.add_space(tokens::SP_M);

    // Elenco: con su número y quien lo interpreta.
    if !cast.is_empty() {
        box_with_title(ui, p, "Elenco", p.accent, |ui| {
            for (number, name, actor) in &cast {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(number.map_or("  ".to_string(), |n| format!("{n}.")))
                            .color(p.accent),
                    );
                    ui.label(RichText::new(name).color(p.text));
                    if let Some(actor) = actor {
                        ui.label(RichText::new(format!("— {actor}")).color(p.dim));
                    }
                });
            }
        });
        ui.add_space(tokens::SP_S);
    }

    // Una caja por categoría con lo que pide la escena.
    let mut edits: Vec<(u64, Option<u32>)> = Vec::new();
    for cat in categories.iter().filter(|c| {
        !matches!(
            c.source,
            CategorySource::Characters | CategorySource::Locations
        )
    }) {
        let mine: Vec<_> = items.iter().filter(|i| i.category == cat.id).collect();
        if mine.is_empty() {
            continue;
        }
        box_with_title(ui, p, &cat.name, rgb(cat.color), |ui| {
            for item in mine {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&item.name).color(p.text));
                    // Lo marcado en el texto lleva cuántos hay en esta escena; lo que sale de una ficha, no.
                    if let (Some(item_id), false) = (item.item_id, item.auto) {
                        let mut amount = item.scene_quantity.unwrap_or(0);
                        let resp = ui.add(
                            egui::DragValue::new(&mut amount)
                                .range(0..=999)
                                .prefix("x ")
                                .speed(0.2),
                        );
                        if resp
                            .on_hover_text("Cuántos hay en esta escena (0: sin indicar)")
                            .changed()
                        {
                            edits.push((item_id, (amount > 0).then_some(amount)));
                        }
                        if item.scene_quantity.is_none() && item.quantity > 0 {
                            ui.label(
                                RichText::new(format!("(en total {})", item.quantity))
                                    .size(12.0)
                                    .color(p.dim),
                            );
                        }
                    }
                    if item.status != Acquisition::Pending {
                        ui.label(RichText::new(item.status.label()).size(12.0).color(p.dim));
                    }
                    if !item.notes.is_empty() {
                        ui.label(RichText::new(&item.notes).size(12.0).color(p.dim));
                    }
                });
            }
        });
        ui.add_space(tokens::SP_S);
    }
    if items.iter().all(|i| {
        matches!(
            categories
                .iter()
                .find(|c| c.id == i.category)
                .map(|c| c.source),
            Some(CategorySource::Characters | CategorySource::Locations)
        )
    }) && cast.is_empty()
    {
        ui.label(RichText::new("Esta escena todavía no tiene nada marcado. Marcá texto del guion desde el panel derecho (pestaña Desglose).").color(p.dim));
    }
    if !meta.notes.is_empty() {
        box_with_title(ui, p, "Notas de la escena", p.dim, |ui| {
            ui.add(egui::Label::new(RichText::new(&meta.notes).color(p.text)).wrap());
        });
    }
    for (item, quantity) in edits {
        out.changed |= project.set_scene_quantity(item, id, quantity);
    }
    out
}

/// Una caja con una franja de color y un título, como las de la hoja de desglose en papel.
fn box_with_title(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    color: Color32,
    add: impl FnOnce(&mut Ui),
) {
    let frame = egui::Frame::new()
        .fill(p.container)
        .stroke(egui::Stroke::new(
            1.0,
            theme::mix(p.container, p.border, 0.7),
        ))
        .corner_radius(8)
        .inner_margin(egui::Margin {
            left: 18,
            right: 12,
            top: 8,
            bottom: 10,
        });
    let shown = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title.to_uppercase()).size(11.0).color(color));
        ui.add_space(tokens::SP_XS);
        add(ui);
    });
    let rect = shown.response.rect;
    let stripe = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 5.0, rect.max.y));
    ui.painter().rect_filled(
        stripe,
        egui::CornerRadius {
            nw: 8,
            sw: 8,
            ne: 0,
            se: 0,
        },
        color,
    );
}

// ------------------------------------------------------------------------------------------------
// Lista de compras y alquileres
// ------------------------------------------------------------------------------------------------

/// Un campo de costo en pesos: 0 es «sin costo».
fn cost_field(ui: &mut Ui, cents: &mut Option<i64>) -> bool {
    let mut pesos = cents.unwrap_or(0) as f64 / 100.0;
    let changed = ui
        .add(
            egui::DragValue::new(&mut pesos)
                .range(0.0..=1_000_000_000.0)
                .speed(50.0)
                .prefix("$ "),
        )
        .changed();
    if changed {
        *cents = (pesos > 0.0).then(|| (pesos * 100.0).round() as i64);
    }
    changed
}

pub fn shopping(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut ViewState,
    p: &Palette,
    paper: Paper,
) -> Out {
    let mut out = Out::default();
    // Qué se compra: los estados que se eligen y cómo se agrupa.
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Estados").size(12.0).color(p.dim));
        for (n, status) in Acquisition::ALL.iter().enumerate() {
            ui.checkbox(&mut state.statuses[n], status.label());
        }
        ui.add_space(tokens::SP_M);
        ui.label(RichText::new("Agrupar por").size(12.0).color(p.dim));
        ui.selectable_value(&mut state.group, Group::Category, "Categoría");
        ui.selectable_value(&mut state.group, Group::Responsible, "Responsable");
    });
    ui.add_space(tokens::SP_S);

    let rows = project.shopping_list(&state.selected());
    let total = reports::total(&rows);
    crate::ui::kit::sides(
        ui,
        |ui| {
            ui.label(
                RichText::new(format!(
                    "{} elementos · {}",
                    rows.len(),
                    reports::money(total)
                ))
                .font(egui::FontId::new(15.0, theme::bold_family()))
                .color(p.accent),
            );
        },
        |ui| {
            if widgets::icon_button(
                ui,
                p,
                icon::DOWNLOAD_SIMPLE,
                "Exportar la lista a CSV (planilla)",
                !rows.is_empty(),
            )
            .clicked()
            {
                out.status = save(
                    &format!("{} - compras.csv", project.title),
                    ("CSV", &["csv"]),
                    reports::shopping_csv(project, &rows).into_bytes(),
                );
            }
            if ui.add_enabled(!rows.is_empty(), egui::Button::new(format!("{}  Exportar", icon::EXPORT))).on_hover_text("Exportar la lista a PDF").clicked()
            {
                let pages = reports::shopping_pages(project, rows.clone(), state.group, paper);
                let sheet = paper.sheet();
                out.status = crate::pdf::render(
                    &pages,
                    sheet.width * 72.0,
                    sheet.height * 72.0,
                    &format!("{} — compras", project.title),
                    &project.author,
                )
                .map_err(|e| format!("Error al exportar: {e}"))
                .map(|bytes| {
                    save(
                        &format!("{} - compras.pdf", project.title),
                        ("PDF", &["pdf"]),
                        bytes,
                    )
                })
                .unwrap_or_else(Some);
            }
        },
    );
    ui.add_space(tokens::SP_S);

    let people: Vec<(u64, String)> = project
        .contacts
        .iter()
        .map(|c| (c.id, c.name.clone()))
        .collect();
    if rows.is_empty() {
        ui.label(RichText::new("No hay elementos con esos estados. Los elementos marcados en el guion empiezan Pendientes; cambiales el estado en la vista Por categoría o acá.").color(p.dim));
    }
    for (title, group) in reports::grouped(project, rows.clone(), state.group) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&title)
                    .font(egui::FontId::new(15.0, theme::bold_family()))
                    .color(p.text),
            );
            ui.label(RichText::new(reports::money(reports::total(&group))).color(p.dim));
        });
        const COLS: [TableCol; 7] = [
            TableCol::new("Elemento", 200.0),
            TableCol::new("Cant.", 76.0),
            TableCol::new("Estado", 120.0),
            TableCol::new("Escenas", 120.0),
            TableCol::new("Responsable", 150.0),
            TableCol::new("Costo", 110.0),
            TableCol::rest("Notas", 200.0),
        ];
        kit::table_in_scroll(
            ui,
            &format!("shopping_{title}"),
            &COLS,
            group.len(),
            |ui, i| kit::table_header_text(ui, p, &COLS[i].header),
            |tr| {
                let row = &group[tr.index()];
                let mut changed = false;
                let Some(item) = project
                    .breakdown_items
                    .iter_mut()
                    .find(|i| i.id == row.item)
                else {
                    return;
                };
                tr.col(|ui| {
                    ui.add(egui::Label::new(RichText::new(&row.name).color(p.text)).truncate())
                        .on_hover_text(&row.name);
                });
                tr.col(|ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut item.quantity)
                                .range(0..=999)
                                .speed(0.2),
                        )
                        .changed()
                });
                tr.col(|ui| {
                    egui::ComboBox::from_id_salt(("shop_status", row.item))
                        .width(kit::fill_width(ui, 60.0, 200.0))
                        .truncate()
                        .selected_text(item.status.label())
                        .show_ui(ui, |ui| {
                            for s in Acquisition::ALL {
                                changed |= ui
                                    .selectable_value(&mut item.status, s, s.label())
                                    .changed();
                            }
                        });
                });
                tr.col(|ui| {
                    let scenes = row.scenes.join(", ");
                    ui.add(
                        egui::Label::new(RichText::new(&scenes).size(12.0).color(p.dim)).truncate(),
                    )
                    .on_hover_text(&scenes);
                });
                tr.col(|ui| {
                    let shown = item
                        .responsible
                        .and_then(|r| people.iter().find(|(i, _)| *i == r))
                        .map_or("—".to_string(), |(_, n)| n.clone());
                    egui::ComboBox::from_id_salt(("shop_who", row.item))
                        .width(kit::fill_width(ui, 60.0, 260.0))
                        .truncate()
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(item.responsible.is_none(), "Sin responsable")
                                .clicked()
                            {
                                item.responsible = None;
                                changed = true;
                            }
                            for (pid, name) in &people {
                                if ui
                                    .selectable_label(item.responsible == Some(*pid), name)
                                    .clicked()
                                {
                                    item.responsible = Some(*pid);
                                    changed = true;
                                }
                            }
                        });
                });
                tr.col(|ui| {
                    changed |= cost_field(ui, &mut item.cost_cents);
                    if ui
                        .selectable_label(item.unit_cost, "c/u")
                        .on_hover_text("Costo por unidad: se multiplica por la cantidad")
                        .clicked()
                    {
                        item.unit_cost = !item.unit_cost;
                        changed = true;
                    }
                });
                tr.col(|ui| {
                    let width = kit::fill_width(ui, 40.0, 1000.0);
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut item.notes).desired_width(width))
                        .changed();
                });
                out.changed |= changed;
            },
        );
        ui.add_space(tokens::SP_M);
    }

    // Presupuesto simple: la suma de lo cargado, por categoría (no es un presupuesto de producción).
    let budget = project.budget_by_category();
    if !budget.is_empty() {
        ui.add_space(tokens::SP_S);
        widgets::card(ui, p, |ui| {
            ui.label(
                RichText::new("LO QUE CUESTA LO QUE PIDE EL GUION")
                    .size(11.0)
                    .color(p.dim),
            );
            ui.add_space(tokens::SP_S);
            let mut sum = 0;
            for (category, cents, count) in &budget {
                let name = project
                    .breakdown_categories
                    .iter()
                    .find(|c| c.id == *category)
                    .map_or("", |c| c.name.as_str());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).color(p.text));
                    ui.label(
                        RichText::new(format!("{} · {count} con costo", reports::money(*cents)))
                            .color(p.dim),
                    );
                });
                sum += cents;
            }
            ui.add_space(tokens::SP_XS);
            ui.label(
                RichText::new(format!("Total  {}", reports::money(sum)))
                    .font(egui::FontId::new(15.0, theme::bold_family()))
                    .color(p.accent),
            );
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind::*;

    fn project() -> Project {
        let mut p = Project::default();
        p.paragraphs = [
            (SceneHeading, "INT. A - DÍA"),
            (Action, "Toma la linterna."),
            (SceneHeading, "EXT. B - DÍA"),
            (Action, "Pasa una camioneta."),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p.ensure_breakdown_defaults();
        p.auto_link_all();
        let cat = p
            .breakdown_categories
            .iter()
            .find(|c| c.name == "Utilería")
            .unwrap()
            .id;
        let pid = p.paragraphs[1].id;
        p.add_breakdown_tag(pid, "linterna", cat, "Linterna", false);
        p
    }

    fn frame(ctx: &egui::Context, mut run: impl FnMut(&mut Ui)) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1300.0, 900.0),
            )),
            focused: true,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| run(ui));
        out.textures_delta.clear();
    }

    #[test]
    fn the_sheet_and_the_shopping_list_draw_and_follow_the_selected_states() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let palette = crate::storage::Settings::default().scheme.palette();
        let mut project = project();
        let mut state = ViewState::default();
        // Lo marcado en el guion empieza pendiente: entra en la lista; «conseguido» no.
        assert_eq!(project.shopping_list(&state.selected()).len(), 1);
        project.breakdown_items.last_mut().unwrap().status = Acquisition::Have;
        assert!(project.shopping_list(&state.selected()).is_empty());
        state.statuses[1] = true;
        assert_eq!(project.shopping_list(&state.selected()).len(), 1);
        for _ in 0..3 {
            frame(&ctx, |ui| {
                sheet(ui, &mut project, &mut state, &palette, Paper::A4);
            });
            frame(&ctx, |ui| {
                shopping(ui, &mut project, &mut state, &palette, Paper::A4);
            });
        }
        // Con costos, agrupar por responsable y el presupuesto también dibujan.
        project.breakdown_items.last_mut().unwrap().cost_cents = Some(250_000);
        state.group = Group::Responsible;
        frame(&ctx, |ui| {
            shopping(ui, &mut project, &mut state, &palette, Paper::A4);
        });
        assert_eq!(
            project
                .budget_by_category()
                .iter()
                .map(|(_, c, _)| *c)
                .sum::<i64>(),
            250_000
        );
    }
}
