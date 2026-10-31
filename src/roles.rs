//! Las pantallas de los roles (guion sonoro, Dirección de arte): una lista editable de pedidos por escena, para que cada
//! rol cargue lo que necesita en su propio lugar.

use std::collections::HashSet;

use eframe::egui::{self, RichText, Ui, vec2};

use crate::{
    icon,
    model::{Project, Role, RoleEntry, RoleLink},
    theme::{self, Palette},
    ui::{kit, tokens},
};

/// Ancho de las columnas de texto (tipo y estado son desplegables de ancho propio).
const COMBO_W: f32 = 110.0;
const LINK_W: f32 = 170.0;
const TEXT_W: [f32; 4] = [180.0, 180.0, 150.0, 220.0];

/// Lo que recuerdan las pantallas de roles: las escenas plegadas y la pestaña de Dirección de sonido.
#[derive(Default)]
pub struct RoleState {
    collapsed: HashSet<(Role, Option<u64>)>,
}

impl RoleState {
    /// Pliega todas las escenas del rol (o las despliega).
    pub fn set_all(&mut self, project: &Project, role: Role, collapsed: bool) {
        let keys = std::iter::once(None).chain(project.script_scenes().into_iter().map(|(id, _)| Some(id)));
        for scene in keys {
            if collapsed {
                self.collapsed.insert((role, scene));
            } else {
                self.collapsed.remove(&(role, scene));
            }
        }
    }
}

/// La lista del rol en CSV (una tabla): escena, tipo, título, detalle, extra, estado y notas.
pub fn csv(project: &Project, role: Role) -> String {
    let field = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
    let labels = project.scene_labels();
    let scenes = project.script_scenes();
    let h = role.headers();
    let mut out = format!(
        "Escena,{}\n",
        [h[0], h[1], h[2], h[3], h[4], h[5]].join(",")
    );
    for e in project.role_entries(role) {
        let scene = e
            .scene
            .and_then(|id| scenes.iter().position(|(s, _)| *s == id))
            .map_or("General".to_string(), |n| {
                format!("{}. {}", labels[n].1, scenes[n].1.trim())
            });
        out.push_str(&[scene, e.kind.clone(), e.title.clone(), e.detail.clone(), e.extra.clone(), e.status.clone(), e.notes.clone()]
            .iter()
            .map(|t| field(t))
            .collect::<Vec<_>>()
            .join(","));
        out.push('\n');
    }
    out
}

/// Pide dónde guardar la tabla del rol y la escribe. Devuelve el mensaje para la barra de estado.
pub fn export_csv(project: &Project, role: Role) -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name(format!("{} - {}.csv", project.title, role.title()))
        .save_file()?;
    Some(match std::fs::write(&path, csv(project, role)) {
        Ok(()) => "Exportado".to_string(),
        Err(e) => format!("Error al exportar: {e}"),
    })
}

