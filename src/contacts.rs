//! Contactos: la gente del proyecto (elenco, equipo, contactos de las locaciones). Viven en el proyecto y se pueden
//! guardar en la agenda de la app, que es compartida entre proyectos, o pasar a otra persona como vCard.

use std::collections::HashMap;

use eframe::egui::{self, RichText, Ui};

use crate::{
    icon,
    model::{Contact, Project, parse_date_ranges},
    storage,
    theme::{self, Palette},
    ui::{
        kit::{self, ColAlign, ColumnDef, TableRowKind},
        tokens,
    },
    vcard, widgets,
};

#[derive(Default)]
pub struct ContactsState {
    editing: HashMap<(u64, u8), String>,
    pub selected: Option<u64>,
    query: String,
    group: Option<Group>,
    /// La agenda de la app abierta para elegir de ahí.
    agenda: Option<Vec<Contact>>,
}

#[derive(Default)]
pub struct Out {
    pub changed: bool,
    /// Un mensaje breve para el aviso de abajo.
    pub status: Option<String>,
}

/// Guarda en un contacto lo escrito en una columna (0 nombre, 1 rol, 2 departamento, 3 teléfono, 4 mail, 5 fechas).
fn apply(project: &mut Project, id: u64, column: u8, text: &str) -> bool {
    let Some(c) = project.contacts.iter_mut().find(|c| c.id == id) else {
        return false;
    };
    let before = c.clone();
    let text = text.trim();
    match column {
        0 => c.name = text.to_string(),
        1 => c.role = text.to_string(),
        2 => c.department = text.to_string(),
        3 => c.phone = text.to_string(),
        4 => c.email = text.to_string(),
        _ => c.unavailable = parse_date_ranges(text),
    }
    *c != before
}

/// Guarda (o actualiza) el contacto en la agenda de la app. Devuelve su id de agenda.
fn save_to_agenda(contact: &Contact) -> u64 {
    let mut agenda = storage::load_agenda();
    let id = contact.agenda_id.unwrap_or_else(storage::new_agenda_id);
    let entry = Contact {
        id,
        agenda_id: Some(id),
        ..contact.clone()
    };
    match agenda.iter_mut().find(|c| c.id == id) {
        Some(existing) => *existing = entry,
        None => agenda.push(entry),
    }
    storage::save_agenda(&agenda);
    id
}

/// Copia de un contacto de la agenda dentro del proyecto.
fn adopt(project: &mut Project, from: &Contact) -> u64 {
    let id = project.add_contact(&from.name);
    if let Some(c) = project.contacts.iter_mut().find(|c| c.id == id) {
        *c = Contact {
            id,
            agenda_id: Some(from.id),
            ..from.clone()
        };
    }
    id
}

/// Lo que hace cada contacto en el proyecto, para filtrar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Cast,
    Crew,
    Places,
}

impl Group {
    const ALL: [Group; 3] = [Group::Cast, Group::Crew, Group::Places];

    fn label(self) -> &'static str {
        match self {
            Group::Cast => "Elenco",
            Group::Crew => "Equipo",
            Group::Places => "Locaciones",
        }
    }
}

fn is_cast(project: &Project, id: u64) -> bool {
    project.characters.iter().any(|c| c.actor == Some(id))
}

fn is_place_contact(project: &Project, id: u64) -> bool {
    project
        .shoot_locations
        .iter()
        .any(|l| l.contact == Some(id))
}

fn in_group(project: &Project, id: u64, group: Group) -> bool {
    match group {
        Group::Cast => is_cast(project, id),
        Group::Places => is_place_contact(project, id),
        Group::Crew => !is_cast(project, id) && !is_place_contact(project, id),
    }
}

