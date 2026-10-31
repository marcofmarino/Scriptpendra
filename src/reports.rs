//! Informes imprimibles del desglose: la hoja de cada escena y la lista de compras y alquileres, en PDF (letra
//! monoespaciada, la misma del guion) y en CSV para planillas.

use crate::{
    model::{Acquisition, CategorySource, Project, ShoppingRow, fmt_eighths, heading_parts},
    pdf::Line,
    schedule::{StripEntry, fmt_minutes},
    storage::Paper,
};

/// Dinero como se escribe acá: miles con punto y centavos con coma («$ 1.234,50»); sin centavos si son cero.
pub fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.abs();
    let (whole, rest) = (cents / 100, cents % 100);
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (n, c) in digits.chars().enumerate() {
        if n > 0 && (digits.len() - n) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    if rest == 0 {
        format!("{sign}$ {grouped}")
    } else {
        format!("{sign}$ {grouped},{rest:02}")
    }
}

/// Cómo se agrupa la lista de compras.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    #[default]
    Category,
    Responsible,
}

/// La lista de compras partida en grupos: (título, filas).
pub fn grouped(
    project: &Project,
    rows: Vec<ShoppingRow>,
    by: Group,
) -> Vec<(String, Vec<ShoppingRow>)> {
    let mut out: Vec<(String, Vec<ShoppingRow>)> = Vec::new();
    for row in rows {
        let title = match by {
            Group::Category => project
                .breakdown_categories
                .iter()
                .find(|c| c.id == row.category)
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            Group::Responsible => row
                .responsible
                .and_then(|r| project.contacts.iter().find(|c| c.id == r))
                .map_or("Sin responsable".to_string(), |c| c.name.clone()),
        };
        match out.iter_mut().find(|(t, _)| *t == title) {
            Some((_, group)) => group.push(row),
            None => out.push((title, vec![row])),
        }
    }
    // Por responsable: los nombres en orden y, al final, lo que no tiene a nadie.
    if by == Group::Responsible {
        out.sort_by(|a, b| (a.0 == "Sin responsable", &a.0).cmp(&(b.0 == "Sin responsable", &b.0)));
    }
    out
}

/// Lo que suman los costos de unas filas.
pub fn total(rows: &[ShoppingRow]) -> i64 {
    rows.iter().filter_map(|r| r.cost_cents).sum()
}