/// La lista del rol agrupada por escena. Devuelve si el proyecto cambió.
pub fn show(
    ui: &mut Ui,
    project: &mut Project,
    p: &Palette,
    role: Role,
    state: &mut RoleState,
) -> bool {
    // Lo que se puso en las plantas aparece acá, y lo de acá en el Desglose.
    let mut changed = project.sync_equipment();
    if matches!(role, Role::Art | Role::SoundGear | Role::PhotoGear) {
        ui.label(
            RichText::new("Lo que cargues acá aparece también en el Desglose.")
                .size(tokens::FONT_SMALL)
                .color(p.dim),
        );
        ui.add_space(tokens::SP_XS);
    }
    let scenes = project.script_scenes();
    let labels = project.scene_labels();
    let mut add: Option<Option<u64>> = None;
    let mut delete: Option<u64> = None;
    // Las filas de escenas que ya no existen y las generales van juntas, al principio.
    let known: Vec<u64> = scenes.iter().map(|(id, _)| *id).collect();
    let mut groups: Vec<(Option<u64>, String)> = vec![(None, "General".to_string())];
    for (n, (id, heading)) in scenes.iter().enumerate() {
        groups.push((Some(*id), format!("{}. {}", labels[n].1, heading.trim())));
    }
    egui::ScrollArea::both()
        .id_salt(("role_board", role.title()))
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (scene, title) in groups {
                let rows: Vec<usize> = project
                    .role_entries(role)
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| match scene {
                        Some(id) => e.scene == Some(id),
                        None => e.scene.is_none_or(|s| !known.contains(&s)),
                    })
                    .map(|(i, _)| i)
                    .collect();
                if scene.is_some() || !rows.is_empty() {
                    group(ui, p, role, project, state, &title, scene, &rows, &mut changed, &mut add, &mut delete);
                }
            }
            if scenes.is_empty() {
                ui.add_space(tokens::SP_S);
                if ui
                    .button(format!("{}  Agregar una fila general", icon::PLUS))
                    .clicked()
                {
                    add = Some(None);
                }
            }
        });
    if let Some(scene) = add {
        project.add_role_entry(role, scene);
        changed = true;
    }
    if let Some(id) = delete {
        project.remove_role_entry(role, id);
        changed = true;
    }
    if changed {
        project.sync_equipment();
    }
    changed
}

#[allow(clippy::too_many_arguments)]
fn group(
    ui: &mut Ui,
    p: &Palette,
    role: Role,
    project: &mut Project,
    state: &mut RoleState,
    title: &str,
    scene: Option<u64>,
    rows: &[usize],
    changed: &mut bool,
    add: &mut Option<Option<u64>>,
    delete: &mut Option<u64>,
) {
    let key = (role, scene);
    let open = !state.collapsed.contains(&key);
    let (band, resp) = ui.allocate_exact_size(
        vec2(ui.available_width().max(900.0), tokens::ROW_H),
        egui::Sense::click(),
    );
    ui.painter().rect_filled(
        band,
        0,
        if resp.hovered() { p.container_hi } else { p.container },
    );
    ui.painter().text(
        band.left_center() + vec2(tokens::SP_S, 0.0),
        egui::Align2::LEFT_CENTER,
        if open { icon::CARET_DOWN } else { icon::CARET_RIGHT },
        egui::FontId::proportional(tokens::ICON_S),
        p.dim,
    );
    let name = ui.painter().text(
        band.left_center() + vec2(tokens::SP_XL, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::new(tokens::FONT_BODY, theme::bold_family()),
        p.text,
    );
    ui.painter().text(
        egui::pos2(name.right() + tokens::SP_S, band.center().y),
        egui::Align2::LEFT_CENTER,
        rows.len().to_string(),
        egui::FontId::proportional(tokens::FONT_SMALL),
        p.dim,
    );
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        if open {
            state.collapsed.insert(key);
        } else {
            state.collapsed.remove(&key);
        }
    }
    if !open {
        ui.add_space(tokens::SP_XS);
        return;
    }
    let headers = role.headers();
    if !rows.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tokens::SP_S;
            let widths = [
                COMBO_W,
                TEXT_W[0],
                TEXT_W[1],
                TEXT_W[2],
                COMBO_W,
                TEXT_W[3],
            ];
            for (h, w) in headers.iter().zip(widths) {
                ui.add_sized(
                    [w, tokens::ROW_H],
                    egui::Label::new(RichText::new(*h).size(tokens::FONT_SMALL).color(p.dim)),
                );
            }
            if role == Role::Art {
                ui.add_sized(
                    [LINK_W, tokens::ROW_H],
                    egui::Label::new(RichText::new("Vincular a").size(tokens::FONT_SMALL).color(p.dim)),
                );
            }
        });
    }
    // A qué se puede vincular una fila de arte: los objetos clave y las categorías del Desglose.
    let objects: Vec<(u64, String)> = project.objects.iter().map(|o| (o.id, o.name.clone())).collect();
    let categories: Vec<(u64, String)> = project
        .breakdown_categories
        .iter()
        .filter(|c| c.source == crate::model::CategorySource::Custom)
        .map(|c| (c.id, c.name.clone()))
        .collect();
    let kinds = role.kinds();
    let statuses = role.statuses();
    for &i in rows {
        let entry: &mut RoleEntry = &mut project.role_entries_mut(role)[i];
        let id = entry.id;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tokens::SP_S;
            *changed |= pick(ui, ("kind", id), &mut entry.kind, kinds);
            *changed |= text(ui, &mut entry.title, TEXT_W[0], headers[1]);
            *changed |= text(ui, &mut entry.detail, TEXT_W[1], headers[2]);
            *changed |= text(ui, &mut entry.extra, TEXT_W[2], headers[3]);
            *changed |= pick(ui, ("status", id), &mut entry.status, statuses);
            *changed |= text(ui, &mut entry.notes, TEXT_W[3], headers[5]);
            if role == Role::Art {
                *changed |= link_pick(ui, id, &mut entry.link, &objects, &categories);
            }
            // Las filas que vienen de la planta se quitan sacando la cosa de la planta.
            let from_plan = entry.plan_key.is_some();
            if kit::icon_button(
                ui,
                p,
                icon::TRASH,
                if from_plan { "Viene de la planta: se quita desde ella" } else { "Quitar la fila" },
                !from_plan,
            )
            .clicked()
                && !from_plan
            {
                *delete = Some(id);
            }
        });
    }
    if ui
        .button(format!("{}  Agregar", icon::PLUS))
        .on_hover_text("Agregar una fila a esta escena")
        .clicked()
    {
        *add = Some(scene);
    }
    ui.add_space(tokens::SP_M);
}