/// Herramientas de la barra: contacto nuevo, agenda de la app y vCard.
pub fn toolbar(ui: &mut Ui, state: &mut ContactsState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    kit::toolbar_groups(ui, "contacts_tools", &[3, 2, 1], |ui, group| match group {
        0 => {
            if ui
                .button(format!("{}  Contacto", icon::PLUS))
                .on_hover_text("Contacto nuevo")
                .clicked()
            {
                state.selected = Some(project.add_contact(""));
                out.changed = true;
            }
        }
        1 => {
            ui.menu_button(format!("{}  Agenda", icon::FOLDER_OPEN), |ui| {
                if ui
                    .add_enabled(
                        state.selected.is_some(),
                        egui::Button::new("Guardar en la agenda"),
                    )
                    .clicked()
                {
                    if let Some(c) = state
                        .selected
                        .and_then(|id| project.contacts.iter().find(|c| c.id == id))
                        .cloned()
                    {
                        let agenda_id = save_to_agenda(&c);
                        if let Some(mine) = project.contacts.iter_mut().find(|m| m.id == c.id) {
                            mine.agenda_id = Some(agenda_id);
                        }
                        out.changed = true;
                        out.status = Some("Guardado en la agenda".to_string());
                    }
                    ui.close();
                }
                if ui.button("Traer de la agenda").clicked() {
                    state.agenda = Some(storage::load_agenda());
                    ui.close();
                }
            });
        }
        _ => {
            ui.menu_button(format!("{}  vCard", icon::EXPORT), |ui| {
                if ui.button("Importar…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("vCard", &["vcf"])
                        .pick_file()
                    {
                        match std::fs::read_to_string(&path) {
                            Ok(text) => {
                                let found = vcard::parse(&text);
                                for c in &found {
                                    adopt(
                                        project,
                                        &Contact {
                                            agenda_id: None,
                                            ..c.clone()
                                        },
                                    );
                                }
                                out.changed |= !found.is_empty();
                                out.status = Some(format!("{} contactos importados", found.len()));
                            }
                            Err(e) => out.status = Some(format!("No se pudo leer el archivo: {e}")),
                        }
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !project.contacts.is_empty(),
                        egui::Button::new("Exportar todos…"),
                    )
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("vCard", &["vcf"])
                        .set_file_name("contactos.vcf")
                        .save_file()
                    {
                        out.status = Some(
                            match std::fs::write(&path, vcard::export(&project.contacts)) {
                                Ok(()) => "Contactos exportados".to_string(),
                                Err(e) => format!("Error al exportar: {e}"),
                            },
                        );
                    }
                    ui.close();
                }
            });
        }
    });
    let _ = p;
    out
}

/// Barra de contexto: buscar y filtrar por Elenco, Equipo o Locaciones.
pub fn context_bar(ui: &mut Ui, state: &mut ContactsState, p: &Palette) {
    crate::ui::shell::context_bar(ui, p, |ui| {
        let width = kit::fill_width(ui, 100.0, 220.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .desired_width(width)
                .hint_text(format!("{} Buscar", icon::MAGNIFYING_GLASS)),
        );
        ui.separator();
        let mut labels = vec!["Todos".to_string()];
        labels.extend(Group::ALL.iter().map(|g| g.label().to_string()));
        let active = state.group.map_or(0, |g| {
            Group::ALL.iter().position(|x| *x == g).map_or(0, |i| i + 1)
        });
        if let Some(i) = kit::dropdown(ui, "contacts_group", &labels, active) {
            state.group = (i > 0).then(|| Group::ALL[i - 1]);
        }
    });
}

/// El inspector: pestaña CONTACTO con los campos del contacto elegido y sus vínculos.
pub fn inspector(
    ui: &mut Ui,
    state: &mut ContactsState,
    project: &mut Project,
    p: &Palette,
) -> Out {
    let mut out = Out::default();
    (|ui: &mut Ui| {
        let Some(id) = state
            .selected
            .filter(|id| project.contacts.iter().any(|c| c.id == *id))
        else {
            kit::empty_state(
                ui,
                p,
                icon::USERS_THREE,
                "Elegí un contacto de la tabla para ver sus datos.",
                None,
            );
            return;
        };
        let c = project
            .contacts
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .unwrap_or_default();
        let ranges = c
            .unavailable
            .iter()
            .map(|r| r.label())
            .collect::<Vec<_>>()
            .join("; ");
        let mut edits: Vec<(u8, String)> = Vec::new();
        kit::section(ui, p, "Datos", None);
        for (column, label, shown, hint) in [
            (0u8, "Nombre", c.name.as_str(), "Nombre"),
            (1, "Rol", c.role.as_str(), "Rol"),
            (2, "Departamento", c.department.as_str(), "Área"),
            (3, "Teléfono", c.phone.as_str(), "Teléfono"),
            (4, "Mail", c.email.as_str(), "Mail"),
        ] {
            if let Some(text) = kit::draft_field(
                ui,
                p,
                &mut state.editing,
                (id, column),
                label,
                shown,
                hint,
                1,
            ) {
                edits.push((column, text));
            }
        }
        kit::section(ui, p, "No disponible", None);
        if let Some(text) = kit::draft_field(
            ui,
            p,
            &mut state.editing,
            (id, 5),
            "Fechas (AAAA-MM-DD a AAAA-MM-DD; …)",
            &ranges,
            "—",
            3,
        ) {
            edits.push((5, text));
        }
        kit::section(ui, p, "Vínculos", None);
        let plays: Vec<String> = project
            .characters
            .iter()
            .filter(|ch| ch.actor == Some(id))
            .map(|ch| crate::model::clean_character_name(&ch.name))
            .collect();
        let places: Vec<String> = project
            .shoot_locations
            .iter()
            .filter(|l| l.contact == Some(id))
            .map(|l| l.name.clone())
            .collect();
        let linked = |list: &[String]| {
            if list.is_empty() {
                "—".to_string()
            } else {
                list.join(", ")
            }
        };
        kit::prop_row(ui, p, "Interpreta a", |ui| {
            ui.add(egui::Label::new(RichText::new(linked(&plays)).color(p.dim)).wrap());
        });
        kit::prop_row(ui, p, "Contacto de", |ui| {
            ui.add(egui::Label::new(RichText::new(linked(&places)).color(p.dim)).wrap());
        });
        ui.add_space(tokens::SP_M);
        ui.horizontal_wrapped(|ui| {
            let tip = if c.agenda_id.is_some() {
                "Actualizar en la agenda"
            } else {
                "Guardar en la agenda"
            };
            if ui.button(format!("{}  {tip}", icon::FLOPPY_DISK)).clicked() {
                let agenda_id = save_to_agenda(&c);
                if let Some(mine) = project.contacts.iter_mut().find(|m| m.id == id) {
                    mine.agenda_id = Some(agenda_id);
                }
                out.changed = true;
                out.status = Some("Guardado en la agenda".to_string());
            }
            if ui
                .button(
                    RichText::new(format!("{}  Quitar del proyecto", icon::TRASH)).color(p.danger),
                )
                .clicked()
            {
                project.remove_contact(id);
                state.selected = None;
                out.changed = true;
            }
        });
        for (column, text) in edits {
            out.changed |= apply(project, id, column, &text);
        }
    })(ui);
    out
}

