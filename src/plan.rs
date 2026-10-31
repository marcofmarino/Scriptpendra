//! Plan de rodaje: el tablero de tiras (una por escena) separadas por día, con la bandeja de escenas sin programar a un
//! lado, y el calendario del elenco. Las tiras se arrastran (de a una o varias seleccionadas) o se mueven con las
//! flechas. Los datos y los cálculos están en `schedule.rs`.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Color32, RichText, Sense, Stroke, Ui, vec2};

use crate::{
    icon,
    model::{Project, fmt_eighths, heading_parts},
    reports,
    schedule::{CastCode, StripEntry, fmt_minutes, is_night},
    storage::Paper,
    theme::{self, Palette},
    ui::{kit, tokens},
    widgets,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Board,
    Cast,
}

/// Ancho inicial y mínimo de la bandeja de escenas sin programar.
const PLAN_TRAY_W: f32 = 300.0;
const PLAN_TRAY_MIN: f32 = 240.0;

/// Cómo se agrupa la bandeja de escenas sin programar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrayGroup {
    #[default]
    None,
    Location,
    Character,
    TimeOfDay,
}

impl PlanState {
    /// ¿Se ve el tablero de tiras (y no el calendario del elenco)?
    pub fn is_board(&self) -> bool {
        self.tab == Tab::Board
    }
}

#[derive(Default)]
pub struct PlanState {
    tab: Tab,
    selected: HashSet<u64>,
    tray: TrayGroup,
    banner: String,
    /// El día elegido para la pestaña DÍA del inspector.
    day: Option<u64>,
    /// Ancho de la bandeja de escenas sin programar (0: el de siempre).
    tray_w: f32,
    /// Lo que se escribe en la ficha de la escena del inspector.
    drafts: HashMap<(u64, u8), String>,
    /// Escenas de la bandeja con sus planos desplegados.
    tray_open: HashSet<u64>,
    /// Días del tablero plegados (solo se ve su cabecera).
    days_collapsed: HashSet<u64>,
    /// Planos elegidos en la bandeja (se arrastran juntos).
    picked_shots: HashSet<u64>,
}

#[derive(Default)]
pub struct Out {
    pub changed: bool,
    pub status: Option<String>,
    /// Abrir esta escena en el guion.
    pub open_scene: Option<u64>,
    /// Pestaña del inspector que se pidió abrir.
    pub inspector_tab: Option<usize>,
    /// Enlace de la ficha de la escena (planta, desglose o planos).
    pub link: Option<(crate::scenes::SceneLink, u64)>,
}

/// Lo que se arrastra: escenas enteras (en el orden en que están en el plan o en el guion) o planos de una escena.
enum Drag {
    Scenes(Vec<u64>),
    Shots(u64, Vec<u64>),
}

/// Un plano de un tramo o de la bandeja, listo para dibujar.
struct ShotLine {
    id: u64,
    number: String,
    size: String,
    description: String,
    /// Duración en pantalla, y si es la de referencia porque el plano no la tiene cargada.
    seconds: String,
    guessed: bool,
}

fn shot_lines(project: &Project, ids: &[u64]) -> Vec<ShotLine> {
    let numbers = project.shot_numbers();
    ids.iter()
        .filter_map(|id| project.shots.iter().find(|s| s.id == *id))
        .map(|shot| {
            let parsed = crate::techscript::parse_seconds(&shot.duration);
            let seconds = parsed.unwrap_or(crate::schedule::DEFAULT_SHOT_SECONDS);
            ShotLine {
                id: shot.id,
                number: numbers.get(&shot.id).copied().unwrap_or(0).to_string(),
                size: shot.size.clone(),
                description: shot
                    .description
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                seconds: crate::pages::fmt_duration(seconds),
                guessed: parsed.is_none(),
            }
        })
        .collect()
}

/// Lo que hay que mostrar de una tira.
struct Strip {
    pos: usize,
    kind: Kind,
}

enum Kind {
    Scene(Box<SceneStrip>),
    Part(Box<SceneStrip>, Vec<ShotLine>),
    Banner,
}

struct SceneStrip {
    id: u64,
    /// Si es un tramo, los números de sus planos.
    shots: String,
    number: String,
    heading: String,
    eighths: String,
    place: String,
    cast: String,
    minutes: String,
    estimated: bool,
    color: Color32,
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn scene_strip(
    project: &Project,
    id: u64,
    labels: &HashMap<u64, String>,
    headings: &[(u64, String)],
) -> SceneStrip {
    let (minutes, estimated) = project.scene_minutes(id);
    let cast: Vec<String> = project
        .scene_cast_ids(id)
        .iter()
        .filter_map(|c| project.character(*c))
        .map(|c| {
            c.cast_number.map_or_else(
                || crate::model::clean_character_name(&c.name),
                |n| n.to_string(),
            )
        })
        .collect();
    SceneStrip {
        id,
        shots: String::new(),
        number: labels.get(&id).cloned().unwrap_or_default(),
        heading: headings
            .iter()
            .find(|(s, _)| *s == id)
            .map_or(String::new(), |(_, h)| h.trim().to_string()),
        eighths: fmt_eighths(project.scene_eighths(id)),
        place: project
            .scene_shoot_location(id)
            .and_then(|s| project.shoot_locations.iter().find(|l| l.id == s))
            .map_or(String::new(), |l| l.name.clone()),
        cast: cast.join(", "),
        minutes: fmt_minutes(minutes),
        estimated,
        color: rgb(project.strip_color(id)),
    }
}

/// Una acción pedida durante el dibujo, que se aplica al terminar.
#[derive(Clone)]
enum Action {
    OpenScene(u64),
    Place(Vec<u64>, Option<usize>),
    Unschedule(Vec<u64>),
    Nudge(usize, i32),
    RemoveBanner(usize),
    RemoveDay(u64),
    AddBanner(usize),
    SortDay(u64),
    PlaceShots(u64, Vec<u64>, Option<usize>),
    UnscheduleShots(Vec<u64>),
    SplitScene(u64),
    NudgeShot(usize, u64, i32),
}

/// Guarda el archivo que se genera, pidiendo dónde.
fn save(name: &str, filter: (&str, &[&str]), bytes: Vec<u8>) -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter(filter.0, filter.1)
        .set_file_name(name)
        .save_file()?;
    Some(match std::fs::write(&path, bytes) {
        Ok(()) => "Exportado".to_string(),
        Err(e) => format!("Error al exportar: {e}"),
    })
}

fn export_pdf(
    project: &Project,
    paper: Paper,
    name: &str,
    pages: Vec<Vec<crate::pdf::Line>>,
) -> Option<String> {
    let sheet = paper.sheet();
    match crate::pdf::render(
        &pages,
        sheet.width * 72.0,
        sheet.height * 72.0,
        &format!("{} — {name}", project.title),
        &project.author,
    ) {
        Ok(bytes) => save(
            &format!("{} - {name}.pdf", project.title),
            ("PDF", &["pdf"]),
            bytes,
        ),
        Err(e) => Some(format!("Error al exportar: {e}")),
    }
}

