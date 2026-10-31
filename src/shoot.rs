//! Locaciones de rodaje: los lugares reales donde se filma. A diferencia de las locaciones del guion («INT. LIVING»),
//! tienen dirección, permiso, electricidad y ruidos, y varias del guion pueden filmarse en el mismo lugar.

use eframe::egui::{self, RichText, Ui};

use crate::{
    icon,
    model::{PermitStatus, Project, ShootLocation, fmt_eighths},
    pages,
    theme::Palette,
    ui::{
        kit::{self, ColAlign, ColumnDef, TableRowKind},
        tokens,
    },
    widgets::{self, field},
};

#[derive(Default)]
pub struct ShootState {
    pub selected: Option<u64>,
}

#[derive(Default)]
pub struct Out {
    pub changed: bool,
    /// Abrir la ficha de este lugar como página (tipo E).
    pub open_page: Option<u64>,
    /// Pedido de eliminar un lugar: la app confirma.
    pub delete: Option<u64>,
}

/// Cuánta potencia (W) piden las luces de las plantas de las escenas que se filman en el lugar: la mayor de sus
/// plantas, porque no se filman todas a la vez.
fn lights_watts(project: &Project, place: u64) -> u32 {
    project
        .blockings
        .iter()
        .filter(|b| project.scene_shoot_location(b.scene) == Some(place))
        .map(|b| {
            b.items
                .iter()
                .filter(|i| i.kind == crate::model::BlockKind::Light)
                .map(|i| i.watts)
                .sum::<u32>()
        })
        .max()
        .unwrap_or(0)
}

/// Solo para las pruebas de otras pantallas.
#[cfg(test)]
pub fn lights_for_test(project: &Project, place: u64) -> u32 {
    lights_watts(project, place)
}

/// Cuántos amperes son `watts` a `volts`.
pub fn amperes(watts: u32, volts: f32) -> f32 {
    watts as f32 / volts
}

fn section(ui: &mut Ui, p: &Palette, text: &str) {
    kit::section(ui, p, text, None);
}

/// Herramientas de la barra: lugar nuevo y eliminar el elegido.
pub fn toolbar(ui: &mut Ui, state: &mut ShootState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    if kit::icon_button(ui, p, icon::PLUS, "Locación de rodaje nueva", true).clicked() {
        state.selected = Some(project.add_shoot_location(""));
        out.changed = true;
    }
    if kit::icon_button(
        ui,
        p,
        icon::TRASH,
        "Eliminar la locación de rodaje elegida",
        state.selected.is_some(),
    )
    .clicked()
    {
        out.delete = state.selected;
    }
    out
}

fn permit_color(p: &Palette, status: PermitStatus) -> egui::Color32 {
    match status {
        PermitStatus::Pending => p.container_hi,
        PermitStatus::Requested => p.warning,
        PermitStatus::Approved => p.success,
    }
}

