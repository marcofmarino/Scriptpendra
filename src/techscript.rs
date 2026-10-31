//! Guion técnico: la tabla de planos de cada escena (tipo de plano, movimiento, descripción, sonido, duración y la
//! imagen del storyboard) y una vista para ver el storyboard completo.
//!
//! Todo lo que se muestra sale de `Project::shots`, así que el mismo dato sirve después para exportar la tabla, el
//! storyboard o ambos juntos, y para vincular cada plano con el bloqueo de cámara y la puesta de luces.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, FontId, RichText, Stroke, TextEdit, Ui, pos2, vec2};

use crate::ui::{
    kit::{self, ColAlign, ColumnDef, TableRowKind},
    tokens,
};
use crate::{
    icon,
    model::{ColumnKind, Project, SHOT_SIZES, Shot, ShotColumn},
    pages,
    sketch::{self, Board, Mode},
    theme::{self, Palette},
    widgets,
};

/// Movimientos de cámara que se sugieren (se escriben tal cual).
const MOVEMENTS: [&str; 10] = [
    "Fijo",
    "Paneo",
    "Tilt",
    "Travelling",
    "Dolly",
    "Grúa",
    "Cámara en mano",
    "Steadicam",
    "Zoom",
    "Seguimiento",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Table,
    Storyboard,
}

#[derive(Default)]
pub struct TechState {
    pub view: View,
    /// Escenas cuya tabla está plegada.
    collapsed: HashSet<u64>,
    /// Plano seleccionado: sobre él actúan los botones de la barra.
    pub selected: Option<u64>,
    /// Pestaña del inspector que se pidió abrir (0 Plano, 1 Columnas, 2 Formato).
    pub tab_request: Option<usize>,
    /// Escena que se muestra (`None`: todas).
    scene_filter: Option<u64>,
    /// Ver los planos en el orden de rodaje en vez del de montaje.
    shoot_order_view: bool,
    /// Ancho de las viñetas del storyboard, en puntos.
    card_w: f32,
    /// Columna seleccionada en el panel de columnas.
    selected_column: Option<u64>,
    /// Configuración Completa de la tabla (por defecto, la Compacta).
    full: bool,
    /// Casilla de texto tocada por última vez: a ella va lo que se importa del guion.
    cell: Option<(u64, Field)>,
    import: Option<Import>,
    /// Ventana para sumar una imagen (o un boceto) al storyboard.
    board: Option<Board>,
    /// Plano cuya imagen se ve en grande.
    viewer: Option<u64>,
    /// Ancho del guion literario al costado.
    script_w: f32,
}

/// Las casillas a las que se puede importar texto del guion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Description,
    Dialogue,
    Sound,
    Custom(u64),
}

/// El guion literario al costado: se lee mientras se arma el guion técnico y se pasa lo elegido a la casilla tocada.
struct Import {
    /// Escena que se muestra (`None`: todo el guion).
    scene: Option<u64>,
    /// Lo último que se seleccionó, en caracteres del texto mostrado.
    selection: Option<(usize, usize)>,
    /// El último plano elegido: al cambiar, el guion pasa a la escena de ese plano.
    followed: Option<u64>,
}

impl TechState {
    /// Muestra los planos de una escena (el filtro de la barra de contexto).
    pub fn show_scene(&mut self, scene: u64) {
        self.scene_filter = Some(scene);
    }

    /// Abre la ventana del storyboard sobre una escena y, si se indica, un plano.
    pub fn open_board(
        &mut self,
        project: &Project,
        scene: Option<u64>,
        shot: Option<u64>,
        mode: Mode,
    ) {
        self.board = Some(Board::new(project, scene, shot, mode));
    }

    /// Un clic en la imagen de un plano: si ya tiene, se ve en grande; si no, se abre la ventana para cargarla.
    fn open_shot_image(&mut self, project: &Project, shot: u64) {
        let has = project
            .shots
            .iter()
            .any(|s| s.id == shot && s.image.is_some());
        if has {
            self.viewer = Some(shot);
        } else {
            self.open_board(project, None, Some(shot), Mode::File);
        }
    }

    /// Abre el guion literario al costado, en la escena de la casilla (o del plano) elegida.
    pub fn open_import(&mut self, project: &Project) {
        let shot = self.cell.map(|(id, _)| id).or(self.selected);
        let scene = shot
            .and_then(|id| project.shots.iter().find(|s| s.id == id))
            .map(|s| s.scene)
            .filter(|scene| project.script_scenes().iter().any(|(id, _)| id == scene));
        self.import = Some(Import {
            scene,
            selection: None,
            followed: self.selected,
        });
    }

    /// Muestra u oculta el guion literario al costado.
    pub fn toggle_script(&mut self, project: &Project) {
        if self.import.is_some() {
            self.import = None;
        } else {
            self.open_import(project);
        }
    }
}

/// Pedidos que salen de las filas de la tabla mientras se dibujan.
#[derive(Default)]
struct Requests {
    /// Abrir la ventana del storyboard sobre un plano.
    board: Option<u64>,
    /// Ir a una cámara de un bloqueo: (bloqueo, cámara).
    blocking: Option<(u64, u64)>,
}

/// Lo que pide la pantalla además de editar: borrar un plano (la app confirma si tiene contenido).
#[derive(Default)]
pub struct Out {
    pub changed: bool,
    pub delete: Option<u64>,
    /// Columna propia que se pidió quitar (la app confirma si tiene contenido).
    pub delete_column: Option<u64>,
    /// Ir al bloqueo de cámara de un plano: (bloqueo, cámara).
    pub open_blocking: Option<(u64, u64)>,
    /// Un mensaje breve para el aviso de abajo (exportar).
    pub status: Option<String>,
    /// Abrir el inspector (doble clic en una fila).
    pub open_inspector: bool,
}

/// La barra de herramientas del centro: la vista (Tabla o Storyboard) ya no se elige acá, son dos pantallas de Dirección técnica.
pub fn toolbar(_ui: &mut Ui, _state: &mut TechState, _project: &mut Project, _p: &Palette) -> Out {
    Out::default()
}

/// Exportar ▾, contra el borde derecho de la barra de herramientas: la tabla y el storyboard en PDF, y el CSV.
pub fn toolbar_end(ui: &mut Ui, project: &Project, paper: crate::storage::Paper) -> Out {
    let mut out = Out::default();
    kit::export_menu(
        ui,
        |ui| {
            for (label, what) in [
                ("Lista de planos (PDF)", Export::List(false)),
                ("Lista en orden de rodaje (PDF)", Export::List(true)),
                ("Lista de planos (CSV)", Export::Csv),
                (
                    "Storyboard (PDF), 2 cuadros por hoja",
                    Export::Storyboard(2),
                ),
                (
                    "Storyboard (PDF), 3 cuadros por hoja",
                    Export::Storyboard(3),
                ),
                (
                    "Storyboard (PDF), 6 cuadros por hoja",
                    Export::Storyboard(6),
                ),
            ] {
                if ui.button(label).clicked() {
                    out.status = export(project, paper, what);
                    ui.close();
                }
            }
        },
    );
    out
}

/// ¿Hay un plano elegido? Entonces la pestaña PLANO tiene pie fijo.
pub fn has_footer(state: &TechState, project: &Project) -> bool {
    state
        .selected
        .is_some_and(|id| project.shots.iter().any(|s| s.id == id))
}

/// Pie fijo de la pestaña PLANO: «Subir», «Bajar» y «Eliminar plano» (en color de peligro).
pub fn footer(ui: &mut Ui, state: &mut TechState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    let Some(id) = state.selected else {
        return out;
    };
    kit::field_group(ui, 2, |ui, n| {
        let size = vec2(ui.available_width(), tokens::CONTROL_H);
        if n == 0 {
            if ui
                .add_sized(
                    size,
                    egui::Button::new(format!("{}  Subir", icon::UP_ARROW)),
                )
                .clicked()
            {
                out.changed |= project.move_shot(id, -1);
            }
        } else if ui
            .add_sized(
                size,
                egui::Button::new(format!("{}  Bajar", icon::DOWN_ARROW)),
            )
            .clicked()
        {
            out.changed |= project.move_shot(id, 1);
        }
    });
    let size = vec2(ui.available_width(), tokens::CONTROL_H);
    let delete = egui::Button::new(
        RichText::new(format!("{}  Eliminar plano", icon::TRASH)).color(p.danger),
    );
    if ui.add_sized(size, delete).clicked() {
        out.delete = Some(id);
    }
    out
}

/// Barra de contexto del guion técnico: la escena (o todas), el orden y las acciones sobre el contenido.
pub fn context_bar(ui: &mut Ui, state: &mut TechState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    let scenes = project.script_scenes();
    let numbers = project.scene_labels();
    crate::ui::shell::context_bar(ui, p, |ui| {
        let mut labels = vec!["Todas".to_string()];
        labels
            .extend((0..scenes.len()).map(|n| format!("{}. {}", numbers[n].1, scenes[n].1.trim())));
        let active = state
            .scene_filter
            .and_then(|id| scenes.iter().position(|(s, _)| *s == id))
            .map_or(0, |n| n + 1);
        if let Some(i) = kit::stepper_select(ui, p, "tech_scene", &labels, active) {
            state.scene_filter = (i > 0).then(|| scenes[i - 1].0);
        }
        ui.separator();
        kit::toolbar_groups(ui, "tech_context", &[4, 5, 2, 3, 1], |ui, group| {
            match group {
            0 => {
                ui.label(RichText::new("Orden").size(tokens::FONT_SMALL).color(p.dim));
                if let Some(i) = kit::segmented(
                    ui,
                    p,
                    &["Montaje", "Rodaje"],
                    usize::from(state.shoot_order_view),
                ) {
                    state.shoot_order_view = i == 1;
                    // El orden de rodaje sale de agrupar los planos por cámara: sin él las dos vistas serían iguales, así que
                    // se arma al elegirlo por primera vez.
                    if i == 1 && !project.shots.iter().any(|s| s.shoot_order.is_some()) {
                        project.assign_shoot_order();
                        if !project.shot_columns.iter().any(|c| c.kind == ColumnKind::ShootOrder) {
                            project.add_builtin_column(ColumnKind::ShootOrder);
                        }
                        out.changed = true;
                    }
                }
            }
            1 => {
                // Al final de la escena del plano elegido; sin plano elegido, de la que se muestra o de la primera.
                let target = state
                    .selected
                    .and_then(|id| project.shots.iter().find(|s| s.id == id).map(|s| s.scene))
                    .filter(|scene| scenes.iter().any(|(id, _)| id == scene))
                    .or(state.scene_filter)
                    .or_else(|| scenes.first().map(|(id, _)| *id));
                if ui
                    .add_enabled(
                        target.is_some(),
                        egui::Button::new(format!("{}  Plano", icon::PLUS)),
                    )
                    .on_hover_text("Agregar un plano al final de la escena")
                    .clicked()
                {
                    if let Some(scene) = target {
                        state.selected = Some(project.add_shot(scene, None));
                        state.tab_request = Some(0);
                        out.changed = true;
                    }
                }
            }
            2 => {
                // En el storyboard, el tamaño de las viñetas.
                if state.view == View::Storyboard {
                    ui.label(RichText::new("Viñetas").size(tokens::FONT_SMALL).color(p.dim));
                    let mut w = if state.card_w > 0.0 { state.card_w } else { CARD_W };
                    ui.spacing_mut().slider_width = kit::fill_width(ui, 80.0, 140.0);
                    if ui
                        .add(egui::Slider::new(&mut w, CARD_W_RANGE.0..=CARD_W_RANGE.1).show_value(false))
                        .on_hover_text("Tamaño de las viñetas")
                        .changed()
                    {
                        state.card_w = w;
                    }
                    ui.label(
                        RichText::new(format!("{} pt", w.round() as u32))
                            .size(tokens::FONT_SMALL)
                            .color(p.dim),
                    );
                }
            }
            3 => {
                if ui
                    .button(format!("{}  Ordenar por emplazamiento", icon::LIST_NUMBERS))
                    .on_hover_text("Junta los planos de una misma cámara y los numera en el orden en que conviene filmarlos")
                    .clicked()
                {
                    project.assign_shoot_order();
                    if !project.shot_columns.iter().any(|c| c.kind == ColumnKind::ShootOrder) {
                        project.add_builtin_column(ColumnKind::ShootOrder);
                    }
                    out.changed = true;
                }
            }
            _ => {
                let (mut import, mut sketch_it) = (false, false);
                import |= widgets::icon_button(
                    ui,
                    p,
                    icon::SCROLL,
                    "Mostrar u ocultar el guion literario al costado (Ctrl+Mayús+I)",
                    true,
                )
                .clicked();
                sketch_it |= widgets::icon_button(
                    ui,
                    p,
                    icon::NOTE_PENCIL,
                    "Boceto rápido para el storyboard (Ctrl+Mayús+B)",
                    true,
                )
                .clicked();
                if import {
                    state.toggle_script(project);
                }
                if sketch_it {
                    state.open_board(project, None, state.selected, Mode::Draw);
                }
            }
        }
        });
    });
    out
}

/// Una escena para mostrar: su id (`None` para los planos cuya escena ya no está en el guion), su título y los planos.
struct Group {
    scene: Option<u64>,
    label: String,
    shots: Vec<usize>,
}