/// Herramientas de la barra: tiras o calendario del elenco, y los menús para exportar.
pub fn toolbar(
    ui: &mut Ui,
    state: &mut PlanState,
    project: &Project,
    p: &Palette,
    paper: Paper,
    end: bool,
) -> Out {
    let mut out = Out::default();
    let mut draw = |ui: &mut Ui, group: usize| match group {
        0 => {
            if let Some(i) = kit::segmented(
                ui,
                p,
                &["Tiras", "Calendario del elenco"],
                usize::from(state.tab == Tab::Cast),
            ) {
                state.tab = if i == 0 { Tab::Board } else { Tab::Cast };
            }
        }
        _ => {
            kit::export_menu(ui, |ui| {
                match state.tab {
                    Tab::Board => {
                        if ui.button("Tiras en color (PDF)").clicked() {
                            out.status = export_pdf(
                                project,
                                paper,
                                "tiras",
                                reports::strip_pages(project, paper),
                            );
                            ui.close();
                        }
                        if ui.button("Plan día por día (PDF)").clicked() {
                            out.status = export_pdf(
                                project,
                                paper,
                                "plan de rodaje",
                                reports::day_plan_pages(project, paper),
                            );
                            ui.close();
                        }
                    }
                    Tab::Cast => {
                        if ui.button("Calendario del elenco (PDF)").clicked() {
                            out.status = export_pdf(
                                project,
                                paper,
                                "calendario del elenco",
                                reports::cast_calendar_pages(project, paper),
                            );
                            ui.close();
                        }
                    }
                }
                if ui.button("Calendario del elenco (CSV)").clicked() {
                    out.status = save(
                        &format!("{} - calendario del elenco.csv", project.title),
                        ("CSV", &["csv"]),
                        reports::cast_calendar_csv(project).into_bytes(),
                    );
                    ui.close();
                }
            });
        }
    };
    // Lo propio de la pantalla va en el centro; Exportar, contra el borde derecho de la ventana.
    if end {
        draw(ui, 1);
    } else {
        kit::toolbar_groups(ui, "plan_tools", &[2], |ui, group| draw(ui, group));
    }
    out
}

/// Barra de contexto: agregar un día o un cartel, y cómo se agrupa la bandeja.
pub fn context_bar(ui: &mut Ui, state: &mut PlanState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    if state.tab != Tab::Board {
        return out;
    }
    crate::ui::shell::context_bar(ui, p, |ui| {
        if ui
            .button(format!("{}  Día", icon::PLUS))
            .on_hover_text("Agregar un día de rodaje")
            .clicked()
        {
            project.add_shoot_day();
            out.changed = true;
        }
        let width = kit::fill_width(ui, 160.0, 280.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.banner)
                .desired_width(width)
                .hint_text("Texto del cartel (Mudanza, Feriado…)"),
        );
        if ui
            .button(format!("{}  Cartel", icon::PLUS))
            .on_hover_text("Agregar un cartel al final del plan")
            .clicked()
        {
            let text = if state.banner.trim().is_empty() {
                "Cartel".to_string()
            } else {
                state.banner.trim().to_string()
            };
            project.add_banner(None, &text);
            out.changed = true;
        }
        ui.separator();
        // El día elegido en el inspector, o el primero.
        let day = state
            .day
            .or_else(|| project.days_view().first().map(|d| d.id));
        ui.add_enabled_ui(day.is_some(), |ui| {
            ui.menu_button(format!("Ordenar el día por  {}", icon::CARET_DOWN), |ui| {
                if ui.button("Lugar de rodaje").clicked() {
                    if let Some(day) = day {
                        project.sort_day_by_location(day);
                        out.changed = true;
                    }
                    ui.close();
                }
                if ui.button("Emplazamiento").clicked() {
                    if let Some(day) = day {
                        project.sort_day_by_setup(day);
                        out.changed = true;
                    }
                    ui.close();
                }
            });
        });
        ui.separator();
        ui.label(
            RichText::new("Agrupar bandeja por")
                .size(tokens::FONT_SMALL)
                .color(p.dim),
        );
        let groups = [
            (TrayGroup::None, "Orden del guion"),
            (TrayGroup::Location, "Lugar"),
            (TrayGroup::Character, "Personaje"),
            (TrayGroup::TimeOfDay, "Día/noche"),
        ];
        let labels: Vec<String> = groups.iter().map(|(_, l)| l.to_string()).collect();
        let active = groups
            .iter()
            .position(|(g, _)| *g == state.tray)
            .unwrap_or(0);
        if let Some(i) = kit::dropdown(ui, "plan_tray_group", &labels, active) {
            state.tray = groups[i].0;
        }
    });
    out
}

/// El inspector: la escena elegida en el tablero y los ajustes de la jornada y las tiras.
/// Pestaña DÍA: fecha, citación, fin, notas, totales y avisos del día elegido (un clic en el título de su jornada).
fn day_tab(ui: &mut Ui, state: &mut PlanState, project: &mut Project, p: &Palette, out: &mut Out) {
    let views = project.days_view();
    let Some(day) = state
        .day
        .and_then(|id| views.iter().find(|d| d.id == id))
        .or(views.first())
    else {
        kit::empty_state(
            ui,
            p,
            icon::LIST_CHECKS,
            "Todavía no hay días de rodaje. Agregá uno desde la barra de contexto.",
            None,
        );
        return;
    };
    let stats = project.day_stats(day);
    let warnings = project.day_warnings(day);
    ui.add(
        egui::Label::new(
            RichText::new(format!("Jornada {}", day.number))
                .font(egui::FontId::new(tokens::FONT_BODY, theme::bold_family()))
                .color(p.text),
        )
        .truncate(),
    );
    if let Some(shoot) = project.shoot_days.iter_mut().find(|d| d.id == day.id) {
        kit::section(ui, p, "Horarios", None);
        out.changed |=
            kit::format_field(ui, p, "Fecha", |ui| kit::date_field(ui, &mut shoot.date)).changed();
        kit::field_group(ui, 2, |ui, n| {
            let response = if n == 0 {
                kit::format_field(ui, p, "Citación", |ui| {
                    kit::time_field(ui, &mut shoot.call_time)
                })
            } else {
                kit::format_field(ui, p, "Fin", |ui| kit::time_field(ui, &mut shoot.wrap_time))
            };
            out.changed |= response.changed();
        });
        kit::section(ui, p, "Notas", None);
        out.changed |= kit::field(ui, p, "Notas del día", &mut shoot.notes, 3);
    }
    kit::section(ui, p, "Totales", None);
    kit::kv_row(ui, p, "Escenas", &stats.scenes.to_string());
    kit::kv_row(
        ui,
        p,
        "Páginas",
        &fmt_eighths(stats.eighths.min(u32::from(u16::MAX)) as u16),
    );
    kit::kv_row(
        ui,
        p,
        "Duración",
        &format!(
            "{}{}",
            fmt_minutes(stats.minutes),
            if stats.estimated { " (aprox.)" } else { "" }
        ),
    );
    kit::kv_row(ui, p, "Emplazamientos", &stats.setups.to_string());
    if stats.screen_seconds > 0.0 {
        kit::kv_row(
            ui,
            p,
            "En pantalla",
            &format!(
                "{}{}",
                crate::pages::fmt_duration(stats.screen_seconds),
                if stats.screen_estimated {
                    " (con 5 s de referencia por plano sin duración)"
                } else {
                    ""
                }
            ),
        );
    }
    kit::section(ui, p, "Avisos", None);
    if warnings.is_empty() {
        ui.label(RichText::new("Sin avisos.").color(p.dim));
    }
    for w in &warnings {
        ui.add(
            egui::Label::new(
                RichText::new(format!("⚠ {w}"))
                    .size(tokens::FONT_SMALL)
                    .color(p.warning),
            )
            .wrap(),
        );
    }
}

pub fn inspector(
    ui: &mut Ui,
    state: &mut PlanState,
    project: &mut Project,
    p: &Palette,
    tab: usize,
) -> Out {
    let mut out = Out::default();
    (|ui: &mut Ui| match tab {
        0 => day_tab(ui, state, project, p, &mut out),
        1 => {
            let single = (state.selected.len() == 1)
                .then(|| state.selected.iter().copied().next())
                .flatten();
            let Some((id, heading)) =
                single.and_then(|id| project.script_scenes().into_iter().find(|(s, _)| *s == id))
            else {
                kit::empty_state(
                    ui,
                    p,
                    icon::LIST_CHECKS,
                    "Elegí una sola tira para ver el resumen de la escena y sus enlaces.",
                    None,
                );
                return;
            };
            let mut scene_out = crate::scenes::Out::default();
            crate::scenes::card(
                ui,
                &mut state.drafts,
                project,
                p,
                id,
                &heading,
                &mut scene_out,
            );
            out.changed |= scene_out.changed;
            if scene_out.goto.is_some() {
                out.open_scene = scene_out.goto;
            }
            out.link = scene_out.link;
        }
        _ => settings(ui, project, p, &mut out),
    })(ui);
    out
}