/// La tabla de lugares de rodaje: foto, nombre, dirección, permiso, escenas y octavos.
pub fn show(ui: &mut Ui, state: &mut ShootState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    if state
        .selected
        .is_some_and(|id| !project.shoot_locations.iter().any(|l| l.id == id))
    {
        state.selected = None;
    }
    if project.shoot_locations.is_empty() {
        if kit::empty_state(
            ui,
            p,
            icon::HOUSE,
            "Todavía no hay locaciones de rodaje. Una locación de rodaje es el lugar real donde se filma: una casa, una calle, un estudio.",
            Some("Crear la primera"),
        ) {
            state.selected = Some(project.add_shoot_location(""));
            out.changed = true;
        }
        return out;
    }
    // Las columnas, declaradas una sola vez (reglas §2.6). Las filas llevan miniatura, así que miden `ROW_H_TALL`.
    const COLS: [ColumnDef; 6] = [
        ColumnDef::new("photo", "Foto", 56.0).align(ColAlign::Center),
        ColumnDef::new("name", "Nombre", 160.0).initial(200.0),
        ColumnDef::new("address", "Dirección", 200.0).rest(),
        ColumnDef::new("permit", "Permiso", 110.0),
        ColumnDef::new("scenes", "Escenas", 80.0).align(ColAlign::Right),
        ColumnDef::new("eighths", "Octavos", 80.0).align(ColAlign::Right),
    ];
    let rows: Vec<(u64, usize, u16)> = project
        .shoot_locations
        .iter()
        .map(|l| {
            let scenes = project.shoot_location_scenes(l.id);
            (l.id, scenes.len(), scenes.iter().map(|(_, _, e)| *e).sum())
        })
        .collect();
    let base = project.base_dir.clone();
    let kinds = vec![TableRowKind::Item; rows.len()];
    let selected = state
        .selected
        .and_then(|s| rows.iter().position(|(id, _, _)| *id == s));
    let table = kit::data_table_with(
        ui,
        p,
        "shoot_table",
        &COLS,
        true,
        tokens::ROW_H_TALL,
        true,
        &kinds,
        selected,
        |ui, n, col| {
            let (id, scenes, eighths) = rows[n];
            let Some(place) = project.shoot_locations.iter().find(|l| l.id == id) else {
                return;
            };
            match col.id {
                "photo" => {
                    if let Some(photo) = place.photos.first() {
                        let shown = match base.as_deref() {
                            Some(b) if photo.is_relative() => b.join(photo),
                            _ => photo.clone(),
                        };
                        let rect = egui::Rect::from_center_size(
                            ui.max_rect().center(),
                            egui::vec2(tokens::ROW_H_TALL - 8.0, tokens::ROW_H_TALL - 8.0),
                        );
                        pages::paint_cover(ui, &shown, rect, tokens::R_CONTROL);
                    } else {
                        ui.label(RichText::new(icon::IMAGE).color(p.dim));
                    }
                }
                "name" => {
                    kit::cell_text(ui, p, &place.name);
                }
                "address" => {
                    kit::cell_text(ui, p, &place.address);
                }
                "permit" => {
                    kit::chip(ui, p, place.permit.label(), permit_color(p, place.permit));
                }
                "scenes" => {
                    ui.label(scenes.to_string());
                }
                _ => {
                    ui.label(fmt_eighths(eighths));
                }
            }
        },
        |ui, n| {
            // Menú de la fila: eliminar la locación de rodaje.
            if ui.button("Eliminar locación de rodaje").clicked() {
                out.delete = Some(rows[n].0);
                ui.close();
            }
        },
    );
    if let Some(n) = table.clicked.or(table.double_clicked) {
        state.selected = Some(rows[n].0);
    }
    out
}

/// El inspector: pestaña FICHA con todos los datos del lugar elegido.
pub fn inspector(ui: &mut Ui, state: &mut ShootState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    (|ui: &mut Ui| {
        let Some(id) = state
            .selected
            .filter(|id| project.shoot_locations.iter().any(|l| l.id == *id))
        else {
            kit::empty_state(
                ui,
                p,
                icon::HOUSE,
                "Elegí una locación de la tabla para ver su ficha.",
                None,
            );
            return;
        };
        if ui
            .button(format!("{}  Abrir como página", icon::ARROW_SQUARE_OUT))
            .clicked()
        {
            out.open_page = Some(id);
        }
        ui.add_space(tokens::SP_S);
        ficha(ui, p, project, id, &mut out);
    })(ui);
    out
}

/// La ficha de un lugar abierta como página (tipo E): una columna centrada.
pub fn page(ui: &mut Ui, project: &mut Project, id: u64, p: &Palette) -> Out {
    let mut out = Out::default();
    egui::ScrollArea::vertical()
        .id_salt("shoot_page")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            widgets::column(ui, tokens::FICHA_MAX, |ui| {
                ficha(ui, p, project, id, &mut out);
                ui.add_space(tokens::SP_XL);
            });
        });
    out
}

