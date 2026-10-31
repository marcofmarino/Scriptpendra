//! Escenas: una tabla con todas las escenas del guion y los datos de producción que el texto no tiene (octavos de
//! página, día de la historia, resumen, tiempo estimado, notas). Lo que se puede leer del guion se calcula solo.

use std::collections::HashMap;

use eframe::egui::{self, RichText, Ui};

use crate::{
    model::{Project, fmt_eighths, heading_parts, parse_eighths},
    theme::{self, Palette},
    ui::{
        kit::{self, ColAlign, ColumnDef, TableRowKind},
        tokens,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Number,
    Kind,
    Time,
    Day,
    Eighths,
}

#[derive(Default)]
pub struct ScenesState {
    sort: Option<(Sort, bool)>,
    /// La escena elegida: su ficha completa está en el inspector.
    pub selected: Option<u64>,
    query: String,
    /// Filtros de la barra de contexto: INT./EXT. y momento del día (vacío: todos).
    kind_filter: String,
    time_filter: String,
    /// Lo que se está escribiendo en una casilla: (escena, columna) y su texto.
    editing: HashMap<(u64, u8), String>,
    /// Configuración Completa de la tabla (por defecto, la Compacta).
    full: bool,
}

#[derive(Default)]
pub struct Out {
    pub changed: bool,
    /// Ir a esta escena en el guion.
    pub goto: Option<u64>,
    /// Ir a otra pantalla con esta escena: (destino, escena).
    pub link: Option<(SceneLink, u64)>,
    /// Mensaje breve para la barra de estado.
    pub status: Option<String>,
}

/// A dónde llevan los enlaces de la ficha de una escena.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SceneLink {
    Blocking,
    Breakdown,
    Shots,
}

struct Row {
    id: u64,
    number: String,
    kind: String,
    time: String,
    heading: String,
    eighths: u16,
    cast: Vec<String>,
}

/// Guarda en la ficha de la escena lo escrito en una columna (0 octavos, 1 día, 2 resumen, 3 minutos, 4 notas).
/// Devuelve si algo cambió.
fn apply(project: &mut Project, id: u64, column: u8, text: &str) -> bool {
    let current = project.scenes.get(&id).cloned().unwrap_or_default();
    let mut meta = current.clone();
    match column {
        0 => meta.eighths = parse_eighths(text),
        1 => meta.story_day = text.trim().parse::<u32>().ok().filter(|d| *d > 0),
        2 => meta.summary = text.trim().to_string(),
        3 => meta.est_minutes = text.trim().parse::<u32>().ok().filter(|m| *m > 0),
        _ => meta.notes = text.trim().to_string(),
    }
    if meta == current {
        return false;
    }
    project.scenes.insert(id, meta);
    true
}

/// Herramientas de la barra: bloquear la numeración y exportar a CSV.
pub fn toolbar(ui: &mut Ui, project: &mut Project, _p: &Palette) -> Out {
    let mut out = Out::default();
    kit::toolbar_groups(ui, "scenes_tools", &[2, 1], |ui, group| match group {
        0 => {
            if project.scene_numbers_locked {
                let button =
                    egui::Button::new(format!("{}  Numeración bloqueada", crate::icon::KEY));
                if ui
                    .add(button)
                    .on_hover_text("La numeración se conserva aunque se agreguen o borren escenas")
                    .clicked()
                {
                    project.unlock_scene_numbers();
                    out.changed = true;
                }
            } else if ui
                .button(format!("{}  Bloquear numeración", crate::icon::KEY))
                .on_hover_text("Cada escena conserva su número")
                .clicked()
            {
                project.lock_scene_numbers();
                out.changed = true;
            }
        }
        _ => {
            if kit::export_button(ui, "Exportar las escenas a CSV (planilla)").clicked()
            {
                out.status = export_csv(project);
            }
        }
    });
    out
}