pub fn show(ui: &mut Ui, state: &mut PlanState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    project.prune_schedule();
    match state.tab {
        Tab::Board => board(ui, state, project, p, &mut out),
        Tab::Cast => cast_calendar(ui, project, p),
    }
    out
}

/// Cuánto dura la jornada, cuánto se calcula por octavo y los colores de las tiras.
fn settings(ui: &mut Ui, project: &mut Project, p: &Palette, out: &mut Out) {
    kit::section(ui, p, "Jornada", None);
    kit::prop_row(ui, p, "Duración", |ui| {
        let mut hours = project.day_minutes as f32 / 60.0;
        if ui
            .add(
                egui::DragValue::new(&mut hours)
                    .range(1.0..=24.0)
                    .speed(0.1)
                    .suffix(" h")
                    .max_decimals(1),
            )
            .changed()
        {
            project.day_minutes = (hours * 60.0).round() as u32;
            out.changed = true;
        }
    });
    kit::prop_row(ui, p, "Por 1/8 de página", |ui| {
        out.changed |= ui
            .add(
                egui::DragValue::new(&mut project.minutes_per_eighth)
                    .range(1..=240)
                    .suffix(" min"),
            )
            .changed();
    });
    kit::section(ui, p, "Colores de las tiras", None);
    for (n, label) in ["INT día", "EXT día", "INT noche", "EXT noche"]
        .into_iter()
        .enumerate()
    {
        kit::prop_row(ui, p, label, |ui| {
            out.changed |= ui
                .color_edit_button_srgb(&mut project.strip_colors[n])
                .changed()
        });
    }
    if ui.button("Restablecer colores").clicked() {
        project.strip_colors = crate::schedule::DEFAULT_STRIP_COLORS;
        out.changed = true;
    }
}

// ------------------------------------------------------------------------------------------------
// Tablero
// ------------------------------------------------------------------------------------------------

fn board(ui: &mut Ui, state: &mut PlanState, project: &mut Project, p: &Palette, out: &mut Out) {
    let labels: HashMap<u64, String> = project.scene_labels().into_iter().collect();
    let headings = project.script_scenes();
    if headings.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::LIST_CHECKS,
            "Todavía no hay escenas. Escribí un encabezado en el Guion y acá se arma el plan.",
            None,
        );
        return;
    }
    let mut actions: Vec<Action> = Vec::new();
    // Dos columnas: la bandeja «Sin programar» (ancho mínimo 240, con un divisor que se arrastra) y los días.
    let size = ui.available_size();
    let tray_width = if state.tray_w > 0.0 {
        state.tray_w
    } else {
        PLAN_TRAY_W
    }
    .clamp(PLAN_TRAY_MIN, (size.x * 0.5).max(PLAN_TRAY_MIN));
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(
            vec2(tray_width, size.y),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(tray_width);
                tray(ui, state, project, p, &labels, &headings, &mut actions);
            },
        );
        // El divisor: se arrastra para cambiar el ancho de la bandeja.
        let (rect, resp) = ui.allocate_exact_size(vec2(tokens::SP_XL, size.y), Sense::drag());
        let hot = resp.hovered() || resp.dragged();
        ui.painter().line_segment(
            [rect.center_top(), rect.center_bottom()],
            Stroke::new(
                if hot { 2.0 } else { 1.0 },
                if hot { p.accent } else { p.border },
            ),
        );
        if hot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.dragged() {
            state.tray_w = (tray_width + resp.drag_delta().x)
                .clamp(PLAN_TRAY_MIN, (size.x * 0.5).max(PLAN_TRAY_MIN));
        }
        let rest = ui.available_width();
        ui.allocate_ui_with_layout(
            vec2(rest, size.y),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(rest);
                days_board(ui, state, project, p, &labels, &headings, &mut actions, out);
            },
        );
    });
    for action in actions {
        // Ir al guion no cambia el proyecto.
        out.changed |= !matches!(action, Action::OpenScene(_));
        match action {
            Action::Place(scenes, at) => {
                project.place_scenes(&scenes, at);
            }
            Action::Unschedule(scenes) => project.unschedule_scenes(&scenes),
            Action::Nudge(at, delta) => {
                project.nudge_entry(at, delta);
            }
            Action::RemoveBanner(at) => {
                if matches!(project.schedule.get(at), Some(StripEntry::Banner(_))) {
                    project.schedule.remove(at);
                }
            }
            Action::RemoveDay(id) => project.remove_shoot_day(id),
            Action::AddBanner(at) => {
                let text = if state.banner.trim().is_empty() {
                    "Cartel".to_string()
                } else {
                    state.banner.trim().to_string()
                };
                project.add_banner(Some(at), &text);
            }
            Action::SortDay(id) => project.sort_day_by_location(id),
            Action::OpenScene(id) => {
                out.open_scene = Some(id);
            }
            Action::PlaceShots(scene, shots, at) => {
                project.place_shots(scene, &shots, at);
            }
            Action::UnscheduleShots(shots) => project.unschedule_shots(&shots),
            Action::SplitScene(scene) => {
                project.split_scene_into_part(scene);
            }
            Action::NudgeShot(at, shot, delta) => {
                project.nudge_part_shot(at, shot, delta);
            }
        }
    }
}

/// Una tira (o una escena sin programar): una fila de alto `STRIP_H` con el color de INT/EXT y día/noche, y cada dato en su
/// celda con su ancho. Devuelve su respuesta de clic. `compact` (la bandeja) deja solo el número, el encabezado, los
/// octavos y el lugar de rodaje.
fn strip_row(
    ui: &mut Ui,
    p: &Palette,
    s: &SceneStrip,
    selected: bool,
    compact: bool,
) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), tokens::STRIP_H), Sense::hover());
    let ink = theme::ink_on(s.color, p);
    ui.painter().rect(
        rect,
        tokens::R_CONTROL,
        s.color,
        if selected {
            Stroke::new(2.0, p.accent)
        } else {
            Stroke::new(1.0, theme::mix(s.color, p.border, 0.5))
        },
        egui::StrokeKind::Inside,
    );
    // Los anchos de las celdas: los datos cortos tienen el suyo y el resto se reparte lo que sobra.
    const NUMBER_W: f32 = 40.0;
    const EIGHTHS_W: f32 = 56.0;
    const TIME_W: f32 = 64.0;
    const SHOTS_W: f32 = 72.0;
    let pad = tokens::SP_S;
    let inner = rect.shrink2(vec2(pad, 0.0));
    let has_shots = !s.shots.is_empty() && !compact;
    let fixed = NUMBER_W
        + EIGHTHS_W
        + if compact { 0.0 } else { TIME_W }
        + if has_shots { SHOTS_W } else { 0.0 };
    let flex = (inner.width() - fixed).max(0.0);
    let (heading_w, cast_w, place_w) = if compact {
        (flex * 0.6, 0.0, flex * 0.4)
    } else {
        (flex * 0.40, flex * 0.28, flex * 0.32)
    };
    let mut x = inner.left();
    let mut cell = |ui: &mut Ui, width: f32, text: &str, bold: bool, tip: &str| {
        let r = egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            vec2((width - tokens::SP_XS).max(0.0), tokens::STRIP_H),
        );
        x += width;
        if r.width() < 12.0 {
            return;
        }
        let text = RichText::new(if text.is_empty() { "–" } else { text })
            .color(ink)
            .font(if bold {
                egui::FontId::new(tokens::FONT_BODY, theme::bold_family())
            } else {
                egui::FontId::proportional(tokens::FONT_BODY)
            });
        let label = ui.put(r, egui::Label::new(text).truncate());
        if !tip.is_empty() {
            label.on_hover_text(tip);
        }
    };
    cell(ui, NUMBER_W, &s.number, true, "Número de escena");
    cell(ui, heading_w, &s.heading, false, &s.heading);
    if !s.shots.is_empty() && !compact {
        cell(ui, SHOTS_W, &s.shots, false, "Planos de este tramo");
    }
    cell(ui, EIGHTHS_W, &s.eighths, false, "Páginas");
    if !compact {
        let minutes = if s.estimated {
            format!("{}*", s.minutes)
        } else {
            s.minutes.clone()
        };
        cell(ui, TIME_W, &minutes, false, "Tiempo estimado");
        cell(ui, cast_w, &s.cast, false, "Elenco");
    }
    cell(ui, place_w, &s.place, false, "Lugar de rodaje");
    resp.interact(Sense::click())
}