fn groups(project: &Project, filter: Option<u64>, by_shoot: bool) -> Vec<Group> {
    let scenes = project.script_scenes();
    let numbers = project.scene_labels();
    let mut out: Vec<Group> = scenes
        .iter()
        .enumerate()
        .map(|(n, (id, text))| Group {
            scene: Some(*id),
            label: if text.trim().is_empty() {
                format!("ESCENA {} — (sin encabezado)", numbers[n].1)
            } else {
                format!("ESCENA {} — {}", numbers[n].1, text.trim())
            },
            shots: (0..project.shots.len())
                .filter(|&i| project.shots[i].scene == *id)
                .collect(),
        })
        .collect();
    let orphans: Vec<usize> = (0..project.shots.len())
        .filter(|&i| !scenes.iter().any(|(id, _)| *id == project.shots[i].scene))
        .collect();
    if !orphans.is_empty() {
        out.push(Group {
            scene: None,
            label: "SIN ESCENA — la escena ya no está en el guion".into(),
            shots: orphans,
        });
    }
    if let Some(scene) = filter {
        out.retain(|g| g.scene == Some(scene));
    }
    if by_shoot {
        // Orden de rodaje: por el número de emplazamiento; los que no lo tienen van al final, como estaban.
        for g in &mut out {
            g.shots
                .sort_by_key(|&i| project.shots[i].shoot_order.unwrap_or(u32::MAX));
        }
    }
    out
}

pub fn show(ui: &mut Ui, state: &mut TechState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    project.ensure_shot_columns();
    state.selected = state
        .selected
        .filter(|id| project.shots.iter().any(|s| s.id == *id));

    if project.script_scenes().is_empty() && project.shots.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::FILM_STRIP,
            "Todavía no hay escenas. Escribí un encabezado de escena en el Guion (o importá la escaleta) y acá aparece para armar sus planos.",
            None,
        );
        return out;
    }

    if ui.ctx().input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::I,
        )
    }) {
        state.toggle_script(project);
    }
    // El guion literario va a un costado y no tapa el trabajo: se puede leer y pasar texto sin cerrar nada.
    if ui.ctx().input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::B,
        )
    }) {
        state.open_board(project, None, state.selected, Mode::Draw);
    }
    // Alt+↑, Alt+↓ y Supr sobre el plano elegido (sin escribir en ningún campo).
    if state.view == View::Table
        && let Some(id) = state.selected
        && ui.ctx().memory(|m| m.focused()).is_none()
    {
        let action = ui.ctx().input_mut(|i| {
            if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp) {
                Some(RowAction::Up)
            } else if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown) {
                Some(RowAction::Down)
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Delete) {
                Some(RowAction::Delete)
            } else {
                None
            }
        });
        if let Some(action) = action {
            apply_row_action(state, project, &mut out, id, action);
        }
    }
    let before = project.shots.clone();
    // Con el guion al costado, el espacio se parte en dos: el trabajo a la izquierda y el guion a la derecha, cada uno en su
    // propio rectángulo, para que no se pisen.
    let full = ui.available_rect_before_wrap();
    let script_w = (if state.script_w > 0.0 { state.script_w } else { 360.0 })
        .clamp(240.0, (full.width() * 0.6).max(240.0));
    let (main, side) = if state.import.is_some() {
        let split = full.right() - script_w;
        (
            egui::Rect::from_min_max(full.min, egui::pos2(split - tokens::SP_XS, full.max.y)),
            Some(egui::Rect::from_min_max(
                egui::pos2(split, full.min.y),
                full.max,
            )),
        )
    } else {
        (full, None)
    };
    ui.scope_builder(egui::UiBuilder::new().max_rect(main), |ui| {
        ui.set_clip_rect(main.intersect(ui.clip_rect()));
        match state.view {
            View::Table => table(ui, state, project, p, &mut out),
            View::Storyboard => storyboard(ui, state, project, p),
        }
    });
    if let Some(side) = side {
        // El borde se arrastra para cambiar el ancho del guion.
        let grip = egui::Rect::from_x_y_ranges(
            (side.left() - tokens::SP_XS)..=(side.left() + 2.0),
            side.y_range(),
        );
        let resp = ui
            .interact(grip, egui::Id::unique("tech_script_grip"), egui::Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if resp.dragged() {
            state.script_w = (script_w - resp.drag_delta().x).clamp(240.0, 720.0);
        }
        script_panel(ui, side, state, project, p);
    }
    sketch::dialog(ui.ctx(), &mut state.board, project, p);
    image_viewer(ui.ctx(), state, project, p);
    out.changed |= before != project.shots;
    out
}

/// La imagen de un plano en grande, con un botón para cambiarla.
fn image_viewer(ctx: &egui::Context, state: &mut TechState, project: &mut Project, p: &Palette) {
    let Some(id) = state.viewer else {
        return;
    };
    let Some(path) = project
        .shots
        .iter()
        .find(|s| s.id == id)
        .and_then(|s| s.image.clone())
    else {
        state.viewer = None;
        return;
    };
    let number = project.shot_numbers().get(&id).copied().unwrap_or(0);
    let uri = format!("file://{}", project.resolve_path(&path).display());
    let (mut change, mut close) = (false, false);
    let closed = kit::dialog(
        ctx,
        p,
        "shot_image_viewer",
        &format!("Plano {number}"),
        kit::DialogWidth::Wide,
        |ui| {
            let max = vec2(
                ui.available_width(),
                (ctx.content_rect().height() - 220.0).max(200.0),
            );
            ui.vertical_centered(|ui| {
                ui.add(
                    egui::Image::new(uri.clone())
                        .max_size(max)
                        .maintain_aspect_ratio(true)
                        .corner_radius(4),
                );
            });
        },
        |ui| {
            if ui.button("Cambiar imagen").clicked() {
                change = true;
            }
            if ui.button("Cerrar").clicked() {
                close = true;
            }
        },
    );
    if change {
        state.viewer = None;
        state.open_board(project, None, Some(id), Mode::File);
    } else if closed || close {
        state.viewer = None;
    }
}

/// Una opción de sugerencia: lo que se escribe en la celda y su nombre completo.
#[derive(Clone)]
struct Opt {
    value: String,
    full: String,
}

impl Opt {
    fn label(&self) -> String {
        if self.value == self.full {
            self.value.clone()
        } else {
            format!("{} — {}", self.value, self.full)
        }
    }
}

/// Lo que se agrega a un tipo de plano (por ejemplo «PG c/ref»).
fn modifiers() -> Vec<Opt> {
    vec![Opt {
        value: "c/ref".into(),
        full: "con referencia".into(),
    }]
}

/// Las columnas de la tabla, declaradas una sola vez (reglas §2.6): la tabla y la pestaña COLUMNAS leen este arreglo. La
/// configuración Compacta (la de por defecto) muestra las que están en `in_compact`; la Completa, las de `in_full`.
const COLUMNS: [(ColumnKind, ColumnDef); 11] = [
    (
        ColumnKind::Scene,
        ColumnDef::new("scene", "Escena", 64.0).align(ColAlign::Center),
    ),
    (
        ColumnKind::Number,
        ColumnDef::new("shot", "Plano", 64.0).align(ColAlign::Center),
    ),
    (ColumnKind::Size, ColumnDef::new("size", "Tipo", 96.0)),
    (
        ColumnKind::Movement,
        ColumnDef::new("movement", "Movimiento", 104.0),
    ),
    (
        ColumnKind::Description,
        ColumnDef::new("description", "Descripción", 240.0).rest(),
    ),
    (
        ColumnKind::Dialogue,
        ColumnDef::new("dialogue", "Diálogo", 160.0).full_only(),
    ),
    (
        ColumnKind::Sound,
        ColumnDef::new("sound", "Sonido", 120.0).full_only(),
    ),
    (
        ColumnKind::Duration,
        ColumnDef::new("duration", "Duración", 80.0).align(ColAlign::Right),
    ),
    (
        ColumnKind::Camera,
        ColumnDef::new("camera", "Cámara", 140.0),
    ),
    (
        ColumnKind::ShootOrder,
        ColumnDef::new("shoot_order", "Rodaje", 80.0)
            .full_only()
            .align(ColAlign::Right)
            .tooltip("Orden de rodaje"),
    ),
    (
        ColumnKind::Storyboard,
        ColumnDef::new("image", "Imagen", 64.0).align(ColAlign::Center),
    ),
];

/// Un texto con vida de programa: los títulos de las columnas que arma el usuario se guardan una sola vez.
pub(crate) fn interned(text: &str) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES.get_or_init(Default::default).lock().unwrap();
    if let Some(found) = names.get(text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.to_string().into_boxed_str());
    names.insert(leaked);
    leaked
}

/// La definición de una columna del proyecto. Las de siempre salen de `COLUMNS`; las que se agregan (código, lente,
/// soporte… y las propias) tienen la suya según su tipo y se ven en las dos configuraciones.
fn column_def(c: &ShotColumn) -> ColumnDef {
    if let Some((_, def)) = COLUMNS.iter().find(|(kind, _)| *kind == c.kind) {
        return *def;
    }
    let id = interned(&format!("column{}", c.id));
    let title = interned(&c.name);
    let def = ColumnDef::new(id, title, 96.0);
    match c.kind {
        ColumnKind::Code => ColumnDef::new(id, title, 64.0).align(ColAlign::Center),
        ColumnKind::Estimate => ColumnDef::new(id, title, 80.0).align(ColAlign::Right),
        ColumnKind::Support | ColumnKind::Angle => ColumnDef::new(id, title, 110.0),
        ColumnKind::InFrame => ColumnDef::new(id, title, 160.0),
        _ => def,
    }
}

/// ¿Se ve esta columna en la configuración elegida? Las agregadas se ven siempre.
fn column_shown(c: &ShotColumn, full: bool) -> bool {
    COLUMNS
        .iter()
        .find(|(kind, _)| *kind == c.kind)
        .is_none_or(|(_, def)| if full { def.in_full } else { def.in_compact })
}

/// Las columnas que se dibujan, en el orden del proyecto.
fn visible_columns(project: &Project, full: bool) -> Vec<(ShotColumn, ColumnDef)> {
    project
        .shot_columns
        .iter()
        .filter(|c| column_shown(c, full))
        .map(|c| (c.clone(), column_def(c)))
        .collect()
}

/// El ancho que pide la pantalla: en la vista Tabla, el de sus columnas (con los márgenes del contenido); si no entra junto
/// al inspector, este pasa a cajón y la tabla se lee sin desplazamiento horizontal.
pub fn content_min(state: &TechState, project: &Project) -> f32 {
    if state.view != View::Table {
        return tokens::CONTENT_MIN;
    }
    let columns = visible_columns(project, state.full);
    let widths: f32 = columns.iter().map(|(_, d)| d.min).sum();
    let gaps = columns.len().saturating_sub(1) as f32 * tokens::SP_S;
    (widths + gaps + 2.0 * tokens::CONTENT_PAD + 2.0 * tokens::SP_S).max(tokens::CONTENT_MIN)
}

/// Las sugerencias de las columnas de texto libre: las de siempre y las que ya se escribieron en otros planos.
struct Options {
    size: Vec<Opt>,
    movement: Vec<Opt>,
    mods: Vec<Opt>,
    support: Vec<Opt>,
    angle: Vec<Opt>,
}

impl Options {
    fn new(project: &Project) -> Self {
        let plain = |list: &[&str]| {
            list.iter()
                .map(|m| Opt {
                    value: m.to_string(),
                    full: m.to_string(),
                })
                .collect::<Vec<_>>()
        };
        Self {
            size: options_for(
                project,
                SHOT_SIZES
                    .iter()
                    .map(|(abbr, full)| Opt {
                        value: abbr.to_string(),
                        full: full.to_string(),
                    })
                    .collect(),
                |s| &s.size,
            ),
            movement: options_for(project, plain(&MOVEMENTS), |s| &s.movement),
            mods: modifiers(),
            support: options_for(project, plain(&SUPPORTS), |s| &s.support),
            angle: options_for(project, plain(&ANGLES), |s| &s.angle),
        }
    }
}

/// Una fila de la tabla: el encabezado de una escena, un plano o la fila para sumar uno al final de la escena.
enum TRow {
    Band(usize),
    Shot(usize, usize),
    Add(u64),
}

/// Lo que se puede hacer con un plano desde el menú de su fila o con el teclado.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowAction {
    Up,
    Down,
    Delete,
}

