//! Ventana para sumar una imagen al storyboard: se elige a qué escena y plano corresponde y se la carga de un
//! archivo o se la dibuja ahí mismo con un boceto rápido, con los datos del plano a la vista.

use crate::ui::{
    kit::{self, DialogWidth},
    tokens,
};
use std::path::PathBuf;

use eframe::egui::{self, Color32, FontId, Id, Key, Modifiers, RichText, Stroke, Ui, pos2, vec2};
use egui_extras::{Size, StripBuilder};

use crate::{
    icon,
    model::{ColumnKind, Project},
    pages,
    theme::{self, Palette},
    widgets,
};

/// Ancho de referencia del lienzo: los trazos se guardan en proporciones y sus grosores en este ancho, así sirven para
/// cualquier resolución.
const CANVAS_W: f32 = 640.0;
const PAPER: [u8; 4] = [250, 249, 245, 255];
const COLORS: [[u8; 3]; 5] = [
    [30, 30, 30],
    [120, 120, 120],
    [200, 50, 45],
    [40, 90, 190],
    [40, 140, 80],
];

use crate::model::{SketchStroke as Line, SketchTool as Tool};

const TOOLS: [(Tool, &str); 6] = [
    (Tool::Pen, "Lápiz"),
    (Tool::Eraser, "Borrador"),
    (Tool::Line, "Línea"),
    (Tool::Arrow, "Flecha (movimiento de cámara o de personajes)"),
    (Tool::Rect, "Rectángulo"),
    (Tool::Ellipse, "Elipse"),
];

/// Ancho fijo de la tarjeta «DATOS DEL PLANO», a la derecha del editor.
const INFO_W: f32 = 260.0;

/// Los puntos que recorre un trazo: el lápiz y el borrador siguen al mouse; las figuras se arman con dos puntos. La
/// punta de una flecha se arma con la proporción del cuadro (`aspect`) para que salga con el mismo ángulo en cualquier formato.
fn outline(line: &Line, aspect: f32) -> Vec<(f32, f32)> {
    let (Some(&a), Some(&b)) = (line.points.first(), line.points.last()) else {
        return Vec::new();
    };
    match line.tool {
        Tool::Pen | Tool::Eraser => line.points.clone(),
        Tool::Line => vec![a, b],
        Tool::Arrow => {
            let (dx, dy) = ((b.0 - a.0) * aspect, b.1 - a.1);
            if dx.hypot(dy) < 0.01 {
                return vec![a, b];
            }
            // La punta mide 6 % del alto del cuadro, abierta 28° a cada lado.
            let angle = dy.atan2(dx);
            let head = |turn: f32| {
                let t = angle + std::f32::consts::PI + turn;
                (b.0 + 0.06 * t.cos() / aspect, b.1 + 0.06 * t.sin())
            };
            vec![a, b, head(0.49), b, head(-0.49)]
        }
        Tool::Rect => vec![a, (b.0, a.1), b, (a.0, b.1), a],
        Tool::Ellipse => {
            let (cx, cy, rx, ry) = (
                (a.0 + b.0) / 2.0,
                (a.1 + b.1) / 2.0,
                (b.0 - a.0).abs() / 2.0,
                (b.1 - a.1).abs() / 2.0,
            );
            (0..=48)
                .map(|i| {
                    let t = i as f32 / 48.0 * std::f32::consts::TAU;
                    (cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect()
        }
    }
}

/// Dibuja los trazos sobre un papel en blanco de `w` × `h` píxeles.
fn rasterize(lines: &[Line], w: u32, h: u32) -> image::RgbaImage {
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba(PAPER));
    let scale = w as f32 / CANVAS_W;
    let disc = |img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, color: [u8; 4]| {
        for y in (cy - r).floor().max(0.0) as u32..=((cy + r).ceil().max(0.0) as u32).min(h - 1) {
            for x in (cx - r).floor().max(0.0) as u32..=((cx + r).ceil().max(0.0) as u32).min(w - 1)
            {
                if (x as f32 - cx).hypot(y as f32 - cy) <= r {
                    img.put_pixel(x, y, image::Rgba(color));
                }
            }
        }
    };
    for line in lines {
        let r = (line.width * scale / 2.0).max(0.6);
        let px: Vec<(f32, f32)> = outline(line, w as f32 / h as f32)
            .into_iter()
            .map(|(x, y)| (x * w as f32, y * h as f32))
            .collect();
        if let [only] = px.as_slice() {
            disc(&mut img, only.0, only.1, r, line.color);
        }
        for seg in px.windows(2) {
            let (len, step) = (
                (seg[1].0 - seg[0].0).hypot(seg[1].1 - seg[0].1),
                (r * 0.5).max(0.5),
            );
            let n = (len / step).ceil().max(1.0) as u32;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                disc(
                    &mut img,
                    seg[0].0 + (seg[1].0 - seg[0].0) * t,
                    seg[0].1 + (seg[1].1 - seg[0].1) * t,
                    r,
                    line.color,
                );
            }
        }
    }
    img
}

/// Guarda el boceto como imagen. Queda en una carpeta temporal hasta que el proyecto se guarda: ahí se copia a su
/// carpeta de recursos como cualquier otra imagen.
fn save_sketch(lines: &[Line], aspect: f32) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("scriptpendra_bocetos");
    std::fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = dir.join(format!("boceto-{stamp}.png"));
    rasterize(lines, 1280, (1280.0 / aspect).round() as u32)
        .save(&path)
        .map_err(std::io::Error::other)?;
    Ok(path)
}