/// Una fila de plano (en la bandeja o dentro de un tramo): número, tipo, descripción recortada y duración, esta en color
/// de aviso cuando es la de referencia. Devuelve su respuesta de clic.
fn shot_row(ui: &mut Ui, p: &Palette, line: &ShotLine, selected: bool) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::hover());
    ui.painter().rect(
        rect,
        tokens::R_CONTROL,
        if selected { p.selection } else { p.container },
        Stroke::new(1.0, theme::mix(p.container, p.border, 0.6)),
        egui::StrokeKind::Inside,
    );
    const NUMBER_W: f32 = 32.0;
    const SIZE_W: f32 = 56.0;
    const TIME_W: f32 = 48.0;
    let inner = rect.shrink2(vec2(tokens::SP_S, 0.0));
    let rest = (inner.width() - NUMBER_W - SIZE_W - TIME_W).max(0.0);
    let mut x = inner.left();
    let mut put = |ui: &mut Ui, width: f32, text: &str, color: egui::Color32, bold: bool| {
        let r = egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            vec2((width - tokens::SP_XS).max(0.0), tokens::ROW_H),
        );
        x += width;
        if r.width() < 10.0 {
            return;
        }
        let text = RichText::new(if text.is_empty() { "–" } else { text })
            .color(color)
            .font(if bold {
                egui::FontId::new(tokens::FONT_BODY, theme::bold_family())
            } else {
                egui::FontId::proportional(tokens::FONT_BODY)
            });
        ui.put(r, egui::Label::new(text).truncate());
    };
    put(ui, NUMBER_W, &line.number, p.accent, true);
    put(ui, SIZE_W, &line.size, p.text, false);
    put(ui, rest, &line.description, p.text, false);
    // La duración de referencia (el plano no la tiene cargada) va en color de aviso.
    put(
        ui,
        TIME_W,
        &line.seconds,
        if line.guessed { p.warning } else { p.dim },
        false,
    );
    if line.guessed {
        resp.clone()
            .on_hover_text("Sin duración cargada: se cuentan 5 s de referencia");
    }
    resp.interact(Sense::click())
}

/// Una tira del día con sus herramientas (subir, bajar…) al lado, no encima: así un clic en una herramienta nunca arrastra
/// la tira. Las herramientas aparecen al pasar el mouse o con la tira elegida (el lugar queda reservado). Devuelve el
/// rectángulo de la tira.
#[allow(clippy::too_many_arguments)]
fn strip_with_tools(
    ui: &mut Ui,
    p: &Palette,
    state: &mut PlanState,
    pos: usize,
    id: egui::Id,
    payload: Drag,
    strip: &SceneStrip,
    selected: bool,
    tools: &[(&str, &str, Action)],
    scene: u64,
    actions: &mut Vec<Action>,
) -> egui::Rect {
    let hover_id = id.with("hover");
    let hovered_before = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(hover_id))
        .unwrap_or(false);
    let reserved = tools.len() as f32 * (tokens::ICON_BTN + tokens::SP_XS);
    let width = (ui.available_width() - reserved).max(40.0);
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tokens::SP_XS;
        let shown = ui
            .allocate_ui(vec2(width, tokens::STRIP_H), |ui| {
                ui.dnd_drag_source(id, payload, |ui| strip_row(ui, p, strip, selected, false))
            })
            .inner;
        for (code, tip, action) in tools {
            if selected || hovered_before {
                if widgets::icon_button(ui, p, code, tip, true).clicked() {
                    actions.push(action.clone());
                }
            } else {
                ui.add_space(tokens::ICON_BTN);
            }
        }
        shown
    });
    let shown = row.inner;
    if shown.inner.clicked() {
        click_select(ui, state, scene);
    }
    let _ = pos;
    let over = ui.rect_contains_pointer(row.response.rect);
    ui.ctx().data_mut(|d| d.insert_temp(hover_id, over));
    shown.response.rect
}

/// Aplica un clic a la selección: con Ctrl se suma o se saca, sin Ctrl queda solo esa.
fn click_select(ui: &Ui, state: &mut PlanState, id: u64) {
    let ctrl = ui.input(|i| i.modifiers.command);
    if ctrl {
        if !state.selected.remove(&id) {
            state.selected.insert(id);
        }
    } else if state.selected.len() == 1 && state.selected.contains(&id) {
        state.selected.clear();
    } else {
        state.selected = HashSet::from([id]);
    }
}