fn table(ui: &mut Ui, state: &mut TechState, project: &mut Project, p: &Palette, out: &mut Out) {
    let minutes = project.shot_minutes();
    let numbers = project.shot_numbers();
    let scene_numbers: HashMap<u64, String> = project.scene_labels().into_iter().collect();
    let group_list = groups(project, state.scene_filter, state.shoot_order_view);
    let mut flat: Vec<TRow> = Vec::new();
    let mut kinds: Vec<TableRowKind> = Vec::new();
    for (g, group) in group_list.iter().enumerate() {
        let open = !state.collapsed.contains(&group.scene.unwrap_or(0));
        flat.push(TRow::Band(g));
        kinds.push(TableRowKind::Group {
            title: &group.label,
            count: group.shots.len(),
            open,
        });
        if !open {
            continue;
        }
        for &i in &group.shots {
            flat.push(TRow::Shot(i, g));
            kinds.push(TableRowKind::Item);
        }
        // Al final de cada escena, la fila para sumar un plano a ella.
        if let Some(scene) = group.scene {
            flat.push(TRow::Add(scene));
            kinds.push(TableRowKind::Add("Añadir plano"));
        }
    }
    let selected = flat.iter().position(
        |r| matches!(r, TRow::Shot(i, _) if state.selected == Some(project.shots[*i].id)),
    );
    let columns = visible_columns(project, state.full);
    let defs: Vec<ColumnDef> = columns.iter().map(|(_, def)| *def).collect();
    let mut actions: Vec<(u64, RowAction)> = Vec::new();
    let options = Options::new(project);
    let selected_id = state.selected;
    let row_ids: Vec<Option<u64>> = flat
        .iter()
        .map(|r| match r {
            TRow::Shot(i, _) => Some(project.shots[*i].id),
            TRow::Band(_) | TRow::Add(_) => None,
        })
        .collect();
    let mut requests = Requests::default();
    // La casilla tocada en este cuadro: el plano y, si se puede importar texto a ella, cuál.
    let mut touched: Option<(u64, Option<Field>)> = None;

    let result = kit::data_table_with(
        ui,
        p,
        "tech_table",
        &defs,
        !state.full,
        tokens::ROW_H,
        true,
        &kinds,
        selected,
        |ui, n, def| {
            let TRow::Shot(i, g) = flat[n] else {
                return;
            };
            let Some((column, _)) = columns.iter().find(|(_, d)| d.id == def.id) else {
                return;
            };
            let id = project.shots[i].id;
            let scene_cast: Vec<(u64, String)> = group_list[g]
                .scene
                .map(|s| project.scene_cast_ids(s))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|c| {
                    project
                        .character(c)
                        .map(|c| (c.id, crate::model::clean_character_name(&c.name)))
                })
                .collect();
            let row = RowCtx {
                number: numbers.get(&id).copied().unwrap_or(0),
                scene_no: group_list[g]
                    .scene
                    .and_then(|s| scene_numbers.get(&s))
                    .cloned(),
                selected: selected_id == Some(id),
                options: &options,
                minutes: minutes.get(&id).copied(),
                scene_cast: &scene_cast,
                align: def.align,
            };
            let (hit, field) = shot_cell(ui, p, project, i, column, &row, &mut requests);
            if hit {
                touched = Some((id, field));
            }
        },
        |ui, n| {
            let Some(id) = row_ids[n] else {
                return;
            };
            for (label, action) in [
                ("Subir    Alt+↑", RowAction::Up),
                ("Bajar    Alt+↓", RowAction::Down),
                ("Eliminar plano    Supr", RowAction::Delete),
            ] {
                if ui.button(label).clicked() {
                    actions.push((id, action));
                    ui.close();
                }
            }
        },
    );

    if let Some((id, field)) = touched {
        if state.selected != Some(id) {
            state.tab_request = Some(0);
        }
        state.selected = Some(id);
        state.cell = field.map(|f| (id, f));
    }
    if let Some(id) = requests.board {
        state.open_shot_image(project, id);
    }
    out.open_blocking = requests.blocking;
    if let Some(n) = result.clicked.or(result.double_clicked)
        && let TRow::Shot(i, _) = flat[n]
    {
        let id = project.shots[i].id;
        if state.selected != Some(id) {
            state.tab_request = Some(0);
        }
        state.selected = Some(id);
        if touched.is_none() {
            state.cell = None;
        }
    }
    if result.double_clicked.is_some() {
        // El texto largo se lee y se edita en el inspector.
        state.tab_request = Some(0);
        out.open_inspector = true;
    }
    if let Some(n) = result.toggled
        && let TRow::Band(g) = flat[n]
    {
        let key = group_list[g].scene.unwrap_or(0);
        if !state.collapsed.remove(&key) {
            state.collapsed.insert(key);
        }
    }
    if let Some(n) = result.added
        && let TRow::Add(scene) = flat[n]
    {
        state.selected = Some(project.add_shot(scene, None));
        state.tab_request = Some(0);
        out.changed = true;
    }
    for (id, action) in actions {
        apply_row_action(state, project, out, id, action);
    }
}

/// Sube, baja o borra un plano.
fn apply_row_action(
    state: &mut TechState,
    project: &mut Project,
    out: &mut Out,
    id: u64,
    action: RowAction,
) {
    state.selected = Some(id);
    match action {
        RowAction::Up => out.changed |= project.move_shot(id, -1),
        RowAction::Down => out.changed |= project.move_shot(id, 1),
        RowAction::Delete => out.delete = Some(id),
    }
}

/// Opciones para sugerir en una columna: las de siempre y las que ya se escribieron en otros planos.
fn options_for(
    project: &Project,
    presets: Vec<Opt>,
    column: impl Fn(&Shot) -> &String,
) -> Vec<Opt> {
    let mut out = presets;
    for shot in &project.shots {
        let value = column(shot).trim();
        if !value.is_empty() && !value.contains("->") && !out.iter().any(|o| o.value == value) {
            out.push(Opt {
                value: value.to_string(),
                full: value.to_string(),
            });
        }
    }
    out
}

/// Sugerencias para lo que se está escribiendo: cada una es (cómo se muestra, cómo queda todo el texto).
///
/// El texto puede encadenar planos con «->» (por ejemplo «PG -> PP»): se sugiere para el último tramo. Se busca por
/// la abreviatura o por el nombre completo («primer» ofrece «PP — Primer plano»). Si el tramo ya tiene su tipo y se
/// sigue escribiendo, se ofrecen los modificadores («PG c» → «PG c/ref»).
fn suggestions(text: &str, opts: &[Opt], mods: &[Opt]) -> Vec<(String, String)> {
    let (head, rest) = match text.rfind("->") {
        Some(i) => (&text[..i + 2], &text[i + 2..]),
        None => ("", text),
    };
    let lead = rest.len() - rest.trim_start().len();
    let head = format!("{head}{}", &rest[..lead]);
    let seg = &rest[lead..];
    let typed = seg.trim().to_lowercase();
    let mut out: Vec<(String, String)> = Vec::new();
    if !seg.ends_with(' ') {
        for o in opts {
            let hit = typed.is_empty()
                || o.value.to_lowercase().starts_with(&typed)
                || o.full.to_lowercase().contains(&typed);
            if hit && o.value != seg {
                out.push((o.label(), format!("{head}{}", o.value)));
            }
        }
    }
    if out.is_empty() {
        if let Some(space) = seg.rfind(' ') {
            let (before, word) = (&seg[..=space], seg[space + 1..].to_lowercase());
            for m in mods {
                if m.value != word
                    && (word.is_empty()
                        || m.value.to_lowercase().starts_with(&word)
                        || m.full.to_lowercase().contains(&word))
                {
                    out.push((m.label(), format!("{head}{before}{}", m.value)));
                }
            }
        }
    }
    out
}

fn cell_edit(text: &mut String, id: egui::Id, locked: bool, inner: egui::Rect) -> TextEdit<'_> {
    TextEdit::multiline(text)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .lock_focus(locked)
        .desired_width(inner.width())
        .min_size(inner.size())
}

/// Celda de texto que ocupa toda la casilla, con aire alrededor y borde naranja cuando tiene el foco. Con `opts`
/// ofrece sugerencias mientras se escribe (↑↓ eligen, Tab acepta, Esc descarta), pero el texto es libre. Devuelve si
/// se la tocó (clic o foco), para seleccionar su fila.
fn text_cell(
    ui: &mut Ui,
    p: &Palette,
    id: egui::Id,
    rect: egui::Rect,
    text: &mut String,
    opts: &[Opt],
    mods: &[Opt],
    selected: bool,
) -> bool {
    let has_focus = ui.memory(|m| m.has_focus(id));
    let idx_id = id.with("suggestion");
    let dismissed_id = id.with("dismissed");
    let mut matches: Vec<(String, String)> = Vec::new();
    if has_focus && !opts.is_empty() {
        let dismissed = ui
            .data(|d| d.get_temp::<String>(dismissed_id))
            .is_some_and(|d| d == *text);
        if !dismissed {
            matches = suggestions(text, opts, mods);
        }
    }
    let mut accepted: Option<String> = None;
    // Estas celdas retienen el Tab para aceptar sugerencias; sin ninguna, el Tab pasa a la celda de al lado.
    if has_focus
        && !opts.is_empty()
        && matches.is_empty()
        && ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
    {
        let back = ui.ctx().input(|i| i.modifiers.shift);
        ui.memory_mut(|m| {
            m.move_focus(if back {
                egui::FocusDirection::Previous
            } else {
                egui::FocusDirection::Next
            })
        });
    }
    if !matches.is_empty() {
        let mut idx = ui
            .data(|d| d.get_temp::<usize>(idx_id))
            .unwrap_or(0)
            .min(matches.len() - 1);
        let key = |ui: &Ui, k: egui::Key| {
            ui.ctx()
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, k))
        };
        if key(ui, egui::Key::Escape) {
            ui.data_mut(|d| d.insert_temp(dismissed_id, text.clone()));
        } else if key(ui, egui::Key::ArrowDown) {
            idx = (idx + 1) % matches.len();
            ui.data_mut(|d| d.insert_temp(idx_id.with("scroll"), true));
        } else if key(ui, egui::Key::ArrowUp) {
            idx = (idx + matches.len() - 1) % matches.len();
            ui.data_mut(|d| d.insert_temp(idx_id.with("scroll"), true));
        } else if key(ui, egui::Key::Tab) {
            accepted = Some(matches[idx].1.clone());
        }
        ui.data_mut(|d| d.insert_temp(idx_id, idx));
    }
    if let Some(value) = &accepted {
        *text = value.clone();
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        let end = egui::text::CCursor::new(value.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ui.ctx(), id);
    }

    // Todo el casillero recibe el clic; el texto va con aire alrededor.
    let clicked = ui
        .interact(rect, id.with("cell"), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::Text)
        .clicked();
    if clicked {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let inner = rect.shrink2(vec2(tokens::SP_S, tokens::SP_XS));
    // Si el texto no entra, en la fila elegida la casilla crece hacia abajo sobre un fondo propio (así se lee aunque
    // tape lo de abajo); en las demás se recorta para no pisar a la fila siguiente.
    let height_id = id.with("height");
    let content_h = ui.data(|d| d.get_temp::<f32>(height_id)).unwrap_or(0.0);
    let expanded = selected && content_h > inner.height();
    let locked = !opts.is_empty();
    let output = if expanded {
        let mut output = None;
        egui::Area::new(id.with("expanded"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(p.field)
                    .stroke(Stroke::new(1.5, p.accent))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(inner.width());
                        output = Some(cell_edit(text, id, locked, inner).show(ui));
                    });
            });
        output.expect("el área se dibuja")
    } else {
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            cell_edit(text, id, locked, inner).show(ui)
        })
        .inner
    };
    let resp = output.response.clone();
    ui.data_mut(|d| d.insert_temp(height_id, output.galley.size().y));
    if resp.has_focus() && !expanded {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            0,
            Stroke::new(1.5, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    let popup_at = if expanded {
        pos2(rect.min.x, rect.min.y + content_h + 16.0)
    } else {
        rect.left_bottom()
    };

    // Se dibuja mientras la celda tenía el foco al empezar el cuadro, para que siga ahí al apretar sobre ella.
    if !matches.is_empty() && accepted.is_none() {
        let idx = ui
            .data(|d| d.get_temp::<usize>(idx_id))
            .unwrap_or(0)
            .min(matches.len() - 1);
        let mut chosen: Option<String> = None;
        egui::Area::new(id.with("popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.set_min_width((rect.width() - 12.0).max(160.0));
                    // Se ven todas las opciones: la lista mide siete y media filas y el resto se alcanza con scroll.
                    let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
                    let follow = ui.data_mut(|d| d.remove_temp::<bool>(idx_id.with("scroll"))).unwrap_or(false);
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("popup_scroll"))
                        .max_height(row * 7.5)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            for (n, (label, whole)) in matches.iter().enumerate() {
                                let (clicked, resp) = crate::ui::kit::suggestion_item_resp(ui, n == idx, label);
                                if clicked {
                                    chosen = Some(whole.clone());
                                }
                                if follow && n == idx {
                                    resp.scroll_to_me(None);
                                }
                            }
                        });
                });
            });
        if let Some(value) = chosen {
            *text = value;
        }
    }
    clicked || resp.has_focus()
}

/// Qué se exporta.
#[derive(Clone, Copy)]
enum Export {
    /// La lista de planos en PDF; `true`: en el orden de rodaje.
    List(bool),
    Csv,
    /// El storyboard en PDF, con tantos cuadros por hoja.
    Storyboard(usize),
}

/// Exporta y devuelve el aviso para la barra de estado (`None`: se canceló el diálogo).
fn export(project: &Project, paper: crate::storage::Paper, what: Export) -> Option<String> {
    let sheet = paper.sheet();
    let (w, h) = (sheet.width * 72.0, sheet.height * 72.0);
    let save = |name: &str, filter: (&str, &[&str]), bytes: Vec<u8>| -> Option<String> {
        let path = rfd::FileDialog::new()
            .add_filter(filter.0, filter.1)
            .set_file_name(name)
            .save_file()?;
        Some(match std::fs::write(&path, bytes) {
            Ok(()) => "Exportado".to_string(),
            Err(e) => format!("Error al exportar: {e}"),
        })
    };
    let title = &project.title;
    match what {
        Export::List(shoot_order) => {
            let pages = crate::reports::shot_list_pages(project, shoot_order, paper);
            match crate::pdf::render(&pages, w, h, &format!("{title} — planos"), &project.author)
            {
                Ok(bytes) => save(
                    &format!(
                        "{title} - {}.pdf",
                        if shoot_order {
                            "planos en orden de rodaje"
                        } else {
                            "planos"
                        }
                    ),
                    ("PDF", &["pdf"]),
                    bytes,
                ),
                Err(e) => Some(format!("Error al exportar: {e}")),
            }
        }
        Export::Csv => {
            let shoot_order = project.shots.iter().any(|s| s.shoot_order.is_some());
            save(
                &format!("{title} - planos.csv"),
                ("CSV", &["csv"]),
                crate::reports::shot_list_csv(project, shoot_order).into_bytes(),
            )
        }
        Export::Storyboard(per_page) => {
            let frames = crate::reports::storyboard_frames(project, project.storyboard_only_images);
            match crate::pdf::render_storyboard(
                &frames,
                per_page,
                project.aspect.ratio(),
                w,
                h,
                title,
                &project.author,
            ) {
                Ok(bytes) => save(
                    &format!("{title} - storyboard.pdf"),
                    ("PDF", &["pdf"]),
                    bytes,
                ),
                Err(e) => Some(format!("Error al exportar: {e}")),
            }
        }
    }
}