/// Pone las imágenes en el plano elegido o, si no hay (plano nuevo), en un plano nuevo por imagen. Devuelve los planos
/// tocados.
pub fn assign(
    project: &mut Project,
    scene: u64,
    shot: Option<u64>,
    images: Vec<PathBuf>,
    sketch: &[Line],
) -> Vec<u64> {
    let mut touched = Vec::new();
    match shot {
        Some(id) => {
            if let (Some(image), Some(s)) = (
                images.into_iter().next(),
                project.shots.iter_mut().find(|s| s.id == id),
            ) {
                s.image = Some(image);
                s.sketch = sketch.to_vec();
                touched.push(id);
            }
        }
        None => {
            for image in images {
                let id = project.add_shot(scene, None);
                if let Some(s) = project.shots.iter_mut().find(|s| s.id == id) {
                    s.image = Some(image);
                    s.sketch = sketch.to_vec();
                }
                touched.push(id);
            }
        }
    }
    touched
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    File,
    Draw,
}

pub struct Board {
    /// Escena y plano elegidos (`None`: plano nuevo en esa escena).
    pub scene: Option<u64>,
    pub shot: Option<u64>,
    pub mode: Mode,
    files: Vec<PathBuf>,
    lines: Vec<Line>,
    current: Option<Line>,
    tool: Tool,
    color: Color32,
    width: f32,
    /// Ya se dibujó o se borró algo en esta ventana: de ahí en más no se pisa al cambiar de plano.
    touched: bool,
    /// Trazos deshechos, para poder rehacerlos.
    redo: Vec<Line>,
    /// Proporción del cuadro de la película (ancho sobre alto), la del lienzo y la de la imagen que se guarda.
    aspect: f32,
}

impl Board {
    pub fn new(project: &Project, scene: Option<u64>, shot: Option<u64>, mode: Mode) -> Self {
        let scenes = project.script_scenes();
        let scene = shot
            .and_then(|id| project.shots.iter().find(|s| s.id == id))
            .map(|s| s.scene)
            .or(scene)
            .filter(|id| scenes.iter().any(|(s, _)| s == id))
            .or_else(|| scenes.first().map(|(id, _)| *id));
        let lines = shot
            .and_then(|id| project.shots.iter().find(|s| s.id == id))
            .map(|s| s.sketch.clone())
            .unwrap_or_default();
        Self {
            scene,
            shot,
            mode,
            files: Vec::new(),
            lines,
            current: None,
            tool: Tool::Pen,
            color: Color32::from_rgb(30, 30, 30),
            width: 3.0,
            touched: false,
            redo: Vec::new(),
            aspect: project.aspect.ratio(),
        }
    }

    /// El boceto del plano elegido pasa al lienzo, siempre que no se haya empezado a dibujar otro.
    fn load_sketch(&mut self, project: &Project) {
        if !self.touched {
            self.redo.clear();
            self.lines = self
                .shot
                .and_then(|id| project.shots.iter().find(|s| s.id == id))
                .map(|s| s.sketch.clone())
                .unwrap_or_default();
        }
    }
}