fn csv_field(text: &str) -> String {
    if text.contains([';', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// La lista de compras como CSV con punto y coma (lo que abren las planillas en español) y coma decimal.
pub fn shopping_csv(project: &Project, rows: &[ShoppingRow]) -> String {
    let mut out =
        String::from("Categoría;Elemento;Cantidad;Estado;Escenas;Responsable;Costo;Notas\n");
    for row in rows {
        let category = project
            .breakdown_categories
            .iter()
            .find(|c| c.id == row.category)
            .map_or("", |c| c.name.as_str());
        let responsible = row
            .responsible
            .and_then(|r| project.contacts.iter().find(|c| c.id == r))
            .map_or("", |c| c.name.as_str());
        let cost = row
            .cost_cents
            .map_or(String::new(), |c| format!("{},{:02}", c / 100, c % 100));
        let fields = [
            category,
            &row.name,
            &row.quantity.to_string(),
            row.status.label(),
            &row.scenes.join(" "),
            responsible,
            &cost,
            &row.notes,
        ];
        out.push_str(
            &fields
                .iter()
                .map(|f| csv_field(f))
                .collect::<Vec<_>>()
                .join(";"),
        );
        out.push('\n');
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Desglose como tabla
// ------------------------------------------------------------------------------------------------

/// Una fila de la tabla del desglose para exportar.
pub struct BreakdownLine {
    pub category: String,
    pub name: String,
    pub scenes: Vec<String>,
    pub status: Acquisition,
    pub quantity: u32,
    /// El costo cargado (por unidad si `per_unit`, si no el total) y lo que da en total.
    pub cost_cents: Option<i64>,
    pub per_unit: bool,
    pub total_cents: Option<i64>,
    pub responsible: String,
    pub notes: String,
}

/// Las filas del desglose que tienen algo: aparecen en alguna escena o tienen datos cargados (estado, cantidad, costo,
/// responsable o notas). Las categorías sin filas no figuran.
pub fn breakdown_lines(project: &Project) -> Vec<BreakdownLine> {
    let labels: Vec<String> = project.scene_labels().into_iter().map(|(_, n)| n).collect();
    let mut out = Vec::new();
    for row in project.breakdown_rows(false) {
        let item = row.item.map(|i| &project.breakdown_items[i]);
        let deal = row.entity.and_then(|e| project.location_deal(e));
        let mut status = item.map_or(Acquisition::Pending, |i| i.status);
        if status == Acquisition::Pending {
            status = deal.and_then(|d| d.status).unwrap_or(status);
        }
        let cost = item
            .and_then(|i| i.cost_cents)
            .or(deal.and_then(|d| d.cost_cents));
        let quantity = item.map_or(0, |i| i.quantity);
        let per_unit = item.is_some_and(|i| i.unit_cost);
        let responsible = item
            .and_then(|i| i.responsible)
            .or(deal.and_then(|d| d.responsible))
            .and_then(|r| project.contacts.iter().find(|c| c.id == r))
            .map_or(String::new(), |c| c.name.clone());
        let notes = item.map(|i| i.notes.clone()).unwrap_or_default();
        let filled = !row.scenes.is_empty()
            || status != Acquisition::Pending
            || quantity > 0
            || cost.is_some()
            || !responsible.is_empty()
            || !notes.is_empty();
        if !filled {
            continue;
        }
        let category = project
            .breakdown_categories
            .iter()
            .find(|c| c.id == row.category)
            .map_or(String::new(), |c| c.name.clone());
        out.push(BreakdownLine {
            category,
            name: row.name.clone(),
            scenes: row
                .scenes
                .iter()
                .filter_map(|n| labels.get(n - 1).cloned())
                .collect(),
            status,
            quantity,
            cost_cents: cost,
            per_unit,
            total_cents: cost.map(|c| {
                if per_unit {
                    c * i64::from(quantity.max(1))
                } else {
                    c
                }
            }),
            responsible,
            notes,
        });
    }
    out
}

/// El desglose como CSV con punto y coma y coma decimal: una fila por ítem con datos.
pub fn breakdown_csv(lines: &[BreakdownLine]) -> String {
    let mut out = String::from(
        "Categoría;Elemento;Escenas;Estado;Cantidad;Costo;El costo es;Total;Responsable;Notas\n",
    );
    let plain = |c: i64| format!("{},{:02}", c / 100, c % 100);
    for l in lines {
        let fields = [
            l.category.clone(),
            l.name.clone(),
            l.scenes.join(" "),
            l.status.label().to_string(),
            if l.quantity > 0 {
                l.quantity.to_string()
            } else {
                String::new()
            },
            l.cost_cents.map_or(String::new(), plain),
            if l.cost_cents.is_none() {
                String::new()
            } else if l.per_unit {
                "por unidad".to_string()
            } else {
                "total".to_string()
            },
            l.total_cents.map_or(String::new(), plain),
            l.responsible.clone(),
            l.notes.clone(),
        ];
        out.push_str(
            &fields
                .iter()
                .map(|f| csv_field(f))
                .collect::<Vec<_>>()
                .join(";"),
        );
        out.push('\n');
    }
    out
}

/// El desglose como hojas de texto, por categoría y solo con las categorías que tienen filas.
pub fn breakdown_pages(project: &Project, lines: &[BreakdownLine], paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    doc.line(
        &format!("DESGLOSE · {}", project.title.to_uppercase()),
        true,
    );
    doc.blank();
    let mut categories: Vec<&str> = Vec::new();
    for l in lines {
        if !categories.contains(&l.category.as_str()) {
            categories.push(&l.category);
        }
    }
    let mut all_total = 0;
    for category in categories {
        doc.line(&category.to_uppercase(), true);
        for l in lines.iter().filter(|l| l.category == category) {
            let scenes = if l.scenes.is_empty() {
                String::new()
            } else {
                format!(" · esc. {}", l.scenes.join(", "))
            };
            let amount = if l.quantity > 0 {
                format!(" x{}", l.quantity)
            } else {
                String::new()
            };
            let who = if l.responsible.is_empty() {
                String::new()
            } else {
                format!(" · {}", l.responsible)
            };
            let cost = l.total_cents.map_or(String::new(), |c| {
                all_total += c;
                money(c)
            });
            let text = format!("{}{amount} · {}{scenes}{who}", l.name, l.status.label());
            let width = doc.cols().saturating_sub(16);
            for (n, part) in crate::pdf::wrap(&text, width).iter().enumerate() {
                let lead = if n == 0 { "  - " } else { "    " };
                let right = if n == 0 { cost.clone() } else { String::new() };
                doc.line(&format!("{lead}{part:<w$}{right:>12}", w = width), false);
            }
            if !l.notes.is_empty() {
                doc.wrapped("      ", 6, &l.notes, false);
            }
        }
        doc.blank();
    }
    if lines.is_empty() {
        doc.line("Todavía no hay ítems con datos en el desglose.", false);
    } else {
        doc.line(
            &format!(
                "TOTAL{:>width$}",
                money(all_total),
                width = doc.cols().saturating_sub(5)
            ),
            true,
        );
    }
    doc.finish()
}

// ------------------------------------------------------------------------------------------------
// Hojas de texto
// ------------------------------------------------------------------------------------------------

/// Un documento de renglones en letra monoespaciada de 10 puntos que pasa de hoja solo.
struct Doc {
    width: f32,
    height: f32,
    margin: f32,
    pages: Vec<Vec<Line>>,
    row: usize,
}

const SIZE: f32 = 10.0;
const CHAR: f32 = SIZE * 0.6;
const ROW: f32 = SIZE * 1.3;

impl Doc {
    fn new(paper: Paper) -> Self {
        let sheet = paper.sheet();
        Self {
            width: sheet.width * 72.0,
            height: sheet.height * 72.0,
            margin: 54.0,
            pages: vec![Vec::new()],
            row: 0,
        }
    }

    fn cols(&self) -> usize {
        ((self.width - 2.0 * self.margin) / CHAR).floor() as usize
    }

    fn rows_per_page(&self) -> usize {
        ((self.height - 2.0 * self.margin) / ROW).floor() as usize
    }

    fn new_page(&mut self) {
        if !self.pages.last().is_some_and(Vec::is_empty) {
            self.pages.push(Vec::new());
        }
        self.row = 0;
    }

    fn line(&mut self, text: &str, bold: bool) {
        if self.row >= self.rows_per_page() {
            self.new_page();
        }
        let y = self.margin + self.row as f32 * ROW;
        if let Some(page) = self.pages.last_mut() {
            page.push(Line {
                x: self.margin,
                y,
                text: text.to_string(),
                bold,
                underline: false,
                size: SIZE,
                swatch: None,
            });
        }
        self.row += 1;
    }

    fn blank(&mut self) {
        // Una línea en blanco arriba de una hoja no hace falta.
        if self.row > 0 {
            self.row += 1;
        }
    }

    /// Un texto partido en renglones; los siguientes al primero van con `hang` espacios de sangría.
    fn wrapped(&mut self, first: &str, hang: usize, text: &str, bold: bool) {
        let width = self
            .cols()
            .saturating_sub(first.chars().count().max(hang))
            .max(10);
        for (n, row) in crate::pdf::wrap(text, width).into_iter().enumerate() {
            let lead = if n == 0 {
                first.to_string()
            } else {
                " ".repeat(hang)
            };
            self.line(&format!("{lead}{row}"), bold);
        }
    }

    /// Los renglones de una hoja ya armados.
    fn finish(mut self) -> Vec<Vec<Line>> {
        if self.pages.len() > 1 && self.pages.last().is_some_and(Vec::is_empty) {
            self.pages.pop();
        }
        self.pages
    }
}

/// Una hoja de desglose por escena (o más, si la escena tiene mucho), con lo que la escena pide.
pub fn scene_sheets(project: &Project, paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    let labels: std::collections::HashMap<u64, String> =
        project.scene_labels().into_iter().collect();
    let scenes = project.script_scenes();
    for (n, (id, heading)) in scenes.iter().enumerate() {
        if n > 0 {
            doc.new_page();
        }
        let meta = project.scenes.get(id).cloned().unwrap_or_default();
        let (kind, location, time) = heading_parts(heading);
        let number = labels.get(id).cloned().unwrap_or_default();
        doc.line(
            &format!("HOJA DE DESGLOSE · {}", project.title.to_uppercase()),
            true,
        );
        doc.line(&format!("ESCENA {number}"), true);
        doc.wrapped("", 0, heading.trim(), true);
        doc.blank();
        let day = meta.story_day.map_or("—".to_string(), |d| d.to_string());
        doc.line(
            &format!(
                "{} · {} · {} págs. · Día {}",
                if kind.is_empty() { "—" } else { &kind },
                if time.is_empty() { "—" } else { &time },
                fmt_eighths(project.scene_eighths(*id)),
                day
            ),
            false,
        );
        let place = project
            .scene_shoot_location(*id)
            .and_then(|s| project.shoot_locations.iter().find(|l| l.id == s))
            .map_or("—".to_string(), |l| l.name.clone());
        doc.wrapped(
            "Locación: ",
            10,
            &format!("{location} · se filma en {place}"),
            false,
        );
        if !meta.summary.is_empty() {
            doc.wrapped("Resumen: ", 9, &meta.summary, false);
        }
        doc.blank();

        let cast = project.scene_cast_detail(*id);
        if !cast.is_empty() {
            doc.line("ELENCO", true);
            for (number, name, actor) in cast {
                let who = actor.map_or(String::new(), |a| format!(" — {a}"));
                doc.line(
                    &format!(
                        "  {} {name}{who}",
                        number.map_or("  ".to_string(), |n| format!("{n}."))
                    ),
                    false,
                );
            }
            doc.blank();
        }
        let items = project.scene_sheet_items(*id);
        for cat in project.breakdown_categories.iter().filter(|c| {
            !matches!(
                c.source,
                CategorySource::Characters | CategorySource::Locations
            )
        }) {
            let mine: Vec<_> = items.iter().filter(|i| i.category == cat.id).collect();
            if mine.is_empty() {
                continue;
            }
            doc.line(&cat.name.to_uppercase(), true);
            for item in mine {
                let amount = item
                    .scene_quantity
                    .or((item.quantity > 0).then_some(item.quantity))
                    .map_or(String::new(), |q| format!(" (x{q})"));
                let status = if item.status == Acquisition::Pending {
                    String::new()
                } else {
                    format!(" · {}", item.status.label())
                };
                let notes = if item.notes.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", item.notes)
                };
                doc.wrapped(
                    "  - ",
                    4,
                    &format!("{}{amount}{status}{notes}", item.name),
                    false,
                );
            }
            doc.blank();
        }
        if !meta.notes.is_empty() {
            doc.line("NOTAS", true);
            doc.wrapped("  ", 2, &meta.notes, false);
        }
    }
    if scenes.is_empty() {
        doc.line("Todavía no hay escenas en el guion.", false);
    }
    doc.finish()
}

/// La lista de compras y alquileres, agrupada, con subtotales y el total.
pub fn shopping_pages(
    project: &Project,
    rows: Vec<ShoppingRow>,
    by: Group,
    paper: Paper,
) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    doc.line(
        &format!(
            "LISTA DE COMPRAS Y ALQUILERES · {}",
            project.title.to_uppercase()
        ),
        true,
    );
    doc.blank();
    let all_total = total(&rows);
    let count = rows.len();
    for (title, group) in grouped(project, rows, by) {
        doc.line(
            &format!(
                "{}{:>width$}",
                title.to_uppercase(),
                money(total(&group)),
                width = doc.cols().saturating_sub(title.chars().count())
            ),
            true,
        );
        for row in &group {
            let scenes = if row.scenes.is_empty() {
                String::new()
            } else {
                format!(" · esc. {}", row.scenes.join(", "))
            };
            let who = match (
                by,
                row.responsible
                    .and_then(|r| project.contacts.iter().find(|c| c.id == r)),
            ) {
                (Group::Category, Some(c)) => format!(" · {}", c.name),
                _ => String::new(),
            };
            let amount = if row.quantity > 0 {
                format!(" x{}", row.quantity)
            } else {
                String::new()
            };
            let cost = row.cost_cents.map_or(String::new(), money);
            let text = format!("{}{amount} · {}{scenes}{who}", row.name, row.status.label());
            let width = doc.cols().saturating_sub(16);
            let wrapped = crate::pdf::wrap(&text, width);
            for (n, part) in wrapped.iter().enumerate() {
                let lead = if n == 0 { "  - " } else { "    " };
                let right = if n == 0 { cost.clone() } else { String::new() };
                doc.line(&format!("{lead}{part:<w$}{right:>12}", w = width), false);
            }
            if !row.notes.is_empty() {
                doc.wrapped("      ", 6, &row.notes, false);
            }
        }
        doc.blank();
    }
    if count == 0 {
        doc.line("No hay elementos con esos estados.", false);
    } else {
        doc.line(
            &format!(
                "TOTAL ({count} elementos){:>width$}",
                money(all_total),
                width = doc
                    .cols()
                    .saturating_sub(format!("TOTAL ({count} elementos)").chars().count())
            ),
            true,
        );
    }
    doc.finish()
}

// ------------------------------------------------------------------------------------------------
// Plan de rodaje
// ------------------------------------------------------------------------------------------------

/// El encabezado de una tira del plan: el de la escena, y si es un tramo, con los números de sus planos.
fn part_heading(project: &Project, entry: &StripEntry, heading: &str) -> String {
    let StripEntry::Part(_, shots) = entry else {
        return heading.to_string();
    };
    let numbers = project.shot_numbers();
    let list: Vec<String> = shots
        .iter()
        .filter_map(|s| numbers.get(s).map(|n| n.to_string()))
        .collect();
    format!("{heading} (planos {})", list.join(", "))
}

impl Doc {
    /// Un rectángulo de color de `rows` renglones de alto desde el renglón actual, detrás del texto que siga.
    fn band(&mut self, rows: usize, color: [u8; 3]) {
        if self.row + rows > self.rows_per_page() {
            self.new_page();
        }
        let y = self.margin + self.row as f32 * ROW - 2.0;
        if let Some(page) = self.pages.last_mut() {
            let at = page.iter().take_while(|l| l.swatch.is_some()).count();
            page.insert(
                at,
                Line {
                    x: self.margin - 4.0,
                    y,
                    text: String::new(),
                    bold: false,
                    underline: false,
                    size: SIZE,
                    swatch: Some((
                        self.width - 2.0 * self.margin + 8.0,
                        rows as f32 * ROW,
                        color,
                    )),
                },
            );
        }
    }
}

/// Un tono clarito del color de una tira, para que el texto negro se lea.
fn soften(color: [u8; 3]) -> [u8; 3] {
    color.map(|c| (c as f32 * 0.45 + 255.0 * 0.55) as u8)
}

fn day_title(project: &Project, day: &crate::schedule::DayView) -> String {
    let date = project
        .shoot_days
        .iter()
        .find(|d| d.id == day.id)
        .map(|d| d.date.clone())
        .unwrap_or_default();
    if date.is_empty() {
        format!("JORNADA {}", day.number)
    } else {
        format!("JORNADA {} · {date}", day.number)
    }
}

/// Las tiras del plan, de a una por escena, con el color de su tipo, separadas por día.
pub fn strip_pages(project: &Project, paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    let labels: std::collections::HashMap<u64, String> =
        project.scene_labels().into_iter().collect();
    let headings = project.script_scenes();
    doc.line(&format!("TIRAS · {}", project.title.to_uppercase()), true);
    for day in project.days_view() {
        doc.blank();
        let stats = project.day_stats(&day);
        doc.line(
            &format!(
                "{} · {} escenas · {} págs. · {}{}",
                day_title(project, &day),
                stats.scenes,
                fmt_eighths(stats.eighths.min(u32::from(u16::MAX)) as u16),
                fmt_minutes(stats.minutes),
                if stats.estimated { " (aprox.)" } else { "" }
            ),
            true,
        );
        for &i in &day.entries {
            match &project.schedule[i] {
                StripEntry::Banner(text) => doc.line(&format!("*** {text} ***"), true),
                StripEntry::DayBreak(_) => {}
                entry @ (StripEntry::Scene(_) | StripEntry::Part(..)) => {
                    let id = &entry.scene().unwrap_or_default();
                    // Un tramo lleva los números de sus planos.
                    let heading = headings
                        .iter()
                        .find(|(s, _)| s == id)
                        .map_or("", |(_, h)| h.trim());
                    let heading = &part_heading(project, entry, heading);
                    let place = project
                        .scene_shoot_location(*id)
                        .and_then(|s| project.shoot_locations.iter().find(|l| l.id == s))
                        .map_or("—".to_string(), |l| l.name.clone());
                    let cast: Vec<String> = project
                        .scene_cast_ids(*id)
                        .iter()
                        .filter_map(|c| project.character(*c))
                        .map(|c| {
                            c.cast_number
                                .map_or(crate::model::clean_character_name(&c.name), |n| {
                                    n.to_string()
                                })
                        })
                        .collect();
                    let summary = project
                        .scenes
                        .get(id)
                        .map(|m| m.summary.clone())
                        .unwrap_or_default();
                    doc.band(2, soften(project.strip_color(*id)));
                    doc.line(
                        &format!(
                            "{:>4}  {}  [{} págs.]  {}",
                            labels.get(id).cloned().unwrap_or_default(),
                            heading,
                            fmt_eighths(project.scene_eighths(*id)),
                            place
                        ),
                        true,
                    );
                    doc.line(
                        &format!(
                            "      {}  ·  Elenco: {}",
                            if summary.is_empty() {
                                "—".to_string()
                            } else {
                                summary
                            },
                            if cast.is_empty() {
                                "—".to_string()
                            } else {
                                cast.join(", ")
                            }
                        ),
                        false,
                    );
                }
            }
        }
    }
    let loose = project.unscheduled_scenes();
    if !loose.is_empty() {
        doc.blank();
        doc.line(&format!("SIN PROGRAMAR · {} escenas", loose.len()), true);
        for id in loose {
            let heading = headings
                .iter()
                .find(|(s, _)| *s == id)
                .map_or("", |(_, h)| h.trim());
            doc.line(
                &format!(
                    "  {:>4}  {heading}",
                    labels.get(&id).cloned().unwrap_or_default()
                ),
                false,
            );
        }
    }
    if project.schedule.is_empty() && project.unscheduled_scenes().is_empty() {
        doc.line("Todavía no hay escenas en el guion.", false);
    }
    doc.finish()
}

/// El plan día por día en lista, con los avisos de cada uno.
pub fn day_plan_pages(project: &Project, paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    let labels: std::collections::HashMap<u64, String> =
        project.scene_labels().into_iter().collect();
    let headings = project.script_scenes();
    doc.line(
        &format!("PLAN DE RODAJE · {}", project.title.to_uppercase()),
        true,
    );
    for day in project.days_view() {
        doc.blank();
        let stats = project.day_stats(&day);
        doc.line(&day_title(project, &day), true);
        let shoot = project
            .shoot_days
            .iter()
            .find(|d| d.id == day.id)
            .cloned()
            .unwrap_or_default();
        let times = match (shoot.call_time.is_empty(), shoot.wrap_time.is_empty()) {
            (true, true) => String::new(),
            _ => format!(
                " · citación {} · fin {}",
                if shoot.call_time.is_empty() {
                    "—"
                } else {
                    &shoot.call_time
                },
                if shoot.wrap_time.is_empty() {
                    "—"
                } else {
                    &shoot.wrap_time
                }
            ),
        };
        doc.line(
            &format!(
                "{} escenas · {} págs. · {}{} · {} emplazamientos{times}",
                stats.scenes,
                fmt_eighths(stats.eighths.min(u32::from(u16::MAX)) as u16),
                fmt_minutes(stats.minutes),
                if stats.estimated { " (aprox.)" } else { "" },
                stats.setups
            ),
            false,
        );
        for &i in &day.entries {
            match &project.schedule[i] {
                StripEntry::Banner(text) => doc.line(&format!("  *** {text} ***"), true),
                entry @ (StripEntry::Scene(_) | StripEntry::Part(..)) => {
                    let id = &entry.scene().unwrap_or_default();
                    let heading = headings
                        .iter()
                        .find(|(s, _)| s == id)
                        .map_or("", |(_, h)| h.trim());
                    let heading = &part_heading(project, entry, heading);
                    let place = project
                        .scene_shoot_location(*id)
                        .and_then(|s| project.shoot_locations.iter().find(|l| l.id == s))
                        .map_or(String::new(), |l| format!(" · {}", l.name));
                    doc.wrapped(
                        &format!("  {:>4}  ", labels.get(id).cloned().unwrap_or_default()),
                        8,
                        &format!(
                            "{heading} [{}]{place}",
                            fmt_eighths(project.scene_eighths(*id))
                        ),
                        false,
                    );
                }
                StripEntry::DayBreak(_) => {}
            }
        }
        for warning in project.day_warnings(&day) {
            doc.wrapped("  ! ", 4, &warning, false);
        }
    }
    if project.schedule.is_empty() {
        doc.line("Todavía no hay días de rodaje en el plan.", false);
    }
    doc.finish()
}

/// El calendario del elenco como lista de filas: encabezado y una por personaje.
fn cast_table(project: &Project) -> (Vec<String>, Vec<Vec<String>>) {
    let (days, rows) = project.cast_calendar();
    let mut header = vec![
        "N°".to_string(),
        "Personaje".to_string(),
        "Intérprete".to_string(),
    ];
    header.extend(days);
    header.extend(["Trabaja".to_string(), "En espera".to_string()]);
    let body = rows
        .into_iter()
        .map(|r| {
            let mut cells = vec![
                r.number.map_or(String::new(), |n| n.to_string()),
                r.name,
                r.actor.unwrap_or_default(),
            ];
            cells.extend(r.codes.iter().map(|c| c.code().to_string()));
            cells.extend([r.worked.to_string(), r.held.to_string()]);
            cells
        })
        .collect();
    (header, body)
}

/// El calendario del elenco como CSV.
pub fn cast_calendar_csv(project: &Project) -> String {
    let (header, body) = cast_table(project);
    std::iter::once(header)
        .chain(body)
        .map(|row| {
            row.iter()
                .map(|f| csv_field(f))
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// El calendario del elenco en una hoja apaisada si hace falta lugar: los días en columnas, con los códigos SW, W, WF y H.
pub fn cast_calendar_pages(project: &Project, paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    doc.line(
        &format!("CALENDARIO DEL ELENCO · {}", project.title.to_uppercase()),
        true,
    );
    doc.line(
        "SW primer día · W trabaja · WF último día · SWF un solo día · H en espera",
        false,
    );
    doc.blank();
    let (header, body) = cast_table(project);
    let days = header.len() - 5;
    // Cuántas columnas de días entran: lo que sobra después de número, nombre, intérprete y totales, a 11 cada una.
    let fixed = 4 + 17 + 18 + 2 * 9;
    let per_row = (doc.cols().saturating_sub(fixed) / 11).max(1);
    for start in (0..days.max(1)).step_by(per_row) {
        let end = (start + per_row).min(days);
        let row_text = |cells: &[String], header: bool| {
            let mut line = format!("{:<4}{:<16.16} {:<17.17}", cells[0], cells[1], cells[2]);
            for cell in &cells[3 + start..3 + end] {
                line.push_str(&format!("{:^11.10}", cell));
            }
            let (worked, held) = if header {
                ("Trab.".to_string(), "Espera".to_string())
            } else {
                (
                    cells[cells.len() - 2].clone(),
                    cells[cells.len() - 1].clone(),
                )
            };
            line.push_str(&format!("{worked:>9}{held:>9}"));
            line
        };
        doc.line(&row_text(&header, true), true);
        for row in &body {
            doc.line(&row_text(row, false), false);
        }
        doc.blank();
    }
    if body.is_empty() {
        doc.line("Todavía no hay personajes programados.", false);
    }
    doc.finish()
}

// ------------------------------------------------------------------------------------------------
// Hoja de llamado
// ------------------------------------------------------------------------------------------------

fn or_dash(text: &str) -> &str {
    if text.trim().is_empty() {
        "—"
    } else {
        text.trim()
    }
}

fn scene_text(s: &crate::callsheet::SceneLine) -> String {
    let summary = if s.summary.is_empty() {
        String::new()
    } else {
        format!(" — {}", s.summary)
    };
    let cast = if s.cast.is_empty() {
        String::new()
    } else {
        format!(" · Elenco {}", s.cast)
    };
    format!("{} [{} págs.]{summary}{cast}", s.heading, s.eighths)
}

/// La hoja de llamado completa de un día.
pub fn call_sheet_pages(sheet: &crate::callsheet::CallSheet, paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    doc.line(
        &format!("HOJA DE LLAMADO · {}", sheet.project.to_uppercase()),
        true,
    );
    doc.line(
        &format!(
            "JORNADA {} DE {} · {}",
            sheet.day_number,
            sheet.days_total,
            or_dash(&sheet.date)
        ),
        true,
    );
    doc.blank();
    doc.line(
        &format!(
            "Citación general {} · Almuerzo {} · Fin estimado {}",
            or_dash(&sheet.call),
            or_dash(&sheet.lunch),
            or_dash(&sheet.wrap)
        ),
        false,
    );
    doc.line(
        &format!(
            "Sale el sol {} · Se pone {} · Pronóstico: {}",
            sheet.sunrise.as_deref().unwrap_or("—"),
            sheet.sunset.as_deref().unwrap_or("—"),
            or_dash(&sheet.forecast)
        ),
        false,
    );
    doc.blank();

    doc.line("LOCACIÓN", true);
    if sheet.places.is_empty() {
        doc.line("  Sin lugar de rodaje asignado.", false);
    }
    for place in &sheet.places {
        doc.wrapped(
            "  ",
            2,
            &format!("{} — {}", place.name, or_dash(&place.address)),
            true,
        );
        if let Some((name, phone)) = &place.contact {
            doc.line(&format!("  Contacto: {name} {phone}"), false);
        }
        if !place.access_hours.is_empty() {
            doc.wrapped("  Horario de entrada: ", 4, &place.access_hours, false);
        }
        if !place.parking.is_empty() {
            doc.wrapped("  Estacionamiento: ", 4, &place.parking, false);
        }
        if !place.hospital.is_empty() {
            doc.wrapped("  Hospital más cercano: ", 4, &place.hospital, false);
        }
    }
    doc.blank();

    doc.line("ESCENAS", true);
    for s in &sheet.scenes {
        doc.wrapped(&format!("  {:>4}  ", s.number), 8, &scene_text(s), false);
    }
    doc.blank();

    doc.line("ELENCO", true);
    for c in &sheet.cast {
        let who = if c.actor.is_empty() {
            c.name.clone()
        } else {
            format!("{} — {}", c.name, c.actor)
        };
        doc.wrapped(
            &format!(
                "  {:>3} ",
                c.number.map_or(String::new(), |n| format!("{n}."))
            ),
            6,
            &format!("{who} {}", c.phone),
            false,
        );
        doc.line(
            &format!(
                "        citación {} · maquillaje {} · al set {}",
                or_dash(&c.call),
                or_dash(&c.makeup),
                or_dash(&c.set)
            ),
            false,
        );
    }
    doc.blank();

    if !sheet.crew.is_empty() {
        doc.line("EQUIPO TÉCNICO", true);
        for area in &sheet.crew {
            doc.line(
                &format!(
                    "  {}{}",
                    area.department.to_uppercase(),
                    if area.call.is_empty() {
                        String::new()
                    } else {
                        format!(" · citación {}", area.call)
                    }
                ),
                true,
            );
            for (name, role, phone) in &area.people {
                doc.line(
                    &format!(
                        "    {name}{} {phone}",
                        if role.is_empty() {
                            String::new()
                        } else {
                            format!(" ({role})")
                        }
                    ),
                    false,
                );
            }
        }
        doc.blank();
    }
    if !sheet.requirements.is_empty() {
        doc.line("REQUERIMIENTOS", true);
        for (category, items) in &sheet.requirements {
            doc.wrapped(&format!("  {}: ", category), 4, &items.join(", "), false);
        }
        doc.blank();
    }
    if !sheet.area_notes.is_empty() || !sheet.notes.trim().is_empty() {
        doc.line("NOTAS", true);
        for (area, note) in &sheet.area_notes {
            doc.wrapped(&format!("  {area}: "), 4, note, false);
        }
        if !sheet.notes.trim().is_empty() {
            doc.wrapped("  ", 2, sheet.notes.trim(), false);
        }
        doc.blank();
    }
    if let Some((number, date, scenes)) = &sheet.next {
        doc.line(
            &format!("PRÓXIMA JORNADA · {number} · {}", or_dash(date)),
            true,
        );
        for s in scenes {
            doc.wrapped(
                &format!("  {:>4}  ", s.number),
                8,
                &format!("{} [{} págs.]", s.heading, s.eighths),
                false,
            );
        }
    }
    doc.finish()
}

/// La hoja reducida para una persona del elenco: cuándo la citan, dónde y qué escenas filma ese día.
pub fn actor_sheet_pages(
    sheet: &crate::callsheet::CallSheet,
    character: u64,
    paper: Paper,
) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    actor_sheet(&mut doc, sheet, character);
    doc.finish()
}

/// Una hoja reducida por cada persona del elenco del día.
pub fn all_actor_sheets(sheet: &crate::callsheet::CallSheet, paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    for (n, c) in sheet.cast.iter().enumerate() {
        if n > 0 {
            doc.new_page();
        }
        actor_sheet(&mut doc, sheet, c.character);
    }
    if sheet.cast.is_empty() {
        doc.line("Hoy no hay elenco en el plan.", false);
    }
    doc.finish()
}

fn actor_sheet(doc: &mut Doc, sheet: &crate::callsheet::CallSheet, character: u64) {
    let Some(me) = sheet.cast.iter().find(|c| c.character == character) else {
        return;
    };
    doc.line(
        &format!(
            "{} · JORNADA {} DE {}",
            sheet.project.to_uppercase(),
            sheet.day_number,
            sheet.days_total
        ),
        true,
    );
    doc.line(
        &format!(
            "Para: {}{}",
            me.name,
            if me.actor.is_empty() {
                String::new()
            } else {
                format!(" — {}", me.actor)
            }
        ),
        true,
    );
    doc.line(&format!("Fecha: {}", or_dash(&sheet.date)), false);
    doc.blank();
    doc.line(
        &format!(
            "Citación: {}",
            or_dash(if me.call.is_empty() {
                &sheet.call
            } else {
                &me.call
            })
        ),
        true,
    );
    doc.line(
        &format!(
            "Maquillaje: {} · Al set: {}",
            or_dash(&me.makeup),
            or_dash(&me.set)
        ),
        false,
    );
    doc.line(
        &format!(
            "Almuerzo: {} · Fin estimado: {}",
            or_dash(&sheet.lunch),
            or_dash(&sheet.wrap)
        ),
        false,
    );
    if let (Some(rise), Some(set)) = (&sheet.sunrise, &sheet.sunset) {
        doc.line(&format!("Sale el sol {rise} · se pone {set}"), false);
    }
    if !sheet.forecast.is_empty() {
        doc.wrapped("Pronóstico: ", 12, &sheet.forecast, false);
    }
    doc.blank();
    doc.line("DÓNDE", true);
    for place in &sheet.places {
        doc.wrapped(
            "  ",
            2,
            &format!("{} — {}", place.name, or_dash(&place.address)),
            false,
        );
        if let Some((name, phone)) = &place.contact {
            doc.line(&format!("  Contacto: {name} {phone}"), false);
        }
    }
    if sheet.places.is_empty() {
        doc.line("  A confirmar.", false);
    }
    doc.blank();
    doc.line("TUS ESCENAS", true);
    for s in sheet
        .scenes
        .iter()
        .filter(|s| s.cast_ids.contains(&character))
    {
        doc.wrapped(
            &format!("  {:>4}  ", s.number),
            8,
            &format!(
                "{}{}",
                s.heading,
                if s.summary.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", s.summary)
                }
            ),
            false,
        );
    }
}

// ------------------------------------------------------------------------------------------------
// Lista de planos y storyboard
// ------------------------------------------------------------------------------------------------

/// Los planos en el orden pedido: el del montaje (el de la tabla) o el de rodaje.
fn ordered_shots(project: &Project, shoot_order: bool) -> Vec<u64> {
    if shoot_order {
        project.shots_in_shoot_order()
    } else {
        project.shots.iter().map(|s| s.id).collect()
    }
}

fn lens_text(project: &Project, shot: u64) -> String {
    project.shot_lens(shot).map_or(String::new(), |(lens, eq)| {
        if (lens - eq).abs() < 0.5 {
            crate::optics::fmt_mm(lens)
        } else {
            format!(
                "{} (eq. {})",
                crate::optics::fmt_mm(lens),
                crate::optics::fmt_mm(eq)
            )
        }
    })
}

/// La lista de planos como CSV: una fila por plano, con lo que tiene la tabla.
pub fn shot_list_csv(project: &Project, shoot_order: bool) -> String {
    let codes = project.shot_codes();
    let numbers = project.shot_numbers();
    let minutes = project.shot_minutes();
    let mut out = String::from(
        "Orden de rodaje;Código;Plano;Escena;Tipo;Movimiento;Lente;Soporte;Angulación;En cuadro;Descripción;Diálogo;Sonido;Duración;Cámara;Minutos\n",
    );
    for id in ordered_shots(project, shoot_order) {
        let Some(shot) = project.shots.iter().find(|s| s.id == id) else {
            continue;
        };
        let camera = shot
            .blocking
            .zip(shot.camera)
            .and_then(|(b, c)| {
                project
                    .blockings
                    .iter()
                    .find(|x| x.id == b)?
                    .items
                    .iter()
                    .find(|i| i.id == c)
            })
            .map_or(String::new(), |i| i.label.clone());
        let in_frame: Vec<String> = shot
            .in_frame
            .iter()
            .filter_map(|c| project.character(*c))
            .map(|c| crate::model::clean_character_name(&c.name))
            .collect();
        let scene = project
            .scene_labels()
            .into_iter()
            .find(|(s, _)| *s == shot.scene)
            .map_or(String::new(), |(_, n)| n);
        let fields = [
            shot.shoot_order.map_or(String::new(), |n| n.to_string()),
            codes.get(&id).cloned().unwrap_or_default(),
            numbers.get(&id).map_or(String::new(), |n| n.to_string()),
            scene,
            shot.size.clone(),
            shot.movement.clone(),
            lens_text(project, id),
            shot.support.clone(),
            shot.angle.clone(),
            in_frame.join(", "),
            shot.description.clone(),
            shot.dialogue.clone(),
            shot.sound.clone(),
            shot.duration.clone(),
            camera,
            minutes.get(&id).map_or(String::new(), |m| m.to_string()),
        ];
        out.push_str(
            &fields
                .iter()
                .map(|f| csv_field(f))
                .collect::<Vec<_>>()
                .join(";"),
        );
        out.push('\n');
    }
    out
}

/// La lista de planos para imprimir, en el orden del montaje o en el de rodaje.
pub fn shot_list_pages(project: &Project, shoot_order: bool, paper: Paper) -> Vec<Vec<Line>> {
    let mut doc = Doc::new(paper);
    let codes = project.shot_codes();
    doc.line(
        &format!(
            "LISTA DE PLANOS · {} · {}",
            project.title.to_uppercase(),
            if shoot_order {
                "ORDEN DE RODAJE"
            } else {
                "ORDEN DE MONTAJE"
            }
        ),
        true,
    );
    doc.blank();
    for id in ordered_shots(project, shoot_order) {
        let Some(shot) = project.shots.iter().find(|s| s.id == id) else {
            continue;
        };
        let order = if shoot_order {
            format!(
                "{:>3}. ",
                shot.shoot_order.map_or("—".to_string(), |n| n.to_string())
            )
        } else {
            String::new()
        };
        let kind = [
            shot.size.as_str(),
            shot.movement.as_str(),
            shot.support.as_str(),
            shot.angle.as_str(),
        ]
        .iter()
        .filter(|t| !t.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
        let lens = lens_text(project, id);
        let head = [codes.get(&id).cloned().unwrap_or_default(), kind, lens]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        doc.wrapped(&order, order.chars().count(), &head, true);
        if !shot.description.trim().is_empty() {
            doc.wrapped(
                &" ".repeat(order.chars().count().max(2)),
                order.chars().count().max(2),
                shot.description.trim(),
                false,
            );
        }
        if !shot.dialogue.trim().is_empty() {
            doc.wrapped(
                &format!("{}«", " ".repeat(order.chars().count().max(2))),
                order.chars().count().max(2) + 1,
                &format!("{}»", shot.dialogue.trim()),
                false,
            );
        }
        doc.blank();
    }
    if project.shots.is_empty() {
        doc.line("Todavía no hay planos en el guion técnico.", false);
    }
    doc.finish()
}

/// Los cuadros del storyboard, uno por plano.
pub fn storyboard_frames(project: &Project, only_with_image: bool) -> Vec<crate::pdf::BoardFrame> {
    let codes = project.shot_codes();
    project
        .shots
        .iter()
        .filter(|s| !only_with_image || s.image.is_some())
        .map(|s| crate::pdf::BoardFrame {
            code: codes.get(&s.id).cloned().unwrap_or_default(),
            kind: [s.size.as_str(), s.movement.as_str()]
                .iter()
                .filter(|t| !t.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · "),
            description: s.description.clone(),
            dialogue: s.dialogue.clone(),
            image: s.image.as_ref().map(|p| project.resolve_path(p)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind::*;

    fn project() -> Project {
        let mut p = Project::default();
        p.title = "El nido".into();
        p.paragraphs = [
            (SceneHeading, "INT. LIVING - NOCHE"),
            (Action, "Ana toma la linterna y abre la puerta."),
            (Character, "ANA"),
            (Dialogue, "Hola."),
            (SceneHeading, "EXT. CALLE - DÍA"),
            (Action, "Pasa una camioneta."),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p.ensure_breakdown_defaults();
        p.auto_link_all();
        p
    }

    /// Marca un texto de un párrafo como un elemento de una categoría y devuelve el id del elemento.
    fn mark(p: &mut Project, paragraph: usize, quote: &str, category: &str, name: &str) -> u64 {
        let category = p
            .breakdown_categories
            .iter()
            .find(|c| c.name == category)
            .unwrap()
            .id;
        let pid = p.paragraphs[paragraph].id;
        p.add_breakdown_tag(pid, quote, category, name, false);
        p.breakdown_items
            .iter()
            .find(|i| i.category == category && i.name == name)
            .unwrap()
            .id
    }

    #[test]
    fn money_is_written_the_local_way() {
        assert_eq!(money(123_450), "$ 1.234,50");
        assert_eq!(money(80_000), "$ 800");
        assert_eq!(money(100_000_000), "$ 1.000.000");
        assert_eq!(money(5), "$ 0,05");
    }

    #[test]
    fn the_shopping_list_has_what_is_to_be_rented_bought_or_built_with_costs_and_scenes() {
        let mut p = project();
        let van = mark(&mut p, 5, "camioneta", "Vehículos", "Camioneta");
        let lamp = mark(&mut p, 1, "linterna", "Utilería", "Linterna");
        let luis = p.add_contact("Luis");
        for (id, status, cost, who) in [
            (van, Acquisition::Rent, Some(1_500_000), Some(luis)),
            (lamp, Acquisition::Buy, Some(250_050), None),
        ] {
            let item = p.breakdown_items.iter_mut().find(|i| i.id == id).unwrap();
            item.status = status;
            item.cost_cents = cost;
            item.responsible = who;
            item.quantity = 1;
        }
        let rows = p.shopping_list(&[Acquisition::Rent, Acquisition::Buy]);
        assert_eq!(
            rows.iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>()
                .len(),
            2
        );
        assert_eq!(total(&rows), 1_750_050);
        // Con otro filtro no aparecen.
        assert!(p.shopping_list(&[Acquisition::Build]).is_empty());
        assert!(
            p.shopping_list(&[Acquisition::Rent])
                .iter()
                .all(|r| r.name == "Camioneta")
        );
        // Agrupada por responsable.
        let groups = grouped(&p, rows.clone(), Group::Responsible);
        assert_eq!(
            groups
                .iter()
                .map(|(t, g)| (t.as_str(), g.len()))
                .collect::<Vec<_>>(),
            [("Luis", 1), ("Sin responsable", 1)]
        );
        // CSV: encabezado, coma decimal y comillas donde hace falta.
        let csv = shopping_csv(&p, &rows);
        assert!(
            csv.starts_with("Categoría;Elemento;Cantidad;Estado;Escenas;Responsable;Costo;Notas\n")
        );
        assert!(
            csv.contains("Camioneta;1;Alquilar;2;Luis;15000,00;"),
            "{csv}"
        );
        assert!(csv.contains("Linterna;1;Comprar;1;;2500,50;"), "{csv}");
        // El presupuesto suma por categoría.
        let budget = p.budget_by_category();
        assert_eq!(budget.iter().map(|(_, c, _)| *c).sum::<i64>(), 1_750_050);
        // PDF de la lista: títulos de grupo con su subtotal y el total.
        let pages = shopping_pages(&p, rows, Group::Category, Paper::A4);
        let text: String = pages
            .iter()
            .flatten()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("VEHÍCULOS") && text.contains("$ 15.000") && text.contains("$ 17.500,50"),
            "{text}"
        );
        assert!(
            crate::pdf::render(&pages, 595.0, 842.0, "Compras", "")
                .unwrap()
                .starts_with(b"%PDF-")
        );
    }

    #[test]
    fn each_scene_gets_a_sheet_with_its_header_cast_and_items() {
        let mut p = project();
        let lamp = mark(&mut p, 1, "linterna", "Utilería", "Linterna");
        let ana = p.character_id_by_name("ANA").unwrap();
        p.characters
            .iter_mut()
            .find(|c| c.id == ana)
            .unwrap()
            .cast_number = Some(1);
        let actor = p.add_contact("Luisa Pérez");
        p.characters.iter_mut().find(|c| c.id == ana).unwrap().actor = Some(actor);
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        p.scenes.entry(ids[0]).or_default().summary = "Ana sale".into();
        // La cantidad en la escena se guarda en la marca.
        assert!(p.set_scene_quantity(lamp, ids[0], Some(3)));
        assert_eq!(p.scene_quantity(lamp, ids[0]), Some(3));
        assert!(
            !p.set_scene_quantity(lamp, ids[1], Some(1)),
            "no hay marcas en la otra escena"
        );
        let pages = scene_sheets(&p, Paper::A4);
        assert_eq!(pages.len(), 2, "una hoja por escena");
        let first: Vec<String> = pages[0].iter().map(|l| l.text.clone()).collect();
        let joined = first.join("\n");
        for needle in [
            "ESCENA 1",
            "INT. LIVING - NOCHE",
            "NOCHE",
            "Resumen: Ana sale",
            "ELENCO",
            "1. ANA — Luisa Pérez",
            "UTILERÍA",
            "- Linterna (x3)",
        ] {
            assert!(joined.contains(needle), "falta «{needle}» en:\n{joined}");
        }
        assert!(pages[1].iter().any(|l| l.text == "ESCENA 2"));
        assert!(!pages[1].iter().any(|l| l.text.contains("Linterna")));
        assert!(
            pages
                .iter()
                .flatten()
                .all(|l| l.y + SIZE < 842.0 && l.size == SIZE)
        );
    }

    #[test]
    fn a_scene_with_many_items_overflows_onto_another_sheet_instead_of_off_the_page() {
        let mut p = project();
        for n in 0..90 {
            mark(&mut p, 1, "linterna", "Utilería", &format!("Cosa {n}"));
        }
        let pages = scene_sheets(&p, Paper::A4);
        assert!(pages.len() >= 3, "{}", pages.len());
        assert!(pages.iter().flatten().all(|l| l.y + ROW <= 842.0));
    }

    #[test]
    fn the_plan_prints_as_colored_strips_a_list_by_day_and_a_cast_calendar() {
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        let ana = p.character_id_by_name("ANA").unwrap();
        p.characters
            .iter_mut()
            .find(|c| c.id == ana)
            .unwrap()
            .cast_number = Some(1);
        p.add_shoot_day();
        p.place_scenes(&[ids[0]], None);
        p.add_banner(None, "Mudanza a la casa de Pedro");
        p.add_shoot_day();
        p.place_scenes(&[ids[1]], None);
        p.shoot_days[0].date = "2026-11-02".into();
        let strips = strip_pages(&p, Paper::A4);
        let text: String = strips
            .iter()
            .flatten()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [
            "JORNADA 1 · 2026-11-02",
            "JORNADA 2",
            "*** Mudanza a la casa de Pedro ***",
            "INT. LIVING - NOCHE",
            "Elenco: 1",
        ] {
            assert!(text.contains(needle), "falta «{needle}» en:\n{text}");
        }
        // Cada tira lleva su rectángulo de color, el de su tipo (interior de noche = azul).
        let swatches: Vec<[u8; 3]> = strips
            .iter()
            .flatten()
            .filter_map(|l| l.swatch.map(|(_, _, c)| c))
            .collect();
        assert_eq!(swatches.len(), 2);
        assert_eq!(swatches[0], soften(p.strip_colors[2]));
        assert_eq!(swatches[1], soften(p.strip_colors[1]));
        let list: String = day_plan_pages(&p, Paper::A4)
            .iter()
            .flatten()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            list.contains("PLAN DE RODAJE")
                && list.contains("1 escenas")
                && list.contains("0 emplazamientos"),
            "{list}"
        );
        let csv = cast_calendar_csv(&p);
        assert!(
            csv.starts_with("N°;Personaje;Intérprete;2026-11-02;D2;Trabaja;En espera\n"),
            "{csv}"
        );
        assert!(csv.contains("1;ANA;;SWF;;1;0\n"), "{csv}");
        let calendar: String = cast_calendar_pages(&p, Paper::A4)
            .iter()
            .flatten()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            calendar.contains("ANA") && calendar.contains("SWF"),
            "{calendar}"
        );
        for pages in [
            strips,
            day_plan_pages(&p, Paper::A4),
            cast_calendar_pages(&p, Paper::A4),
        ] {
            assert!(
                crate::pdf::render(&pages, 595.0, 842.0, "Plan", "")
                    .unwrap()
                    .starts_with(b"%PDF-")
            );
        }
    }

    #[test]
    fn the_call_sheet_prints_every_block_and_the_actor_sheet_only_what_concerns_them() {
        use crate::schedule::CastCall;
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        let day = p.add_shoot_day();
        p.place_scenes(&[ids[0], ids[1]], None);
        let ana = p.character_id_by_name("ANA").unwrap();
        let luisa = p.add_contact("Luisa");
        p.characters.iter_mut().find(|c| c.id == ana).unwrap().actor = Some(luisa);
        let place = p.add_shoot_location("Casa de Pedro");
        p.shoot_locations[0].address = "Calle 1".into();
        p.scenes.entry(ids[0]).or_default().shoot_location = Some(place);
        let shoot = p.shoot_days.iter_mut().find(|d| d.id == day).unwrap();
        shoot.date = "2026-12-21".into();
        shoot.call_time = "07:00".into();
        shoot.forecast = "Soleado".into();
        shoot.cast_calls.insert(
            ana,
            CastCall {
                call: "08:00".into(),
                makeup: "07:30".into(),
                set: "09:00".into(),
            },
        );
        let sheet = crate::callsheet::build(&p, day).unwrap();
        let text = |pages: Vec<Vec<Line>>| {
            pages
                .iter()
                .flatten()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let full = text(call_sheet_pages(&sheet, Paper::A4));
        for needle in [
            "HOJA DE LLAMADO · EL NIDO",
            "JORNADA 1 DE 1 · 2026-12-21",
            "Citación general 07:00",
            "Pronóstico: Soleado",
            "Casa de Pedro — Calle 1",
            "ESCENAS",
            "ELENCO",
            "ANA — Luisa",
            "citación 08:00 · maquillaje 07:30 · al set 09:00",
        ] {
            assert!(full.contains(needle), "falta «{needle}» en:\n{full}");
        }
        let mine = text(actor_sheet_pages(&sheet, ana, Paper::A4));
        assert!(
            mine.contains("Para: ANA — Luisa")
                && mine.contains("Citación: 08:00")
                && mine.contains("Maquillaje: 07:30 · Al set: 09:00"),
            "{mine}"
        );
        assert!(
            mine.contains("INT. LIVING - NOCHE") && !mine.contains("EXT. CALLE"),
            "solo las escenas de ella: {mine}"
        );
        assert!(!mine.contains("EQUIPO") && !mine.contains("REQUERIMIENTOS"));
        // Una hoja por cada persona del elenco del día.
        assert_eq!(
            all_actor_sheets(&sheet, Paper::A4).len(),
            sheet.cast.len().max(1)
        );
        for pages in [
            call_sheet_pages(&sheet, Paper::A4),
            all_actor_sheets(&sheet, Paper::A4),
        ] {
            assert!(
                crate::pdf::render(&pages, 595.0, 842.0, "Llamado", "")
                    .unwrap()
                    .starts_with(b"%PDF-")
            );
        }
    }

    #[test]
    fn the_shot_list_and_the_storyboard_follow_the_montage_or_the_shooting_order() {
        use crate::model::BlockKind;
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        let blocking = p.add_blocking(ids[0]);
        let cam_a = p
            .add_block_item(blocking, BlockKind::Camera, 0.0, 0.0)
            .unwrap();
        let cam_b = p
            .add_block_item(blocking, BlockKind::Camera, 2.0, 0.0)
            .unwrap();
        let canon = p.add_camera_body("Canon", 1.6);
        p.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == cam_a)
            .unwrap()
            .body = Some(canon);
        p.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == cam_a)
            .unwrap()
            .lens_mm = 50.0;
        let mut shots = Vec::new();
        for (text, cam) in [
            ("Ana entra", cam_a),
            ("Primer plano de Ana", cam_b),
            ("Ana sale", cam_a),
        ] {
            let id = p.add_shot(ids[0], None);
            let s = p.shots.iter_mut().find(|s| s.id == id).unwrap();
            s.description = text.into();
            (s.blocking, s.camera) = (Some(blocking), Some(cam));
            shots.push(id);
        }
        p.shots[0].size = "PG".into();
        p.shots[0].support = "Trípode".into();
        p.assign_shoot_order();
        let text = |pages: Vec<Vec<Line>>| {
            pages
                .iter()
                .flatten()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let montage = text(shot_list_pages(&p, false, Paper::A4));
        let shoot = text(shot_list_pages(&p, true, Paper::A4));
        assert!(montage.contains("ORDEN DE MONTAJE") && shoot.contains("ORDEN DE RODAJE"));
        assert!(
            montage.contains("1 · PG · Trípode · 50 mm (eq. 80 mm)"),
            "{montage}"
        );
        let at = |t: &str, what: &str| {
            t.find(what)
                .unwrap_or_else(|| panic!("falta {what} en {t}"))
        };
        assert!(
            at(&montage, "Primer plano de Ana") < at(&montage, "Ana sale"),
            "montaje: como en la tabla"
        );
        assert!(
            at(&shoot, "Ana sale") < at(&shoot, "Primer plano de Ana"),
            "rodaje: juntos los de la cámara A"
        );
        let csv = shot_list_csv(&p, true);
        assert!(csv.starts_with("Orden de rodaje;Código;Plano;"));
        let rows: Vec<&str> = csv.lines().skip(1).collect();
        assert!(
            rows[1].contains("Ana sale") && rows[1].starts_with("2;3;3;1;"),
            "{rows:?}"
        );
        // El storyboard: un cuadro por plano (o solo los que tienen imagen) y un PDF válido con cada formato de hoja.
        assert_eq!(storyboard_frames(&p, false).len(), 3);
        assert!(storyboard_frames(&p, true).is_empty());
        for per_page in [2, 3, 6] {
            let pdf = crate::pdf::render_storyboard(
                &storyboard_frames(&p, false),
                per_page,
                16.0 / 9.0,
                595.0,
                842.0,
                "El nido",
                "",
            )
            .unwrap();
            assert!(pdf.starts_with(b"%PDF-"));
        }
        assert!(
            crate::pdf::render_storyboard(&[], 6, 2.39, 595.0, 842.0, "Vacío", "")
                .unwrap()
                .starts_with(b"%PDF-")
        );
    }

    #[test]
    fn a_unit_cost_is_multiplied_by_the_quantity_and_the_table_only_lists_what_has_data() {
        let mut p = project();
        let vehicles = p
            .breakdown_categories
            .iter()
            .find(|c| c.name == "Vehículos")
            .unwrap()
            .id;
        let idx = p.breakdown_item_for(vehicles, "Camioneta").unwrap();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        {
            let item = &mut p.breakdown_items[idx];
            item.quantity = 3;
            item.cost_cents = Some(1_000_00);
            item.unit_cost = true;
            // Una escena marcada a mano, que en el guion no tiene marca.
            item.extra_scenes.push(ids[0]);
        }
        assert_eq!(p.breakdown_items[idx].total_cents(), Some(3_000_00));
        p.breakdown_items[idx].unit_cost = false;
        assert_eq!(p.breakdown_items[idx].total_cents(), Some(1_000_00));
        p.breakdown_items[idx].unit_cost = true;

        let lines = breakdown_lines(&p);
        let camioneta = lines.iter().find(|l| l.name == "Camioneta").unwrap();
        assert_eq!(camioneta.total_cents, Some(3_000_00));
        assert_eq!(camioneta.scenes, vec!["1".to_string()]);
        // Las categorías sin ítems con datos no aparecen.
        assert!(lines.iter().all(|l| l.category != "Vestuario"));
        let csv = breakdown_csv(&lines);
        assert!(csv.contains("Camioneta;1;"), "{csv}");
        assert!(csv.contains("por unidad;3000,00"), "{csv}");
        // La lista de compras suma el total, no el costo de una unidad.
        p.breakdown_items[idx].status = Acquisition::Rent;
        let shopping = p.shopping_list(&[Acquisition::Rent]);
        assert_eq!(total(&shopping), 3_000_00);
    }
}