fn tray(
    ui: &mut Ui,
    state: &mut PlanState,
    project: &Project,
    p: &Palette,
    labels: &HashMap<u64, String>,
    headings: &[(u64, String)],
    actions: &mut Vec<Action>,
) {
    let loose = project.unscheduled_scenes();
    // El título y sus totales son datos de una línea de totales: se unen con « · ».
    ui.add(
        egui::Label::new(
            RichText::new(format!(
                "SIN PROGRAMAR · {} escenas · {} págs.",
                loose.len(),
                fmt_eighths(project.unscheduled_eighths().min(u32::from(u16::MAX)) as u16)
            ))
            .size(tokens::FONT_SMALL)
            .color(p.dim),
        )
        .truncate(),
    );
    ui.add_space(tokens::SP_S);
    // El botón va arriba de la lista: la lista ocupa el resto del alto y nada queda por debajo de la barra de estado.
    let chosen: Vec<u64> = loose
        .iter()
        .copied()
        .filter(|s| state.selected.contains(s))
        .collect();
    if !loose.is_empty()
        && widgets::wide_button(
            ui,
            p,
            icon::PLUS,
            &if chosen.is_empty() {
                "Agregar todas al plan".to_string()
            } else {
                format!("Agregar {} al plan", chosen.len())
            },
            ui.available_width(),
        )
        .clicked()
    {
        let scenes = if chosen.is_empty() {
            loose.clone()
        } else {
            chosen
        };
        state.selected.clear();
        actions.push(Action::Place(scenes, None));
    }
    ui.add_space(tokens::SP_S);
    // Zona para soltar tiras y sacarlas del plan: se resalta solo mientras se arrastra algo.
    let zone = egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 0));
    egui::ScrollArea::vertical()
        .id_salt("plan_tray")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let (_, dropped) = ui.dnd_drop_zone::<Drag, _>(zone, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(tokens::DROP_MIN_H);
                if loose.is_empty() {
                    kit::empty_state(
                        ui,
                        p,
                        icon::LIST_CHECKS,
                        "Todas las escenas están en el plan. Arrastrá una tira hasta acá para sacarla.",
                        None,
                    );
                }
                for (title, ids) in tray_groups(project, state.tray, &loose, headings) {
                    if let Some(title) = title {
                        ui.add_space(tokens::SP_S);
                        ui.add(
                            egui::Label::new(
                                RichText::new(title)
                                    .font(egui::FontId::new(
                                        tokens::FONT_BODY,
                                        theme::bold_family(),
                                    ))
                                    .color(p.text),
                            )
                            .truncate(),
                        );
                    }
                    for id in ids {
                        let strip = scene_strip(project, id, labels, headings);
                        let selected = state.selected.contains(&id);
                        let own = project.unscheduled_shots(id);
                        let total = project.shots.iter().filter(|s| s.scene == id).count();
                        // Una escena con planos ya repartidos en otros días: su encabezado lleva solo los que quedaron
                        // sueltos, sin volver a juntar lo que ya se asignó.
                        let partial = !own.is_empty() && own.len() < total;
                        let payload = if partial {
                            Drag::Shots(id, own.clone())
                        } else {
                            Drag::Scenes(if selected {
                                loose
                                    .iter()
                                    .copied()
                                    .filter(|s| state.selected.contains(s))
                                    .collect()
                            } else {
                                vec![id]
                            })
                        };
                        let open = state.tray_open.contains(&id);
                        let mut toggled = false;
                        // El botón de desplegar va al lado de la tira, no encima: un clic en él nunca arrastra.
                        let shown = ui
                            .horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = tokens::SP_XS;
                                if own.is_empty() {
                                    ui.add_space(tokens::ICON_BTN);
                                } else {
                                    let tip = if open {
                                        "Ocultar los planos de la escena"
                                    } else {
                                        "Ver los planos de la escena para llevarlos al plan de a uno"
                                    };
                                    toggled = widgets::icon_button(
                                        ui,
                                        p,
                                        if open { icon::CARET_DOWN } else { icon::CARET_RIGHT },
                                        tip,
                                        true,
                                    )
                                    .clicked();
                                }
                                ui.dnd_drag_source(
                                    egui::Id::unique(("tray", id)),
                                    payload,
                                    |ui| strip_row(ui, p, &strip, selected, true),
                                )
                            })
                            .inner;
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(
                                egui::Id::unique(("plan_tray_card", id)),
                                shown.response.rect,
                            )
                        });
                        if shown.inner.clicked() {
                            click_select(ui, state, id);
                        }
                        shown.inner.on_hover_text(
                            "Clic: elegir (Ctrl: sumar). Arrastrala al tablero, o usá el botón de arriba.",
                        );
                        if toggled && !state.tray_open.remove(&id) {
                            state.tray_open.insert(id);
                        }
                        // Con planos sueltos, la escena se puede desplegar para llevar al plan solo algunos.
                        if !own.is_empty() {
                            if open {
                                ui.add_space(tokens::SP_XS);
                                for line in shot_lines(project, &own) {
                                    let picked = state.picked_shots.contains(&line.id);
                                    let payload = Drag::Shots(
                                        id,
                                        if picked {
                                            own.iter()
                                                .copied()
                                                .filter(|s| state.picked_shots.contains(s))
                                                .collect()
                                        } else {
                                            vec![line.id]
                                        },
                                    );
                                    let row = ui.dnd_drag_source(
                                        egui::Id::unique(("tray_shot", line.id)),
                                        payload,
                                        |ui| {
                                            ui.horizontal(|ui| {
                                                ui.add_space(tokens::SP_L);
                                                shot_row(ui, p, &line, picked)
                                            })
                                            .inner
                                        },
                                    );
                                    if row.inner.clicked() && !state.picked_shots.remove(&line.id) {
                                        state.picked_shots.insert(line.id);
                                    }
                                    row.inner.on_hover_text(
                                        "Clic: elegir. Arrastrá los planos elegidos a un día del tablero.",
                                    );
                                    ui.add_space(tokens::SP_XS);
                                }
                            }
                        }
                        ui.add_space(tokens::SP_XS);
                    }
                }
            });
            match dropped.as_deref() {
                Some(Drag::Scenes(scenes)) => actions.push(Action::Unschedule(scenes.clone())),
                Some(Drag::Shots(_, shots)) => actions.push(Action::UnscheduleShots(shots.clone())),
                None => {}
            }
        });
}