fn export_buttons(
    ui: &mut Ui,
    p: &Palette,
    project: &Project,
    paper: crate::storage::Paper,
    out: &mut Out,
) {
    for (label, what) in [
        ("Lista de planos (PDF)", Export::List(false)),
        ("Lista en orden de rodaje (PDF)", Export::List(true)),
    ] {
        if widgets::wide_button(ui, p, icon::EXPORT, label, ui.available_width()).clicked() {
            out.status = export(project, paper, what);
        }
        ui.add_space(tokens::SP_XS);
    }
    if widgets::wide_button(
        ui,
        p,
        icon::DOWNLOAD_SIMPLE,
        "Lista de planos (CSV)",
        ui.available_width(),
    )
    .clicked()
    {
        out.status = export(project, paper, Export::Csv);
    }
    ui.add_space(tokens::SP_S);
    ui.label(
        RichText::new("Storyboard (PDF), cuadros por hoja")
            .size(12.0)
            .color(p.dim),
    );
    ui.horizontal(|ui| {
        for per_page in [2usize, 3, 6] {
            if ui.button(per_page.to_string()).clicked() {
                out.status = export(project, paper, Export::Storyboard(per_page));
            }
        }
    });
}

/// Lo que una fila necesita saber de afuera.
struct RowCtx<'a> {
    number: usize,
    scene_no: Option<String>,
    selected: bool,
    options: &'a Options,
    minutes: Option<u32>,
    /// Los personajes de la escena del plano, entre los que se elige quién está en cuadro.
    scene_cast: &'a [(u64, String)],
    align: ColAlign,
}

/// Un texto editable que ocupa la casilla (una línea, con sugerencias si hay `opts`).
#[allow(clippy::too_many_arguments)]
fn edit_cell(
    ui: &mut Ui,
    p: &Palette,
    key: (&str, u64),
    rect: egui::Rect,
    text: &mut String,
    opts: &[Opt],
    mods: &[Opt],
    row: &RowCtx,
) -> bool {
    let hit = text_cell(
        ui,
        p,
        egui::Id::unique(("shot_cell_text", key.0, key.1)),
        rect,
        text,
        opts,
        mods,
        row.selected,
    );
    hit
}

/// Un texto de solo lectura que ocupa la casilla; un clic selecciona la fila.
fn read_cell(
    ui: &mut Ui,
    p: &Palette,
    rect: egui::Rect,
    key: (&str, u64),
    text: &str,
    color: egui::Color32,
    row: &RowCtx,
) -> bool {
    let shown = if text.trim().is_empty() { "–" } else { text };
    let color = if text.trim().is_empty() { p.dim } else { color };
    let inner = rect.shrink2(vec2(tokens::SP_S, 0.0));
    let (anchor, align) = match row.align {
        ColAlign::Left => (inner.left_center(), egui::Align2::LEFT_CENTER),
        ColAlign::Right => (inner.right_center(), egui::Align2::RIGHT_CENTER),
        ColAlign::Center => (inner.center(), egui::Align2::CENTER_CENTER),
    };
    ui.painter().with_clip_rect(rect).text(
        anchor,
        align,
        shown,
        FontId::proportional(tokens::FONT_BODY),
        color,
    );
    ui.interact(
        rect,
        egui::Id::unique(("shot_cell_read", key.0, key.1)),
        egui::Sense::click(),
    )
    .clicked()
}

/// Una casilla de texto con una flecha tenue que se abre con un clic en una lista de opciones (sin controles con borde).
fn menu_cell(
    ui: &mut Ui,
    p: &Palette,
    key: (&str, u64),
    rect: egui::Rect,
    text: &str,
    menu: impl FnOnce(&mut Ui),
) -> bool {
    let resp = ui
        .interact(
            rect,
            egui::Id::unique(("shot_cell_menu", key.0, key.1)),
            egui::Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let inner = rect.shrink2(vec2(tokens::SP_S, 0.0));
    let painter = ui.painter().with_clip_rect(rect);
    let caret = painter.text(
        inner.right_center(),
        egui::Align2::RIGHT_CENTER,
        icon::CARET_DOWN,
        FontId::proportional(tokens::FONT_SMALL),
        p.dim,
    );
    let empty = text.trim().is_empty();
    painter
        .with_clip_rect(egui::Rect::from_min_max(
            inner.min,
            pos2(caret.left() - tokens::SP_XS, inner.max.y),
        ))
        .text(
            inner.left_center(),
            egui::Align2::LEFT_CENTER,
            if empty { "–" } else { text },
            FontId::proportional(tokens::FONT_BODY),
            if empty { p.dim } else { p.text },
        );
    egui::Popup::menu(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(menu);
    resp.clicked()
}

/// Una casilla de la fila de un plano. Devuelve si se la tocó (para seleccionar la fila) y, en tal caso, qué casilla
/// importable se tocó.
fn shot_cell(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    i: usize,
    column: &ShotColumn,
    row: &RowCtx,
    req: &mut Requests,
) -> (bool, Option<Field>) {
    let id = project.shots[i].id;
    // El casillero entero, hasta los bordes de la columna.
    let rect = ui.max_rect().expand2(vec2(tokens::ROW_PAD_X, 0.0));
    let o = row.options;
    let mut touched = false;
    let mut field: Option<Field> = None;
    match column.kind {
        ColumnKind::Scene => {
            touched |= read_cell(
                ui,
                p,
                rect,
                ("scene", id),
                row.scene_no.as_deref().unwrap_or(""),
                p.text,
                row,
            )
        }
        ColumnKind::Number | ColumnKind::Code => {
            touched |= read_cell(
                ui,
                p,
                rect,
                ("number", id),
                &row.number.to_string(),
                p.accent,
                row,
            )
        }
        ColumnKind::Estimate => {
            touched |= read_cell(
                ui,
                p,
                rect,
                ("estimate", id),
                &row.minutes.map_or(String::new(), |m| format!("{m} min")),
                p.dim,
                row,
            )
        }
        ColumnKind::ShootOrder => {
            touched |= read_cell(
                ui,
                p,
                rect,
                ("order", id),
                &project.shots[i]
                    .shoot_order
                    .map_or(String::new(), |n| n.to_string()),
                p.text,
                row,
            )
        }
        ColumnKind::Size => {
            touched |= edit_cell(
                ui,
                p,
                ("size", id),
                rect,
                &mut project.shots[i].size,
                &o.size,
                &o.mods,
                row,
            )
        }
        ColumnKind::Movement => {
            touched |= edit_cell(
                ui,
                p,
                ("move", id),
                rect,
                &mut project.shots[i].movement,
                &o.movement,
                &[],
                row,
            )
        }
        ColumnKind::Support => {
            touched |= edit_cell(
                ui,
                p,
                ("support", id),
                rect,
                &mut project.shots[i].support,
                &o.support,
                &[],
                row,
            )
        }
        ColumnKind::Angle => {
            touched |= edit_cell(
                ui,
                p,
                ("angle", id),
                rect,
                &mut project.shots[i].angle,
                &o.angle,
                &[],
                row,
            )
        }
        ColumnKind::Duration => {
            touched |= edit_cell(
                ui,
                p,
                ("duration", id),
                rect,
                &mut project.shots[i].duration,
                &[],
                &[],
                row,
            )
        }
        ColumnKind::Description => {
            let hit = edit_cell(
                ui,
                p,
                ("desc", id),
                rect,
                &mut project.shots[i].description,
                &[],
                &[],
                row,
            );
            if hit {
                field = Some(Field::Description);
            }
            touched |= hit;
        }
        ColumnKind::Dialogue => {
            let hit = edit_cell(
                ui,
                p,
                ("dialogue", id),
                rect,
                &mut project.shots[i].dialogue,
                &[],
                &[],
                row,
            );
            if hit {
                field = Some(Field::Dialogue);
            }
            touched |= hit;
        }
        ColumnKind::Sound => {
            let hit = edit_cell(
                ui,
                p,
                ("sound", id),
                rect,
                &mut project.shots[i].sound,
                &[],
                &[],
                row,
            );
            if hit {
                field = Some(Field::Sound);
            }
            touched |= hit;
        }
        ColumnKind::Custom => {
            let text = project.shots[i].extra.entry(column.id).or_default();
            let hit = edit_cell(ui, p, ("extra", id ^ column.id), rect, text, &[], &[], row);
            if hit {
                field = Some(Field::Custom(column.id));
            }
            touched |= hit;
            if project.shots[i]
                .extra
                .get(&column.id)
                .is_some_and(String::is_empty)
            {
                project.shots[i].extra.remove(&column.id);
            }
        }
        ColumnKind::Lens => touched |= lens_cell(ui, p, project, i, rect, row.selected),
        ColumnKind::Camera => touched |= camera_menu_cell(ui, p, project, i, rect, req),
        ColumnKind::InFrame => {
            touched |= in_frame_menu_cell(ui, p, project, i, rect, row.scene_cast)
        }
        ColumnKind::Storyboard => {
            // Solo un icono de estado; un clic abre la ventana para cargar o dibujar la imagen.
            let has = project.shots[i].image.is_some();
            ui.painter().with_clip_rect(rect).text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon::IMAGE,
                FontId::proportional(tokens::FONT_BODY + 2.0),
                if has { p.accent } else { p.dim },
            );
            let resp = ui
                .interact(
                    rect,
                    egui::Id::unique(("shot_cell_image", id)),
                    egui::Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(if has {
                    "Tiene imagen. Un clic para verla en grande"
                } else {
                    "Sin imagen. Un clic para cargarla o dibujarla"
                });
            if resp.clicked() {
                req.board = Some(id);
                touched = true;
            }
        }
    }
    (touched, field)
}

/// La cámara de la planta que filma el plano: texto con una flecha tenue que abre la lista de cámaras.
fn camera_menu_cell(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    i: usize,
    rect: egui::Rect,
    req: &mut Requests,
) -> bool {
    let id = project.shots[i].id;
    let cameras = project.scene_cameras(project.shots[i].scene);
    let current = project.shots[i].blocking.zip(project.shots[i].camera);
    let shown = current
        .and_then(|(b, c)| cameras.iter().find(|(cb, cc, _)| (*cb, *cc) == (b, c)))
        .map_or(String::new(), |(_, _, name)| name.clone());
    let mut chosen: Option<Option<(u64, u64)>> = None;
    let mut go = false;
    let mut touched = menu_cell(ui, p, ("camera", id), rect, &shown, |ui| {
        if ui.selectable_label(current.is_none(), "—").clicked() {
            chosen = Some(None);
        }
        for (b, c, name) in &cameras {
            if ui
                .selectable_label(current == Some((*b, *c)), name)
                .clicked()
            {
                chosen = Some(Some((*b, *c)));
            }
        }
        if cameras.is_empty() {
            ui.label(
                RichText::new("La escena no tiene cámaras en ningún bloqueo.")
                    .size(tokens::FONT_SMALL)
                    .color(p.dim),
            );
        }
        if current.is_some() {
            ui.separator();
            go = ui.button("Ir a la cámara en el bloqueo").clicked();
        }
    });
    if let Some(choice) = chosen {
        project.shots[i].blocking = choice.map(|(b, _)| b);
        project.shots[i].camera = choice.map(|(_, c)| c);
        touched = true;
    }
    if go {
        req.blocking = current;
    }
    touched
}

/// Quién está en cuadro: texto con una flecha tenue que abre la lista de personajes de la escena.
fn in_frame_menu_cell(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    i: usize,
    rect: egui::Rect,
    cast: &[(u64, String)],
) -> bool {
    let id = project.shots[i].id;
    let names: Vec<&str> = cast
        .iter()
        .filter(|(c, _)| project.shots[i].in_frame.contains(c))
        .map(|(_, n)| n.as_str())
        .collect();
    let shown = names.join(", ");
    let mut changed = false;
    let mut in_frame = project.shots[i].in_frame.clone();
    let touched = menu_cell(ui, p, ("in_frame", id), rect, &shown, |ui| {
        if cast.is_empty() {
            ui.label(
                RichText::new("La escena no tiene personajes.")
                    .size(tokens::FONT_SMALL)
                    .color(p.dim),
            );
        }
        for (character, name) in cast {
            let mut on = in_frame.contains(character);
            if ui.checkbox(&mut on, name).changed() {
                in_frame.retain(|c| c != character);
                if on {
                    in_frame.push(*character);
                }
                changed = true;
            }
        }
    });
    if changed {
        project.shots[i].in_frame = in_frame;
    }
    touched || changed
}

/// Soportes y angulaciones que se sugieren al escribir.
const SUPPORTS: [&str; 7] = [
    "Trípode", "Mano", "Gimbal", "Slider", "Dolly", "Grúa", "Dron",
];
const ANGLES: [&str; 5] = ["Normal", "Picado", "Contrapicado", "Cenital", "Nadir"];

/// La casilla de la lente: se escribe en mm; sin nada propio se ve la de la cámara, con su equivalente en full frame.
fn lens_cell(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    i: usize,
    rect: egui::Rect,
    selected: bool,
) -> bool {
    let id = project.shots[i].id;
    let own = project.shots[i].lens_mm;
    let mut text = own.map_or(String::new(), |l| {
        crate::optics::fmt_mm(l)
            .trim_end_matches(" mm")
            .replace(',', ".")
    });
    let before = text.clone();
    let touched = text_cell(
        ui,
        p,
        egui::Id::unique(("shot_lens", id)),
        rect,
        &mut text,
        &[],
        &[],
        selected,
    );
    if text != before {
        project.shots[i].lens_mm = text
            .trim()
            .trim_end_matches("mm")
            .trim()
            .replace(',', ".")
            .parse::<f32>()
            .ok()
            .filter(|l| *l > 0.0);
    }
    if let Some((lens, equivalent)) = project.shot_lens(id) {
        let label = if own.is_some() || (lens - equivalent).abs() < 0.5 {
            format!("eq. {}", crate::optics::fmt_mm(equivalent))
        } else {
            format!(
                "{} · eq. {}",
                crate::optics::fmt_mm(lens),
                crate::optics::fmt_mm(equivalent)
            )
        };
        // Sin lente propia, lo que viene de la cámara se ve en gris donde iría el texto.
        let (anchor, align) = if text.is_empty() {
            (
                rect.left_center() + vec2(10.0, 0.0),
                egui::Align2::LEFT_CENTER,
            )
        } else {
            (
                rect.right_bottom() - vec2(6.0, 4.0),
                egui::Align2::RIGHT_BOTTOM,
            )
        };
        ui.painter()
            .text(anchor, align, label, FontId::proportional(12.0), p.dim);
    }
    touched
}

/// La casilla de «en cuadro»: los personajes de la escena con una casilla para tildar.
fn in_frame_cell(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    i: usize,
    cell: egui::Rect,
    cast: &[(u64, String)],
) -> bool {
    let id = project.shots[i].id;
    let shown = {
        let names: Vec<&str> = cast
            .iter()
            .filter(|(c, _)| project.shots[i].in_frame.contains(c))
            .map(|(_, n)| n.as_str())
            .collect();
        if names.is_empty() {
            "—".to_string()
        } else {
            names.join(", ")
        }
    };
    let inner = cell.shrink2(vec2(tokens::SP_S, 1.0));
    let combo = egui::Rect::from_center_size(
        inner.center(),
        vec2(inner.width().max(40.0), tokens::CONTROL_H),
    );
    let mut touched = false;
    ui.put(combo, |ui: &mut Ui| {
        egui::ComboBox::from_id_salt(("shot_in_frame", id))
            .width(combo.width() - 24.0)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if cast.is_empty() {
                    ui.label(
                        RichText::new("La escena no tiene personajes.")
                            .size(12.0)
                            .color(p.dim),
                    );
                }
                for (character, name) in cast {
                    let mut on = project.shots[i].in_frame.contains(character);
                    if ui.checkbox(&mut on, name).changed() {
                        let list = &mut project.shots[i].in_frame;
                        list.retain(|c| c != character);
                        if on {
                            list.push(*character);
                        }
                        touched = true;
                    }
                }
            })
            .response
    });
    touched
}