/// La ventana. Devuelve si el proyecto cambió.
pub fn dialog(
    ctx: &egui::Context,
    board: &mut Option<Board>,
    project: &mut Project,
    p: &Palette,
) -> bool {
    let Some(b) = board.as_mut() else {
        return false;
    };
    b.aspect = project.aspect.ratio();
    let scenes = project.script_scenes();
    let numbers = project.shot_numbers();
    let ready = b.scene.is_some()
        && match b.mode {
            Mode::File => !b.files.is_empty(),
            Mode::Draw => !b.lines.is_empty(),
        };
    let (mut accept, mut cancel) = (false, false);
    let closed = kit::dialog(
        ctx,
        p,
        "storyboard_board",
        "Añadir al storyboard",
        DialogWidth::Wide,
        |ui| {
            // Escena y plano, siempre arriba y en una fila de dos.
            let before = b.shot;
            selector_row(ui, p, b, project, &scenes, &numbers);
            if b.shot != before {
                b.load_sketch(project);
            }
            let active = usize::from(b.mode == Mode::Draw);
            if let Some(i) = widgets::tab_row(ui, p, &["Cargar imagen", "Boceto"], active) {
                b.mode = if i == 0 { Mode::File } else { Mode::Draw };
            }
            // Dos columnas: el editor se queda con lo que sobra y los datos del plano tienen ancho fijo.
            let gap = tokens::SP_L;
            let editor_w = (ui.available_width() - INFO_W - gap).max(160.0);
            // Las dos pestañas miden lo mismo: la ventana no cambia de tamaño al pasar de una a otra.
            let tools_h = tokens::ICON_BTN + tokens::SP_XS + tokens::SWATCH;
            let editor_h = tools_h + tokens::SP_S + editor_w / b.aspect + tokens::SP_L;
            ui.allocate_ui(vec2(ui.available_width(), editor_h), |ui| {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(gap))
                    .size(Size::exact(INFO_W))
                    .clip(true)
                    .horizontal(|mut strip| {
                        strip.cell(|ui| match b.mode {
                            Mode::File => file_area(ui, p, b),
                            Mode::Draw => canvas_area(ui, p, b),
                        });
                        strip.empty();
                        strip.cell(|ui| info_area(ui, p, b, project, &numbers));
                    });
            });
        },
        |ui| {
            let why = "Elegí una imagen o dibujá un boceto para poder añadirlo";
            if kit::primary_button(ui, p, "Añadir al storyboard", (!ready).then_some(why)).clicked()
            {
                accept = true;
            }
            if ui.button("Cancelar").clicked() {
                cancel = true;
            }
        },
    );
    if closed {
        cancel = true;
    }
    if accept {
        let (scene, shot) = (b.scene, b.shot);
        let (images, sketch) = match b.mode {
            Mode::File => (b.files.clone(), Vec::new()),
            Mode::Draw => (
                save_sketch(&b.lines, b.aspect).into_iter().collect(),
                b.lines.clone(),
            ),
        };
        *board = None;
        if let Some(scene) = scene {
            return !assign(project, scene, shot, images, &sketch).is_empty();
        }
    } else if cancel {
        *board = None;
    }
    false
}

/// Elegir de qué escena y plano es la imagen: dos desplegables en una fila.
fn selector_row(
    ui: &mut Ui,
    p: &Palette,
    b: &mut Board,
    project: &Project,
    scenes: &[(u64, String)],
    numbers: &std::collections::HashMap<u64, usize>,
) {
    if scenes.is_empty() {
        ui.label(RichText::new("Todavía no hay escenas en el guion.").color(p.dim));
        return;
    }
    let scene_numbers = project.scene_labels();
    let scene_label = |n: usize| format!("{}. {}", scene_numbers[n].1, scenes[n].1.trim());
    let current = b
        .scene
        .and_then(|id| scenes.iter().position(|(s, _)| *s == id));
    let in_scene: Vec<&crate::model::Shot> = project
        .shots
        .iter()
        .filter(|s| Some(s.scene) == b.scene)
        .collect();
    let shot_label = |s: &crate::model::Shot| {
        let hint: String = [s.size.as_str(), s.description.trim()]
            .iter()
            .filter(|t| !t.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        format!(
            "Plano {}{}",
            numbers.get(&s.id).copied().unwrap_or(0),
            if hint.is_empty() {
                String::new()
            } else {
                format!(" — {}", hint.chars().take(28).collect::<String>())
            }
        )
    };
    let shot_shown = b
        .shot
        .and_then(|id| in_scene.iter().find(|s| s.id == id))
        .map_or("Plano nuevo".to_string(), |s| shot_label(s));
    kit::field_group(ui, 2, |ui, n| {
        if n == 0 {
            kit::combo_field(
                ui,
                p,
                "Escena",
                "board_scene",
                &current.map_or(String::new(), scene_label),
                |ui| {
                    for (n, (id, _)) in scenes.iter().enumerate() {
                        if ui
                            .selectable_label(b.scene == Some(*id), scene_label(n))
                            .clicked()
                            && b.scene != Some(*id)
                        {
                            b.scene = Some(*id);
                            b.shot = None;
                        }
                    }
                },
            );
        } else {
            kit::combo_field(ui, p, "Plano", "board_shot", &shot_shown, |ui| {
                if ui
                    .selectable_label(b.shot.is_none(), "Plano nuevo")
                    .clicked()
                {
                    b.shot = None;
                }
                for s in &in_scene {
                    if ui
                        .selectable_label(b.shot == Some(s.id), shot_label(s))
                        .clicked()
                    {
                        b.shot = Some(s.id);
                    }
                }
            });
        }
    });
}

/// Los datos del plano elegido, de solo lectura, para dibujar con ellos a la vista.
fn info_area(
    ui: &mut Ui,
    p: &Palette,
    b: &Board,
    project: &Project,
    numbers: &std::collections::HashMap<u64, usize>,
) {
    ui.add(
        egui::Label::new(
            RichText::new("DATOS DEL PLANO")
                .size(tokens::FONT_SMALL)
                .color(p.dim),
        )
        .truncate(),
    );
    ui.add_space(tokens::SP_S);
    let Some(shot) = b
        .shot
        .and_then(|id| project.shots.iter().find(|s| s.id == id))
    else {
        ui.add(egui::Label::new(RichText::new("Se crea un plano nuevo en esta escena; sus datos se completan después en la tabla.").size(tokens::FONT_SMALL).color(p.dim)).wrap());
        return;
    };
    egui::Frame::new()
        .fill(p.field)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(tokens::R_CONTROL)
        .inner_margin(tokens::SP_M)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let row = |ui: &mut Ui, label: &str, value: &str| {
                if value.trim().is_empty() {
                    return;
                }
                let one_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
                ui.add(
                    egui::Label::new(RichText::new(label.to_uppercase()).size(10.0).color(p.dim))
                        .truncate(),
                );
                ui.add(
                    egui::Label::new(RichText::new(&one_line).size(13.0).color(p.text)).truncate(),
                )
                .on_hover_text(value.trim());
                ui.add_space(tokens::SP_S);
            };
            row(
                ui,
                "Plano",
                &numbers.get(&shot.id).copied().unwrap_or(0).to_string(),
            );
            for column in &project.shot_columns {
                match column.kind {
                    ColumnKind::Size => row(ui, &column.name, &shot.size),
                    ColumnKind::Movement => row(ui, &column.name, &shot.movement),
                    ColumnKind::Description => row(ui, &column.name, &shot.description),
                    ColumnKind::Dialogue => row(ui, &column.name, &shot.dialogue),
                    ColumnKind::Sound => row(ui, &column.name, &shot.sound),
                    ColumnKind::Duration => row(ui, &column.name, &shot.duration),
                    ColumnKind::Custom => row(
                        ui,
                        &column.name,
                        shot.extra.get(&column.id).map_or("", String::as_str),
                    ),
                    _ => {}
                }
            }
            if shot.image.is_some() {
                ui.add(
                    egui::Label::new(
                        RichText::new("Este plano ya tiene imagen: la nueva la reemplaza.")
                            .size(tokens::FONT_SMALL)
                            .color(p.accent),
                    )
                    .wrap(),
                );
            }
        });
}