/// La bandeja partida en grupos con su título. Una escena aparece en cada grupo que le toca (con varios personajes, en
/// varios).
fn tray_groups(
    project: &Project,
    group: TrayGroup,
    loose: &[u64],
    headings: &[(u64, String)],
) -> Vec<(Option<String>, Vec<u64>)> {
    let time_of = |id: u64| {
        headings
            .iter()
            .find(|(s, _)| *s == id)
            .map_or(String::new(), |(_, h)| heading_parts(h).2)
    };
    let mut out: Vec<(Option<String>, Vec<u64>)> = Vec::new();
    let mut put = |title: String, id: u64| match out
        .iter_mut()
        .find(|(t, _)| t.as_deref() == Some(title.as_str()))
    {
        Some((_, ids)) => ids.push(id),
        None => out.push((Some(title), vec![id])),
    };
    match group {
        TrayGroup::None => return vec![(None, loose.to_vec())],
        TrayGroup::Location => {
            for &id in loose {
                put(
                    project
                        .scene_shoot_location(id)
                        .and_then(|s| project.shoot_locations.iter().find(|l| l.id == s))
                        .map_or("Sin lugar de rodaje".to_string(), |l| l.name.clone()),
                    id,
                );
            }
        }
        TrayGroup::TimeOfDay => {
            for &id in loose {
                put(
                    if is_night(&time_of(id)) {
                        "De noche".to_string()
                    } else {
                        "De día".to_string()
                    },
                    id,
                );
            }
        }
        TrayGroup::Character => {
            for &id in loose {
                let names: Vec<String> = project
                    .scene_cast_ids(id)
                    .iter()
                    .filter_map(|c| project.character(*c))
                    .map(|c| crate::model::clean_character_name(&c.name))
                    .collect();
                if names.is_empty() {
                    put("Sin personajes".to_string(), id);
                }
                for name in names {
                    put(name, id);
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[allow(clippy::too_many_arguments)]
fn days_board(
    ui: &mut Ui,
    state: &mut PlanState,
    project: &mut Project,
    p: &Palette,
    labels: &HashMap<u64, String>,
    headings: &[(u64, String)],
    actions: &mut Vec<Action>,
    out: &mut Out,
) {
    let views = project.days_view();
    egui::ScrollArea::vertical().id_salt("plan_board").auto_shrink([false; 2]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if views.is_empty() {
            ui.add_space(tokens::SP_M);
            ui.label(RichText::new("Todavía no hay días de rodaje. Agregá uno y arrastrá las escenas desde la bandeja.").color(p.dim));
        }
        for day in &views {
            let stats = project.day_stats(day);
            let warnings = project.day_warnings(day);
            // Lo que se va a dibujar de cada tira, antes de tomar prestado el proyecto para editar.
            let strips: Vec<Strip> = day
                .entries
                .iter()
                .map(|&pos| match &project.schedule[pos] {
                    StripEntry::Scene(id) => Strip { pos, kind: Kind::Scene(Box::new(scene_strip(project, *id, labels, headings))) },
                    StripEntry::Part(id, shots) => {
                        let mut strip = scene_strip(project, *id, labels, headings);
                        let numbers = project.shot_numbers();
                        strip.shots = shots.iter().filter_map(|s| numbers.get(s).map(|n| n.to_string())).collect::<Vec<_>>().join(", ");
                        strip.eighths = fmt_eighths(project.part_eighths(*id, shots).min(u32::from(u16::MAX)) as u16);
                        strip.minutes = fmt_minutes(project.part_minutes(shots));
                        strip.estimated = true;
                        Strip { pos, kind: Kind::Part(Box::new(strip), shot_lines(project, shots)) }
                    }
                    _ => Strip { pos, kind: Kind::Banner },
                })
                .collect();
            let day_scenes: Vec<u64> = strips.iter().filter_map(|s| if let Kind::Scene(sc) | Kind::Part(sc, _) = &s.kind { Some(sc.id) } else { None }).collect();
            let over = stats.minutes > project.day_minutes;

            let start = ui.cursor().min;
            let mut rects: Vec<(usize, egui::Rect)> = Vec::new();
            let collapsed = state.days_collapsed.contains(&day.id);
            let chosen_day = state.day == Some(day.id) || (state.day.is_none() && views.first().is_some_and(|d| d.id == day.id));
            // Todo el contenedor del día elige el día para el inspector. Se registra antes que lo de adentro (con el
            // rectángulo del cuadro anterior) para que los botones y las tiras sigan recibiendo sus propios clics.
            let card_key = egui::Id::unique(("day_card_rect", day.id));
            if let Some(prev) = ui.ctx().data(|d| d.get_temp::<egui::Rect>(card_key)) {
                if ui.interact(prev, egui::Id::unique(("day_card", day.id)), Sense::click()).clicked() {
                    state.day = Some(day.id);
                    out.inspector_tab = Some(0);
                }
            }
            let card = egui::Frame::new().fill(p.panel).stroke(if chosen_day { Stroke::new(2.0, p.accent) } else { Stroke::new(1.0, if warnings.is_empty() { theme::mix(p.panel, p.border, 0.6) } else { p.warning }) }).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                // Cabecera del día: «JORNADA 2», la fecha y los horarios (cada dato en su texto) y, contra el borde derecho,
                // las acciones. La fecha, la citación y el fin se editan en el inspector (pestaña DÍA).
                let (date, call, wrap) = project
                    .shoot_days
                    .iter()
                    .find(|d| d.id == day.id)
                    .map(|d| (d.date.clone(), d.call_time.clone(), d.wrap_time.clone()))
                    .unwrap_or_default();
                let hours = if call.is_empty() && wrap.is_empty() {
                    String::new()
                } else {
                    format!("{}–{}", call, wrap)
                };
                crate::ui::kit::sides(
                    ui,
                    |ui| {
                        if widgets::icon_button(
                            ui,
                            p,
                            if collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN },
                            if collapsed { "Desplegar el día" } else { "Plegar el día" },
                            true,
                        )
                        .clicked()
                            && !state.days_collapsed.remove(&day.id)
                        {
                            state.days_collapsed.insert(day.id);
                        }
                        let title = ui.add(
                            egui::Label::new(
                                RichText::new(format!("JORNADA {}", day.number))
                                    .font(egui::FontId::new(tokens::FONT_BODY, theme::bold_family()))
                                    .color(p.text),
                            )
                            .truncate()
                            .sense(Sense::click()),
                        );
                        if title
                            .on_hover_text("Ver los datos de este día en el inspector")
                            .clicked()
                        {
                            state.day = Some(day.id);
                            out.inspector_tab = Some(0);
                        }
                        for text in [&date, &hours] {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(if text.is_empty() { "–" } else { text.as_str() })
                                        .color(p.dim),
                                )
                                .truncate(),
                            );
                        }
                    },
                    |ui| {
                        if widgets::icon_button(ui, p, icon::TRASH, "Quitar el día (sus tiras pasan al anterior)", true).clicked() {
                            actions.push(Action::RemoveDay(day.id));
                        }
                        if widgets::icon_button(ui, p, icon::NOTE_PENCIL, "Agregar un cartel al final del día (por ejemplo «Mudanza»)", true).clicked() {
                            actions.push(Action::AddBanner(day.entries.last().map_or(day.at + 1, |l| l + 1)));
                        }
                        if widgets::icon_button(ui, p, icon::ARROWS_DOWN_UP, "Ordenar por lugar de rodaje (y día antes que noche)", day_scenes.len() > 1).clicked() {
                            actions.push(Action::SortDay(day.id));
                        }
                    },
                );
                let pending = project.day_pending_items(day);
                let totals = format!(
                    "{} escenas · {} págs. · {}{} · {} emplazamientos{}",
                    stats.scenes,
                    fmt_eighths(stats.eighths.min(u32::from(u16::MAX)) as u16),
                    fmt_minutes(stats.minutes),
                    if stats.estimated { " (aprox.)" } else { "" },
                    stats.setups,
                    if pending > 0 { format!(" · faltan {pending} elementos por conseguir") } else { String::new() }
                );
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(totals).size(tokens::FONT_SMALL).color(if over { p.warning } else { p.dim })).truncate());
                    if !warnings.is_empty() {
                        ui.label(RichText::new(icon::WARNING).color(p.warning))
                            .on_hover_text(warnings.join("\n"));
                    }
                });
                ui.add_space(tokens::SP_S);

                for strip in strips.iter().filter(|_| !collapsed) {
                    match &strip.kind {
                        Kind::Scene(scene) => {
                            let selected = state.selected.contains(&scene.id);
                            let payload = Drag::Scenes(if selected { day_scenes_in_plan(project).into_iter().filter(|s| state.selected.contains(s)).collect() } else { vec![scene.id] });
                            let splittable = project.shots.iter().any(|s| s.scene == scene.id);
                            let mut tools = vec![
                                (icon::UP_ARROW, "Subir", Action::Nudge(strip.pos, -1)),
                                (icon::DOWN_ARROW, "Bajar", Action::Nudge(strip.pos, 1)),
                                (icon::SCROLL, "Ir a la escena en el guion", Action::OpenScene(scene.id)),
                            ];
                            if splittable {
                                tools.insert(0, (icon::LIST_CHECKS, "Dividir en planos: elegir cuáles se filman en este día", Action::SplitScene(scene.id)));
                            }
                            let rect = strip_with_tools(ui, p, state, strip.pos, egui::Id::unique(("strip", scene.id)), payload, scene, selected, &tools, scene.id, actions);
                            rects.push((strip.pos, rect));
                        }
                        Kind::Part(scene, lines) => {
                            let selected = state.selected.contains(&scene.id);
                            let shot_ids: Vec<u64> = lines.iter().map(|l| l.id).collect();
                            let tools = [
                                (icon::UP_ARROW, "Subir", Action::Nudge(strip.pos, -1)),
                                (icon::DOWN_ARROW, "Bajar", Action::Nudge(strip.pos, 1)),
                                (icon::X, "Sacar este tramo del día: sus planos vuelven a la bandeja", Action::UnscheduleShots(shot_ids.clone())),
                            ];
                            let rect = strip_with_tools(ui, p, state, strip.pos, egui::Id::unique(("part", strip.pos)), Drag::Shots(scene.id, shot_ids.clone()), scene, selected, &tools, scene.id, actions);
                            // Los planos del tramo, en el orden en que se filman.
                            let count = lines.len();
                            for (n, line) in lines.iter().enumerate() {
                                let row = ui.horizontal(|ui| {
                                    ui.add_space(tokens::SP_L);
                                    let width = crate::ui::kit::beside(ui, 3, 0.0, 40.0);
                                    let dragged = ui.dnd_drag_source(
                                        egui::Id::unique(("part_shot", line.id)),
                                        Drag::Shots(scene.id, vec![line.id]),
                                        |ui| {
                                            ui.allocate_ui(vec2(width, tokens::ROW_H), |ui| shot_row(ui, p, line, false)).inner
                                        },
                                    );
                                    dragged.inner.on_hover_text("Arrastralo a otro día para llevarlo, o a la bandeja para sacarlo");
                                    if widgets::icon_button(ui, p, icon::UP_ARROW, "Filmarlo antes", n > 0).clicked() {
                                        actions.push(Action::NudgeShot(strip.pos, line.id, -1));
                                    }
                                    if widgets::icon_button(ui, p, icon::DOWN_ARROW, "Filmarlo después", n + 1 < count).clicked() {
                                        actions.push(Action::NudgeShot(strip.pos, line.id, 1));
                                    }
                                    if widgets::icon_button(ui, p, icon::X, "Sacarlo del día", true).clicked() {
                                        actions.push(Action::UnscheduleShots(vec![line.id]));
                                    }
                                });
                                rects.push((strip.pos, row.response.rect));
                            }
                            rects.push((strip.pos, rect));
                        }
                        Kind::Banner => {
                            let row = ui.horizontal(|ui| {
                                if let Some(StripEntry::Banner(text)) = project.schedule.get_mut(strip.pos) {
                                    out.changed |= ui.add(egui::TextEdit::singleline(text).desired_width(crate::ui::kit::beside(ui, 3, 0.0, 40.0)).font(egui::FontId::new(14.0, theme::bold_family()))).changed();
                                }
                                if widgets::icon_button(ui, p, icon::UP_ARROW, "Subir", true).clicked() {
                                    actions.push(Action::Nudge(strip.pos, -1));
                                }
                                if widgets::icon_button(ui, p, icon::DOWN_ARROW, "Bajar", true).clicked() {
                                    actions.push(Action::Nudge(strip.pos, 1));
                                }
                                if widgets::icon_button(ui, p, icon::X, "Quitar el cartel", true).clicked() {
                                    actions.push(Action::RemoveBanner(strip.pos));
                                }
                            });
                            rects.push((strip.pos, row.response.rect));
                        }
                    }
                    ui.add_space(tokens::SP_XS);
                }
                if strips.is_empty() && !collapsed {
                    // Zona para soltar: se resalta solo mientras se arrastra algo.
                    let (zone, _) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::DROP_MIN_H), Sense::hover());
                    let dragging = egui::DragAndDrop::has_payload_of_type::<Drag>(ui.ctx());
                    if dragging && ui.rect_contains_pointer(zone) {
                        ui.painter().rect_stroke(zone, tokens::R_CONTROL, Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
                    }
                    ui.put(zone, egui::Label::new(RichText::new("Arrastrá escenas hasta acá.").color(p.dim)).truncate());
                }
            });
            ui.ctx().data_mut(|d| d.insert_temp(card_key, card.response.rect));
            let area = egui::Rect::from_min_max(start, egui::pos2(ui.min_rect().right().max(start.x + 10.0), ui.cursor().min.y));
            // Soltar acá: antes de la primera tira cuyo centro queda debajo del puntero.
            let pointer = ui.input(|i| i.pointer.interact_pos());
            let slot = |y: f32| rects.iter().find(|(_, r)| y < r.center().y).map_or_else(|| day.entries.last().map_or(day.at + 1, |l| l + 1), |(pos, _)| *pos);
            let response = ui.interact(area, egui::Id::unique(("day_drop", day.id)), Sense::hover());
            #[cfg(test)]
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(("plan_day_area", day.id)), area));
            if let (Some(pos), true) = (pointer, egui::DragAndDrop::has_payload_of_type::<Drag>(ui.ctx()) && response.contains_pointer()) {
                // Una raya donde va a caer.
                let target = slot(pos.y);
                let y = rects.iter().find(|(p, _)| *p == target).map_or_else(|| rects.last().map_or(start.y + 60.0, |(_, r)| r.bottom() + 2.0), |(_, r)| r.top() - 2.0);
                ui.painter().line_segment([egui::pos2(area.left() + 12.0, y), egui::pos2(area.right() - 12.0, y)], Stroke::new(3.0, p.accent));
            }
            if let (Some(drag), Some(pos)) = (response.dnd_release_payload::<Drag>(), pointer) {
                match &*drag {
                    Drag::Scenes(scenes) => actions.push(Action::Place(scenes.clone(), Some(slot(pos.y)))),
                    Drag::Shots(scene, shots) => actions.push(Action::PlaceShots(*scene, shots.clone(), Some(slot(pos.y)))),
                }
            }
            ui.add_space(tokens::SP_M);
        }
        ui.add_space(tokens::SP_XXL);
    });
}