/// Las escenas como CSV: número, encabezado, octavos, día, resumen, locación de rodaje, elenco, minutos y notas.
fn export_csv(project: &Project) -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name(format!("{} - escenas.csv", project.title))
        .save_file()?;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let labels: HashMap<u64, String> = project.scene_labels().into_iter().collect();
    let mut csv =
        String::from("Nº,Encabezado,Octavos,Día,Resumen,Se filma en,Personajes,Min,Notas\n");
    for (id, heading) in project.script_scenes() {
        let meta = project.scenes.get(&id).cloned().unwrap_or_default();
        let place = project
            .scene_shoot_location(id)
            .and_then(|l| project.shoot_locations.iter().find(|s| s.id == l))
            .map_or(String::new(), |l| l.name.clone());
        let fields = [
            labels.get(&id).cloned().unwrap_or_default(),
            heading.trim().to_string(),
            fmt_eighths(project.scene_eighths(id)),
            meta.story_day.map_or(String::new(), |d| d.to_string()),
            meta.summary,
            place,
            project.scene_cast(id).join(", "),
            meta.est_minutes.map_or(String::new(), |m| m.to_string()),
            meta.notes,
        ];
        csv.push_str(
            &fields
                .iter()
                .map(|f| quote(f))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }
    Some(match std::fs::write(&path, csv) {
        Ok(()) => "Escenas exportadas".to_string(),
        Err(e) => format!("Error al exportar: {e}"),
    })
}

/// Barra de contexto: buscar y filtrar por INT./EXT. y por momento del día.
pub fn context_bar(ui: &mut Ui, state: &mut ScenesState, project: &Project, p: &Palette) {
    let mut kinds: Vec<String> = vec![String::new()];
    let mut times: Vec<String> = vec![String::new()];
    for (_, heading) in project.script_scenes() {
        let (kind, _, time) = heading_parts(&heading);
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        if !times.contains(&time) {
            times.push(time);
        }
    }
    crate::ui::shell::context_bar(ui, p, |ui| {
        let width = kit::fill_width(ui, 100.0, 220.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .desired_width(width)
                .hint_text(format!("{} Buscar", crate::icon::MAGNIFYING_GLASS)),
        );
        ui.separator();
        // Las columnas de la tabla: las principales o todas.
        if let Some(i) = kit::segmented(ui, p, &["Compacta", "Completa"], usize::from(state.full)) {
            state.full = i == 1;
        }
        ui.separator();
        for (id, filter, options, all) in [
            ("scene_kind", &mut state.kind_filter, &kinds, "INT./EXT."),
            ("scene_time", &mut state.time_filter, &times, "Momento"),
        ] {
            let labels: Vec<String> = options
                .iter()
                .map(|o| {
                    if o.is_empty() {
                        all.to_string()
                    } else {
                        o.clone()
                    }
                })
                .collect();
            let active = options.iter().position(|o| o == filter).unwrap_or(0);
            if let Some(i) = kit::dropdown(ui, id, &labels, active) {
                *filter = options[i].clone();
            }
        }
    });
}

/// El inspector: pestaña ESCENA con la ficha completa de la escena elegida y los enlaces a sus otras pantallas.
pub fn inspector(ui: &mut Ui, state: &mut ScenesState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    (|ui: &mut Ui| {
        let Some((id, heading)) = state
            .selected
            .and_then(|id| project.script_scenes().into_iter().find(|(s, _)| *s == id))
        else {
            kit::empty_state(
                ui,
                p,
                crate::icon::LIST_NUMBERS,
                "Elegí una escena de la tabla para ver su ficha.",
                None,
            );
            return;
        };
        card(ui, &mut state.editing, project, p, id, &heading, &mut out);
    })(ui);
    out
}