fn file_area(ui: &mut Ui, p: &Palette, b: &mut Board) {
    let mut pick = ui
        .add_sized(
            vec2(ui.available_width(), tokens::CONTROL_H),
            egui::Button::new(format!("{}  Elegir imagen…", icon::IMAGE)),
        )
        .clicked();
    ui.add_space(tokens::SP_S);
    // La imagen se ve con la proporción del cuadro de la película, a todo el ancho del editor.
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, width / b.aspect), egui::Sense::click());
    match b.files.first() {
        Some(file) => pages::paint_cover(ui, file, rect, tokens::R_CONTROL),
        None => {
            let fill = if resp.hovered() {
                theme::mix(p.field, p.accent, 0.12)
            } else {
                p.field
            };
            ui.painter().rect(
                rect,
                tokens::R_CONTROL,
                fill,
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center() - vec2(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                icon::IMAGE,
                FontId::proportional(34.0),
                p.dim,
            );
            ui.painter().text(
                rect.center() + vec2(0.0, 20.0),
                egui::Align2::CENTER_CENTER,
                "Elegir imagen…",
                FontId::proportional(tokens::FONT_BODY),
                p.dim,
            );
        }
    }
    pick |= resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked();
    if pick {
        let dialog = rfd::FileDialog::new().add_filter("Imágenes", &["png", "jpg", "jpeg", "webp"]);
        // Con «plano nuevo» se pueden elegir varias: cada una pasa a ser un plano.
        let picked = if b.shot.is_none() {
            dialog.pick_files().unwrap_or_default()
        } else {
            dialog.pick_file().into_iter().collect()
        };
        if !picked.is_empty() {
            b.files = picked;
        }
    }
    ui.add_space(tokens::SP_XS);
    let note = match b.files.len() {
        0 => "Elegí la imagen del plano.".to_string(),
        1 => "Un clic en la imagen elige otra.".to_string(),
        n => format!("{n} imágenes: cada una se suma como un plano nuevo."),
    };
    ui.add(egui::Label::new(RichText::new(note).size(tokens::FONT_SMALL).color(p.dim)).truncate());
}