/// Los datos de un lugar, en tarjetas por tema. La usan el inspector y la página.
fn ficha(ui: &mut Ui, p: &Palette, project: &mut Project, id: u64, out: &mut Out) {
    let contacts: Vec<(u64, String)> = project
        .contacts
        .iter()
        .map(|c| (c.id, c.name.clone()))
        .collect();
    let scenes = project.shoot_location_scenes(id);
    let base = project.base_dir.clone();
    let watts_needed = lights_watts(project, id);
    let Some(place) = project.shoot_locations.iter_mut().find(|l| l.id == id) else {
        return;
    };
    {
        {
            kit::card(ui, p, |ui| {
                out.changed |= field(ui, p, "Nombre", &mut place.name, 1);
                out.changed |= field(ui, p, "Dirección", &mut place.address, 2);
                {
                    let mut lat = place.latitude.unwrap_or(0.0);
                    let mut lon = place.longitude.unwrap_or(0.0);
                    let a = kit::drag_prop(
                        ui,
                        p,
                        "Latitud",
                        egui::DragValue::new(&mut lat)
                            .range(-90.0..=90.0)
                            .speed(0.01)
                            .max_decimals(5)
                            .suffix("°"),
                    );
                    let b = kit::drag_prop(
                        ui,
                        p,
                        "Longitud",
                        egui::DragValue::new(&mut lon)
                            .range(-180.0..=180.0)
                            .speed(0.01)
                            .max_decimals(5)
                            .suffix("°"),
                    );
                    if a || b {
                        (place.latitude, place.longitude) = (Some(lat), Some(lon));
                        out.changed = true;
                    }
                    if place.latitude.is_some() && ui.button("Quitar coordenadas").clicked() {
                        (place.latitude, place.longitude) = (None, None);
                        out.changed = true;
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Para la salida y la puesta del sol en la hoja de llamado.",
                            )
                            .size(tokens::FONT_SMALL)
                            .color(p.dim),
                        )
                        .wrap(),
                    );
                }
                let shown = place
                    .contact
                    .and_then(|c| contacts.iter().find(|(i, _)| *i == c))
                    .map_or("Sin contacto".to_string(), |(_, n)| n.clone());
                kit::combo_field(ui, p, "Contacto", "shoot_contact", &shown, |ui| {
                    if ui
                        .selectable_label(place.contact.is_none(), "Sin contacto")
                        .clicked()
                    {
                        place.contact = None;
                        out.changed = true;
                    }
                    for (cid, name) in &contacts {
                        if ui
                            .selectable_label(place.contact == Some(*cid), name)
                            .clicked()
                        {
                            place.contact = Some(*cid);
                            out.changed = true;
                        }
                    }
                });
                if contacts.is_empty() {
                    ui.label(
                        RichText::new(
                            "Cargá a la persona en la pantalla Contactos para elegirla acá.",
                        )
                        .size(12.0)
                        .color(p.dim),
                    );
                }
                ui.add_space(tokens::SP_S);
                out.changed |= field(
                    ui,
                    p,
                    "Horarios en los que se puede entrar",
                    &mut place.access_hours,
                    1,
                );
                kit::field_group(ui, 2, |ui, n| {
                    out.changed |= if n == 0 {
                        field(ui, p, "Estacionamiento", &mut place.parking, 2)
                    } else {
                        field(ui, p, "Baños", &mut place.bathrooms, 2)
                    };
                });
            });
            ui.add_space(tokens::SP_M);
            kit::card(ui, p, |ui| {
                section(ui, p, "PERMISO");
                let label = place.permit.label();
                kit::combo_field(ui, p, "Estado", "shoot_permit", label, |ui| {
                    for status in PermitStatus::ALL {
                        out.changed |= ui
                            .selectable_value(&mut place.permit, status, status.label())
                            .changed();
                    }
                });
                out.changed |= kit::format_field(ui, p, "Fecha", |ui| {
                    kit::date_field(ui, &mut place.permit_date)
                })
                .changed();
                let mut pesos = place.permit_cost_cents.unwrap_or(0) as f64 / 100.0;
                if kit::drag_prop(
                    ui,
                    p,
                    "Costo",
                    egui::DragValue::new(&mut pesos)
                        .range(0.0..=100_000_000.0)
                        .speed(50.0)
                        .prefix("$ "),
                ) {
                    place.permit_cost_cents = (pesos > 0.0).then(|| (pesos * 100.0).round() as i64);
                    out.changed = true;
                }
            });
            ui.add_space(tokens::SP_M);
            kit::card(ui, p, |ui| {
                section(ui, p, "ELECTRICIDAD");
                let mut watts = place.power_watts.unwrap_or(0);
                if kit::drag_prop(
                    ui,
                    p,
                    "Potencia máxima",
                    egui::DragValue::new(&mut watts)
                        .range(0..=1_000_000)
                        .speed(50.0)
                        .suffix(" W"),
                ) {
                    place.power_watts = (watts > 0).then_some(watts);
                    out.changed = true;
                }
                if let Some(w) = place.power_watts {
                    kit::kv_row(
                        ui,
                        p,
                        "Corriente a 220 V",
                        &format!("≈ {:.0} A", amperes(w, 220.0)),
                    );
                }
                out.changed |= kit::check_prop(ui, p, "Generador", &mut place.generator);
                out.changed |= field(ui, p, "Circuitos disponibles", &mut place.circuits, 2);
                if let (Some(limit), true) = (place.power_watts, watts_needed > 0) {
                    if watts_needed > limit {
                        ui.label(
                            RichText::new(format!(
                                "Las luces piden {watts_needed} W y el lugar da {limit} W."
                            ))
                            .color(p.accent),
                        );
                    }
                }
            });
            ui.add_space(tokens::SP_M);
            kit::card(ui, p, |ui| {
                section(ui, p, "SONIDO Y SEGURIDAD");
                out.changed |= field(
                    ui,
                    p,
                    "Ruidos del lugar (tránsito, aviones, heladera, eco…) y en qué horarios",
                    &mut place.noise_notes,
                    3,
                );
                out.changed |= field(
                    ui,
                    p,
                    "Hospital o guardia más cercana",
                    &mut place.hospital,
                    2,
                );
                out.changed |= field(ui, p, "Notas", &mut place.notes, 3);
            });
            ui.add_space(tokens::SP_M);
            kit::card(ui, p, |ui| {
                section(ui, p, "SCOUTING");
                photos(ui, p, place, base.as_deref(), out);
                ui.add_space(tokens::SP_M);
                ui.label(RichText::new("Plano del lugar").size(12.0).color(p.dim));
                ui.add_space(tokens::SP_XS);
                out.changed |= pages::image_picker(
                    ui,
                    p,
                    &mut place.floor_plan,
                    kit::fill_width(ui, 120.0, 360.0),
                    pages::LANDSCAPE,
                    base.as_deref(),
                );
                let mut meters = place.plan_width_m.unwrap_or(0.0);
                if kit::drag_prop(
                    ui,
                    p,
                    "Ancho del plano",
                    egui::DragValue::new(&mut meters)
                        .range(0.0..=1000.0)
                        .speed(0.1)
                        .suffix(" m"),
                ) {
                    place.plan_width_m = (meters > 0.0).then_some(meters);
                    out.changed = true;
                }
            });
            ui.add_space(tokens::SP_M);
            kit::card(ui, p, |ui| {
                section(ui, p, "ESCENAS QUE SE FILMAN ACÁ");
                if scenes.is_empty() {
                    ui.add(
                        egui::Label::new(RichText::new("Ninguna todavía.").size(12.0).color(p.dim))
                            .wrap(),
                    );
                }
                let total: u32 = scenes.iter().map(|(_, _, e)| u32::from(*e)).sum();
                for (number, heading, eighths) in &scenes {
                    ui.label(format!(
                        "{number}. {}  ·  {}",
                        heading.trim(),
                        fmt_eighths(*eighths)
                    ));
                }
                if !scenes.is_empty() {
                    ui.add_space(tokens::SP_XS);
                    ui.label(
                        RichText::new(format!(
                            "{} escenas · {} páginas",
                            scenes.len(),
                            fmt_eighths(total.min(u32::from(u16::MAX)) as u16)
                        ))
                        .color(p.dim),
                    );
                }
            });
        }
    }
}