/// Las escenas del plan en el orden en que están.
fn day_scenes_in_plan(project: &Project) -> Vec<u64> {
    project
        .schedule
        .iter()
        .filter_map(|e| {
            if let StripEntry::Scene(id) = e {
                Some(*id)
            } else {
                None
            }
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------
// Calendario del elenco
// ------------------------------------------------------------------------------------------------

fn cast_calendar(ui: &mut Ui, project: &Project, p: &Palette) {
    let (days, rows) = project.cast_calendar();
    if rows.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::USERS_THREE,
            "Todavía no hay personajes programados. Cuando las escenas estén en el tablero, acá se ve qué días trabaja cada uno.",
            None,
        );
        return;
    }
    const NUM_W: f32 = 36.0;
    const NAME_W: f32 = 170.0;
    const ACTOR_W: f32 = 170.0;
    const DAY_W: f32 = 52.0;
    const TOTAL_W: f32 = 84.0;
    let row_h = tokens::ROW_H + 4.0;
    let total_w = NUM_W + NAME_W + ACTOR_W + DAY_W * days.len() as f32 + TOTAL_W * 2.0;
    let fill_of = |code: CastCode| match code {
        CastCode::Off => Color32::TRANSPARENT,
        CastCode::Hold => theme::mix(p.container, p.dim, 0.3),
        CastCode::Work => theme::mix(p.container, p.success, 0.45),
        CastCode::Start | CastCode::StartFinish => theme::mix(p.container, p.accent, 0.55),
        CastCode::Finish => theme::mix(p.container, p.warning, 0.5),
    };
    egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let text = |ui: &mut Ui, rect: egui::Rect, align: egui::Align2, t: &str, font: egui::FontId, color: Color32| {
            let x = match align {
                egui::Align2::CENTER_CENTER => rect.center().x,
                _ => rect.left() + tokens::SP_S,
            };
            ui.painter().with_clip_rect(rect).text(egui::pos2(x, rect.center().y), align, t, font, color);
        };
        let small = egui::FontId::proportional(tokens::FONT_SMALL);
        let body = egui::FontId::proportional(tokens::FONT_BODY);
        // Encabezado.
        let (head, _) = ui.allocate_exact_size(vec2(total_w, row_h), Sense::hover());
        ui.painter().rect_filled(head, tokens::R_CONTROL, p.container);
        let mut x = head.left();
        let mut cell = |w: f32| {
            let r = egui::Rect::from_min_size(egui::pos2(x, head.top()), vec2(w, row_h));
            x += w;
            r
        };
        text(ui, cell(NUM_W), egui::Align2::LEFT_CENTER, "N°", small.clone(), p.dim);
        text(ui, cell(NAME_W), egui::Align2::LEFT_CENTER, "Personaje", small.clone(), p.dim);
        text(ui, cell(ACTOR_W), egui::Align2::LEFT_CENTER, "Intérprete", small.clone(), p.dim);
        for day in &days {
            text(ui, cell(DAY_W), egui::Align2::CENTER_CENTER, day, small.clone(), p.dim);
        }
        text(ui, cell(TOTAL_W), egui::Align2::CENTER_CENTER, "Trabaja", small.clone(), p.dim);
        text(ui, cell(TOTAL_W), egui::Align2::CENTER_CENTER, "En espera", small.clone(), p.dim);
        // Filas, con el fondo alternado.
        for (n, row) in rows.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(total_w, row_h), Sense::hover());
            if n % 2 == 1 {
                ui.painter().rect_filled(rect, 0, theme::mix(p.bg, p.container, 0.35));
            }
            let mut x = rect.left();
            let mut cell = |w: f32| {
                let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(w, row_h));
                x += w;
                r
            };
            let num = row.number.map_or(String::new(), |n| n.to_string());
            text(ui, cell(NUM_W), egui::Align2::LEFT_CENTER, &num, body.clone(), p.accent);
            text(ui, cell(NAME_W), egui::Align2::LEFT_CENTER, &row.name, body.clone(), p.text);
            text(ui, cell(ACTOR_W), egui::Align2::LEFT_CENTER, row.actor.as_deref().unwrap_or(""), small.clone(), p.dim);
            for code in &row.codes {
                let r = cell(DAY_W);
                if *code != CastCode::Off {
                    let chip = egui::Rect::from_center_size(r.center(), vec2(DAY_W - 8.0, row_h - 8.0));
                    ui.painter().rect_filled(chip, tokens::R_CONTROL, fill_of(*code));
                    text(ui, chip, egui::Align2::CENTER_CENTER, code.code(), egui::FontId::new(tokens::FONT_SMALL, theme::bold_family()), p.text);
                }
            }
            text(ui, cell(TOTAL_W), egui::Align2::CENTER_CENTER, &row.worked.to_string(), body.clone(), p.text);
            text(ui, cell(TOTAL_W), egui::Align2::CENTER_CENTER, &row.held.to_string(), body.clone(), p.dim);
        }
        // Referencia de los códigos.
        ui.add_space(tokens::SP_M);
        ui.horizontal(|ui| {
            for (code, what) in [
                (CastCode::Start, "primer día"),
                (CastCode::Work, "trabaja"),
                (CastCode::Finish, "último día"),
                (CastCode::StartFinish, "un solo día"),
                (CastCode::Hold, "en espera"),
            ] {
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 20.0), Sense::hover());
                ui.painter().rect_filled(r, tokens::R_CONTROL, fill_of(code));
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, code.code(), egui::FontId::new(tokens::FONT_SMALL, theme::bold_family()), p.text);
                ui.label(RichText::new(what).size(tokens::FONT_SMALL).color(p.dim));
                ui.add_space(tokens::SP_S);
            }
        });
        ui.add_space(tokens::SP_L);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind::*;

    fn project() -> Project {
        let mut p = Project::default();
        p.paragraphs = [
            (SceneHeading, "INT. LIVING - DÍA"),
            (Character, "ANA"),
            (Dialogue, "Hola."),
            (SceneHeading, "EXT. CALLE - NOCHE"),
            (Action, "Llueve."),
            (SceneHeading, "INT. COCINA - DÍA"),
            (Character, "JUAN"),
            (Dialogue, "Chau."),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p.auto_link_all();
        p
    }

    fn frame(
        ctx: &egui::Context,
        state: &mut PlanState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
    ) -> Out {
        let palette = crate::storage::Settings::default().scheme.palette();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1500.0, 900.0),
            )),
            time: Some(t),
            focused: true,
            events,
            ..Default::default()
        };
        let mut out = Out::default();
        let mut o = ctx.run_ui(input, |ui| {
            toolbar(ui, state, project, &palette, Paper::A4, false);
            out = show(ui, state, project, &palette);
            context_bar(ui, state, project, &palette);
            inspector(ui, state, project, &palette, 1);
        });
        o.textures_delta.clear();
        out
    }

    #[test]
    fn the_board_and_the_cast_calendar_draw_empty_and_full() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut state = PlanState::default();
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        for n in 0..3 {
            frame(&ctx, &mut state, &mut p, vec![], n as f64 * 0.1);
        }
        p.add_shoot_day();
        p.place_scenes(&[ids[0], ids[1]], None);
        p.add_banner(None, "Mudanza");
        p.add_shoot_day();
        p.place_scenes(&[ids[2]], None);
        state.selected.insert(ids[0]);
        for group in [
            TrayGroup::None,
            TrayGroup::Location,
            TrayGroup::Character,
            TrayGroup::TimeOfDay,
        ] {
            state.tray = group;
            for tab in [Tab::Board, Tab::Cast] {
                state.tab = tab;
                for n in 0..3 {
                    frame(&ctx, &mut state, &mut p, vec![], 1.0 + n as f64 * 0.1);
                }
            }
        }
    }

    #[test]
    fn the_tray_groups_scenes_by_place_character_or_time_of_day() {
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        let headings = p.script_scenes();
        let house = p.add_shoot_location("Casa");
        p.scenes.entry(ids[0]).or_default().shoot_location = Some(house);
        p.scenes.entry(ids[2]).or_default().shoot_location = Some(house);
        let by_place = tray_groups(&p, TrayGroup::Location, &ids, &headings);
        assert_eq!(
            by_place
                .iter()
                .map(|(t, g)| (t.clone().unwrap(), g.len()))
                .collect::<Vec<_>>(),
            [
                ("Casa".to_string(), 2),
                ("Sin lugar de rodaje".to_string(), 1)
            ]
        );
        let by_time = tray_groups(&p, TrayGroup::TimeOfDay, &ids, &headings);
        assert_eq!(
            by_time
                .iter()
                .map(|(t, g)| (t.clone().unwrap(), g.len()))
                .collect::<Vec<_>>(),
            [("De día".to_string(), 2), ("De noche".to_string(), 1)]
        );
        let by_character = tray_groups(&p, TrayGroup::Character, &ids, &headings);
        assert_eq!(
            by_character
                .iter()
                .map(|(t, _)| t.clone().unwrap())
                .collect::<Vec<_>>(),
            ["ANA", "JUAN", "Sin personajes"]
        );
        assert_eq!(
            tray_groups(&p, TrayGroup::None, &ids, &headings),
            vec![(None, ids)]
        );
    }

    #[test]
    fn dragging_a_scene_from_the_tray_onto_a_day_schedules_it_and_dragging_it_back_unschedules_it()
    {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut state = PlanState::default();
        let mut p = project();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        let day = p.add_shoot_day();
        for n in 0..3 {
            frame(&ctx, &mut state, &mut p, vec![], n as f64 * 0.1);
        }
        let rect = |key: egui::Id| {
            ctx.data(|d| d.get_temp::<egui::Rect>(key))
                .expect("dibujado")
        };
        let card = rect(egui::Id::unique(("plan_tray_card", ids[1]))).center();
        let area = rect(egui::Id::unique(("plan_day_area", day)));
        let target = egui::pos2(area.center().x, area.top() + 30.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut t = 1.0;
        let mut step = |state: &mut PlanState, p: &mut Project, events: Vec<egui::Event>| {
            t += 0.1;
            frame(&ctx, state, p, events, t)
        };
        step(&mut state, &mut p, vec![egui::Event::PointerMoved(card)]);
        step(&mut state, &mut p, vec![button(card, true)]);
        step(
            &mut state,
            &mut p,
            vec![egui::Event::PointerMoved(card + vec2(0.0, 20.0))],
        );
        step(&mut state, &mut p, vec![egui::Event::PointerMoved(target)]);
        step(&mut state, &mut p, vec![button(target, false)]);
        let out = step(&mut state, &mut p, vec![]);
        let _ = out;
        assert_eq!(p.schedule.len(), 2, "{:?}", p.schedule);
        assert_eq!(p.schedule[1], StripEntry::Scene(ids[1]));
        assert!(!p.unscheduled_scenes().contains(&ids[1]));
    }

    #[test]
    fn the_board_gives_the_tray_and_the_days_a_usable_width() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut state = PlanState::default();
        let mut p = project();
        let day = p.add_shoot_day();
        for n in 0..3 {
            frame(&ctx, &mut state, &mut p, vec![], n as f64 * 0.1);
        }
        let area = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("plan_day_area", day))))
            .expect("dibujado");
        // En 1500 de ancho: la bandeja de 300 y el divisor dejan a los días más de la mitad.
        assert!(area.width() > 700.0, "los días miden {}", area.width());
        assert!(
            area.left() > PLAN_TRAY_MIN && area.left() < 500.0,
            "los días arrancan en {}",
            area.left()
        );
    }
}