/// La casilla de la cámara: se elige entre las cámaras de los bloqueos de la escena y un botón lleva a ella.
fn camera_cell(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    i: usize,
    cell: egui::Rect,
    req: &mut Requests,
) -> bool {
    let id = project.shots[i].id;
    let cameras = project.scene_cameras(project.shots[i].scene);
    let current = project.shots[i].blocking.zip(project.shots[i].camera);
    let shown = current
        .and_then(|(b, c)| cameras.iter().find(|(cb, cc, _)| (*cb, *cc) == (b, c)))
        .map_or("—".to_string(), |(_, _, name)| name.clone());
    let inner = cell.shrink2(vec2(tokens::SP_S, 1.0));
    let mut touched = false;
    let combo = egui::Rect::from_min_size(
        pos2(inner.min.x, inner.center().y - tokens::CONTROL_H / 2.0),
        vec2(
            (inner.width() - tokens::ICON_BTN).max(40.0),
            tokens::CONTROL_H,
        ),
    );
    let mut chosen: Option<Option<(u64, u64)>> = None;
    ui.put(combo, |ui: &mut Ui| {
        egui::ComboBox::from_id_salt(("shot_camera", id))
            .width(combo.width() - 24.0)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "—").clicked() {
                    chosen = Some(None);
                }
                for (b, c, name) in &cameras {
                    if ui
                        .selectable_label(current == Some((*b, *c)), name)
                        .clicked()
                    {
                        chosen = Some(Some((*b, *c)));
                    }
                }
                if cameras.is_empty() {
                    ui.label(
                        RichText::new("La escena no tiene cámaras en ningún bloqueo.")
                            .size(12.0)
                            .color(p.dim),
                    );
                }
            })
            .response
    });
    if let Some(choice) = chosen {
        project.shots[i].blocking = choice.map(|(b, _)| b);
        project.shots[i].camera = choice.map(|(_, c)| c);
        touched = true;
    }
    let go = egui::Rect::from_center_size(
        pos2(inner.max.x - tokens::ICON_BTN / 2.0, inner.center().y),
        vec2(tokens::ICON_BTN, tokens::ICON_BTN),
    );
    if ui
        .put(go, |ui: &mut Ui| {
            widgets::icon_button(
                ui,
                p,
                icon::COMPASS,
                "Ir a la cámara en el bloqueo",
                current.is_some(),
            )
        })
        .clicked()
    {
        req.blocking = current;
        touched = true;
    }
    touched
}

/// El guion (o una de sus escenas) como texto corrido, para elegir de él lo que se importa.
fn script_text(project: &Project, scene: Option<u64>) -> String {
    use crate::model::ParagraphKind::*;
    let mut out = String::new();
    let mut inside = scene.is_none();
    for para in &project.paragraphs {
        if para.kind == SceneHeading {
            inside = scene.is_none_or(|id| id == para.id);
        }
        if !inside || para.text.trim().is_empty() || !para.kind.is_filmed() {
            continue;
        }
        // Una línea en blanco separa cada bloque; el diálogo va pegado a su personaje.
        if !out.is_empty() && matches!(para.kind, SceneHeading | Action | Character | Transition) {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(para.text.trim());
    }
    out
}

/// Agrega texto a una casilla: lo pone si está vacía o lo suma en una línea nueva.
fn append_to_field(project: &mut Project, shot: u64, field: Field, text: &str) -> bool {
    let Some(shot) = project.shots.iter_mut().find(|s| s.id == shot) else {
        return false;
    };
    let target = match field {
        Field::Description => &mut shot.description,
        Field::Dialogue => &mut shot.dialogue,
        Field::Sound => &mut shot.sound,
        Field::Custom(column) => shot.extra.entry(column).or_default(),
    };
    if !target.trim().is_empty() {
        target.push('\n');
    }
    target.push_str(text);
    true
}

/// El nombre de una casilla de destino, para el botón de pasar texto.
fn field_name(project: &Project, field: Field) -> String {
    match field {
        Field::Description => "Descripción".to_string(),
        Field::Dialogue => "Diálogo".to_string(),
        Field::Sound => "Sonido".to_string(),
        Field::Custom(id) => project
            .shot_columns
            .iter()
            .find(|c| c.id == id)
            .map_or_else(|| "Casilla".to_string(), |c| c.name.clone()),
    }
}

/// El guion literario como panel a la derecha del guion técnico (redimensionable). Se puede seleccionar texto y pasarlo a la
/// casilla que se tocó por última vez.
fn script_panel(
    ui: &mut Ui,
    area: egui::Rect,
    state: &mut TechState,
    project: &mut Project,
    p: &Palette,
) {
    if state.import.is_none() {
        return;
    }
    let (cell, selected) = (state.cell, state.selected);
    let Some(import) = state.import.as_mut() else {
        return;
    };
    // Al elegir otro plano, el guion pasa a su escena.
    if import.followed != selected {
        import.followed = selected;
        if let Some(scene) = selected
            .and_then(|id| project.shots.iter().find(|s| s.id == id))
            .map(|s| s.scene)
            .filter(|scene| project.script_scenes().iter().any(|(id, _)| id == scene))
        {
            import.scene = Some(scene);
            import.selection = None;
        }
    }
    let scenes = project.script_scenes();
    let numbers = project.scene_labels();
    let text = script_text(project, import.scene);
    let chosen: Option<String> = import
        .selection
        .map(|(a, b)| text.chars().skip(a).take(b - a).collect::<String>());
    let target = cell.and_then(|(shot, field)| {
        let n = project.shot_numbers().get(&shot).copied()?;
        Some((shot, field, format!("{} del plano {n}", field_name(project, field))))
    });
    let ready = target.is_some() && chosen.as_ref().is_some_and(|c| !c.trim().is_empty());
    let mut close = false;
    let mut accept = false;
    ui.painter().rect_filled(area, 0, p.panel);
    ui.scope_builder(egui::UiBuilder::new().max_rect(area.shrink(tokens::SP_S)), |ui| {
            ui.set_clip_rect(area.intersect(ui.clip_rect()));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Guion").font(FontId::new(
                    tokens::FONT_CARD_TITLE,
                    theme::bold_family(),
                )));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, p, icon::X, "Cerrar el guion", true).clicked() {
                        close = true;
                    }
                });
            });
            let shown = match import
                .scene
                .and_then(|id| scenes.iter().position(|(s, _)| *s == id))
            {
                Some(n) => format!("{}. {}", numbers[n].1, scenes[n].1.trim()),
                None => "Todo el guion".to_string(),
            };
            egui::ComboBox::from_id_salt("tech_import_scene")
                .width(ui.available_width())
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(import.scene.is_none(), "Todo el guion")
                        .clicked()
                    {
                        import.scene = None;
                        import.selection = None;
                    }
                    for (n, (id, heading)) in scenes.iter().enumerate() {
                        if ui
                            .selectable_label(
                                import.scene == Some(*id),
                                format!("{}. {}", numbers[n].1, heading.trim()),
                            )
                            .clicked()
                        {
                            import.scene = Some(*id);
                            import.selection = None;
                        }
                    }
                });
            let button = match &target {
                Some((.., label)) => format!("Pasar la selección a {label}"),
                None => "Tocá una casilla para elegir el destino".to_string(),
            };
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new(RichText::new(button).color(p.accent))
                        .min_size(vec2(ui.available_width(), 0.0)),
                )
                .clicked()
            {
                accept = true;
            }
            egui::ScrollArea::vertical()
                .id_salt("tech_script_panel_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Un texto de solo lectura: se puede seleccionar pero no cambiar.
                    let mut shown: &str = &text;
                    let out = TextEdit::multiline(&mut shown)
                        .font(FontId::monospace(tokens::FONT_BODY))
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY)
                        .show(ui);
                    if out.response.has_focus() {
                        import.selection = out.cursor_range.and_then(|r| {
                            let (a, b) =
                                (usize::from(r.primary.index), usize::from(r.secondary.index));
                            (a != b).then_some((a.min(b), a.max(b)))
                        });
                    }
                });
        });
    if ready && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) {
        accept = true;
    }
    if accept
        && let (Some((shot, field, _)), Some(chosen)) = (target, chosen)
    {
        append_to_field(project, shot, field, chosen.trim());
        if let Some(import) = state.import.as_mut() {
            import.selection = None;
        }
    }
    if close {
        state.import = None;
    }
}

/// Ancho de una viñeta del storyboard por defecto y sus límites.
const CARD_W: f32 = 260.0;
const CARD_W_RANGE: (f32, f32) = (160.0, 420.0);
const CARD_GAP: f32 = 14.0;

/// Un cuadro del storyboard o el botón para sumar uno nuevo al final de la escena.
enum Card {
    Shot(usize),
    Add(u64),
}

/// Lo que se pidió en el storyboard; se aplica después de dibujar.
enum Pick {
    /// Cargar o cambiar la imagen de un plano.
    Image(u64),
    Clear(u64),
    /// Abrir la ventana del storyboard sobre una escena (imagen nueva).
    Add(u64),
    /// Abrir el boceto de un plano.
    Sketch(u64),
}