pub fn show(ui: &mut Ui, state: &mut ContactsState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    if state
        .selected
        .is_some_and(|id| !project.contacts.iter().any(|c| c.id == id))
    {
        state.selected = None;
    }
    if project.contacts.is_empty() {
        if kit::empty_state(
            ui,
            p,
            icon::USERS_THREE,
            "Todavía no hay contactos. Agregá al elenco y al equipo, o traelos de la agenda de la app.",
            Some("Crear el primero"),
        ) {
            state.selected = Some(project.add_contact(""));
            out.changed = true;
        }
    } else {
        let query = state.query.trim().to_lowercase();
        let ids: Vec<u64> = project
            .contacts
            .iter()
            .filter(|c| state.group.is_none_or(|g| in_group(project, c.id, g)))
            .filter(|c| {
                query.is_empty()
                    || [&c.name, &c.role, &c.department, &c.phone, &c.email]
                        .iter()
                        .any(|f| f.to_lowercase().contains(&query))
            })
            .map(|c| c.id)
            .collect();
        if ids.is_empty() {
            kit::empty_state(
                ui,
                p,
                icon::USERS_THREE,
                "Ningún contacto coincide con la búsqueda o el filtro.",
                None,
            );
        } else {
            // Las columnas, declaradas una sola vez (reglas §2.6).
            const COLS: [ColumnDef; 6] = [
                ColumnDef::new("name", "Nombre", 160.0).initial(200.0),
                ColumnDef::new("role", "Rol", 120.0).initial(150.0),
                ColumnDef::new("department", "Departamento", 120.0).initial(150.0),
                ColumnDef::new("phone", "Teléfono", 120.0).initial(140.0),
                ColumnDef::new("email", "Mail", 160.0).rest(),
                ColumnDef::new("agenda", "En la agenda", 110.0).align(ColAlign::Center),
            ];
            let rows = vec![TableRowKind::Item; ids.len()];
            let selected = state
                .selected
                .and_then(|s| ids.iter().position(|id| *id == s));
            let out_table = kit::data_table(
                ui,
                p,
                "contacts_table",
                &COLS,
                true,
                &rows,
                selected,
                |ui, n, col| {
                    let Some(c) = project.contacts.iter().find(|c| c.id == ids[n]) else {
                        return;
                    };
                    match col.id {
                        "name" => {
                            kit::cell_text(ui, p, &c.name);
                        }
                        "role" => {
                            kit::cell_text(ui, p, &c.role);
                        }
                        "department" => {
                            kit::cell_text(ui, p, &c.department);
                        }
                        "phone" => {
                            kit::cell_text(ui, p, &c.phone);
                        }
                        "email" => {
                            kit::cell_text(ui, p, &c.email);
                        }
                        _ => {
                            ui.label(
                                RichText::new(if c.agenda_id.is_some() { "Sí" } else { "No" })
                                    .color(p.dim),
                            );
                        }
                    }
                },
            );
            if let Some(n) = out_table.clicked.or(out_table.double_clicked) {
                state.selected = Some(ids[n]);
            }
        }
    }

    // La agenda de la app: se agrega al proyecto una copia del contacto elegido.
    if state.agenda.is_some() {
        let mut close = false;
        let mut pick: Option<Contact> = None;
        let modal = egui::Modal::new(egui::Id::unique("contacts_agenda")).show(ui.ctx(), |ui| {
            ui.set_max_width(tokens::DIALOG_MAX);
            ui.label(
                RichText::new("Agenda de la app")
                    .font(egui::FontId::new(19.0, theme::bold_family())),
            );
            ui.add_space(tokens::SP_S);
            let present: Vec<u64> = project
                .contacts
                .iter()
                .filter_map(|c| c.agenda_id)
                .collect();
            let agenda = state.agenda.as_ref().map_or(&[][..], |a| a.as_slice());
            let pending: Vec<&Contact> =
                agenda.iter().filter(|c| !present.contains(&c.id)).collect();
            if pending.is_empty() {
                ui.label(
                    RichText::new(if agenda.is_empty() {
                        "La agenda está vacía: guardá contactos con el disquete."
                    } else {
                        "Todos los contactos de la agenda ya están en el proyecto."
                    })
                    .color(p.dim),
                );
            }
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for c in pending {
                        ui.horizontal(|ui| {
                            if widgets::icon_button(ui, p, icon::PLUS, "Agregar al proyecto", true)
                                .clicked()
                            {
                                pick = Some(c.clone());
                            }
                            ui.label(RichText::new(&c.name).color(p.text));
                            ui.label(
                                RichText::new(format!("{} {}", c.role, c.phone))
                                    .size(12.0)
                                    .color(p.dim),
                            );
                        });
                    }
                });
            ui.add_space(tokens::SP_M);
            if ui.button("Cerrar").clicked() {
                close = true;
            }
        });
        if let Some(c) = pick {
            adopt(project, &c);
            out.changed = true;
        }
        if close || modal.should_close() {
            state.agenda = None;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_in_a_cell_reaches_the_contact() {
        let mut p = Project::default();
        let id = p.add_contact("Ana");
        assert!(apply(&mut p, id, 1, "  Directora "));
        assert!(apply(&mut p, id, 3, "11 5555"));
        assert!(apply(
            &mut p,
            id,
            5,
            "2026-11-05 a 2026-11-03; 2026-11-10; mañana"
        ));
        let c = &p.contacts[0];
        assert_eq!(
            (c.role.as_str(), c.phone.as_str()),
            ("Directora", "11 5555")
        );
        // Los rangos se ordenan, las fechas sueltas valen un día y lo que no es fecha se ignora.
        assert_eq!(
            c.unavailable.iter().map(|r| r.label()).collect::<Vec<_>>(),
            ["2026-11-03 a 2026-11-05", "2026-11-10"]
        );
        assert!(
            c.unavailable[0].contains("2026-11-04") && !c.unavailable[0].contains("2026-11-06")
        );
        assert!(
            !apply(&mut p, id, 1, "Directora"),
            "lo mismo no cambia nada"
        );
        assert!(apply(&mut p, id, 5, ""));
        assert!(p.contacts[0].unavailable.is_empty());
    }

    #[test]
    fn removing_a_contact_frees_whatever_pointed_to_it() {
        let mut p = Project::default();
        let actor = p.add_contact("Luis");
        let place = p.add_shoot_location("Casa de Pedro");
        let ana = p.new_character("ANA");
        p.characters[0].actor = Some(actor);
        p.shoot_locations[0].contact = Some(actor);
        let _ = (place, ana);
        p.remove_contact(actor);
        assert_eq!(
            (p.characters[0].actor, p.shoot_locations[0].contact),
            (None, None)
        );
    }

    #[test]
    fn adopting_from_the_agenda_keeps_the_agenda_id_and_the_data() {
        let mut p = Project::default();
        let from = Contact {
            id: 99,
            agenda_id: Some(99),
            name: "Marta".into(),
            phone: "11 4444".into(),
            ..Default::default()
        };
        let id = adopt(&mut p, &from);
        let c = p.contacts.iter().find(|c| c.id == id).unwrap();
        assert_eq!(
            (c.agenda_id, c.name.as_str(), c.phone.as_str()),
            (Some(99), "Marta", "11 4444")
        );
        assert_ne!(c.id, 99, "el contacto del proyecto tiene su propio id");
    }
}