fn text(ui: &mut Ui, value: &mut String, width: f32, hint: &str) -> bool {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(width)
            .hint_text(hint),
    )
    .changed()
}

fn pick(ui: &mut Ui, id: (&str, u64), value: &mut String, options: &[&str]) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(COMBO_W)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for option in options {
                if ui.selectable_label(value == option, *option).clicked() {
                    *value = option.to_string();
                    changed = true;
                }
            }
        });
    changed
}

/// El vínculo de una fila de arte: sin vincular, un objeto clave (comparte su fila en el Desglose) o una categoría del Desglose.
fn link_pick(
    ui: &mut Ui,
    id: u64,
    link: &mut Option<RoleLink>,
    objects: &[(u64, String)],
    categories: &[(u64, String)],
) -> bool {
    let mut changed = false;
    let shown = match *link {
        Some(RoleLink::Object(o)) => objects
            .iter()
            .find(|(i, _)| *i == o)
            .map_or("Sin vincular".to_string(), |(_, n)| format!("Objeto clave: {n}")),
        Some(RoleLink::Category(c)) => categories
            .iter()
            .find(|(i, _)| *i == c)
            .map_or("Sin vincular".to_string(), |(_, n)| format!("Categoría: {n}")),
        None => "Sin vincular".to_string(),
    };
    egui::ComboBox::from_id_salt(("role_link", id))
        .width(LINK_W)
        .truncate()
        .selected_text(shown)
        .show_ui(ui, |ui| {
            if ui.selectable_label(link.is_none(), "Sin vincular").clicked() {
                *link = None;
                changed = true;
            }
            if !objects.is_empty() {
                ui.label(RichText::new("Objetos clave").size(tokens::FONT_SMALL));
                for (o, name) in objects {
                    if ui.selectable_label(*link == Some(RoleLink::Object(*o)), name).clicked() {
                        *link = Some(RoleLink::Object(*o));
                        changed = true;
                    }
                }
            }
            if !categories.is_empty() {
                ui.label(RichText::new("Categorías del Desglose").size(tokens::FONT_SMALL));
                for (c, name) in categories {
                    if ui.selectable_label(*link == Some(RoleLink::Category(*c)), name).clicked() {
                        *link = Some(RoleLink::Category(*c));
                        changed = true;
                    }
                }
            }
        });
    changed
}