/// Todos los cuadros del storyboard, escena por escena, en filas que se acomodan al ancho disponible.
///
/// Cada cuadro es un plano de la tabla: sumar una imagen acá crea la fila en el guion técnico y cargarla allá la
/// muestra acá.
fn storyboard(ui: &mut Ui, state: &mut TechState, project: &mut Project, p: &Palette) {
    let numbers = project.shot_numbers();
    let only_images = project.storyboard_only_images;
    let mut pick: Option<Pick> = None;
    egui::ScrollArea::vertical()
        .id_salt("tech_storyboard")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let card_w = if state.card_w > 0.0 {
                state.card_w.clamp(CARD_W_RANGE.0, CARD_W_RANGE.1)
            } else {
                CARD_W
            };
            let per_row =
                (((ui.available_width() + CARD_GAP) / (card_w + CARD_GAP)).floor() as usize).max(1);
            for group in groups(project, state.scene_filter, state.shoot_order_view) {
                let visible: Vec<usize> = group
                    .shots
                    .iter()
                    .copied()
                    .filter(|&i| !only_images || project.shots[i].image.is_some())
                    .collect();
                if group.scene.is_none() && visible.is_empty() {
                    continue;
                }
                // El encabezado de la escena ocupa todo el ancho, como en la tabla.
                let (band, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), tokens::ROW_H),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(band, 0, p.container);
                let painter = ui.painter().with_clip_rect(band);
                let title = painter.text(
                    band.left_center() + vec2(tokens::SP_M, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &group.label,
                    FontId::new(tokens::FONT_BODY, theme::bold_family()),
                    p.text,
                );
                painter.text(
                    pos2(title.right() + tokens::SP_S, band.center().y),
                    egui::Align2::LEFT_CENTER,
                    group.shots.len().to_string(),
                    FontId::proportional(tokens::FONT_SMALL),
                    p.dim,
                );
                ui.add_space(tokens::SP_S);
                let mut cards: Vec<Card> = visible.into_iter().map(Card::Shot).collect();
                if let Some(scene) = group.scene {
                    cards.push(Card::Add(scene));
                }
                for row in cards.chunks(per_row) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = CARD_GAP;
                        for card in row {
                            match card {
                                Card::Shot(i) => {
                                    let shot = &project.shots[*i];
                                    let number = numbers.get(&shot.id).copied().unwrap_or(0);
                                    if let Some(request) =
                                        frame_card(ui, p, project, shot, number, card_w)
                                    {
                                        pick = Some(request);
                                    }
                                }
                                Card::Add(scene) => {
                                    if add_card(ui, p, card_w, project.aspect.ratio()) {
                                        pick = Some(Pick::Add(*scene));
                                    }
                                }
                            }
                        }
                    });
                    ui.add_space(CARD_GAP);
                }
                ui.add_space(tokens::SP_M);
            }
            ui.add_space(tokens::SP_XXL);
        });

    match pick {
        Some(Pick::Image(id)) => state.open_shot_image(project, id),
        Some(Pick::Clear(id)) => {
            if let Some(shot) = project.shots.iter_mut().find(|s| s.id == id) {
                shot.image = None;
                shot.sketch.clear();
            }
        }
        Some(Pick::Add(scene)) => state.open_board(project, Some(scene), None, Mode::File),
        Some(Pick::Sketch(id)) => state.open_board(project, None, Some(id), Mode::Draw),
        None => {}
    }
}