/// La ficha de producción de una escena: octavos, día, tiempo, resumen, notas, lugar de rodaje, elenco y enlaces. La
/// usan el inspector de Escenas y la pestaña FICHA del Guion.
pub fn card(
    ui: &mut Ui,
    editing: &mut HashMap<(u64, u8), String>,
    project: &mut Project,
    p: &Palette,
    id: u64,
    heading: &str,
    out: &mut Out,
) {
    let places: Vec<(u64, String)> = project
        .shoot_locations
        .iter()
        .map(|l| (l.id, l.name.clone()))
        .collect();
    let number = project
        .scene_labels()
        .into_iter()
        .find(|(s, _)| *s == id)
        .map(|(_, n)| n)
        .unwrap_or_default();
    ui.add(
        egui::Label::new(
            RichText::new(format!("{number}. {}", heading.trim()))
                .font(egui::FontId::new(tokens::FONT_BODY, theme::bold_family()))
                .color(p.text),
        )
        .wrap(),
    );
    ui.add_space(tokens::SP_S);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Abrir en el guion").clicked() {
            out.goto = Some(id);
        }
        for (label, link) in [
            ("Planta", SceneLink::Blocking),
            ("Desglose", SceneLink::Breakdown),
            ("Planos", SceneLink::Shots),
        ] {
            if ui.button(label).clicked() {
                out.link = Some((link, id));
            }
        }
    });
    let meta = project.scenes.get(&id).cloned().unwrap_or_default();
    let hint_day = project
        .story_day_hint(id)
        .map_or("—".to_string(), |d| d.to_string());
    let hint_summary = project.scene_summary_hint(id);
    let eighths = if meta.eighths.is_some() {
        fmt_eighths(project.scene_eighths(id))
    } else {
        String::new()
    };
    let mut edits: Vec<(u8, String)> = Vec::new();
    let mut cell = |ui: &mut Ui, label: &str, column: u8, shown: &str, hint: &str, rows: usize| {
        if let Some(text) = kit::draft_field(ui, p, editing, (id, column), label, shown, hint, rows)
        {
            edits.push((column, text));
        }
    };
    kit::section(ui, p, "Producción", None);
    cell(
        ui,
        "Octavos",
        0,
        &eighths,
        &fmt_eighths(project.scene_eighths(id)),
        1,
    );
    cell(
        ui,
        "Día de la historia",
        1,
        &meta.story_day.map_or(String::new(), |d| d.to_string()),
        &hint_day,
        1,
    );
    cell(
        ui,
        "Tiempo estimado (min)",
        3,
        &meta.est_minutes.map_or(String::new(), |m| m.to_string()),
        "—",
        1,
    );
    kit::section(ui, p, "Resumen", None);
    cell(ui, "Resumen", 2, &meta.summary, &hint_summary, 3);
    cell(ui, "Notas", 4, &meta.notes, "", 3);
    kit::section(ui, p, "Rodaje", None);
    let own = meta.shoot_location;
    let inherited = project.scene_shoot_location(id).filter(|_| own.is_none());
    let name_of = |l: Option<u64>| {
        l.and_then(|i| places.iter().find(|(p, _)| *p == i))
            .map(|(_, n)| n.clone())
    };
    let shown = match (own, inherited) {
        (Some(_), _) => name_of(own).unwrap_or_default(),
        (None, Some(_)) => format!("{} (del guion)", name_of(inherited).unwrap_or_default()),
        _ => "—".to_string(),
    };
    let mut chosen: Option<Option<u64>> = None;
    kit::combo_field(
        ui,
        p,
        "Se filma en",
        ("scene_inspector_place", id),
        &shown,
        |ui| {
            if ui
                .selectable_label(own.is_none(), "Según la locación del guion")
                .clicked()
            {
                chosen = Some(None);
            }
            for (pid, name) in &places {
                if ui.selectable_label(own == Some(*pid), name).clicked() {
                    chosen = Some(Some(*pid));
                }
            }
        },
    );
    if let Some(place) = chosen {
        project.scenes.entry(id).or_default().shoot_location = place;
        out.changed = true;
    }
    kit::section(ui, p, "Elenco", None);
    let cast = project.scene_cast(id).join(", ");
    ui.add(
        egui::Label::new(
            RichText::new(if cast.is_empty() {
                "—".to_string()
            } else {
                cast
            })
            .color(p.dim),
        )
        .wrap(),
    );
    for (column, text) in edits {
        out.changed |= apply(project, id, column, &text);
    }
}