/// Botón de herramienta con su ícono dibujado a mano (el lápiz, el borrador, la línea y las dos figuras).
fn tool_button(ui: &mut Ui, p: &Palette, tool: Tool, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(tokens::ICON_BTN, tokens::ICON_BTN),
        egui::Sense::click(),
    );
    if active {
        ui.painter().rect(
            rect,
            4,
            theme::mix(p.panel, p.accent, 0.22),
            Stroke::new(1.0, p.accent),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4, p.container_hi);
    }
    let color = if active { p.accent } else { p.text };
    let stroke = Stroke::new(1.8, color);
    let c = rect.center();
    let painter = ui.painter();
    match tool {
        Tool::Pen => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                icon::NOTE_PENCIL,
                FontId::proportional(19.0),
                color,
            );
        }
        // Un borrador inclinado.
        Tool::Eraser => {
            let body = [
                pos2(c.x - 9.0, c.y + 2.0),
                pos2(c.x - 1.0, c.y - 8.0),
                pos2(c.x + 9.0, c.y - 1.0),
                pos2(c.x + 1.0, c.y + 9.0),
            ];
            painter.add(egui::Shape::closed_line(body.to_vec(), stroke));
            painter.line_segment(
                [pos2(c.x - 4.0, c.y - 3.0), pos2(c.x + 4.5, c.y + 4.0)],
                stroke,
            );
        }
        Tool::Line => {
            painter.line_segment(
                [pos2(c.x - 9.0, c.y + 8.0), pos2(c.x + 9.0, c.y - 8.0)],
                stroke,
            );
            painter.circle_filled(pos2(c.x - 9.0, c.y + 8.0), 2.2, color);
            painter.circle_filled(pos2(c.x + 9.0, c.y - 8.0), 2.2, color);
        }
        Tool::Arrow => {
            let (tail, tip) = (pos2(c.x - 9.0, c.y + 7.0), pos2(c.x + 9.0, c.y - 7.0));
            painter.line_segment([tail, tip], stroke);
            painter.line_segment([tip, pos2(tip.x - 8.0, tip.y + 1.0)], stroke);
            painter.line_segment([tip, pos2(tip.x - 1.0, tip.y + 8.0)], stroke);
        }
        Tool::Rect => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, vec2(18.0, 13.0)),
                1,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Tool::Ellipse => {
            let points: Vec<egui::Pos2> = (0..32)
                .map(|i| {
                    let t = i as f32 / 32.0 * std::f32::consts::TAU;
                    pos2(c.x + 10.0 * t.cos(), c.y + 7.0 * t.sin())
                })
                .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn canvas_area(ui: &mut Ui, p: &Palette, b: &mut Board) {
    // Primera fila: deshacer, rehacer, borrar todo, herramientas y grosor. Segunda: colores.
    let redo_keys = ui.input_mut(|i| {
        i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || i.consume_key(Modifiers::COMMAND, Key::Y)
    });
    let undo_key = !redo_keys && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z));
    ui.scope(|ui| {
        ui.horizontal_wrapped(|ui| {
            if (widgets::icon_button(
                ui,
                p,
                icon::ARROW_COUNTER_CLOCKWISE,
                "Deshacer trazo (Ctrl+Z)",
                !b.lines.is_empty(),
            )
            .clicked()
                || undo_key)
                && !b.lines.is_empty()
            {
                if let Some(line) = b.lines.pop() {
                    b.redo.push(line);
                }
                b.touched = true;
            }
            if (widgets::icon_button(
                ui,
                p,
                icon::ARROW_CLOCKWISE,
                "Rehacer trazo (Ctrl+Y)",
                !b.redo.is_empty(),
            )
            .clicked()
                || redo_keys)
                && !b.redo.is_empty()
            {
                if let Some(line) = b.redo.pop() {
                    b.lines.push(line);
                }
                b.touched = true;
            }
            if widgets::icon_button(
                ui,
                p,
                icon::TRASH,
                "Borrar todo el boceto",
                !b.lines.is_empty(),
            )
            .clicked()
            {
                b.lines.clear();
                b.redo.clear();
                b.touched = true;
            }
            ui.separator();
            for (tool, label) in TOOLS {
                if tool_button(ui, p, tool, b.tool == tool)
                    .on_hover_text(label)
                    .clicked()
                {
                    b.tool = tool;
                }
            }
            ui.separator();
            let delta = widgets::stepper(
                ui,
                p,
                &format!("{} px", b.width as u32),
                b.width > 1.0,
                b.width < 24.0,
            );
            b.width = (b.width + delta as f32).clamp(1.0, 24.0);
        });
        ui.horizontal(|ui| {
            // Colores de uso rápido y un selector para cualquier otro.
            for c in COLORS {
                let color = Color32::from_rgb(c[0], c[1], c[2]);
                let (rect, resp) = ui.allocate_exact_size(
                    vec2(tokens::SWATCH, tokens::SWATCH),
                    egui::Sense::click(),
                );
                ui.painter()
                    .circle_filled(rect.center(), tokens::SWATCH * 0.36, color);
                if b.color == color && b.tool != Tool::Eraser {
                    ui.painter().circle_stroke(
                        rect.center(),
                        tokens::SWATCH * 0.47,
                        Stroke::new(1.5, p.accent),
                    );
                }
                if resp
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    b.color = color;
                    if b.tool == Tool::Eraser {
                        b.tool = Tool::Pen;
                    }
                }
            }
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut b.color,
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
                && b.tool == Tool::Eraser
            {
                b.tool = Tool::Pen;
            }
        });
    });
    ui.add_space(tokens::SP_S);

    // El lienzo tiene la proporción del cuadro de la película y ocupa todo el ancho del editor.
    let cw = ui.available_width();
    let ch = cw / b.aspect;
    let (rect, _) = ui.allocate_exact_size(vec2(cw, ch), egui::Sense::hover());
    let resp = ui.interact(
        rect,
        Id::unique("sketch_canvas"),
        egui::Sense::click_and_drag(),
    );
    let to_unit = |pos: egui::Pos2| ((pos.x - rect.min.x) / cw, (pos.y - rect.min.y) / ch);
    let clamp = |(x, y): (f32, f32)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
    if let Some(pos) = resp.interact_pointer_pos() {
        let point = clamp(to_unit(pos));
        if resp.drag_started() || (resp.clicked() && b.current.is_none()) {
            let color = if b.tool == Tool::Eraser {
                PAPER
            } else {
                [b.color.r(), b.color.g(), b.color.b(), 255]
            };
            let width = if b.tool == Tool::Eraser {
                b.width * 3.0
            } else {
                b.width
            };
            b.current = Some(Line {
                tool: b.tool,
                points: vec![point],
                color,
                width,
            });
        } else if let Some(line) = b.current.as_mut() {
            match line.tool {
                Tool::Pen | Tool::Eraser => line.points.push(point),
                _ => {
                    line.points.truncate(1);
                    line.points.push(point);
                }
            }
        }
    }
    if resp.drag_stopped() || (resp.clicked() && b.current.is_some()) {
        if let Some(line) = b.current.take() {
            b.lines.push(line);
            b.redo.clear();
            b.touched = true;
        }
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0, Color32::from_rgb(PAPER[0], PAPER[1], PAPER[2]));
    for line in b.lines.iter().chain(b.current.iter()) {
        let points: Vec<egui::Pos2> = outline(line, b.aspect)
            .into_iter()
            .map(|(x, y)| pos2(rect.min.x + x * cw, rect.min.y + y * ch))
            .collect();
        let scale = cw / CANVAS_W;
        let stroke = Stroke::new(
            line.width * scale,
            Color32::from_rgb(line.color[0], line.color[1], line.color[2]),
        );
        match points.as_slice() {
            [only] => {
                painter.circle_filled(*only, line.width * scale / 2.0, stroke.color);
            }
            _ => {
                painter.add(egui::Shape::line(points.clone(), stroke));
                // Puntas redondas, como el lápiz de verdad.
                for end in [points.first(), points.last()].into_iter().flatten() {
                    painter.circle_filled(*end, line.width * scale / 2.0, stroke.color);
                }
            }
        }
    }
    painter.rect_stroke(
        rect,
        0,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(tool: Tool, points: &[(f32, f32)]) -> Line {
        Line {
            tool,
            points: points.to_vec(),
            color: [200, 50, 45, 255],
            width: 8.0,
        }
    }

    #[test]
    fn a_sketch_is_rasterized_with_its_strokes_over_the_paper() {
        let img = rasterize(&[line(Tool::Line, &[(0.1, 0.5), (0.9, 0.5)])], 640, 360);
        assert_eq!(img.get_pixel(5, 5).0, PAPER); // el papel queda limpio
        assert_eq!(img.get_pixel(320, 180).0, [200, 50, 45, 255]); // el trazo pasa por el medio
        // El borrador pinta del color del papel encima de lo anterior.
        let erased = rasterize(
            &[
                line(Tool::Line, &[(0.1, 0.5), (0.9, 0.5)]),
                Line {
                    tool: Tool::Eraser,
                    points: vec![(0.5, 0.5)],
                    color: PAPER,
                    width: 30.0,
                },
            ],
            640,
            360,
        );
        assert_eq!(erased.get_pixel(320, 180).0, PAPER);
        // Un rectángulo cierra sus cuatro lados.
        let rect = outline(&line(Tool::Rect, &[(0.2, 0.2), (0.8, 0.6)]), 16.0 / 9.0);
        assert_eq!(
            (rect.first(), rect.last()),
            (Some(&(0.2, 0.2)), Some(&(0.2, 0.2)))
        );
    }

    #[test]
    fn the_image_goes_to_the_chosen_shot_or_to_new_shots_of_the_chosen_scene() {
        use crate::model::ParagraphKind::SceneHeading;
        let mut project = Project::default();
        project.paragraphs = vec![
            project.make_paragraph(SceneHeading, "INT. CASA - DÍA"),
            project.make_paragraph(SceneHeading, "EXT. FARO - NOCHE"),
        ];
        let (house, lighthouse) = (project.paragraphs[0].id, project.paragraphs[1].id);
        let existing = project.add_shot(lighthouse, None);
        // A un plano que ya existe: lo pone ahí y no crea nada.
        assert_eq!(
            assign(
                &mut project,
                lighthouse,
                Some(existing),
                vec!["a.png".into(), "b.png".into()],
                &[]
            ),
            vec![existing]
        );
        assert_eq!(project.shots.len(), 1);
        assert_eq!(project.shots[0].image, Some("a.png".into()));
        // Plano nuevo: uno por imagen, en la escena elegida.
        let made = assign(
            &mut project,
            house,
            None,
            vec!["c.png".into(), "d.png".into()],
            &[],
        );
        assert_eq!(made.len(), 2);
        assert!(project.shots.iter().filter(|s| s.scene == house).count() == 2);
        // La ventana arranca en la escena del plano o, sin plano, en la indicada.
        let board = Board::new(&project, Some(house), Some(existing), Mode::Draw);
        assert_eq!(board.scene, Some(lighthouse));
        assert_eq!(
            Board::new(&project, None, None, Mode::File).scene,
            Some(house)
        );
    }

    #[test]
    fn a_sketch_keeps_its_strokes_so_it_can_be_edited_and_a_file_replaces_them() {
        use crate::model::ParagraphKind::SceneHeading;
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let shot = project.add_shot(scene, None);
        let strokes = vec![
            line(Tool::Line, &[(0.1, 0.5), (0.9, 0.5)]),
            line(Tool::Rect, &[(0.2, 0.2), (0.8, 0.6)]),
        ];
        assign(
            &mut project,
            scene,
            Some(shot),
            vec!["boceto.png".into()],
            &strokes,
        );
        assert_eq!(project.shots[0].sketch, strokes);

        // Al volver a abrirlo, el lienzo trae lo que había.
        let mut board = Board::new(&project, None, Some(shot), Mode::Draw);
        assert_eq!(board.lines, strokes);
        // Cambiar a un plano nuevo sin haber tocado nada deja el lienzo vacío; habiendo dibujado, lo conserva.
        board.shot = None;
        board.load_sketch(&project);
        assert!(board.lines.is_empty());
        board.shot = Some(shot);
        board.load_sketch(&project);
        board.touched = true;
        board.lines.pop();
        board.shot = None;
        board.load_sketch(&project);
        assert_eq!(board.lines.len(), 1);

        // Los trazos se guardan con el proyecto y vuelven al abrirlo.
        let json = serde_json::to_string(&project).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.shots[0].sketch, strokes);

        // Una imagen de archivo ya no es el boceto: se olvidan los trazos.
        assign(
            &mut project,
            scene,
            Some(shot),
            vec!["foto.png".into()],
            &[],
        );
        assert!(project.shots[0].sketch.is_empty());
    }

    #[test]
    fn the_window_keeps_its_size_and_place_across_tabs_and_shots() {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let p = crate::storage::Settings::default().scheme.palette();
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let short = project.add_shot(scene, None);
        let long = project.add_shot(scene, None);
        project.shots[1].description =
            "Una descripción larguísima que ocupa varias líneas del panel de datos. ".repeat(12);
        project.shots[1].dialogue =
            "Y un diálogo también largo para llenar el panel de la derecha. ".repeat(8);
        let third = project.add_shot(scene, None);
        project.shots[2].image = Some("/no/existe.png".into());
        project.shots[2].size = "PG".into();
        let mut board = Some(Board::new(&project, None, Some(short), Mode::File));
        let rect_after = |board: &mut Option<Board>, project: &mut Project| {
            let mut last = None;
            for n in 0..10 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1400.0, 900.0),
                    )),
                    time: Some(n as f64 * 0.1),
                    focused: true,
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    dialog(ui.ctx(), board, project, &p);
                });
                out.textures_delta.clear();
                last = Some(out);
            }
            // La ventana es el rectángulo con el fondo del panel más grande.
            fn panels(shape: &egui::epaint::Shape, color: Color32, out: &mut Vec<egui::Rect>) {
                match shape {
                    egui::epaint::Shape::Rect(r) if r.fill == color => out.push(r.rect),
                    egui::epaint::Shape::Vec(items) => {
                        items.iter().for_each(|s| panels(s, color, out))
                    }
                    _ => {}
                }
            }
            let mut found = Vec::new();
            last.unwrap()
                .shapes
                .iter()
                .for_each(|s| panels(&s.shape, p.panel, &mut found));
            found
                .into_iter()
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .expect("ventana dibujada")
        };
        let mut seen = Vec::new();
        for (mode, shot) in [
            (Mode::File, short),
            (Mode::Draw, short),
            (Mode::Draw, long),
            (Mode::File, long),
            (Mode::File, third),
            (Mode::Draw, third),
        ] {
            if let Some(b) = board.as_mut() {
                b.mode = mode;
                b.shot = Some(shot);
            }
            seen.push(rect_after(&mut board, &mut project));
        }
        // Centrada en la pantalla y sin pasar de DIALOG_WIDE.
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0));
        assert!(seen[0].width() <= tokens::DIALOG_WIDE + 2.0, "{seen:?}");
        assert!(
            (seen[0].center() - screen.center()).abs().max_elem() < 2.0,
            "{seen:?}"
        );
        for r in &seen {
            assert!(
                (r.size() - seen[0].size()).abs().max_elem() < 1.0,
                "mismo tamaño: {seen:?}"
            );
            assert!(
                (r.min - seen[0].min).abs().max_elem() < 1.0,
                "mismo lugar: {seen:?}"
            );
        }
    }

    #[test]
    fn the_shot_data_is_stacked_label_over_value() {
        use crate::model::ParagraphKind::SceneHeading;
        fn texts(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos)),
                egui::epaint::Shape::Vec(items) => items.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let p = crate::storage::Settings::default().scheme.palette();
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let shot = project.add_shot(scene, None);
        project.ensure_shot_columns();
        project.shots[0].size = "PG".into();
        project.shots[0].movement = "Fijo".into();
        project.shots[0].description = "Ana abre la puerta.".into();
        let mut board = Some(Board::new(&project, None, Some(shot), Mode::File));
        let mut found = Vec::new();
        for n in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1400.0, 900.0),
                )),
                time: Some(n as f64 * 0.1),
                focused: true,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                dialog(ui.ctx(), &mut board, &mut project, &p);
            });
            out.textures_delta.clear();
            found.clear();
            out.shapes.iter().for_each(|c| texts(&c.shape, &mut found));
        }
        let at = |text: &str| {
            found
                .iter()
                .find(|(t, _)| t == text)
                .map(|(_, pos)| *pos)
                .unwrap_or_else(|| panic!("no se dibujó «{text}»: {found:?}"))
        };
        let (plane, kind, kind_value) = (at("PLANO"), at("TIPO DE PLANO"), at("PG"));
        let (movement, movement_value) = (at("MOVIMIENTO"), at("Fijo"));
        let (description, description_value) = (at("DESCRIPCIÓN"), at("Ana abre la puerta."));
        // Cada dato va debajo de su rótulo, alineado a la izquierda, y los rótulos bajan uno tras otro.
        for (label, value) in [
            (kind, kind_value),
            (movement, movement_value),
            (description, description_value),
        ] {
            assert!(
                (value.x - label.x).abs() < 1.0,
                "alineados: {label:?} {value:?}"
            );
            assert!(
                value.y > label.y,
                "el valor va debajo del rótulo: {label:?} {value:?}"
            );
        }
        assert!(plane.y < kind.y && kind.y < movement.y && movement.y < description.y);
    }

    #[test]
    fn the_saved_image_has_the_proportion_of_the_film_frame() {
        let lines = [line(Tool::Line, &[(0.1, 0.5), (0.9, 0.5)])];
        let path = save_sketch(&lines, 2.39).unwrap();
        let (w, h) = image::image_dimensions(&path).unwrap();
        assert!((w as f32 / h as f32 - 2.39).abs() < 0.01);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_arrow_has_a_head_with_the_same_angle_in_any_frame_shape() {
        for aspect in [16.0 / 9.0, 2.39, 4.0 / 3.0] {
            let points = outline(&line(Tool::Arrow, &[(0.2, 0.7), (0.8, 0.3)]), aspect);
            assert_eq!(points.len(), 5);
            // En proporciones reales (ancho × aspecto), la punta mide 0,06 y se abre 28° de la cola.
            let real = |p: (f32, f32)| (p.0 * aspect, p.1);
            let (tip, head) = (real(points[1]), real(points[2]));
            let shaft = (real(points[0]).0 - tip.0, real(points[0]).1 - tip.1);
            let wing = (head.0 - tip.0, head.1 - tip.1);
            assert!((wing.0.hypot(wing.1) - 0.06).abs() < 1e-4);
            let angle = ((shaft.0 * wing.0 + shaft.1 * wing.1)
                / (shaft.0.hypot(shaft.1) * wing.0.hypot(wing.1)))
            .acos()
            .to_degrees();
            assert!((angle - 28.0).abs() < 0.5, "{aspect}: {angle}");
        }
        // Sin recorrido no hay punta.
        assert_eq!(
            outline(&line(Tool::Arrow, &[(0.5, 0.5), (0.5, 0.5)]), 1.78).len(),
            2
        );
        // Se dibuja en la imagen: hay tinta en la punta.
        let img = rasterize(&[line(Tool::Arrow, &[(0.1, 0.5), (0.9, 0.5)])], 640, 360);
        assert_eq!(img.get_pixel(576, 180).0, [200, 50, 45, 255]);
        assert!(
            img.get_pixel(557, 170).0 != PAPER && img.get_pixel(557, 190).0 != PAPER,
            "las dos alas de la punta"
        );
    }
}