/// Las fotos del lugar: miniaturas con una cruz para quitarlas y un botón para agregar más.
fn photos(
    ui: &mut Ui,
    p: &Palette,
    place: &mut ShootLocation,
    base: Option<&std::path::Path>,
    out: &mut Out,
) {
    ui.label(RichText::new("Fotos").size(12.0).color(p.dim));
    ui.add_space(tokens::SP_XS);
    let mut remove: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        for (n, photo) in place.photos.iter().enumerate() {
            let shown = match base {
                Some(b) if photo.is_relative() => b.join(photo),
                _ => photo.clone(),
            };
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(132.0, 99.0), egui::Sense::click());
            pages::paint_cover(ui, &shown, rect, 4);
            let cross = egui::Rect::from_min_size(
                rect.right_top() + egui::vec2(-22.0, 2.0),
                egui::vec2(20.0, 20.0),
            );
            ui.painter().rect_filled(cross, 10, p.shadow);
            ui.painter().text(
                cross.center(),
                egui::Align2::CENTER_CENTER,
                icon::X,
                egui::FontId::proportional(12.0),
                p.text,
            );
            if resp.clicked()
                && resp
                    .interact_pointer_pos()
                    .is_some_and(|pos| cross.contains(pos))
            {
                remove = Some(n);
            }
        }
    });
    if let Some(n) = remove {
        place.photos.remove(n);
        out.changed = true;
    }
    ui.add_space(tokens::SP_XS);
    if widgets::wide_button(ui, p, icon::IMAGES, "Agregar fotos", 180.0).clicked() {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("Imágenes", &["png", "jpg", "jpeg", "webp"])
            .pick_files()
        {
            place.photos.extend(files);
            out.changed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind::*;

    fn project() -> Project {
        let mut p = Project::default();
        p.paragraphs = [
            (SceneHeading, "INT. LIVING - NOCHE"),
            (Action, "Ana espera."),
            (SceneHeading, "INT. PASILLO - NOCHE"),
            (Action, "Camina."),
            (SceneHeading, "EXT. CALLE - DÍA"),
            (Action, "Llueve."),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p.auto_link_all();
        p
    }

    #[test]
    fn several_script_locations_can_be_filmed_in_the_same_real_place() {
        let mut p = project();
        let house = p.add_shoot_location("Casa de Pedro");
        let street = p.add_shoot_location("");
        assert_eq!(p.shoot_locations[1].name, "Locación de rodaje 2");
        let living = p.location_id_by_name("LIVING").unwrap();
        let hall = p.location_id_by_name("PASILLO").unwrap();
        for id in [living, hall] {
            p.location_mut(id).unwrap().shoot_location = Some(house);
        }
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        assert_eq!(p.scene_shoot_location(ids[0]), Some(house));
        assert_eq!(p.scene_shoot_location(ids[1]), Some(house));
        assert_eq!(p.scene_shoot_location(ids[2]), None);
        // Una escena puntual puede ir a otro lugar que el de su locación del guion.
        p.scenes.entry(ids[1]).or_default().shoot_location = Some(street);
        assert_eq!(p.scene_shoot_location(ids[1]), Some(street));
        let at_home = p.shoot_location_scenes(house);
        assert_eq!(at_home.len(), 1);
        assert_eq!(
            (at_home[0].0.as_str(), at_home[0].1.as_str()),
            ("1", "INT. LIVING - NOCHE")
        );
        // Quitar el lugar suelta a quien lo tenía.
        p.remove_shoot_location(house);
        assert_eq!(p.scene_shoot_location(ids[0]), None);
        assert_eq!(p.location_mut(living).unwrap().shoot_location, None);
        assert_eq!(p.scene_shoot_location(ids[1]), Some(street));
    }

    #[test]
    fn the_amperes_follow_the_voltage() {
        assert_eq!(amperes(2200, 220.0), 10.0);
        assert!((amperes(1000, 110.0) - 9.09).abs() < 0.01);
    }

    #[test]
    fn the_photos_and_the_plan_are_copied_into_the_project_folder_like_any_image() {
        let mut p = project();
        let id = p.add_shoot_location("Casa");
        let place = p.shoot_locations.iter_mut().find(|l| l.id == id).unwrap();
        place.photos = vec!["/tmp/a.jpg".into(), "/tmp/b.jpg".into()];
        place.floor_plan = Some("/tmp/plano.png".into());
        assert_eq!(p.image_paths_mut().len(), 3);
    }

    #[test]
    fn the_screen_draws_with_and_without_places() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let palette = crate::storage::Settings::default().scheme.palette();
        let mut project = project();
        let mut state = ShootState::default();
        for step in 0..2 {
            if step == 1 {
                let house = project.add_shoot_location("Casa de Pedro");
                project.add_contact("Pedro");
                project.shoot_locations[0].contact = Some(project.contacts[0].id);
                project.shoot_locations[0].power_watts = Some(3000);
                let _ = house;
                state.selected = project.shoot_locations.first().map(|l| l.id);
            }
            for n in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    time: Some(n as f64 * 0.1),
                    focused: true,
                    ..Default::default()
                };
                let mut o = ctx.run_ui(input, |ui| {
                    show(ui, &mut state, &mut project, &palette);
                    inspector(ui, &mut state, &mut project, &palette);
                    if let Some(id) = state.selected {
                        page(ui, &mut project, id, &palette);
                    }
                });
                o.textures_delta.clear();
            }
        }
        assert!(state.selected.is_some());
    }
}