pub fn show(ui: &mut Ui, state: &mut ScenesState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    let labels: HashMap<u64, String> = project.scene_labels().into_iter().collect();
    let mut rows: Vec<Row> = project
        .script_scenes()
        .into_iter()
        .map(|(id, text)| {
            let (kind, _, time) = heading_parts(&text);
            Row {
                id,
                number: labels.get(&id).cloned().unwrap_or_default(),
                kind,
                time,
                heading: text,
                eighths: project.scene_eighths(id),
                cast: project.scene_cast(id),
            }
        })
        .collect();
    let query = state.query.trim().to_lowercase();
    rows.retain(|r| {
        (state.kind_filter.is_empty() || r.kind == state.kind_filter)
            && (state.time_filter.is_empty() || r.time == state.time_filter)
            && (query.is_empty()
                || r.heading.to_lowercase().contains(&query)
                || project
                    .scenes
                    .get(&r.id)
                    .is_some_and(|m| m.summary.to_lowercase().contains(&query)))
    });
    let places: Vec<(u64, String)> = project
        .shoot_locations
        .iter()
        .map(|l| (l.id, l.name.clone()))
        .collect();
    let day_of = |project: &Project, id: u64| project.scenes.get(&id).and_then(|m| m.story_day);
    if let Some((sort, ascending)) = state.sort {
        match sort {
            Sort::Number => {}
            Sort::Kind => rows.sort_by(|a, b| a.kind.cmp(&b.kind)),
            Sort::Time => rows.sort_by(|a, b| a.time.cmp(&b.time)),
            Sort::Day => rows.sort_by_key(|r| day_of(project, r.id).unwrap_or(u32::MAX)),
            Sort::Eighths => rows.sort_by_key(|r| r.eighths),
        }
        if !ascending {
            rows.reverse();
        }
    }

    let total: u32 = rows.iter().map(|r| u32::from(r.eighths)).sum();
    ui.label(
        RichText::new(format!(
            "{} escenas · {} páginas",
            rows.len(),
            fmt_eighths(total.min(u32::from(u16::MAX)) as u16)
        ))
        .size(tokens::FONT_SMALL)
        .color(p.dim),
    );
    ui.add_space(tokens::SP_S);
    if rows.is_empty() {
        let message = if project.script_scenes().is_empty() {
            "Todavía no hay escenas. Escribí un encabezado en el Guion y acá aparece su ficha de producción."
        } else {
            "Ninguna escena coincide con la búsqueda o los filtros."
        };
        kit::empty_state(ui, p, crate::icon::LIST_NUMBERS, message, None);
        return out;
    }

    // Las columnas, declaradas una sola vez (reglas §2.6). La Compacta es la de por defecto.
    const COLS: [(ColumnDef, Option<Sort>); 12] = [
        (
            ColumnDef::new("number", "Nº", 56.0)
                .align(ColAlign::Center)
                .sortable(),
            Some(Sort::Number),
        ),
        (
            ColumnDef::new("kind", "INT/EXT", 80.0).sortable(),
            Some(Sort::Kind),
        ),
        (
            ColumnDef::new("time", "Momento", 96.0).sortable(),
            Some(Sort::Time),
        ),
        (ColumnDef::new("heading", "Encabezado", 220.0).rest(), None),
        (ColumnDef::new("shoot_day", "Rodaje", 110.0), None),
        (ColumnDef::new("place", "Se filma en", 150.0), None),
        (
            ColumnDef::new("eighths", "Octavos", 80.0)
                .align(ColAlign::Right)
                .sortable(),
            Some(Sort::Eighths),
        ),
        (
            ColumnDef::new("story_day", "Día", 64.0)
                .align(ColAlign::Right)
                .sortable(),
            Some(Sort::Day),
        ),
        (
            ColumnDef::new("summary", "Resumen", 200.0).full_only(),
            None,
        ),
        (
            ColumnDef::new("cast", "Personajes", 160.0).full_only(),
            None,
        ),
        (
            ColumnDef::new("minutes", "Min", 64.0)
                .align(ColAlign::Right)
                .full_only(),
            None,
        ),
        (ColumnDef::new("notes", "Notas", 200.0).full_only(), None),
    ];
    // El orden elegido se marca en el título de su columna.
    let defs: Vec<ColumnDef> = COLS
        .iter()
        .map(|(c, sort)| {
            let mut def = *c;
            if let (Some((now, ascending)), Some(this)) = (state.sort, sort)
                && now == *this
            {
                def.title = crate::techscript::interned(&format!(
                    "{} {}",
                    c.title,
                    if ascending { "▲" } else { "▼" }
                ));
            }
            def
        })
        .collect();
    let kinds = vec![TableRowKind::Item; rows.len()];
    let selected = state
        .selected
        .and_then(|s| rows.iter().position(|r| r.id == s));
    let mut menu_goto: Option<u64> = None;
    let table = kit::data_table_with(
        ui,
        p,
        "scenes_table",
        &defs,
        !state.full,
        tokens::ROW_H,
        true,
        &kinds,
        selected,
        |ui, n, col| {
            let row = &rows[n];
            let meta = project.scenes.get(&row.id).cloned().unwrap_or_default();
            match col.id {
                "number" => {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&row.number)
                                .font(egui::FontId::new(tokens::FONT_BODY, theme::bold_family()))
                                .color(p.accent),
                        )
                        .truncate(),
                    );
                }
                "kind" => {
                    kit::cell_text(ui, p, &row.kind);
                }
                "time" => {
                    kit::cell_text(ui, p, &row.time);
                }
                "heading" => {
                    kit::cell_text(ui, p, row.heading.trim());
                }
                "shoot_day" => {
                    let text = project
                        .scene_day(row.id)
                        .map(|d| {
                            let date = project
                                .shoot_days
                                .iter()
                                .find(|s| s.id == d.id)
                                .map(|s| s.date.clone())
                                .unwrap_or_default();
                            if date.is_empty() {
                                format!("J{}", d.number)
                            } else {
                                format!("J{} · {date}", d.number)
                            }
                        })
                        .unwrap_or_default();
                    kit::cell_text(ui, p, &text);
                }
                "place" => {
                    let own = meta.shoot_location;
                    let inherited = project
                        .scene_shoot_location(row.id)
                        .filter(|_| own.is_none());
                    let name_of = |id: Option<u64>| {
                        id.and_then(|i| places.iter().find(|(p, _)| *p == i))
                            .map(|(_, n)| n.clone())
                    };
                    let text = match (own, inherited) {
                        (Some(_), _) => name_of(own).unwrap_or_default(),
                        (None, Some(_)) => {
                            format!("{} (del guion)", name_of(inherited).unwrap_or_default())
                        }
                        _ => String::new(),
                    };
                    kit::cell_text(ui, p, &text);
                }
                "eighths" => {
                    kit::cell_text(ui, p, &fmt_eighths(row.eighths));
                }
                "story_day" => {
                    let text = meta.story_day.map_or_else(
                        || {
                            project
                                .story_day_hint(row.id)
                                .map_or(String::new(), |d| d.to_string())
                        },
                        |d| d.to_string(),
                    );
                    kit::cell_text(ui, p, &text);
                }
                "summary" => {
                    let text = if meta.summary.is_empty() {
                        project.scene_summary_hint(row.id)
                    } else {
                        meta.summary.clone()
                    };
                    kit::cell_text(ui, p, &text);
                }
                "cast" => {
                    kit::cell_text(ui, p, &row.cast.join(", "));
                }
                "minutes" => {
                    kit::cell_text(
                        ui,
                        p,
                        &meta.est_minutes.map_or(String::new(), |m| m.to_string()),
                    );
                }
                _ => {
                    kit::cell_text(ui, p, &meta.notes);
                }
            }
        },
        |ui, n| {
            // Menú de la fila: ir al guion (la ficha completa se edita en el inspector).
            if ui.button("Abrir en el guion").clicked() {
                menu_goto = Some(rows[n].id);
                ui.close();
            }
        },
    );
    if let Some(n) = table.clicked.or(table.double_clicked) {
        state.selected = Some(rows[n].id);
    }
    if let Some(n) = table.double_clicked {
        out.goto = Some(rows[n].id);
    }
    out.goto = out.goto.or(menu_goto);
    if let Some(id) = table.header_clicked {
        if let Some(s) = COLS.iter().find(|(c, _)| c.id == id).and_then(|(_, s)| *s) {
            state.sort = match state.sort {
                Some((cur, true)) if cur == s => Some((s, false)),
                Some((cur, false)) if cur == s => None,
                _ => Some((s, true)),
            };
        }
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
            (SceneHeading, "INT. LIVING - NOCHE"),
            (Action, "Ana espera junto a la ventana. Mira el reloj."),
            (Character, "ANA"),
            (Dialogue, "No vino nadie."),
            (SceneHeading, "EXT. CALLE - CONTINUO"),
            (Action, "Llueve."),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p.auto_link_all();
        p
    }

    fn frame(
        ctx: &egui::Context,
        state: &mut ScenesState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
    ) -> Out {
        let p = crate::storage::Settings::default().scheme.palette();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 800.0),
            )),
            time: Some(t),
            focused: true,
            events,
            ..Default::default()
        };
        let mut out = Out::default();
        let mut o = ctx.run_ui(input, |ui| out = show(ui, state, project, &p));
        o.textures_delta.clear();
        out
    }

    #[test]
    fn the_scene_card_reads_what_the_script_already_says() {
        let p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        assert_eq!(
            heading_parts("INT. CASA DE ANA - NOCHE"),
            ("INT.".into(), "CASA DE ANA".into(), "NOCHE".into())
        );
        assert_eq!(heading_parts("ext. patio – día").2, "DÍA");
        assert_eq!(heading_parts("INT./EXT. AUTO - DÍA").0, "INT./EXT.");
        // Una escena corta ocupa el mínimo de un octavo y se puede corregir a mano.
        assert_eq!(p.scene_eighths(ids[1]), 1);
        assert_eq!(p.scene_cast(ids[0]), ["ANA"]);
        assert_eq!(
            p.scene_summary_hint(ids[0]),
            "Ana espera junto a la ventana."
        );
        assert_eq!(fmt_eighths(3), "3/8");
        assert_eq!(fmt_eighths(8), "1");
        assert_eq!(fmt_eighths(11), "1 3/8");
        for (text, eighths) in [
            ("3/8", Some(3)),
            ("1 3/8", Some(11)),
            ("2", Some(16)),
            ("", None),
            ("0", None),
            ("3/4", None),
            ("x", None),
        ] {
            assert_eq!(parse_eighths(text), eighths, "{text:?}");
        }
        // Una escena larga: 60 líneas de acción pasan de la página.
        let mut long = project();
        for n in 0..60 {
            let a = long.make_paragraph(Action, &format!("Acción {n}."));
            long.paragraphs.insert(2, a);
        }
        assert!(long.scene_eighths(ids[0]) > 8);
    }

    #[test]
    fn the_continuing_scene_suggests_the_previous_story_day() {
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        assert_eq!(
            p.story_day_hint(ids[1]),
            None,
            "la anterior no tiene día todavía"
        );
        p.scenes.entry(ids[0]).or_default().story_day = Some(2);
        assert_eq!(
            p.story_day_hint(ids[1]),
            Some(2),
            "CONTINUO sigue el día de la escena anterior"
        );
        assert_eq!(p.story_day_hint(ids[0]), None);
    }

    #[test]
    fn a_long_summary_is_cut_in_its_cell_and_never_grows_the_row() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut state = ScenesState::default();
        let mut project = project();
        let ids: Vec<u64> = project
            .script_scenes()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let rows_height = |ctx: &egui::Context, state: &mut ScenesState, project: &mut Project| {
            let mut bottom = 0.0;
            for n in 0..3 {
                let p = crate::storage::Settings::default().scheme.palette();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1500.0, 800.0),
                    )),
                    time: Some(n as f64 * 0.1),
                    focused: true,
                    ..Default::default()
                };
                let mut o = ctx.run_ui(input, |ui| {
                    show(ui, state, project, &p);
                    bottom = ui.min_rect().bottom();
                });
                o.textures_delta.clear();
            }
            bottom
        };
        let short = rows_height(&ctx, &mut state, &mut project);
        project.scenes.entry(ids[0]).or_default().summary = "Ana espera junto a la ventana y mira cómo la lluvia borra la calle mientras nadie llega a la casa. ".repeat(4);
        state.editing.clear();
        let long = rows_height(&ctx, &mut state, &mut project);
        assert!(
            (long - short).abs() < 1.0,
            "la fila no crece: {short} → {long}"
        );
    }

    #[test]
    fn locking_and_unlocking_the_numbers_keeps_the_production_data() {
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        p.scenes.entry(ids[0]).or_default().summary = "Ana espera".into();
        p.lock_scene_numbers();
        assert_eq!(p.scenes[&ids[0]].summary, "Ana espera");
        p.unlock_scene_numbers();
        assert_eq!(p.scenes[&ids[0]].summary, "Ana espera");
        assert!(p.scenes[&ids[0]].number.is_empty());
    }

    #[test]
    fn the_table_draws_and_its_cells_write_into_the_scene_card() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut state = ScenesState::default();
        let mut project = project();
        let ids: Vec<u64> = project
            .script_scenes()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        for n in 0..3 {
            frame(&ctx, &mut state, &mut project, vec![], n as f64 * 0.1);
        }
        // Lo escrito en cada columna llega a la ficha, y vaciarlo vuelve al cálculo.
        assert!(apply(&mut project, ids[0], 0, "1 3/8"));
        assert!(apply(&mut project, ids[0], 1, "2"));
        assert!(apply(&mut project, ids[0], 2, "  Ana espera  "));
        assert!(apply(&mut project, ids[0], 3, "45"));
        assert!(apply(&mut project, ids[0], 4, "revisar"));
        let meta = project.scenes[&ids[0]].clone();
        assert_eq!(
            (
                meta.eighths,
                meta.story_day,
                meta.summary.as_str(),
                meta.est_minutes,
                meta.notes.as_str()
            ),
            (Some(11), Some(2), "Ana espera", Some(45), "revisar")
        );
        assert_eq!(project.scene_eighths(ids[0]), 11);
        assert!(
            !apply(&mut project, ids[0], 2, "Ana espera"),
            "lo mismo no cambia nada"
        );
        assert!(apply(&mut project, ids[0], 0, ""));
        assert_eq!(project.scenes[&ids[0]].eighths, None);
        assert!(!apply(&mut project, ids[1], 1, "no es un número"));
        frame(&ctx, &mut state, &mut project, vec![], 0.5);
        // Todas las ordenaciones dibujan.
        for sort in [
            Sort::Number,
            Sort::Kind,
            Sort::Time,
            Sort::Day,
            Sort::Eighths,
        ] {
            for ascending in [true, false] {
                state.sort = Some((sort, ascending));
                frame(&ctx, &mut state, &mut project, vec![], 1.0);
            }
        }
    }

    #[test]
    fn the_cast_shows_its_number_and_each_scene_can_choose_where_it_is_filmed() {
        let mut p = project();
        let ana = p.character_id_by_name("ANA").unwrap();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        assert_eq!(p.scene_cast(ids[0]), ["ANA"]);
        p.characters
            .iter_mut()
            .find(|c| c.id == ana)
            .unwrap()
            .cast_number = Some(3);
        assert_eq!(p.scene_cast(ids[0]), ["3. ANA"]);
        let house = p.add_shoot_location("Casa");
        p.scenes.entry(ids[1]).or_default().shoot_location = Some(house);
        assert_eq!(p.scene_shoot_location(ids[1]), Some(house));
        // La tabla dibuja con lugares y con el elenco numerado.
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut state = ScenesState::default();
        for n in 0..3 {
            frame(&ctx, &mut state, &mut p, vec![], n as f64 * 0.1);
        }
    }
}