/// El cuadro para sumar imágenes nuevas (cada una pasa a ser un plano de la escena).
fn add_card(ui: &mut Ui, p: &Palette, card_w: f32, aspect: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(card_w, card_w / aspect), egui::Sense::click());
    let hot = resp.hovered();
    ui.painter().rect_filled(
        rect,
        tokens::R_CONTROL,
        if hot {
            theme::mix(p.panel, p.accent, 0.12)
        } else {
            p.panel
        },
    );
    // Viñeta punteada: se distingue de las que ya tienen un plano.
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    ui.painter().extend(egui::Shape::dashed_line(
        &corners,
        Stroke::new(1.0, if hot { p.accent } else { p.dim }),
        6.0,
        4.0,
    ));
    ui.painter().text(
        rect.center() - vec2(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        icon::PLUS,
        FontId::proportional(26.0),
        if hot { p.accent } else { p.dim },
    );
    ui.painter().text(
        rect.center() + vec2(0.0, 16.0),
        egui::Align2::CENTER_CENTER,
        "Añadir imágenes",
        FontId::proportional(12.0),
        if hot { p.accent } else { p.dim },
    );
    resp.on_hover_text("Cada imagen se suma como un plano nuevo de la escena")
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// Un botón redondo sobre una imagen. Se dibuja y se pulsa sin pasar por el diseño: aparecer al pasar el mouse no mueve nada.
fn overlay_button(ui: &mut Ui, p: &Palette, rect: egui::Rect, code: &str, tip: &str) -> bool {
    let resp = ui
        .interact(rect, ui.scope_id().with(("overlay", code)), egui::Sense::click())
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if resp.hovered() {
        theme::mix(p.shadow, p.accent, 0.35)
    } else {
        p.shadow
    };
    ui.painter().circle_filled(rect.center(), 13.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        code,
        FontId::proportional(tokens::ICON_S),
        egui::Color32::WHITE,
    );
    resp.clicked()
}

fn frame_card(
    ui: &mut Ui,
    p: &Palette,
    project: &Project,
    shot: &Shot,
    number: usize,
    card_w: f32,
) -> Option<Pick> {
    let mut request = None;
    ui.vertical(|ui| {
        ui.set_width(card_w);
        let (rect, resp) = ui.allocate_exact_size(
            vec2(card_w, card_w / project.aspect.ratio()),
            egui::Sense::click(),
        );
        // Los botones de encima le quitan el «hover» al cuadro; por eso se mira dónde está el mouse y no la respuesta.
        let over = ui.rect_contains_pointer(rect);
        match &shot.image {
            Some(path) => {
                pages::paint_cover(ui, &project.resolve_path(path), rect, 4);
                let button =
                    egui::Rect::from_min_size(rect.right_top() - vec2(30.0, 0.0), vec2(30.0, 30.0));
                if over && overlay_button(ui, p, button, icon::X, "Quitar la imagen") {
                    request = Some(Pick::Clear(shot.id));
                }
            }
            None => {
                ui.painter().rect(
                    rect,
                    4,
                    if resp.hovered() {
                        theme::mix(p.field, p.accent, 0.12)
                    } else {
                        p.field
                    },
                    Stroke::new(1.0, p.border),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.center() - vec2(0.0, 10.0),
                    egui::Align2::CENTER_CENTER,
                    icon::IMAGE,
                    FontId::proportional(26.0),
                    p.dim,
                );
                ui.painter().text(
                    rect.center() + vec2(0.0, 16.0),
                    egui::Align2::CENTER_CENTER,
                    "Cargar imagen",
                    FontId::proportional(12.0),
                    p.dim,
                );
            }
        }
        // Un boceto nuevo para este plano, sobre cualquier cuadro.
        if over {
            let pencil =
                egui::Rect::from_min_size(rect.right_bottom() - vec2(34.0, 34.0), vec2(30.0, 30.0));
            if overlay_button(
                ui,
                p,
                pencil,
                icon::NOTE_PENCIL,
                "Dibujar un boceto para este plano",
            ) {
                request = Some(Pick::Sketch(shot.id));
            }
        }
        if resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && request.is_none()
        {
            request = Some(Pick::Image(shot.id));
        }
        // El número va sobre un fondo oscuro con borde claro: se lee sobre cualquier imagen.
        let label = ui.painter().layout_no_wrap(
            number.to_string(),
            FontId::new(16.0, theme::bold_family()),
            p.text,
        );
        let badge = egui::Rect::from_min_size(
            rect.left_top() + vec2(8.0, 8.0),
            label.size() + vec2(14.0, 6.0),
        );
        ui.painter().rect(
            badge,
            4,
            p.shadow,
            Stroke::new(1.0, p.text),
            egui::StrokeKind::Outside,
        );
        ui.painter()
            .galley(badge.min + vec2(7.0, 3.0), label, p.text);

        // Debajo de la imagen: el código y el tipo de plano, y la primera línea de la descripción, recortados.
        let info = [number.to_string().as_str(), shot.size.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        ui.add(
            egui::Label::new(RichText::new(info).size(tokens::FONT_SMALL).color(p.accent))
                .truncate(),
        );
        let first_line = shot.description.lines().next().unwrap_or("").trim();
        ui.add(
            egui::Label::new(
                RichText::new(if first_line.is_empty() {
                    "–"
                } else {
                    first_line
                })
                .size(tokens::FONT_SMALL)
                .color(if first_line.is_empty() { p.dim } else { p.text }),
            )
            .truncate(),
        );
    });
    request
}

/// Segundos de una duración escrita a mano: «5», «5 s», «2,5» o «1:30».
pub(crate) fn parse_seconds(text: &str) -> Option<f32> {
    let text = text
        .trim()
        .trim_end_matches(|c: char| c.is_alphabetic() || c == '.')
        .trim()
        .replace(',', ".");
    match text.split_once(':') {
        Some((m, s)) => Some(m.trim().parse::<f32>().ok()? * 60.0 + s.trim().parse::<f32>().ok()?),
        None => text.parse::<f32>().ok(),
    }
}

/// Botón de ícono chico para las listas compactas.
fn mini_button(
    ui: &mut Ui,
    p: &Palette,
    code: &str,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::click());
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 4, p.container_hi);
    }
    let color = if enabled {
        p.text
    } else {
        theme::mix(p.panel, p.dim, 0.4)
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        code,
        FontId::proportional(14.0),
        color,
    );
    if enabled {
        resp.on_hover_text(tooltip)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Un campo de varias líneas del inspector (`label`, filas). Si se lo está escribiendo, a él va lo que se importa del guion.
fn long_field(
    ui: &mut Ui,
    p: &Palette,
    state: &mut TechState,
    shot: u64,
    (label, rows): (&str, usize),
    text: &mut String,
    field: Field,
) {
    let (_, focused) = kit::field_state(ui, p, label, text, rows, false);
    if focused {
        state.cell = Some((shot, field));
    }
}

/// Pestaña PLANO: todos los campos del plano seleccionado.
fn plano_tab(
    ui: &mut Ui,
    state: &mut TechState,
    project: &mut Project,
    p: &Palette,
    out: &mut Out,
) {
    let Some(i) = state
        .selected
        .and_then(|id| project.shots.iter().position(|s| s.id == id))
    else {
        kit::empty_state(
            ui,
            p,
            icon::FILM_STRIP,
            "Elegí un plano de la tabla para ver y editar todos sus datos.",
            None,
        );
        return;
    };
    let id = project.shots[i].id;
    let before = project.shots[i].clone();
    let options = Options::new(project);
    let code = project.shot_codes().get(&id).cloned().unwrap_or_default();
    let minutes = project.shot_minutes().get(&id).copied();
    let cast: Vec<(u64, String)> = project
        .scene_cast_ids(project.shots[i].scene)
        .into_iter()
        .filter_map(|c| {
            project
                .character(c)
                .map(|c| (c.id, crate::model::clean_character_name(&c.name)))
        })
        .collect();
    let mut req = Requests::default();

    kit::block(ui, p, "Plano", |ui| {
        kit::kv_row(ui, p, "Número", &code);
        kit::field_group(ui, 2, |ui, n| {
            if n == 0 {
                kit::cell_field_with(ui, p, "Tipo", kit::CellLook::Dropdown, |ui, r| {
                    text_cell(
                        ui,
                        p,
                        egui::Id::unique(("plano_size", id)),
                        r,
                        &mut project.shots[i].size,
                        &options.size,
                        &options.mods,
                        false,
                    )
                });
            } else {
                kit::cell_field_with(ui, p, "Movimiento", kit::CellLook::Dropdown, |ui, r| {
                    text_cell(
                        ui,
                        p,
                        egui::Id::unique(("plano_move", id)),
                        r,
                        &mut project.shots[i].movement,
                        &options.movement,
                        &[],
                        false,
                    )
                });
            }
        });
        kit::cell_field_with(ui, p, "Angulación", kit::CellLook::Dropdown, |ui, r| {
            text_cell(
                ui,
                p,
                egui::Id::unique(("plano_angle", id)),
                r,
                &mut project.shots[i].angle,
                &options.angle,
                &[],
                false,
            )
        });
        kit::cell_field(ui, p, "Lente (mm)", |ui, r| {
            lens_cell(ui, p, project, i, r, false)
        });
        kit::cell_field_with(ui, p, "Soporte", kit::CellLook::Dropdown, |ui, r| {
            text_cell(
                ui,
                p,
                egui::Id::unique(("plano_support", id)),
                r,
                &mut project.shots[i].support,
                &options.support,
                &[],
                false,
            )
        });
        kit::cell_field_with(
            ui,
            p,
            "Personajes en cuadro",
            kit::CellLook::Bare,
            |ui, r| in_frame_cell(ui, p, project, i, r, &cast),
        );
    });
    kit::block(ui, p, "Contenido", |ui| {
        let shot = &mut project.shots[i];
        long_field(
            ui,
            p,
            state,
            id,
            ("Descripción", 3),
            &mut shot.description,
            Field::Description,
        );
        long_field(
            ui,
            p,
            state,
            id,
            ("Diálogo", 3),
            &mut shot.dialogue,
            Field::Dialogue,
        );
        long_field(
            ui,
            p,
            state,
            id,
            ("Sonido", 2),
            &mut shot.sound,
            Field::Sound,
        );

        // Las columnas propias son texto libre del plano: se escriben acá.
        let custom: Vec<(u64, String)> = project
            .shot_columns
            .iter()
            .filter(|c| c.kind == ColumnKind::Custom)
            .map(|c| (c.id, c.name.clone()))
            .collect();
        for (column, name) in custom {
            let text = project.shots[i].extra.entry(column).or_default();
            let (_, focused) = kit::field_state(ui, p, &name, text, 2, false);
            if focused {
                state.cell = Some((id, Field::Custom(column)));
            }
            if project.shots[i]
                .extra
                .get(&column)
                .is_some_and(String::is_empty)
            {
                project.shots[i].extra.remove(&column);
            }
        }
    });
    kit::block(ui, p, "Tiempo", |ui| {
        kit::cell_field(ui, p, "Duración", |ui, r| {
            text_cell(
                ui,
                p,
                egui::Id::unique(("plano_duration", id)),
                r,
                &mut project.shots[i].duration,
                &[],
                &[],
                false,
            )
        });
        kit::kv_row(
            ui,
            p,
            "Tiempo estimado",
            &minutes.map_or(String::new(), |m| format!("{m} min")),
        );
    });
    kit::block(ui, p, "Vínculos", |ui| {
        kit::cell_field_with(
            ui,
            p,
            "Cámara de la planta",
            kit::CellLook::Bare,
            |ui, r| camera_cell(ui, p, project, i, r, &mut req),
        );
    });
    kit::block(ui, p, "Imagen", |ui| {
        if let Some(path) = project.shots[i].image.clone() {
            let width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(
                vec2(width, width / project.aspect.ratio()),
                egui::Sense::hover(),
            );
            pages::paint_cover(ui, &project.resolve_path(&path), rect, tokens::R_CONTROL);
        }
        kit::field_group(ui, 2, |ui, n| {
            let width = ui.available_width();
            if n == 0 {
                if kit::wide_button(ui, p, icon::NOTE_PENCIL, "Dibujar", width).clicked() {
                    state.open_board(project, None, Some(id), Mode::Draw);
                }
            } else if kit::wide_button(ui, p, icon::IMAGE, "Cargar", width).clicked() {
                state.open_board(project, None, Some(id), Mode::File);
            }
        });
        if project.shots[i].image.is_some()
            && kit::wide_button(ui, p, icon::TRASH, "Quitar imagen", ui.available_width()).clicked()
        {
            project.shots[i].image = None;
            project.shots[i].sketch.clear();
        }
    });
    out.open_blocking = out.open_blocking.or(req.blocking);
    if project.shots[i] != before {
        out.changed = true;
    }
}

/// Panel derecho: numeración, columnas de la tabla y resumen.
pub fn right_panel(
    ui: &mut Ui,
    state: &mut TechState,
    project: &mut Project,
    p: &Palette,
    paper: crate::storage::Paper,
    tab: usize,
) -> Out {
    let mut out = Out::default();
    project.ensure_shot_columns();
    let tab = tab.min(2);
    (|ui: &mut Ui| match tab {
        0 => plano_tab(ui, state, project, p, &mut out),
        1 => {
            ui.label(RichText::new("CONFIGURACIÓN").size(11.0).color(p.dim));
            ui.add_space(tokens::SP_S);
            if let Some(n) =
                kit::segmented(ui, p, &["Compacta", "Completa"], usize::from(state.full))
            {
                state.full = n == 1;
                // «Rodaje» es una columna que se agrega: la configuración Completa la trae.
                if state.full
                    && !project
                        .shot_columns
                        .iter()
                        .any(|c| c.kind == ColumnKind::ShootOrder)
                {
                    project.add_builtin_column(ColumnKind::ShootOrder);
                    out.changed = true;
                }
            }
            ui.add_space(tokens::SP_L);
            ui.label(RichText::new("COLUMNAS").size(11.0).color(p.dim));
            ui.add_space(tokens::SP_S);
            // Como las capas de un programa de dibujo: la barra de arriba actúa sobre la columna elegida.
            let selected = state
                .selected_column
                .filter(|id| project.shot_columns.iter().any(|c| c.id == *id));
            let position =
                selected.and_then(|id| project.shot_columns.iter().position(|c| c.id == id));
            let custom =
                position.is_some_and(|i| project.shot_columns[i].kind == ColumnKind::Custom);
            let removable = position.is_some_and(|i| {
                project.shot_columns[i].kind == ColumnKind::Custom
                    || project.shot_columns[i].kind.is_optional()
            });
            let count = project.shot_columns.len();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if mini_button(ui, p, icon::PLUS, "Añadir una columna", true).clicked() {
                    let n = project
                        .shot_columns
                        .iter()
                        .filter(|c| c.kind == ColumnKind::Custom)
                        .count()
                        + 1;
                    state.selected_column = Some(project.add_shot_column(&format!("Columna {n}")));
                    out.changed = true;
                }
                if mini_button(ui, p, icon::TRASH, "Quitar la columna elegida", removable).clicked()
                {
                    out.delete_column = selected;
                }
                ui.add_space(tokens::SP_S);
                if mini_button(
                    ui,
                    p,
                    icon::UP_ARROW,
                    "Mover a la izquierda (arriba en la lista)",
                    position.is_some_and(|i| i > 0),
                )
                .clicked()
                {
                    out.changed |= selected.is_some_and(|id| project.move_shot_column(id, -1));
                }
                if mini_button(
                    ui,
                    p,
                    icon::DOWN_ARROW,
                    "Mover a la derecha (abajo en la lista)",
                    position.is_some_and(|i| i + 1 < count),
                )
                .clicked()
                {
                    out.changed |= selected.is_some_and(|id| project.move_shot_column(id, 1));
                }
            });
            ui.add_space(tokens::SP_XS);
            egui::Frame::new()
                .fill(p.field)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(4)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.set_width(ui.available_width());
                    for column in &project.shot_columns {
                        let shown = column_shown(column, state.full);
                        let (rect, resp) = ui.allocate_exact_size(
                            vec2(ui.available_width(), 26.0),
                            egui::Sense::click(),
                        );
                        let chosen = selected == Some(column.id);
                        if chosen {
                            ui.painter()
                                .rect_filled(rect, 0, theme::mix(p.field, p.accent, 0.25));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 0, p.container_hi);
                        }
                        ui.painter().text(
                            rect.left_center() + vec2(10.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            column_def(column).title,
                            FontId::proportional(13.0),
                            if shown { p.text } else { p.dim },
                        );
                        if resp.clicked() {
                            state.selected_column = Some(column.id);
                        }
                    }
                });
            if custom {
                if let Some(i) = position {
                    ui.add_space(tokens::SP_S);
                    ui.label(
                        RichText::new("Nombre de la columna")
                            .size(12.0)
                            .color(p.dim),
                    );
                    if ui
                        .add(
                            TextEdit::singleline(&mut project.shot_columns[i].name)
                                .desired_width(ui.available_width()),
                        )
                        .changed()
                    {
                        out.changed = true;
                    }
                }
            }

            // Columnas predefinidas que todavía no están en la tabla.
            let missing: Vec<ColumnKind> = ColumnKind::OPTIONAL
                .iter()
                .copied()
                .filter(|k| !project.shot_columns.iter().any(|c| c.kind == *k))
                .collect();
            if !missing.is_empty() {
                ui.add_space(tokens::SP_S);
                egui::ComboBox::from_id_salt("tech_add_builtin")
                    .width(crate::ui::kit::fill_width(ui, 60.0, 1000.0))
                    .selected_text("Agregar columna predefinida…")
                    .show_ui(ui, |ui| {
                        for kind in missing {
                            if ui.selectable_label(false, kind.default_name()).clicked() {
                                state.selected_column = project.add_builtin_column(kind);
                                out.changed = true;
                            }
                        }
                    });
            }
        }
        _ => {
            kit::block(ui, p, "NUMERACIÓN DE PLANOS", |ui| {
                if ui
                    .checkbox(&mut project.shots_restart, "Reiniciar en cada escena")
                    .changed()
                {
                    out.changed = true;
                }
                ui.add_space(tokens::SP_XS);
                let note = if project.shots_restart {
                    "Cada escena empieza en el plano 1."
                } else {
                    "La numeración sigue de una escena a la otra, de corrido en todo el guion."
                };
                ui.add(egui::Label::new(RichText::new(note).size(12.0).color(p.dim)).wrap());
            });
            kit::block(ui, p, "STORYBOARD", |ui| {
                if ui
                    .checkbox(
                        &mut project.storyboard_only_images,
                        "Ocultar los planos sin imagen",
                    )
                    .changed()
                {
                    out.changed = true;
                }
                ui.add_space(tokens::SP_XS);
            });
            kit::block(ui, p, "RODAJE", |ui| {
                if widgets::wide_button(
                    ui,
                    p,
                    icon::LIST_NUMBERS,
                    "Ordenar por emplazamiento",
                    ui.available_width(),
                )
                .clicked()
                {
                    project.assign_shoot_order();
                    if !project
                        .shot_columns
                        .iter()
                        .any(|c| c.kind == ColumnKind::ShootOrder)
                    {
                        project.add_builtin_column(ColumnKind::ShootOrder);
                    }
                    out.changed = true;
                }
                ui.add_space(tokens::SP_S);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Por emplazamiento").size(12.0).color(p.dim));
                    out.changed |= ui
                        .add(
                            egui::DragValue::new(&mut project.minutes_per_setup)
                                .range(0..=240)
                                .suffix(" min"),
                        )
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Por plano").size(12.0).color(p.dim));
                    out.changed |= ui
                        .add(
                            egui::DragValue::new(&mut project.minutes_per_shot)
                                .range(0..=240)
                                .suffix(" min"),
                        )
                        .changed();
                });
            });
            kit::block(ui, p, "CUADRO", |ui| {
                ui.horizontal(|ui| {
                    for aspect in crate::optics::Aspect::ALL {
                        out.changed |= ui
                            .selectable_value(&mut project.aspect, aspect, aspect.label())
                            .changed();
                    }
                });
            });
            kit::block(ui, p, "EXPORTAR", |ui| {
                export_buttons(ui, p, project, paper, &mut out);
            });
            kit::block(ui, p, "RESUMEN", |ui| {
                let seconds: f32 = project
                    .shots
                    .iter()
                    .filter_map(|s| parse_seconds(&s.duration))
                    .sum();
                let with_image = project.shots.iter().filter(|s| s.image.is_some()).count();
                let planos = project.shots.len();
                ui.label(format!(
                    "{planos} {}",
                    if planos == 1 { "plano" } else { "planos" }
                ));
                ui.label(format!("{with_image} con imagen de storyboard"));
                if seconds > 0.0 {
                    ui.label(format!(
                        "Duración cargada: {}",
                        pages::fmt_duration(seconds)
                    ));
                }
            });
        }
    })(ui);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_read_in_the_usual_ways() {
        assert_eq!(parse_seconds("5"), Some(5.0));
        assert_eq!(parse_seconds("5 s"), Some(5.0));
        assert_eq!(parse_seconds("2,5 seg"), Some(2.5));
        assert_eq!(parse_seconds("1:30"), Some(90.0));
        assert_eq!(parse_seconds(""), None);
        assert_eq!(parse_seconds("largo"), None);
    }

    fn opts() -> Vec<Opt> {
        SHOT_SIZES
            .iter()
            .map(|(a, f)| Opt {
                value: a.to_string(),
                full: f.to_string(),
            })
            .collect()
    }

    #[test]
    fn shot_types_chain_with_an_arrow_and_suggest_by_abbreviation_name_and_reference() {
        let (opts, mods) = (opts(), modifiers());
        let whole = |text: &str| {
            suggestions(text, &opts, &mods)
                .into_iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>()
        };
        // Por el nombre completo y por la abreviatura.
        assert_eq!(whole("primer"), vec!["PP", "PPP"]);
        assert_eq!(whole("pa"), vec!["PA"]);
        // Con el tipo ya completo no se vuelve a ofrecer el mismo.
        assert!(whole("PG").is_empty());
        // Un pase de un plano a otro: se sugiere para el último tramo y el resto queda.
        assert_eq!(whole("PG -> primer"), vec!["PG -> PP", "PG -> PPP"]);
        assert_eq!(
            whole("PG -> "),
            // Se ofrecen todos (la lista se desplaza), no solo los primeros.
            opts.iter()
                .map(|o| format!("PG -> {}", o.value))
                .collect::<Vec<_>>()
        );
        // La referencia se sugiere después del tipo.
        assert_eq!(whole("PG "), vec!["PG c/ref"]);
        assert_eq!(whole("PG c"), vec!["PG c/ref"]);
        assert_eq!(whole("PG -> PP c/"), vec!["PG -> PP c/ref"]);
        assert!(whole("PG c/ref").is_empty());
    }

    #[test]
    fn the_toolbar_draws_with_a_selected_shot_and_moving_reorders_the_rows() {
        use crate::model::ParagraphKind::SceneHeading;
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let (a, b) = (project.add_shot(scene, None), project.add_shot(scene, None));
        let mut state = TechState::default();
        state.selected = Some(b);
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0));
        let p = crate::storage::Settings::default().scheme.palette();
        // Sin clics la barra no cambia nada.
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(area),
                ..Default::default()
            },
            |ui| {
                let r = toolbar(ui, &mut state, &mut project, &p);
                assert!(!r.changed && r.delete.is_none());
            },
        );
        out.textures_delta.clear();
        assert_eq!(
            project.shots.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![a, b]
        );
        assert!(project.move_shot(b, -1)); // lo que hace el botón «subir»
        assert_eq!(project.shots[0].id, b);
    }

    fn frame_out(
        ctx: &egui::Context,
        state: &mut TechState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
    ) -> Out {
        let p = crate::storage::Settings::default().scheme.palette();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1400.0, 800.0),
            )),
            time: Some(t),
            focused: true,
            events,
            ..Default::default()
        };
        let mut result = Out::default();
        let mut out = ctx.run_ui(input, |ui| {
            result = show(ui, state, project, &p);
        });
        out.textures_delta.clear();
        result
    }

    fn frame(
        ctx: &egui::Context,
        state: &mut TechState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
    ) {
        frame_out(ctx, state, project, events, t);
    }

    /// Dibuja la pestaña PLANO del inspector.
    fn frame_inspector(
        ctx: &egui::Context,
        state: &mut TechState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
    ) {
        let p = crate::storage::Settings::default().scheme.palette();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(400.0, 1400.0),
            )),
            time: Some(t),
            focused: true,
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            right_panel(ui, state, project, &p, crate::storage::Paper::A4, 0);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn the_shot_type_is_free_text_with_suggestions_in_the_inspector() {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let shot = project.add_shot(scene, None);
        let mut state = TechState::default();
        state.selected = Some(shot);
        frame_inspector(&ctx, &mut state, &mut project, vec![], 0.1);

        let id = egui::Id::unique(("plano_size", shot));
        ctx.memory_mut(|m| m.request_focus(id));
        frame_inspector(&ctx, &mut state, &mut project, vec![], 0.2);
        frame_inspector(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::Text("plano me".into())],
            0.3,
        );
        frame_inspector(&ctx, &mut state, &mut project, vec![], 0.4);
        assert_eq!(project.shots[0].size, "plano me");
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: Some(egui::Key::Tab),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame_inspector(&ctx, &mut state, &mut project, vec![tab], 0.5);
        frame_inspector(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::Text(" con referencia".into())],
            0.6,
        );
        frame_inspector(&ctx, &mut state, &mut project, vec![], 0.7);
        assert_eq!(project.shots[0].size, "PM con referencia");
        // El nuevo texto queda como sugerencia para los demás planos.
        assert!(
            options_for(&project, Vec::new(), |s| &s.size)
                .iter()
                .any(|o| o.value == "PM con referencia")
        );
    }

    #[test]
    fn the_selected_shot_moves_and_is_deleted_with_the_keyboard_and_the_rows_stay_one_line() {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let (a, b) = (project.add_shot(scene, None), project.add_shot(scene, None));
        project.shots[0].description = "una línea larga\n".repeat(14);
        let mut state = TechState::default();
        state.selected = Some(b);
        for n in 0..3 {
            frame(&ctx, &mut state, &mut project, vec![], 0.1 * n as f64);
        }
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        // Alt+↑ sube el plano elegido; Alt+↓ lo baja.
        let out = frame_out(
            &ctx,
            &mut state,
            &mut project,
            vec![key(egui::Key::ArrowUp, egui::Modifiers::ALT)],
            0.5,
        );
        assert!(out.changed);
        assert_eq!(
            project.shots.iter().map(|s| s.id).collect::<Vec<_>>(),
            [b, a]
        );
        frame(
            &ctx,
            &mut state,
            &mut project,
            vec![key(egui::Key::ArrowDown, egui::Modifiers::ALT)],
            0.6,
        );
        assert_eq!(
            project.shots.iter().map(|s| s.id).collect::<Vec<_>>(),
            [a, b]
        );
        // Supr pide borrarlo (la app confirma si tiene contenido).
        let out = frame_out(
            &ctx,
            &mut state,
            &mut project,
            vec![key(egui::Key::Delete, egui::Modifiers::NONE)],
            0.7,
        );
        assert_eq!(out.delete, Some(b));
    }

    #[test]
    fn the_compact_columns_fit_in_a_1280_window_and_the_full_ones_add_dialogue_sound_and_shooting_order()
     {
        let mut project = Project::default();
        project.ensure_shot_columns();
        let titles = |project: &Project, full: bool| -> Vec<&'static str> {
            visible_columns(project, full)
                .iter()
                .map(|(_, d)| d.title)
                .collect()
        };
        assert_eq!(
            titles(&project, false),
            [
                "Escena",
                "Plano",
                "Tipo",
                "Movimiento",
                "Descripción",
                "Duración",
                "Cámara",
                "Imagen"
            ]
        );
        project.add_builtin_column(ColumnKind::ShootOrder);
        assert!(titles(&project, true).contains(&"Rodaje"));
        assert!(titles(&project, true).contains(&"Diálogo"));
        assert!(!titles(&project, false).contains(&"Rodaje"));
        // 1280 de ventana, con la barra lateral (340) y los márgenes del contenido: la tabla compacta no se desplaza.
        let compact: f32 = visible_columns(&project, false)
            .iter()
            .map(|(_, d)| d.min)
            .sum();
        assert!(
            compact <= 1280.0 - tokens::SIDEBAR_W - 2.0 * tokens::CONTENT_PAD - 2.0 * tokens::SP_S
        );
    }

    #[test]
    fn script_text_can_be_imported_into_a_cell() {
        use crate::model::ParagraphKind::*;
        let mut project = Project::default();
        project.paragraphs = vec![
            project.make_paragraph(SceneHeading, "INT. CASA - DÍA"),
            project.make_paragraph(Action, "Ana abre la puerta."),
            project.make_paragraph(Character, "ANA"),
            project.make_paragraph(Dialogue, "¿Hay alguien?"),
            project.make_paragraph(SceneHeading, "EXT. FARO - NOCHE"),
            project.make_paragraph(Action, "Llueve."),
        ];
        let (house, lighthouse) = (project.paragraphs[0].id, project.paragraphs[4].id);
        assert_eq!(
            script_text(&project, Some(house)),
            "INT. CASA - DÍA\n\nAna abre la puerta.\n\nANA\n¿Hay alguien?"
        );
        assert_eq!(
            script_text(&project, Some(lighthouse)),
            "EXT. FARO - NOCHE\n\nLlueve."
        );
        assert!(script_text(&project, None).contains("Llueve."));

        let shot = project.add_shot(house, None);
        assert!(append_to_field(
            &mut project,
            shot,
            Field::Dialogue,
            "¿Hay alguien?"
        ));
        assert!(append_to_field(&mut project, shot, Field::Dialogue, "ANA"));
        assert_eq!(project.shots[0].dialogue, "¿Hay alguien?\nANA");
        let column = project.add_shot_column("Notas");
        assert!(append_to_field(
            &mut project,
            shot,
            Field::Custom(column),
            "x"
        ));
        assert_eq!(project.shots[0].extra[&column], "x");

        // La herramienta pide una casilla elegida; con ella abre la ventana en la escena del plano.
        let mut state = TechState::default();
        state.open_import(&project);
        assert_eq!(state.import.as_ref().map(|i| i.scene), Some(None));
        state.cell = Some((shot, Field::Description));
        state.open_import(&project);
        assert_eq!(state.import.as_ref().and_then(|i| i.scene), Some(house));
    }

    #[test]
    fn the_shot_inspector_shows_every_field_of_the_selected_shot_and_an_empty_state_without_one() {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let p = crate::storage::Settings::default().scheme.palette();
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let shot = project.add_shot(scene, None);
        project.shots[0].description = "Ana abre la puerta.".into();
        let mut state = TechState::default();
        for selected in [None, Some(shot)] {
            for tab in 0..3 {
                state.selected = selected;
                for n in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(360.0, 900.0),
                        )),
                        time: Some(n as f64 * 0.1),
                        focused: true,
                        ..Default::default()
                    };
                    let mut changed = false;
                    let mut o = ctx.run_ui(input, |ui| {
                        changed = right_panel(
                            ui,
                            &mut state,
                            &mut project,
                            &p,
                            crate::storage::Paper::A4,
                            tab,
                        )
                        .changed;
                    });
                    o.textures_delta.clear();
                    assert!(!changed, "solo mirar no cambia nada (pestaña {tab})");
                }
            }
        }
        assert_eq!(project.shots[0].description, "Ana abre la puerta.");
    }

    #[test]
    fn the_storyboard_wraps_into_rows_and_can_hide_the_shots_without_image() {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        for _ in 0..24 {
            project.add_shot(scene, None);
        }
        let mut state = TechState {
            view: View::Storyboard,
            ..Default::default()
        };
        for n in 0..3 {
            frame(&ctx, &mut state, &mut project, vec![], 0.1 * n as f64);
        }
        // Con 24 cuadros de 260 px no entran en una sola fila de 1400 px: pasan a la de abajo.
        let used = ctx.globally_used_rect();
        assert!(
            used.max.x <= 1400.5,
            "las filas no se salen de la ventana: {used:?}"
        );

        let empty_frames = |ctx: &egui::Context,
                            state: &mut TechState,
                            project: &mut Project|
         -> usize {
            fn count(shape: &egui::epaint::Shape, out: &mut usize) {
                match shape {
                    egui::epaint::Shape::Text(t) if t.galley.text() == "Cargar imagen" => *out += 1,
                    egui::epaint::Shape::Vec(items) => items.iter().for_each(|s| count(s, out)),
                    _ => {}
                }
            }
            let p = crate::storage::Settings::default().scheme.palette();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1400.0, 800.0),
                )),
                focused: true,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                show(ui, state, project, &p);
            });
            out.textures_delta.clear();
            let mut n = 0;
            out.shapes.iter().for_each(|c| count(&c.shape, &mut n));
            n
        };
        assert_eq!(empty_frames(&ctx, &mut state, &mut project), 24);

        // Con el filtro, los planos sin imagen no se muestran (y tampoco se borran).
        project.storyboard_only_images = true;
        empty_frames(&ctx, &mut state, &mut project);
        assert_eq!(empty_frames(&ctx, &mut state, &mut project), 0);
        assert_eq!(project.shots.len(), 24);
    }

    #[test]
    fn the_storyboard_window_opens_on_the_selected_shot_in_both_modes() {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let shot = project.add_shot(scene, None);
        project.shots[0].size = "PG".into();
        project.shots[0].description = "Ana abre la puerta.".into();
        let mut state = TechState {
            view: View::Storyboard,
            selected: Some(shot),
            ..Default::default()
        };
        for mode in [Mode::File, Mode::Draw] {
            state.open_board(&project, None, state.selected, mode);
            for n in 0..3 {
                frame(&ctx, &mut state, &mut project, vec![], 0.1 * n as f64);
            }
            assert!(state.board.is_some());
            // Con Esc se cierra sin tocar nada.
            let esc = egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: Some(egui::Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            frame(&ctx, &mut state, &mut project, vec![esc], 1.0);
            frame(&ctx, &mut state, &mut project, vec![], 1.1);
            assert!(state.board.is_none());
        }
        assert_eq!(project.shots.len(), 1);
        assert!(project.shots[0].image.is_none());
    }

    #[test]
    fn a_shot_type_suggestion_can_be_chosen_with_a_click() {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let shot = project.add_shot(scene, None);
        let mut state = TechState::default();
        state.selected = Some(shot);
        frame_inspector(&ctx, &mut state, &mut project, vec![], 0.1);
        ctx.memory_mut(|m| m.request_focus(egui::Id::unique(("plano_size", shot))));
        frame_inspector(&ctx, &mut state, &mut project, vec![], 0.2);
        frame_inspector(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::Text("primer".into())],
            0.3,
        );
        frame_inspector(&ctx, &mut state, &mut project, vec![], 0.4);
        let list = ctx
            .memory(|m| m.area_rect(egui::Id::unique(("plano_size", shot)).with("popup")))
            .expect("hay sugerencias");
        // Sobre la primera de las dos opciones («PP» y «PPP»).
        let at = list.min + vec2(20.0, list.height() * 0.3);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_inspector(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::PointerMoved(at)],
            1.0,
        );
        frame_inspector(&ctx, &mut state, &mut project, vec![button(true)], 1.1);
        frame_inspector(&ctx, &mut state, &mut project, vec![button(false)], 1.2);
        frame_inspector(&ctx, &mut state, &mut project, vec![], 1.3);
        assert_eq!(project.shots[0].size, "PP");
    }

    #[test]
    fn the_optional_columns_draw_with_their_data_and_the_panel_offers_them_until_they_are_added() {
        use crate::model::{BlockKind, ParagraphKind::*};
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.paragraphs = vec![
            project.make_paragraph(SceneHeading, "INT. CASA - DÍA"),
            project.make_paragraph(Character, "ANA"),
            project.make_paragraph(Dialogue, "Hola."),
        ];
        project.auto_link_all();
        let scene = project.paragraphs[0].id;
        let blocking = project.add_blocking(scene);
        let camera = project
            .add_block_item(blocking, BlockKind::Camera, 0.0, 0.0)
            .unwrap();
        let canon = project.add_camera_body("Canon", 1.6);
        project.blockings[0].items[0].body = Some(canon);
        project.blockings[0].items[0].lens_mm = 50.0;
        for _ in 0..2 {
            let shot = project.add_shot(scene, None);
            let s = project.shots.iter_mut().find(|s| s.id == shot).unwrap();
            (s.blocking, s.camera) = (Some(blocking), Some(camera));
            s.support = "Trípode".into();
            s.in_frame = project.characters.iter().map(|c| c.id).collect();
        }
        project.ensure_shot_columns();
        for kind in ColumnKind::OPTIONAL {
            assert!(project.add_builtin_column(kind).is_some(), "{kind:?}");
            assert!(
                project.add_builtin_column(kind).is_none(),
                "una columna no se agrega dos veces"
            );
        }
        // Van antes del storyboard, y se pueden quitar; las de siempre, no.
        let storyboard = project
            .shot_columns
            .iter()
            .position(|c| c.kind == ColumnKind::Storyboard)
            .unwrap();
        assert_eq!(storyboard, project.shot_columns.len() - 1);
        let lens = project
            .shot_columns
            .iter()
            .find(|c| c.kind == ColumnKind::Lens)
            .unwrap()
            .id;
        project.assign_shoot_order();
        let mut state = TechState::default();
        state.selected = Some(project.shots[0].id);
        for n in 0..3 {
            frame(&ctx, &mut state, &mut project, vec![], n as f64 * 0.1);
        }
        assert!(project.remove_shot_column(lens));
        assert!(
            !project.remove_shot_column(project.shot_columns[0].id),
            "Escena no se quita"
        );
        assert!(project.add_builtin_column(ColumnKind::Custom).is_none());
        // El panel dibuja con todo, y la lente que se escribe a mano manda sobre la de la cámara.
        let p = crate::storage::Settings::default().scheme.palette();
        for _ in 0..2 {
            let mut o = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(400.0, 1400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    right_panel(
                        ui,
                        &mut state,
                        &mut project,
                        &p,
                        crate::storage::Paper::A4,
                        1,
                    );
                },
            );
            o.textures_delta.clear();
        }
        let id = project.shots[0].id;
        assert_eq!(
            project.shot_lens(id).map(|(l, e)| (l, e.round())),
            Some((50.0, 80.0))
        );
    }
}
