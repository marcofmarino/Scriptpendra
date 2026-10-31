//! Puesta de cámaras: el bloqueo de cada escena visto desde arriba. Se ubican las personas, los muebles, las paredes
//! y las cámaras (con su ángulo de visión), y cada plano del guion técnico se puede vincular con una de esas cámaras.
//!
//! Las medidas están en metros y el ángulo en grados (0° mira hacia la derecha y crece hacia abajo, como el eje y de
//! la pantalla). El dibujo es el mismo para cualquier cosa que se quiera ubicar en planta, así que la puesta de luces
//! puede apoyarse en este mismo lienzo.

mod inspector;
mod plan_pdf;

use std::collections::HashMap;

use crate::figures;
use crate::motion::{self, GhostMode};
use crate::placement::{self, Spec};
use crate::plan_checks::{self, Range, Seen, cone};
use crate::ui::tokens;
use eframe::egui::{self, Align2, FontId, RichText, Stroke, TextEdit, Ui, pos2, vec2};

use crate::ui::{
    kit,
    screen::{Confirm, Screen, ScreenCx, ScreenKind},
};
use crate::{
    icon,
    model::{
        BlockItem, BlockKind, LightFixture, LightRole, MicKind, MicPattern, ModifierFace,
        ModifierKind, OpeningKind, Project, clean_character_name,
    },
    theme::{self, Palette},
    widgets,
};

const MIN_SCALE: f32 = 18.0;
const MAX_SCALE: f32 = 220.0;
/// Lo más chico que se puede dejar un mueble o una pared, en metros.
const MIN_SIZE: f32 = 0.25;
/// Radio, en píxeles, dentro del cual se agarra un tirador o una posición.
const GRAB: f32 = 10.0;
/// Alto de la línea de tiempo.
/// Medidas de la línea de tiempo: la fila de controles, la regla, cada fila de elemento y cuántas se ven sin desplazar.
const TL_HEADER_H: f32 = 36.0;
const TL_RULER_H: f32 = 20.0;
const TL_ROW_H: f32 = 26.0;
const TL_MAX_ROWS: usize = 4;
const TL_LABEL_W: f32 = 128.0;
const TL_MIN_H: f32 = TL_HEADER_H + TL_RULER_H + TL_ROW_H + 8.0;

/// La herramienta activa en el lienzo (interfaz.md §4.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Seleccionar y mover.
    #[default]
    Select,
    /// Trazar el recorrido de lo seleccionado: cada clic suma una posición.
    Path,
    /// Modo de colocación (plantas.md §4): la figura sigue al mouse como fantasma y un clic la coloca.
    Place(Spec),
    /// Pegar lo copiado: las cosas siguen al mouse como fantasma y un clic las coloca.
    Paste,
    /// Medir la distancia entre dos puntos (plantas.md §6.2). No crea nada.
    Measure,
}

/// Cuántos píxeles por metro son el 100 % del zoom.
const BASE_SCALE: f32 = 60.0;

#[derive(Clone, Copy, Debug)]
enum Drag {
    /// Mover algo: dónde dentro de la cosa se agarró (en metros, respecto de su centro).
    Move {
        item: u64,
        grab: (f32, f32),
    },
    Rotate {
        item: u64,
    },
    /// Cambiar el tamaño desde un lado o esquina: -1, 0 o 1 en cada eje (0 = ese eje no cambia).
    Size {
        item: u64,
        side: (i8, i8),
    },
    /// Mover una de las posiciones de la animación.
    Key {
        item: u64,
        index: usize,
    },
    /// Mover la posición guardada de algo desde su fantasma (`index` vacío es la de partida): dónde dentro de la figura se
    /// agarró.
    Pose {
        item: u64,
        index: Option<usize>,
        grab: (f32, f32),
    },
    /// Dibujar un rectángulo de selección desde un punto, en metros.
    Marquee {
        start: (f32, f32),
    },
    /// Agarrado pero sin hacer nada (una cosa bloqueada).
    Idle,
    /// Mover un punto del trayecto: la posición a la que llega su tramo y cuál es dentro de él.
    Via {
        item: u64,
        key: usize,
        index: usize,
    },
    /// Cambiar la curvatura de un nodo suave del trayecto arrastrando uno de sus tiradores (`back` es el de atrás).
    Handle {
        item: u64,
        node: crate::model::PathNodeRef,
        back: bool,
    },
    /// Mover la figura del operador de un boom.
    Operator {
        item: u64,
    },
    /// Cambiar hasta dónde llega una luz, un micrófono o el cono de una cámara.
    Reach {
        item: u64,
    },
    /// Cambiar la apertura del haz de una luz.
    Beam {
        item: u64,
    },
    /// Girar una posición guardada con la manija de su fantasma.
    PoseRotate {
        item: u64,
        index: Option<usize>,
    },
}

/// El instante en que se edita `item` con la línea de tiempo en `time` (ver `BlockState::edit_time_of`).
fn edit_instant(item: &BlockItem, time: f32, auto_record: bool) -> f32 {
    if auto_record || (time >= crate::model::KEY_EPSILON && item.key_at(time).is_some()) {
        time
    } else {
        0.0
    }
}

/// Un plano con su bloqueo, su cámara y su tramo.
type ShotLink = (u64, Option<u64>, Option<u64>, Option<(f32, f32)>);

/// Lo que el historial de deshacer guarda: los bloqueos y a qué cámara y tramo está vinculado cada plano.
#[derive(Clone, PartialEq)]
struct Snapshot {
    blockings: Vec<crate::model::Blocking>,
    shots: Vec<ShotLink>,
}

impl Snapshot {
    fn of(project: &Project) -> Self {
        Self {
            blockings: project.blockings.clone(),
            shots: project
                .shots
                .iter()
                .map(|s| (s.id, s.blocking, s.camera, s.range))
                .collect(),
        }
    }

    /// Pone en el proyecto lo guardado y devuelve lo que había.
    fn apply(self, project: &mut Project) -> Snapshot {
        let before = Snapshot::of(project);
        project.blockings = self.blockings;
        for (id, blocking, camera, range) in self.shots {
            if let Some(s) = project.shots.iter_mut().find(|s| s.id == id) {
                (s.blocking, s.camera, s.range) = (blocking, camera, range);
            }
        }
        before
    }
}

/// Lo que hay del trayecto de una cosa bajo el mouse.
#[derive(Clone, Copy, Debug)]
enum PathHit {
    /// Un punto intermedio: la posición a la que llega su tramo y cuál es dentro de él.
    Via(usize, usize),
    /// Una posición guardada.
    Key(usize),
    /// Un tirador de curvatura (se agarra para arrastrarlo; el doble clic no hace nada).
    Handle,
    /// El trayecto mismo: el tramo, dónde entraría un punto nuevo y el punto sobre el trayecto.
    Line(usize, usize, (f32, f32)),
}

pub struct BlockState {
    scene: Option<u64>,
    blocking: Option<u64>,
    selected: Option<u64>,
    /// Pestaña del inspector que se pidió abrir (al agregar o elegir algo, se muestra su selección).
    tab_request: Option<usize>,
    /// Píxeles por metro.
    scale: f32,
    /// Dónde cae el (0, 0) del bloqueo respecto de la esquina del lienzo.
    offset: egui::Vec2,
    snap: bool,
    drag: Option<Drag>,
    /// Cosa a centrar en la vista en cuanto se conozca el lienzo.
    center_on: Option<u64>,
    /// Instante que se está viendo, en segundos desde el comienzo del bloqueo.
    time: f32,
    playing: bool,
    /// Tamaño del área de la pantalla en el último cuadro: lo nuevo aparece en su centro.
    view: egui::Vec2,
    /// Bloqueo que se pidió eliminar desde la barra de contexto; la pantalla lo entrega a la app.
    pending_delete: Option<u64>,
    /// Qué planta se está trabajando: cámara, luces o sonido.
    pub layer: Layer,
    /// Mostrar las otras plantas en transparencia, como referencia (el interruptor general).
    pub show_others: bool,
    /// Cuáles de las otras plantas se ven cuando `show_others` está prendido (por `Layer::index`).
    layers: [bool; 4],
    /// Ver el guion técnico del lado, para trabajar la planta con los planos a la vista.
    pub tech_panel: bool,
    /// Ancho del guion técnico al costado.
    tech_w: f32,
    /// Lo que se exporta de las plantas (se recuerda entre exportaciones) y si la ventana está abierta.
    export: plan_pdf::PlanExport,
    export_open: bool,
    pub tool: Tool,
    /// A dónde animar la vista: (escala, desplazamiento).
    view_target: Option<(f32, egui::Vec2)>,
    /// Dónde empezó a dibujarse la pared en curso (la cadena de paredes sigue desde ahí), en metros.
    wall_start: Option<(f32, f32)>,
    /// Dónde se apretó el mouse al colocar algo arrastrando, en metros.
    place_press: Option<(f32, f32)>,
    /// Dónde quedó el operador de un boom cuyo primer clic ya se dio, en metros.
    boom_operator: Option<(f32, f32)>,
    /// Lo que elegiste la última vez en cada herramienta que tiene variantes.
    light_fixture: LightFixture,
    modifier_kind: ModifierKind,
    mic_kind: MicKind,
    round_object: bool,
    opening_kind: OpeningKind,
    /// Cuántos fantasmas de posiciones guardadas se ven.
    pub ghosts: GhostMode,
    /// Grabación automática: mover o girar algo en otro instante crea o corrige su posición en ese instante. Apagada, los
    /// cambios van a la posición de partida.
    pub auto_record: bool,
    /// Velocidad de reproducción: 0,5, 1 o 2.
    speed: f32,
    /// La posición elegida: la cosa y su lugar en `keys` (vacío es la de partida).
    sel_pose: Option<(u64, Option<usize>)>,
    /// El plano elegido en la línea de tiempo, cuyo tramo se resalta en el lienzo.
    sel_shot: Option<u64>,
    /// El plano al que se filtra la planta: su cámara se destaca, el tiempo queda en su tramo y lo demás se atenúa.
    shot_filter: Option<u64>,
    /// Se arrastró la vista mientras se tenía apretado Espacio (entonces al soltarlo no reproduce).
    space_panned: bool,
    /// Alto de la línea de tiempo en el último cuadro.
    tl_h: f32,
    /// Todo lo elegido (`selected` es la principal, la que muestra el inspector). Se rehace sola si `selected` cambia.
    multi: Vec<u64>,
    /// El rectángulo de selección que se está dibujando, en metros.
    marquee: Option<((f32, f32), (f32, f32))>,
    /// Lo copiado, para pegar.
    clipboard: Vec<BlockItem>,
    /// La última medida (dos puntos, en metros) y el primer punto de la que está en curso.
    measure: Option<((f32, f32), (f32, f32))>,
    measure_a: Option<(f32, f32)>,
    /// El punto del trayecto elegido: la cosa, el tramo y cuál es dentro de él.
    sel_via: Option<(u64, usize, usize)>,
    /// Los avisos «en cuadro» del último cuadro, para la lista de la pestaña PLANTA.
    warnings: Vec<plan_checks::Warning>,
    /// Dónde está el cursor sobre el lienzo, en metros.
    cursor: Option<(f32, f32)>,
    /// Mostrar el plano de la locación de rodaje detrás del lienzo.
    pub show_plan: bool,
    /// Historial de deshacer y rehacer: copias de los bloqueos antes de cada cambio.
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Cómo estaban los bloqueos al final del cuadro anterior, para saber qué cambió.
    last_seen: Option<Snapshot>,
    last_change: Option<std::time::Instant>,
}

/// Las tres plantas que comparten la base (paredes, muebles y personas) y cambian lo que se agrega (interfaz.md §4.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    #[default]
    Camera,
    Lights,
    Sound,
    /// La planta de escenografía: paredes, aberturas, muebles y objetos.
    Set,
}

impl Layer {
    /// Su lugar en las listas de plantas.
    fn index(self) -> usize {
        match self {
            Layer::Camera => 0,
            Layer::Lights => 1,
            Layer::Sound => 2,
            Layer::Set => 3,
        }
    }

    #[allow(dead_code)]
    pub const ALL: [Layer; 4] = [Layer::Camera, Layer::Lights, Layer::Sound, Layer::Set];

    /// ¿Se puede tocar esta cosa en la planta actual? La base (paredes, muebles y personas) sí, siempre.
    fn owns(self, kind: BlockKind) -> bool {
        match kind {
            // La base (paredes, muebles y personas) se edita en la planta de cámara; las otras la toman de ahí.
            BlockKind::Actor | BlockKind::Camera => self == Layer::Camera,
            // La escenografía también se edita en su propia planta.
            BlockKind::Object | BlockKind::Wall | BlockKind::Opening => {
                self == Layer::Camera || self == Layer::Set
            }
            BlockKind::Light | BlockKind::Modifier => self == Layer::Lights,
            BlockKind::Mic => self == Layer::Sound,
        }
    }
}

impl Default for BlockState {
    fn default() -> Self {
        Self {
            scene: None,
            blocking: None,
            selected: None,
            tab_request: None,
            scale: 60.0,
            offset: vec2(80.0, 80.0),
            snap: true,
            drag: None,
            center_on: None,
            time: 0.0,
            playing: false,
            view: vec2(800.0, 600.0),
            pending_delete: None,
            layer: Layer::Camera,
            show_others: false,
            layers: [true; 4],
            tech_panel: false,
            tech_w: 300.0,
            export: plan_pdf::PlanExport {
                blocking: None,
                layers: [true, false, false],
                combine: false,
                // El cono tapa mucho de la planta: al exportar va apagado salvo que se pida.
                cones: false,
                legend: true,
            },
            export_open: false,
            tool: Tool::Select,
            view_target: None,
            wall_start: None,
            place_press: None,
            boom_operator: None,
            light_fixture: LightFixture::default(),
            modifier_kind: ModifierKind::default(),
            mic_kind: MicKind::default(),
            round_object: false,
            opening_kind: OpeningKind::default(),
            ghosts: GhostMode::default(),
            auto_record: true,
            speed: 1.0,
            sel_pose: None,
            sel_shot: None,
            shot_filter: None,
            space_panned: false,
            tl_h: TL_MIN_H,
            multi: Vec::new(),
            marquee: None,
            clipboard: Vec::new(),
            measure: None,
            measure_a: None,
            warnings: Vec::new(),
            sel_via: None,
            cursor: None,
            show_plan: true,
            undo: Vec::new(),
            redo: Vec::new(),
            last_seen: None,
            last_change: None,
        }
    }
}

impl BlockState {
    /// Datos de la barra de estado: la escala y la posición del cursor en metros.
    pub fn status(&self) -> Vec<String> {
        let mut out = Vec::new();
        // Mientras se coloca, la ayuda de la herramienta.
        match self.tool {
            Tool::Place(spec) => out.push(
                spec.help(self.boom_operator.is_some(), self.wall_start.is_some())
                    .to_string(),
            ),
            Tool::Paste => out.push(
                "Clic para pegar · Shift para seguir pegando · Esc para cancelar".to_string(),
            ),
            Tool::Measure => {
                out.push("Arrastrá entre dos puntos para medir · Esc para salir".to_string())
            }
            _ => {}
        }
        out.push(format!("Escala: 1 m = {:.0} px", self.scale));
        if let Some((x, y)) = self.cursor {
            out.push(format!("x {:.2} m · y {:.2} m", x, y).replace('.', ","));
        }
        out
    }

    /// Deja `multi` de acuerdo con `selected`: si la principal cambió a algo que no está en `multi`, queda solo eso.
    fn sync_multi(&mut self) {
        if self
            .sel_via
            .is_some_and(|(id, ..)| Some(id) != self.selected)
        {
            self.sel_via = None;
        }
        match self.selected {
            Some(id) if !self.multi.contains(&id) => self.multi = vec![id],
            None => self.multi.clear(),
            _ => {}
        }
    }

    /// ¿Se ve, como referencia, la planta `layer`?
    pub fn layer_shown(&self, layer: Layer) -> bool {
        self.show_others && self.layers[layer.index()]
    }

    /// Marca o desmarca una planta de referencia. El interruptor general se prende con la primera que se marca y se apaga
    /// cuando no queda ninguna.
    fn toggle_layer(&mut self, layer: Layer) {
        if !self.show_others {
            self.show_others = true;
            self.layers = [false; 4];
            self.layers[layer.index()] = true;
        } else {
            self.layers[layer.index()] = !self.layers[layer.index()];
        }
        let current = self.layer;
        if ![Layer::Camera, Layer::Lights, Layer::Sound]
            .iter()
            .any(|l| *l != current && self.layers[l.index()])
        {
            self.show_others = false;
            self.layers = [true; 4];
        }
    }

    /// Pasa a otra planta (cámara, luces, sonido o escenografía) y deja la selección y la herramienta.
    pub fn set_layer(&mut self, layer: Layer) {
        if self.layer != layer {
            self.layer = layer;
            self.selected = None;
            self.set_tool(Tool::Select);
        }
    }

    /// Cambia de herramienta y deja lo que estaba a medio hacer (una pared en curso, el primer clic de un boom).
    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.place_press = None;
        self.wall_start = None;
        self.boom_operator = None;
        self.measure = None;
        self.measure_a = None;
    }

    /// El zoom actual en porcentaje (100 % son 60 píxeles por metro).
    fn percent(&self) -> u32 {
        (self.scale / BASE_SCALE * 100.0).round() as u32
    }

    /// Acerca o aleja la vista alrededor del centro, con animación.
    fn zoom_by(&mut self, ctx: &egui::Context, factor: f32) {
        let target = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let center = self.view / 2.0;
        let offset = center - (center - self.offset) * (target / self.scale);
        self.start_view_animation(ctx, target, offset);
    }

    /// Ajusta la vista para que entre todo lo dibujado.
    fn fit(&mut self, ctx: &egui::Context, project: &Project) {
        let Some(b) = self
            .blocking
            .and_then(|id| project.blockings.iter().find(|b| b.id == id))
        else {
            return;
        };
        let (mut min, mut max) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for item in &b.items {
            for (x, y) in corners(item)
                .into_iter()
                .chain(item.keys.iter().map(|k| (k.x, k.y)))
            {
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
            }
        }
        if min.0 > max.0 {
            self.start_view_animation(
                ctx,
                BASE_SCALE,
                vec2(crate::ui::tokens::SP_XXL, crate::ui::tokens::SP_XXL),
            );
            return;
        }
        let pad = crate::ui::tokens::SP_XXL;
        let (w, h) = ((max.0 - min.0).max(1.0), (max.1 - min.1).max(1.0));
        let scale = ((self.view.x - pad * 2.0) / w)
            .min((self.view.y - pad * 2.0) / h)
            .clamp(MIN_SCALE, MAX_SCALE);
        let offset = vec2(
            (self.view.x - w * scale) / 2.0 - min.0 * scale,
            (self.view.y - h * scale) / 2.0 - min.1 * scale,
        );
        self.start_view_animation(ctx, scale, offset);
    }

    fn start_view_animation(&mut self, ctx: &egui::Context, scale: f32, offset: egui::Vec2) {
        // Se parte de donde está la vista: así la animación no salta.
        for (key, value) in [
            ("s", self.scale),
            ("x", self.offset.x),
            ("y", self.offset.y),
        ] {
            ctx.animate_value_with_time(egui::Id::unique("plant_view").with(key), value, 0.0);
        }
        self.view_target = Some((scale, offset));
    }

    /// Avanza la animación del zoom (interpolado con los botones y «Ajustar»; con la rueda es inmediato).
    fn step_view_animation(&mut self, ctx: &egui::Context) {
        let Some((scale, offset)) = self.view_target else {
            return;
        };
        let t = crate::ui::kit::anim(crate::ui::tokens::ANIM);
        let id = egui::Id::unique("plant_view");
        self.scale = ctx.animate_value_with_time(id.with("s"), scale, t);
        self.offset = vec2(
            ctx.animate_value_with_time(id.with("x"), offset.x, t),
            ctx.animate_value_with_time(id.with("y"), offset.y, t),
        );
        if (self.scale - scale).abs() < 0.01 && (self.offset - offset).length() < 0.1 {
            self.view_target = None;
        }
    }

    /// Anota un cambio en el historial de deshacer. Los cambios seguidos (un arrastre) cuentan como uno solo.
    pub fn record(&mut self, project: &Project) {
        let now = std::time::Instant::now();
        let current = Snapshot::of(project);
        match &self.last_seen {
            Some(seen) if *seen != current => {
                let burst = self
                    .last_change
                    .is_some_and(|t| now.duration_since(t) < std::time::Duration::from_millis(400));
                if !burst {
                    self.undo.push(seen.clone());
                    // Se descartan los más viejos, no los nuevos.
                    if self.undo.len() > 100 {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                }
                self.last_change = Some(now);
                self.last_seen = Some(current);
            }
            None => self.last_seen = Some(current),
            _ => {}
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Deshace el último cambio. Devuelve si cambió algo.
    pub fn undo(&mut self, project: &mut Project) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let now = previous.apply(project);
        self.redo.push(now);
        self.after_history_step(project);
        true
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let now = next.apply(project);
        self.undo.push(now);
        self.after_history_step(project);
        true
    }

    /// Después de deshacer o rehacer: lo elegido que ya no existe se suelta y lo que sigue es lo que se ve.
    fn after_history_step(&mut self, project: &Project) {
        self.last_seen = Some(Snapshot::of(project));
        self.last_change = None;
        let exists = |id: u64| {
            project
                .blockings
                .iter()
                .any(|b| b.items.iter().any(|i| i.id == id))
        };
        self.multi.retain(|id| exists(*id));
        self.selected = self.selected.filter(|id| exists(*id));
        self.sel_pose = None;
        self.sel_via = None;
    }

    /// Muestra la puesta de cámaras de una escena.
    pub fn show_scene(&mut self, scene: u64) {
        self.scene = Some(scene);
        self.blocking = None;
        self.selected = None;
    }

    /// Va a una cámara de un bloqueo (desde la tabla del guion técnico).
    pub fn go_to(&mut self, project: &Project, blocking: u64, camera: u64) {
        if let Some(b) = project.blockings.iter().find(|b| b.id == blocking) {
            self.scene = Some(b.scene);
            self.blocking = Some(b.id);
            self.selected = Some(camera);
            self.center_on = Some(camera);
        }
    }
}

#[derive(Default)]
pub struct Out {
    pub changed: bool,
    /// Pedido de borrar un bloqueo entero (la app confirma si tiene contenido).
    pub delete_blocking: Option<u64>,
}

// ------------------------------------------------------------------------------------------------
// Geometría
// ------------------------------------------------------------------------------------------------

fn rotate(v: (f32, f32), degrees: f32) -> (f32, f32) {
    let (s, c) = degrees.to_radians().sin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

/// ¿Está el punto (en metros) sobre la cosa? `pad` agranda el blanco para que sea fácil de agarrar.
fn hit(item: &BlockItem, point: (f32, f32), pad: f32) -> bool {
    let local = rotate((point.0 - item.x, point.1 - item.y), -item.rot);
    match item.kind {
        BlockKind::Actor => local.0.hypot(local.1) <= item.w / 2.0 + pad,
        BlockKind::Wall => {
            local.0.abs() <= item.w / 2.0 + pad && local.1.abs() <= item.h.max(0.3) / 2.0 + pad
        }
        _ => local.0.abs() <= item.w / 2.0 + pad && local.1.abs() <= item.h / 2.0 + pad,
    }
}

/// Las cuatro esquinas de un rectángulo ya girado, en metros.
fn corners(item: &BlockItem) -> [(f32, f32); 4] {
    let (hw, hh) = (item.w / 2.0, item.h / 2.0);
    [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|c| {
        let r = rotate(c, item.rot);
        (item.x + r.0, item.y + r.1)
    })
}

/// Hacia dónde queda el tirador de giro respecto de la dirección de la cosa: las personas y las cámaras lo llevan
/// delante (hacia donde miran); los muebles y las paredes, arriba.
fn handle_dir(kind: BlockKind) -> f32 {
    match kind {
        BlockKind::Actor
        | BlockKind::Camera
        | BlockKind::Light
        | BlockKind::Modifier
        | BlockKind::Mic => 0.0,
        BlockKind::Object | BlockKind::Opening | BlockKind::Wall => -90.0,
    }
}

/// A cuántos píxeles del borde está el tirador de giro (los muebles lo llevan más lejos para no pisar los de tamaño).
fn handle_pad(kind: BlockKind) -> f32 {
    match kind {
        BlockKind::Actor
        | BlockKind::Camera
        | BlockKind::Light
        | BlockKind::Modifier
        | BlockKind::Mic => 14.0,
        BlockKind::Object | BlockKind::Opening | BlockKind::Wall => 28.0,
    }
}

/// Dónde está el tirador para girar, a `pad` metros del borde.
fn handle(item: &BlockItem, pad: f32) -> (f32, f32) {
    let reach = match item.kind {
        BlockKind::Actor
        | BlockKind::Camera
        | BlockKind::Light
        | BlockKind::Modifier
        | BlockKind::Mic => item.w.max(item.h) / 2.0,
        BlockKind::Wall => item.h.max(0.15) / 2.0,
        BlockKind::Object | BlockKind::Opening => item.h / 2.0,
    };
    let r = rotate((reach + pad, 0.0), item.rot + handle_dir(item.kind));
    (item.x + r.0, item.y + r.1)
}

/// Los lados y esquinas desde donde se cambia el tamaño: los muebles por los ocho, las paredes por sus dos puntas.
fn size_sides(kind: BlockKind) -> Vec<(i8, i8)> {
    match kind {
        BlockKind::Object => vec![
            (-1, -1),
            (0, -1),
            (1, -1),
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
        ],
        // Las paredes, las aberturas y los modificadores solo cambian de largo: por sus dos puntas.
        BlockKind::Wall | BlockKind::Opening | BlockKind::Modifier => vec![(-1, 0), (1, 0)],
        BlockKind::Actor | BlockKind::Camera | BlockKind::Light | BlockKind::Mic => Vec::new(),
    }
}

/// Dónde cae, en metros, el tirador de un lado o esquina.
fn size_handle(item: &BlockItem, side: (i8, i8)) -> (f32, f32) {
    let r = rotate(
        (side.0 as f32 * item.w / 2.0, side.1 as f32 * item.h / 2.0),
        item.rot,
    );
    (item.x + r.0, item.y + r.1)
}

/// Estira desde un lado o esquina sin mover el opuesto, llevándolo hasta `point`. Devuelve centro y medidas nuevos:
/// (x, y, ancho, alto).
fn resized(
    item: &BlockItem,
    side: (i8, i8),
    point: (f32, f32),
    snap: bool,
) -> (f32, f32, f32, f32) {
    let local = rotate((point.0 - item.x, point.1 - item.y), -item.rot);
    let axis = |s: i8, size: f32, at: f32| {
        if s == 0 {
            return (size, 0.0);
        }
        let s = s as f32;
        let opposite = -s * size / 2.0;
        let new = snapped((at - opposite) * s, 0.25, snap).max(MIN_SIZE);
        (new, opposite + s * new / 2.0)
    };
    let (w, cx) = axis(side.0, item.w, local.0);
    let (h, cy) = axis(side.1, item.h, local.1);
    let shift = rotate((cx, cy), item.rot);
    (item.x + shift.0, item.y + shift.1, w, h)
}

/// Las cosas en el lugar que ocupan en el instante `t`.
fn posed(items: &[BlockItem], t: f32, fov_of: &dyn Fn(&BlockItem) -> f32) -> Vec<BlockItem> {
    items
        .iter()
        .map(|i| {
            let (x, y, rot) = crate::model::pose_in(items, i, t);
            // La lente de una cámara cambia con el tiempo si alguna posición lleva su zoom.
            let mut lensed = BlockItem {
                lens_mm: i.lens_at(t),
                ..i.clone()
            };
            // Y el operador de un boom, si se lo animó.
            if let (Some(op), Some(spec)) = (i.operator_at(t), lensed.mic.as_mut()) {
                spec.operator = Some(op);
            }
            BlockItem {
                x,
                y,
                rot,
                fov: fov_of(&lensed),
                ..lensed
            }
        })
        .collect()
}

/// Los puntos del cono de visión de una cámara, en metros.
fn snapped(value: f32, step: f32, on: bool) -> f32 {
    if on {
        (value / step).round() * step
    } else {
        value
    }
}

// ------------------------------------------------------------------------------------------------
// Pantalla
// ------------------------------------------------------------------------------------------------

// ------------------------------------------------------------------------------------------------
// Edición de lo elegido (docs/plantas.md §5.4 y §6)
// ------------------------------------------------------------------------------------------------

/// Lo que se puede hacer con lo elegido desde el menú contextual o con una tecla.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditAct {
    Duplicate,
    Copy,
    Paste,
    ToggleHidden,
    ToggleLocked,
    Front,
    Back,
    Delete,
    ShowShots,
}

/// Las cosas elegidas de la planta, tal como están.
fn chosen_items<'a>(state: &BlockState, project: &'a Project, blocking: u64) -> Vec<&'a BlockItem> {
    project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .map(|b| {
            b.items
                .iter()
                .filter(|i| state.multi.contains(&i.id))
                .collect()
        })
        .unwrap_or_default()
}

/// Aplica una acción a lo elegido. Devuelve si cambió el proyecto.
fn apply_act(state: &mut BlockState, project: &mut Project, blocking: u64, act: EditAct) -> bool {
    let ids = state.multi.clone();
    match act {
        EditAct::Copy => {
            state.clipboard = chosen_items(state, project, blocking)
                .into_iter()
                .cloned()
                .collect();
            false
        }
        EditAct::Paste => {
            if !state.clipboard.is_empty() {
                state.set_tool(Tool::Paste);
            }
            false
        }
        EditAct::Duplicate => {
            let copies: Vec<BlockItem> = chosen_items(state, project, blocking)
                .into_iter()
                .cloned()
                .collect();
            let added = project.paste_block_items(blocking, &copies, 0.5, 0.5);
            if added.is_empty() {
                return false;
            }
            state.selected = added.last().copied();
            state.multi = added;
            state.sel_pose = None;
            true
        }
        EditAct::ToggleHidden | EditAct::ToggleLocked => {
            let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) else {
                return false;
            };
            let hide = act == EditAct::ToggleHidden;
            let flag = |i: &BlockItem| if hide { i.hidden } else { i.locked };
            // Si todo lo elegido ya está así, se deshace; si no, se aplica a todo.
            let all = b.items.iter().filter(|i| ids.contains(&i.id)).all(flag);
            for i in b.items.iter_mut().filter(|i| ids.contains(&i.id)) {
                if hide {
                    i.hidden = !all;
                } else {
                    i.locked = !all;
                }
            }
            if hide {
                // Lo oculto deja de estar elegido.
                let hidden: Vec<u64> = b.items.iter().filter(|i| i.hidden).map(|i| i.id).collect();
                state.multi.retain(|id| !hidden.contains(id));
                state.selected = state.multi.last().copied();
            }
            !ids.is_empty()
        }
        EditAct::Front | EditAct::Back => {
            let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) else {
                return false;
            };
            let (picked, rest): (Vec<BlockItem>, Vec<BlockItem>) =
                b.items.drain(..).partition(|i| ids.contains(&i.id));
            b.items = if act == EditAct::Front {
                rest.into_iter().chain(picked).collect()
            } else {
                picked.into_iter().chain(rest).collect()
            };
            !ids.is_empty()
        }
        EditAct::Delete => {
            for id in &ids {
                project.remove_block_item(blocking, *id);
            }
            state.multi.clear();
            state.selected = None;
            state.sel_pose = None;
            !ids.is_empty()
        }
        EditAct::ShowShots => {
            // Los planos vinculados se ven en la selección de la cámara, en el inspector.
            state.tab_request = Some(0);
            false
        }
    }
}

/// Las teclas de edición (plantas.md §5.4): flechas mueven 0,1 m (con Shift, 0,5), R gira 15°, H oculta, Ctrl+L bloquea,
/// Ctrl+D duplica, Ctrl+C y Ctrl+V copian y pegan, y 0 ajusta la vista a todo.
fn edit_keys(ui: &Ui, state: &mut BlockState, project: &mut Project, blocking: u64) -> bool {
    if ui.ctx().egui_wants_keyboard_input() {
        return false;
    }
    let (command, alt, shift) =
        ui.input(|i| (i.modifiers.command, i.modifiers.alt, i.modifiers.shift));
    let pressed = |key: egui::Key| ui.input(|i| i.key_pressed(key));
    let mut changed = false;
    if command && !alt {
        // Deshacer y rehacer (Ctrl+Z, Ctrl+Shift+Z o Ctrl+Y).
        if pressed(egui::Key::Z) {
            return if shift {
                state.redo(project)
            } else {
                state.undo(project)
            };
        }
        if pressed(egui::Key::Y) {
            return state.redo(project);
        }
        for (key, act) in [
            (egui::Key::D, EditAct::Duplicate),
            (egui::Key::C, EditAct::Copy),
            (egui::Key::V, EditAct::Paste),
            (egui::Key::L, EditAct::ToggleLocked),
        ] {
            if pressed(key) && (!state.multi.is_empty() || act == EditAct::Paste) {
                changed |= apply_act(state, project, blocking, act);
            }
        }
        return changed;
    }
    if command || alt {
        return false;
    }
    if pressed(egui::Key::H) && !state.multi.is_empty() {
        changed |= apply_act(state, project, blocking, EditAct::ToggleHidden);
    }
    if pressed(egui::Key::Num0) && !shift {
        state.fit(ui.ctx(), project);
    }
    if state.multi.is_empty() {
        return changed;
    }
    let step = if shift { 0.5 } else { 0.1 };
    let (mut dx, mut dy) = (0.0, 0.0);
    for (key, d) in [
        (egui::Key::ArrowLeft, (-step, 0.0)),
        (egui::Key::ArrowRight, (step, 0.0)),
        (egui::Key::ArrowUp, (0.0, -step)),
        (egui::Key::ArrowDown, (0.0, step)),
    ] {
        if pressed(key) {
            (dx, dy) = (dx + d.0, dy + d.1);
        }
    }
    let turn = if pressed(egui::Key::R) {
        if shift { -15.0 } else { 15.0 }
    } else {
        0.0
    };
    if dx != 0.0 || dy != 0.0 || turn != 0.0 {
        let (now, auto) = (state.time, state.auto_record);
        if let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) {
            for i in b
                .items
                .iter_mut()
                .filter(|i| state.multi.contains(&i.id) && !i.locked && !i.is_attached_lavalier())
            {
                let time = edit_instant(i, now, auto);
                let (x, y, rot) = i.pose_at(time);
                i.set_pose_at(time, x + dx, y + dy, (rot + turn).rem_euclid(360.0));
                changed = true;
            }
        }
    }
    changed
}

/// Pegar: lo copiado sigue al mouse como fantasma (por su centro) y un clic lo coloca. Con Shift al hacer clic se sigue
/// pegando; Esc o el clic derecho cancelan.
fn paste_step(
    ui: &Ui,
    state: &mut BlockState,
    project: &mut Project,
    blocking: u64,
    resp: &egui::Response,
    to_world: &dyn Fn(egui::Pos2) -> (f32, f32),
) -> (bool, Option<Ghost>) {
    let (shift, alt, esc) = ui.input(|i| {
        (
            i.modifiers.shift,
            i.modifiers.alt,
            i.key_pressed(egui::Key::Escape),
        )
    });
    if esc || resp.secondary_clicked() || state.clipboard.is_empty() {
        state.set_tool(Tool::Select);
        return (false, None);
    }
    let n = state.clipboard.len() as f32;
    let centre = (
        state.clipboard.iter().map(|i| i.x).sum::<f32>() / n,
        state.clipboard.iter().map(|i| i.y).sum::<f32>() / n,
    );
    let snap_on = state.snap && !alt;
    let here = ui
        .input(|i| i.pointer.latest_pos())
        .filter(|_| resp.hovered())
        .map(|pos| placement::snap(to_world(pos), 0.25, snap_on));
    let mut ghost = Ghost {
        item: None,
        ok: true,
        aim: None,
        length: None,
        dot: None,
        highlight: None,
        more: Vec::new(),
    };
    if let Some(at) = here {
        let (dx, dy) = (at.0 - centre.0, at.1 - centre.1);
        let mut moved: Vec<BlockItem> = state
            .clipboard
            .iter()
            .map(|i| BlockItem {
                x: i.x + dx,
                y: i.y + dy,
                keys: Vec::new(),
                ..i.clone()
            })
            .collect();
        ghost.item = Some(moved.remove(0));
        ghost.more = moved;
    }
    let mut changed = false;
    if let (true, Some(pos)) = (resp.clicked(), resp.interact_pointer_pos()) {
        let at = placement::snap(to_world(pos), 0.25, snap_on);
        let items = state.clipboard.clone();
        let added = project.paste_block_items(blocking, &items, at.0 - centre.0, at.1 - centre.1);
        if !added.is_empty() {
            state.selected = added.last().copied();
            state.multi = added;
            state.sel_pose = None;
            state.tab_request = Some(0);
            changed = true;
        }
        if !shift {
            state.set_tool(Tool::Select);
        }
    }
    (changed, Some(ghost))
}

/// Medir: un arrastre (o dos clics) entre dos puntos muestra la distancia en metros. No crea nada.
fn measure_step(
    ui: &Ui,
    state: &mut BlockState,
    resp: &egui::Response,
    to_world: &dyn Fn(egui::Pos2) -> (f32, f32),
) {
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) || resp.secondary_clicked() {
        state.set_tool(Tool::Select);
        return;
    }
    let pointer = ui.input(|i| i.pointer.latest_pos()).map(to_world);
    let press = if resp.drag_started() {
        ui.input(|i| i.pointer.press_origin()).map(to_world)
    } else {
        None
    };
    let release = if resp.drag_stopped() { pointer } else { None };
    let click = if resp.clicked() {
        resp.interact_pointer_pos().map(to_world)
    } else {
        None
    };
    for point in [press, release.filter(|_| state.measure_a.is_some()), click]
        .into_iter()
        .flatten()
    {
        match state.measure_a {
            None => {
                state.measure_a = Some(point);
                state.measure = None;
            }
            Some(a) if (a.0 - point.0).hypot(a.1 - point.1) >= 0.05 => {
                state.measure = Some((a, point));
                state.measure_a = None;
            }
            Some(_) => {}
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Movimiento (docs/plantas.md §5)
// ------------------------------------------------------------------------------------------------

/// El tramo del plano elegido en la línea de tiempo: su cámara y desde cuándo hasta cuándo.
fn shot_span_of(project: &Project, state: &BlockState, blocking: u64) -> Option<(u64, f32, f32)> {
    let shot = project
        .shots
        .iter()
        .find(|s| Some(s.id) == state.sel_shot)?;
    let duration = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)?
        .duration;
    let (from, to) = motion::shot_span(shot.range, duration);
    Some((shot.camera?, from, to))
}

/// La cosa `now` (en su lugar del instante que se ve) puesta en una posición guardada: lo que dibuja un fantasma.
fn pose_item(now: &BlockItem, pos: &motion::Position) -> BlockItem {
    BlockItem {
        x: pos.x,
        y: pos.y,
        rot: pos.rot,
        keys: Vec::new(),
        ..now.clone()
    }
}

impl BlockState {
    /// Reproduce o pausa; si ya llegó al final, vuelve a empezar.
    fn toggle_play(&mut self, duration: f32) {
        if !self.playing && self.time >= duration - 0.01 {
            self.time = 0.0;
        }
        self.playing = !self.playing;
    }

    /// Lleva la línea de tiempo a `t` y elige la posición de `item` que esté ahí, si hay una.
    fn go_to_time(&mut self, project: &Project, item: u64, t: f32) {
        self.time = t;
        self.playing = false;
        self.sel_pose = project
            .blockings
            .iter()
            .flat_map(|b| b.items.iter())
            .find(|i| i.id == item)
            .and_then(|i| {
                if t < crate::model::KEY_EPSILON {
                    (!i.keys.is_empty()).then_some((item, None))
                } else {
                    i.key_at(t).map(|n| (item, Some(n)))
                }
            });
    }
}

/// Los atajos del movimiento: Espacio reproduce o pausa (al soltarlo, si no se usó para mover la vista), coma y punto van a
/// la posición anterior y a la siguiente de lo elegido, y K crea una posición en el instante que se ve.
fn motion_keys(
    ui: &Ui,
    state: &mut BlockState,
    project: &mut Project,
    blocking: u64,
    _resp: &egui::Response,
) -> bool {
    if ui.ctx().egui_wants_keyboard_input() || !ui.input(|i| i.modifiers.is_none()) {
        return false;
    }
    let pressed = |key: egui::Key| ui.input(|i| i.key_pressed(key));
    let mut changed = false;
    let duration = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .map_or(0.0, |b| b.duration);
    if pressed(egui::Key::Space) {
        state.space_panned = false;
    }
    if ui.input(|i| i.key_released(egui::Key::Space)) && !state.space_panned {
        state.toggle_play(duration);
    }
    let Some(id) = state.selected else {
        return false;
    };
    let time = state.time;
    let item = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .and_then(|b| b.items.iter().find(|i| i.id == id))
        .cloned();
    let Some(item) = item else { return false };
    if pressed(egui::Key::Comma) || pressed(egui::Key::Period) {
        let (before, after) = motion::neighbours(&item, time);
        let target = if pressed(egui::Key::Comma) {
            before
        } else {
            after
        };
        if let Some(t) = target {
            state.go_to_time(project, id, t);
        }
    }
    if pressed(egui::Key::K) && item.animatable() && time >= crate::model::KEY_EPSILON {
        // Una posición en el instante actual, donde está la figura en ese momento.
        if item.key_at(time).is_none() {
            let (x, y, rot) = item.pose_at(time);
            if let Some(i) = project
                .blockings
                .iter_mut()
                .find(|b| b.id == blocking)
                .and_then(|b| b.items.iter_mut().find(|i| i.id == id))
            {
                i.set_pose_at(time, x, y, rot);
                changed = true;
            }
        }
        state.go_to_time(project, id, time);
        state.playing = false;
    }
    changed
}

// ------------------------------------------------------------------------------------------------
// Modo de colocación (docs/plantas.md §4)
// ------------------------------------------------------------------------------------------------

/// Lo que se ve mientras se coloca algo: el fantasma y sus ayudas. Es solo de la interfaz: no entra al proyecto ni al
/// historial de deshacer hasta el clic.
struct Ghost {
    /// La figura que sigue al mouse, al 50 %.
    item: Option<BlockItem>,
    /// Si soltarla acá es posible (una puerta solo sobre una pared, un corbatero solo sobre una persona).
    ok: bool,
    /// La línea que muestra hacia dónde va a mirar lo que se arrastra.
    aim: Option<((f32, f32), (f32, f32))>,
    /// El largo en metros de la pared que se está dibujando, y dónde mostrarlo.
    length: Option<((f32, f32), f32)>,
    /// Un punto que marca dónde va a empezar algo (el inicio de una pared).
    dot: Option<(f32, f32)>,
    /// La persona que se resalta al pasar con un corbatero.
    highlight: Option<u64>,
    /// Las demás cosas de lo que se pega (la primera va en `item`).
    more: Vec<BlockItem>,
}

/// Lo que cambia entre una cosa y otra al colocarla.
#[derive(Default)]
struct Extra {
    rot: f32,
    size: Option<(f32, f32)>,
    wall: Option<u64>,
    attached: Option<u64>,
    operator: Option<(f32, f32)>,
}

/// Deja `item` (recién armada con los datos de la herramienta) en `at` con lo que trae `extra`.
fn finish(item: &mut BlockItem, at: (f32, f32), extra: &Extra) {
    (item.x, item.y, item.rot) = (at.0, at.1, extra.rot);
    if let Some((w, h)) = extra.size {
        (item.w, item.h) = (w, h);
    }
    item.wall = extra.wall;
    if let Some(mic) = item.mic.as_mut() {
        mic.attached_to = extra.attached;
        mic.operator = extra.operator;
    }
}

/// La cosa que se colocaría, armada como la armaría el proyecto (letra de cámara, nombre y pista que siguen).
fn spec_item(
    project: &Project,
    blocking: u64,
    spec: Spec,
    at: (f32, f32),
    extra: &Extra,
) -> Option<BlockItem> {
    let b = project.blockings.iter().find(|b| b.id == blocking)?;
    let mut item = BlockItem::template(
        spec.kind(),
        0,
        0.0,
        0.0,
        b.next_label(spec.kind()),
        b.next_track(spec.kind()),
    );
    spec.apply(&mut item);
    finish(&mut item, at, extra);
    Some(item)
}

/// Coloca la cosa en el proyecto, de una sola vez (un paso del historial), y la deja elegida. Devuelve si se agregó.
fn place_item(
    state: &mut BlockState,
    project: &mut Project,
    blocking: u64,
    spec: Spec,
    at: (f32, f32),
    extra: &Extra,
) -> bool {
    let added = project.add_block_item_with(blocking, spec.kind(), at.0, at.1, |item| {
        spec.apply(item);
        finish(item, at, extra);
    });
    state.selected = added.or(state.selected);
    state.tab_request = Some(0);
    // Con un plano filtrado, lo que se agrega queda relacionado con él: la cámara pasa a ser la suya si no tenía, y todo lo
    // demás (micrófonos, luces…) se anota en el plano.
    if let (Some(id), Some(shot)) = (added, state.shot_filter)
        && let Some(shot) = project.shots.iter_mut().find(|s| s.id == shot)
    {
        if spec.kind() == BlockKind::Camera && shot.camera.is_none() {
            (shot.blocking, shot.camera) = (Some(blocking), Some(id));
        } else if !matches!(spec.kind(), BlockKind::Wall | BlockKind::Opening | BlockKind::Object) {
            shot.items.push(id);
        }
    }
    added.is_some()
}

/// Un paso del modo de colocación en este cuadro: sigue al mouse con el fantasma, coloca con el clic o al soltar un
/// arrastre, y cancela con Esc o el clic derecho. Devuelve si cambió el proyecto y lo que hay que dibujar.
#[allow(clippy::too_many_arguments)]
fn placement_step(
    ui: &Ui,
    state: &mut BlockState,
    project: &mut Project,
    spec: Spec,
    blocking: u64,
    resp: &egui::Response,
    items: &[BlockItem],
    to_world: &dyn Fn(egui::Pos2) -> (f32, f32),
    scale: f32,
) -> (bool, Option<Ghost>) {
    use placement::{angle_between, opening_on, person_under, snap, snap_to_wall_ends, wall_near};
    let dist = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
    let (shift, alt, esc) = ui.input(|i| {
        (
            i.modifiers.shift,
            i.modifiers.alt,
            i.key_pressed(egui::Key::Escape),
        )
    });
    // Esc termina la cadena de paredes en curso; la segunda vez, o el clic derecho, cancela la herramienta.
    if esc || resp.secondary_clicked() {
        if esc && spec == Spec::Wall && state.wall_start.is_some() {
            state.wall_start = None;
        } else {
            state.set_tool(Tool::Select);
        }
        return (false, None);
    }
    let snap_on = state.snap && !alt;
    let snapped_pt = |p: (f32, f32)| snap(p, 0.25, snap_on);
    let pointer = ui.input(|i| i.pointer.latest_pos()).map(to_world);
    // El fantasma solo está mientras el mouse está sobre el lienzo (o se arrastra desde él).
    let here = if resp.hovered() || resp.dragged() {
        pointer
    } else {
        None
    };
    let press = if resp.drag_started() {
        ui.input(|i| i.pointer.press_origin())
            .map(|pos| snapped_pt(to_world(pos)))
    } else {
        None
    };
    let release = if resp.drag_stopped() {
        pointer.map(snapped_pt)
    } else {
        None
    };
    let click = if resp.clicked() {
        resp.interact_pointer_pos()
            .map(|pos| snapped_pt(to_world(pos)))
    } else {
        None
    };
    let mut changed = false;
    let mut ghost = Ghost {
        item: None,
        ok: true,
        aim: None,
        length: None,
        dot: None,
        highlight: None,
        more: Vec::new(),
    };
    // Con Shift al hacer clic, la herramienta sigue activa; si no, se suelta.
    let done = |state: &mut BlockState, keep: bool| {
        if keep {
            state.place_press = None;
        } else {
            state.set_tool(Tool::Select);
        }
    };
    match spec {
        Spec::Wall => {
            let end = |p: (f32, f32)| snap_to_wall_ends(p, items, placement::WALL_SNAP_M);
            if resp.double_clicked() || resp.triple_clicked() {
                // Doble clic: termina la cadena (el primer clic del doble ya cerró la última pared).
                state.wall_start = None;
                done(state, shift);
                return (false, None);
            }
            // Un arrastre vale por dos clics: uno donde se apretó y otro donde se soltó.
            let events: Vec<(f32, f32)> =
                [press, release.filter(|_| state.wall_start.is_some()), click]
                    .into_iter()
                    .flatten()
                    .map(end)
                    .collect();
            for pt in events {
                match state.wall_start {
                    None => state.wall_start = Some(pt),
                    Some(a) if dist(a, pt) >= MIN_SIZE => {
                        let rot = (pt.1 - a.1)
                            .atan2(pt.0 - a.0)
                            .to_degrees()
                            .rem_euclid(360.0);
                        let extra = Extra {
                            rot,
                            size: Some((dist(a, pt), 0.15)),
                            ..Extra::default()
                        };
                        let mid = ((a.0 + pt.0) / 2.0, (a.1 + pt.1) / 2.0);
                        changed |= place_item(state, project, blocking, spec, mid, &extra);
                        state.wall_start = Some(pt);
                    }
                    Some(_) => {}
                }
            }
            let cursor = here.map(snapped_pt).map(end);
            match (state.wall_start, cursor) {
                (Some(a), Some(c)) if dist(a, c) >= 0.05 => {
                    let rot = (c.1 - a.1).atan2(c.0 - a.0).to_degrees().rem_euclid(360.0);
                    let extra = Extra {
                        rot,
                        size: Some((dist(a, c), 0.15)),
                        ..Extra::default()
                    };
                    let mid = ((a.0 + c.0) / 2.0, (a.1 + c.1) / 2.0);
                    ghost.item = spec_item(project, blocking, spec, mid, &extra);
                    ghost.length = Some((mid, dist(a, c)));
                    ghost.dot = Some(a);
                }
                (Some(a), _) => ghost.dot = Some(a),
                (None, c) => ghost.dot = c,
            }
        }
        Spec::Opening(_) => {
            let tol = placement::OPENING_TOL_M;
            if let Some(a) = press {
                state.place_press = Some(a);
            }
            let from = state.place_press.or(here.map(snapped_pt));
            let wall = from.and_then(|a| wall_near(items, a, tol));
            let to = if state.place_press.is_some() {
                pointer.map(snapped_pt)
            } else {
                from
            };
            let default_w = spec_item(project, blocking, spec, (0.0, 0.0), &Extra::default())
                .map_or(0.9, |i| i.w);
            let build = |wall: &placement::OnWall, from, to| {
                let (center, width) = opening_on(wall, from, to, default_w);
                (
                    center,
                    Extra {
                        rot: wall.rot,
                        size: Some((width, wall.thick)),
                        wall: Some(wall.wall),
                        ..Extra::default()
                    },
                )
            };
            ghost.ok = wall.is_some();
            if let (Some(w), Some(a), Some(b)) = (wall, from, to) {
                let (center, extra) = build(&w, a, b);
                ghost.item = spec_item(project, blocking, spec, center, &extra);
            }
            let commit = match (release, click) {
                (Some(rel), _) => state.place_press.zip(Some(rel)),
                (None, Some(c)) => Some((c, c)),
                _ => None,
            };
            if let Some((a, b)) = commit {
                if let Some(w) = wall_near(items, a, tol) {
                    let (center, extra) = build(&w, a, b);
                    changed |= place_item(state, project, blocking, spec, center, &extra);
                    done(state, shift && click.is_some());
                }
                state.place_press = None;
            }
        }
        Spec::Mic(MicKind::Lavalier) => {
            let person = here.and_then(|h| person_under(items, h, 6.0 / scale));
            let person_item = person.and_then(|id| items.iter().find(|i| i.id == id));
            ghost.ok = person.is_some();
            ghost.highlight = person;
            if let Some(person) = person_item {
                let at = crate::model::lavalier_spot(person);
                let extra = Extra {
                    rot: person.rot,
                    attached: Some(person.id),
                    ..Extra::default()
                };
                ghost.item = spec_item(project, blocking, spec, at, &extra);
                if click.is_some() {
                    changed |= place_item(state, project, blocking, spec, at, &extra);
                    done(state, shift);
                }
            }
        }
        Spec::Mic(MicKind::Boom) => {
            // El primer clic ubica al operador y el segundo la cápsula; la caña se estira entre los dos.
            for pt in [
                press,
                release.filter(|_| state.boom_operator.is_some()),
                click,
            ]
            .into_iter()
            .flatten()
            {
                match state.boom_operator {
                    None => state.boom_operator = Some(pt),
                    Some(op) if dist(op, pt) >= 0.2 => {
                        let extra = Extra {
                            rot: angle_between(op, pt, shift),
                            operator: Some(op),
                            ..Extra::default()
                        };
                        changed |= place_item(state, project, blocking, spec, pt, &extra);
                        state.boom_operator = None;
                        done(state, shift && click.is_some());
                    }
                    Some(_) => {}
                }
            }
            if state.tool == Tool::Place(spec) {
                if let Some(c) = here.map(snapped_pt) {
                    let rot = state
                        .boom_operator
                        .map_or(0.0, |op| angle_between(op, c, shift));
                    let extra = Extra {
                        rot,
                        operator: state.boom_operator,
                        ..Extra::default()
                    };
                    ghost.item = spec_item(project, blocking, spec, c, &extra);
                    ghost.dot = state.boom_operator;
                }
            }
        }
        Spec::Object { .. } => {
            if let Some(a) = press {
                state.place_press = Some(a);
            }
            let from = state.place_press;
            let rect = |a: (f32, f32), b: (f32, f32)| -> Option<((f32, f32), Extra)> {
                let (w, h) = ((a.0 - b.0).abs(), (a.1 - b.1).abs());
                (w >= MIN_SIZE && h >= MIN_SIZE).then(|| {
                    (
                        ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
                        Extra {
                            size: Some((w, h)),
                            ..Extra::default()
                        },
                    )
                })
            };
            // Arrastrando se dibuja el rectángulo (o el círculo); si no, uno de 1 × 1 m en el mouse.
            let (at, extra) = match (from, pointer.map(snapped_pt)) {
                (Some(a), Some(b)) => rect(a, b).unwrap_or((a, Extra::default())),
                _ => (here.map(snapped_pt).unwrap_or_default(), Extra::default()),
            };
            if from.is_some() || here.is_some() {
                ghost.item = spec_item(project, blocking, spec, at, &extra);
            }
            let commit = match (release, click) {
                (Some(rel), _) => from.map(|a| rect(a, rel).unwrap_or((a, Extra::default()))),
                (None, Some(c)) => Some((c, Extra::default())),
                _ => None,
            };
            if let Some((at, extra)) = commit {
                changed |= place_item(state, project, blocking, spec, at, &extra);
                done(state, shift && click.is_some());
            }
        }
        _ => {
            // Una cosa con orientación: un clic la coloca mirando hacia la dirección de siempre; apretar y arrastrar
            // la orienta (con Shift, de a 15°).
            if let Some(a) = press {
                state.place_press = Some(a);
            }
            let from = state.place_press;
            let aim = |a: (f32, f32)| match pointer {
                Some(c) if dist(a, c) >= 0.05 => angle_between(a, c, shift),
                _ => 0.0,
            };
            match (from, here.map(snapped_pt)) {
                (Some(a), _) => {
                    ghost.item = spec_item(
                        project,
                        blocking,
                        spec,
                        a,
                        &Extra {
                            rot: aim(a),
                            ..Extra::default()
                        },
                    );
                    ghost.aim = pointer.map(|c| (a, c));
                }
                (None, Some(c)) => {
                    ghost.item = spec_item(project, blocking, spec, c, &Extra::default())
                }
                _ => {}
            }
            if let (Some(_), Some(a)) = (release, from) {
                changed |= place_item(
                    state,
                    project,
                    blocking,
                    spec,
                    a,
                    &Extra {
                        rot: aim(a),
                        ..Extra::default()
                    },
                );
                done(state, false);
            } else if let Some(c) = click {
                changed |= place_item(state, project, blocking, spec, c, &Extra::default());
                done(state, shift);
            }
        }
    }
    (changed, Some(ghost))
}

/// Las teclas de las herramientas (plantas.md §5.4): cada planta solo responde a las de sus herramientas.
fn tool_keys(ui: &Ui, state: &mut BlockState, has_people: bool) {
    if ui.ctx().egui_wants_keyboard_input() || !ui.input(|i| i.modifiers.is_none()) {
        return;
    }
    let pressed = |key: egui::Key| ui.input(|i| i.key_pressed(key));
    let layer = state.layer;
    let current = state.tool;
    if pressed(egui::Key::V) {
        state.set_tool(Tool::Select);
    }
    let mut pick: Option<Spec> = None;
    if layer == Layer::Camera {
        if pressed(egui::Key::C) {
            pick = Some(Spec::Camera);
        }
        if pressed(egui::Key::P) {
            pick = Some(Spec::Person);
        }
        if pressed(egui::Key::W) {
            pick = Some(Spec::Wall);
        }
        if pressed(egui::Key::O) {
            // Otra vez O, alterna entre rectangular y redondo.
            if matches!(current, Tool::Place(Spec::Object { .. })) {
                state.round_object = !state.round_object;
            }
            pick = Some(Spec::Object {
                round: state.round_object,
            });
        }
        if pressed(egui::Key::D) {
            // Otra vez D, alterna entre puerta y ventana.
            if let Tool::Place(Spec::Opening(kind)) = current {
                state.opening_kind = if kind == OpeningKind::Door {
                    OpeningKind::Window
                } else {
                    OpeningKind::Door
                };
            }
            pick = Some(Spec::Opening(state.opening_kind));
        }
    }
    if layer == Layer::Lights {
        if pressed(egui::Key::L) {
            pick = Some(Spec::Light(state.light_fixture));
        }
        if pressed(egui::Key::B) {
            if matches!(current, Tool::Place(Spec::Modifier(_))) {
                let all = ModifierKind::ALL;
                state.modifier_kind = all[(all
                    .iter()
                    .position(|k| *k == state.modifier_kind)
                    .unwrap_or(0)
                    + 1)
                    % all.len()];
            }
            pick = Some(Spec::Modifier(state.modifier_kind));
        }
    }
    if layer == Layer::Sound && pressed(egui::Key::M) {
        // Otra vez M, pasa al tipo siguiente; sin personas, salta el corbatero.
        if matches!(current, Tool::Place(Spec::Mic(_))) {
            let all = MicKind::ALL;
            let mut next = all.iter().position(|k| *k == state.mic_kind).unwrap_or(0);
            loop {
                next = (next + 1) % all.len();
                if has_people || all[next] != MicKind::Lavalier {
                    break;
                }
            }
            state.mic_kind = all[next];
        }
        pick = Some(Spec::Mic(state.mic_kind));
    }
    // Sin personas no hay corbatero que colocar.
    if let Some(spec) = pick.filter(|s| has_people || *s != Spec::Mic(MicKind::Lavalier)) {
        state.set_tool(Tool::Place(spec));
    }
}

/// Dibuja lo que acompaña al fantasma: la línea de orientación, el punto de inicio y el largo de la pared en curso.
fn draw_overlay(
    painter: &egui::Painter,
    p: &Palette,
    ghost: &Ghost,
    to_screen: &dyn Fn((f32, f32)) -> egui::Pos2,
) {
    if let Some((a, b)) = ghost.aim {
        painter.line_segment([to_screen(a), to_screen(b)], Stroke::new(1.5, p.accent));
    }
    if let Some(at) = ghost.dot {
        painter.circle_filled(to_screen(at), 4.0, p.accent);
    }
    if let Some((at, length)) = ghost.length {
        let text = format!("{length:.2} m").replace('.', ",");
        let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), p.text);
        let pos = to_screen(at) + vec2(0.0, -14.0);
        let back = egui::Rect::from_center_size(pos, galley.size() + vec2(8.0, 4.0));
        painter.rect_filled(back, 3, p.canvas);
        painter.galley(back.center() - galley.size() / 2.0, galley, p.text);
    }
}

/// La planta que se trabaja, con su nombre para el selector de la barra de herramientas.
fn layer_title(layer: Layer) -> &'static str {
    match layer {
        Layer::Camera => "Planta de cámara",
        Layer::Lights => "Planta de luces",
        Layer::Sound => "Planta de sonido",
        Layer::Set => "Planta de escenografía",
    }
}

/// Un desplegable de ancho fijo (más angosto que el de `kit::dropdown`), con el texto recortado.
fn compact_dropdown(ui: &mut Ui, id: &str, options: &[String], active: usize, width: f32) -> Option<usize> {
    let mut picked = None;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .truncate()
        .selected_text(options.get(active).map_or("", String::as_str))
        .show_ui(ui, |ui| {
            for (i, option) in options.iter().enumerate() {
                if ui.add(egui::Button::selectable(i == active, option.as_str()).truncate()).clicked() {
                    picked = Some(i);
                }
            }
        });
    picked
}

/// Escena, variante de la planta y plano al que se filtra: arriba, junto a «Otras plantas», para que la barra de herramientas
/// quede solo con las herramientas.
fn scene_selectors(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) {
    use crate::ui::kit;
    let scenes = project.script_scenes();
    let numbers = project.scene_labels();
    let scene = sync_selection(state, project);
    {
            if scenes.is_empty() {
                return;
            }
            let labels: Vec<String> = (0..scenes.len())
                .map(|n| format!("{}. {}", numbers[n].1, scenes[n].1.trim()))
                .collect();
            let current = scenes.iter().position(|(s, _)| *s == scene).unwrap_or(0);
            if let Some(i) = kit::stepper_select(ui, p, "plant_scene", &labels, current) {
                state.scene = Some(scenes[i].0);
            }
            // Si la escena tiene más de una planta de este tipo, su selector de variante.
            let mine: Vec<(u64, String)> = project
                .blockings
                .iter()
                .filter(|b| b.scene == scene)
                .map(|b| (b.id, b.name.clone()))
                .collect();
            if mine.len() > 1 {
                let names: Vec<String> = mine.iter().map(|(_, n)| n.clone()).collect();
                let active = mine
                    .iter()
                    .position(|(id, _)| Some(*id) == state.blocking)
                    .unwrap_or(0);
                if let Some(i) = kit::dropdown(ui, "plant_variant", &names, active) {
                    state.blocking = Some(mine[i].0);
                    state.selected = None;
                }
            }
            // El filtro por plano: trabajar la planta con un solo plano a la vista.
            let shots: Vec<(u64, String, Option<(u64, u64)>, Option<f32>)> = {
                let numbers = project.shot_numbers();
                project
                    .shots
                    .iter()
                    .filter(|s| s.scene == scene)
                    .map(|s| {
                        let n = numbers.get(&s.id).copied().unwrap_or(0);
                        let label = if s.size.is_empty() {
                            format!("Plano {n}")
                        } else {
                            format!("Plano {n} · {}", s.size)
                        };
                        (s.id, label, s.blocking.zip(s.camera), s.range.map(|r| r.0))
                    })
                    .collect()
            };
            if !shots.is_empty() {
                let mut labels = vec!["Todos los planos".to_string()];
                labels.extend(shots.iter().map(|s| s.1.clone()));
                let active = state
                    .shot_filter
                    .and_then(|id| shots.iter().position(|s| s.0 == id))
                    .map_or(0, |n| n + 1);
                if let Some(i) = compact_dropdown(ui, "plant_shot_filter", &labels, active, 130.0) {
                    if i == 0 {
                        state.shot_filter = None;
                        state.sel_shot = None;
                    } else {
                        let (id, _, camera, from) = shots[i - 1].clone();
                        state.shot_filter = Some(id);
                        state.sel_shot = Some(id);
                        if let Some((blocking, cam)) = camera {
                            state.go_to(project, blocking, cam);
                        }
                        state.time = from.unwrap_or(0.0);
                    }
                }
            }
    }
}

/// Barra de herramientas (centro): «Otras plantas», con una casilla por cada una para elegir cuáles se ven como referencia. La
/// escenografía (paredes, muebles y personas) se ve siempre.
fn toolbar_ui(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) -> bool {
    // Deshacer y rehacer los dibuja el marco.
    crate::ui::kit::toolbar_groups(ui, "plants_tools", &[3, 2], |ui, group| {
        if group == 0 {
            scene_selectors(ui, state, project, p);
            return;
        }
        let any = state.show_others;
        ui.menu_button(
            format!(
                "{}  Otras plantas{}  {}",
                icon::EYE,
                if any { " ·" } else { "" },
                icon::CARET_DOWN
            ),
            |ui| {
                ui.label(RichText::new("Ver como referencia, en transparencia").size(crate::ui::tokens::FONT_SMALL));
                for layer in [Layer::Camera, Layer::Lights, Layer::Sound] {
                    if layer == state.layer {
                        continue;
                    }
                    let mut on = state.layer_shown(layer);
                    if ui.checkbox(&mut on, layer_title(layer)).changed() {
                        state.toggle_layer(layer);
                    }
                }
                ui.separator();
                ui.label(RichText::new("La escenografía se ve siempre").size(crate::ui::tokens::FONT_SMALL));
            },
        )
        .response
        .on_hover_text("Elegí qué otras plantas ver como referencia");
    });
    false
}


/// Las herramientas de colocación de cada planta: qué se coloca y con qué icono.
fn place_tools(layer: Layer, state: &BlockState) -> Vec<(Spec, &'static str)> {
    match layer {
        Layer::Camera => vec![
            (Spec::Camera, icon::VIDEO_CAMERA),
            (Spec::Person, icon::USER),
            (
                Spec::Object {
                    round: state.round_object,
                },
                icon::CUBE,
            ),
            (Spec::Wall, icon::WALL),
            (Spec::Opening(OpeningKind::Door), icon::DOOR),
            (Spec::Opening(OpeningKind::Window), icon::RECTANGLE),
        ],
        Layer::Lights => vec![
            (Spec::Light(state.light_fixture), icon::LAMP),
            (Spec::Modifier(ModifierKind::Bounce), icon::FLASHLIGHT),
            (Spec::Modifier(ModifierKind::Flag), icon::FLAG),
            (Spec::Modifier(ModifierKind::Diffuser), icon::CLOUD),
        ],
        Layer::Set => vec![
            (Spec::Wall, icon::WALL),
            (Spec::Opening(OpeningKind::Door), icon::DOOR),
            (Spec::Opening(OpeningKind::Window), icon::RECTANGLE),
            (
                Spec::Object {
                    round: state.round_object,
                },
                icon::CUBE,
            ),
        ],
        Layer::Sound => vec![
            (Spec::Mic(MicKind::Boom), icon::MICROPHONE_STAGE),
            (Spec::Mic(MicKind::Lavalier), icon::MICROPHONE),
            (Spec::Mic(MicKind::Plant), icon::WAVEFORM),
            (Spec::Mic(MicKind::Stand), icon::BROADCAST),
        ],
    }
}

/// ¿Hay personas en la planta que se trabaja? Sin ellas no se puede colocar un corbatero.
fn has_people(project: &Project, blocking: Option<u64>) -> bool {
    project
        .blockings
        .iter()
        .find(|b| Some(b.id) == blocking)
        .is_some_and(|b| {
            b.items
                .iter()
                .any(|i| i.kind == BlockKind::Actor && !i.hidden)
        })
}

/// Deja la escena y el bloqueo elegidos en algo que existe. Devuelve la escena (0 si no hay ninguna).
fn sync_selection(state: &mut BlockState, project: &Project) -> u64 {
    let scenes = project.script_scenes();
    if state
        .scene
        .is_none_or(|id| !scenes.iter().any(|(s, _)| *s == id))
    {
        state.scene = scenes.first().map(|(id, _)| *id);
    }
    let scene = state.scene.unwrap_or(0);
    if state.blocking.is_none_or(|id| {
        !project
            .blockings
            .iter()
            .any(|b| b.id == id && b.scene == scene)
    }) {
        state.blocking = project
            .blockings
            .iter()
            .find(|b| b.scene == scene)
            .map(|b| b.id);
        state.selected = None;
    }
    scene
}

/// Barra de contexto: la escena con flechas y su variante, las herramientas de edición y el zoom del lienzo.
fn context_ui(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) -> bool {
    use crate::ui::kit;
    sync_selection(state, project);
    let changed = false;
    let mut zoom: Option<f32> = None;
    let mut fit = false;
    let mut toggle_tech = false;
    let mut tool = state.tool;
    let mut place: Option<Spec> = None;
    let mut fixture = None;
    let people = has_people(project, state.blocking);
    let layer = state.layer;
    let percent = state.percent();
    kit::sides(
        ui,
        |ui| {
            kit::toolbar_groups(ui, "plants_context", &[3, 2, 1], |ui, group| match group {
                0 => {
                    for (t, code, tip) in [
                        (
                            Tool::Select,
                            icon::ARROWS_OUT_CARDINAL,
                            "Seleccionar y mover (V)",
                        ),
                        (
                            Tool::Path,
                            icon::PATH,
                            "Trazar recorrido: cada clic suma una posición a lo seleccionado",
                        ),
                        (
                            Tool::Measure,
                            icon::RULER,
                            "Medir: arrastrá entre dos puntos para ver la distancia",
                        ),
                    ] {
                        let resp = kit::icon_button(ui, p, code, tip, true);
                        if state.tool == t {
                            ui.painter().rect_stroke(
                                resp.rect,
                                crate::ui::tokens::R_CONTROL,
                                egui::Stroke::new(1.0, p.accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if resp.clicked() {
                            tool = t;
                        }
                    }
                }
                1 => {
                    ui.label(
                        RichText::new(match layer {
                            Layer::Camera => "Cámara",
                            Layer::Lights => "Luces",
                            Layer::Sound => "Sonido",
                            Layer::Set => "Escenografía",
                        })
                        .size(crate::ui::tokens::FONT_SMALL)
                        .color(p.dim),
                    );
                    for (spec, code) in place_tools(layer, state) {
                        let mut tip = format!("Añadir {} ({})", spec.name(), spec.key());
                        let usable = state.blocking.is_some()
                            && (people || spec != Spec::Mic(MicKind::Lavalier));
                        if !people && spec == Spec::Mic(MicKind::Lavalier) {
                            tip = "Un corbatero se coloca sobre una persona: agregá una en la planta de cámara".into();
                        }
                        let resp = kit::icon_button(ui, p, code, &tip, usable);
                        // La herramienta activa lleva el borde de acento.
                        if state.tool == Tool::Place(spec) {
                            ui.painter().rect_stroke(
                                resp.rect,
                                crate::ui::tokens::R_CONTROL,
                                egui::Stroke::new(1.0, p.accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if resp.clicked() && usable {
                            // Elegir la herramienta no coloca nada: la figura sigue al mouse hasta el clic.
                            place = Some(spec);
                        }
                        // El aparato de la luz se elige al lado de su botón.
                        if matches!(spec, Spec::Light(_)) {
                            let labels: Vec<String> = LightFixture::ALL
                                .iter()
                                .map(|f| f.label().to_string())
                                .collect();
                            let active = LightFixture::ALL
                                .iter()
                                .position(|f| *f == state.light_fixture)
                                .unwrap_or(0);
                            if let Some(i) = kit::dropdown(ui, "plant_fixture", &labels, active) {
                                fixture = Some(LightFixture::ALL[i]);
                            }
                        }
                    }
                    // Los muebles y objetos con figura propia, en la planta de escenografía.
                    if layer == Layer::Set {
                        ui.menu_button(format!("{}  Muebles", icon::CUBE), |ui| {
                            for (shape, name, ..) in crate::model::ObjectShape::FURNITURE {
                                if ui
                                    .add_enabled(state.blocking.is_some(), egui::Button::new(name))
                                    .clicked()
                                {
                                    place = Some(Spec::Furniture(shape));
                                    ui.close();
                                }
                            }
                        });
                    }
                }
                _ => {
                    // Cuántos fantasmas de posiciones se ven (plantas.md §5.1): un desplegable chico, para que no tape las herramientas.
                    let labels: Vec<String> = GhostMode::ALL.iter().map(|m| format!("Posiciones: {}", m.label())).collect();
                    let active = GhostMode::ALL
                        .iter()
                        .position(|m| *m == state.ghosts)
                        .unwrap_or(1);
                    if let Some(i) = compact_dropdown(ui, "plant_ghosts", &labels, active, 150.0) {
                        state.ghosts = GhostMode::ALL[i];
                    }
                }
            });
        },
        |ui| {
            if kit::icon_button(
                ui,
                p,
                icon::FILM_STRIP,
                "Mostrar u ocultar el guion técnico al costado",
                true,
            )
            .clicked()
            {
                toggle_tech = true;
            }
            if ui
                .button("Ajustar")
                .on_hover_text("Ajustar la vista a todo lo dibujado")
                .clicked()
            {
                fit = true;
            }
            if kit::icon_button(ui, p, icon::PLUS, "Acercar", true).clicked() {
                zoom = Some(1.25);
            }
            ui.label(RichText::new(format!("{percent} %")).color(p.text));
            if kit::icon_button(ui, p, icon::MINUS, "Alejar", true).clicked() {
                zoom = Some(0.8);
            }
        },
    );
    if toggle_tech {
        state.tech_panel = !state.tech_panel;
    }
    if tool != state.tool {
        state.set_tool(tool);
    }
    if let Some(f) = fixture {
        state.light_fixture = f;
        if matches!(state.tool, Tool::Place(Spec::Light(_))) {
            state.set_tool(Tool::Place(Spec::Light(f)));
        }
    }
    if let Some(spec) = place {
        match spec {
            Spec::Modifier(kind) => state.modifier_kind = kind,
            Spec::Mic(kind) => state.mic_kind = kind,
            Spec::Opening(kind) => state.opening_kind = kind,
            _ => {}
        }
        state.set_tool(Tool::Place(spec));
    }
    let ctx = ui.ctx().clone();
    if let Some(f) = zoom {
        state.zoom_by(&ctx, f);
    }
    if fit {
        state.fit(&ctx, project);
    }
    changed
}

pub fn show(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) -> Out {
    let mut out = Out::default();
    let scenes = project.script_scenes();
    state.view = ui.available_size() - vec2(0.0, state.tl_h);
    out.delete_blocking = state.pending_delete.take();
    if scenes.is_empty() {
        crate::ui::kit::empty_state(
            ui,
            p,
            icon::VIDEO_CAMERA,
            "Todavía no hay escenas. Escribí un encabezado de escena en el Guion y acá podés armar sus plantas.",
            None,
        );
        return out;
    }
    let scene = sync_selection(state, project);
    // Con el guion técnico al costado, el espacio se parte en dos rectángulos para que no se pisen.
    let full = ui.available_rect_before_wrap();
    let delete = out.delete_blocking.take();
    if state.tech_panel {
        let w = state.tech_w.clamp(220.0, (full.width() * 0.5).max(220.0));
        let side = egui::Rect::from_min_max(egui::pos2(full.right() - w, full.min.y), full.max);
        let main = egui::Rect::from_min_max(
            full.min,
            egui::pos2(side.left() - crate::ui::tokens::SP_XS, full.max.y),
        );
        let grip = egui::Rect::from_x_y_ranges(
            (side.left() - crate::ui::tokens::SP_XS)..=(side.left() + 2.0),
            side.y_range(),
        );
        let resp = ui
            .interact(grip, egui::Id::unique("blocking_tech_grip"), egui::Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if resp.dragged() {
            state.tech_w = (w - resp.drag_delta().x).clamp(220.0, 560.0);
        }
        tech_panel(ui, side, state, project, p, scene);
        let mut out = Out::default();
        ui.scope_builder(egui::UiBuilder::new().max_rect(main), |ui| {
            ui.set_clip_rect(main.intersect(ui.clip_rect()));
            out = show_plan(ui, state, project, p, scene, delete);
        });
        return out;
    }
    show_plan(ui, state, project, p, scene, delete)
}

fn show_plan(
    ui: &mut Ui,
    state: &mut BlockState,
    project: &mut Project,
    p: &Palette,
    scene: u64,
    delete: Option<u64>,
) -> Out {
    let mut out = Out::default();
    state.view = ui.available_size() - vec2(0.0, state.tl_h);
    out.delete_blocking = delete;

    let Some(blocking) = state.blocking else {
        if crate::ui::kit::empty_state(
            ui,
            p,
            icon::VIDEO_CAMERA,
            "Esta escena todavía no tiene una planta.",
            Some("Crear la planta de esta escena"),
        ) {
            state.blocking = Some(project.add_blocking(scene));
            out.changed = true;
        }
        return out;
    };
    state.step_view_animation(ui.ctx());
    // Reproduciendo, el tiempo corre solo hasta el final.
    let duration = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .map_or(0.0, |b| b.duration);
    state.time = state.time.clamp(0.0, duration);
    // Con un plano filtrado, el tiempo se queda en su tramo y la reproducción termina al final de él.
    if state
        .shot_filter
        .is_some_and(|id| !project.shots.iter().any(|s| s.id == id && s.scene == scene))
    {
        state.shot_filter = None;
    }
    let span = state
        .shot_filter
        .and_then(|id| project.shots.iter().find(|s| s.id == id))
        .map(|s| motion::shot_span(s.range, duration));
    let end = span.map_or(duration, |(_, to)| to.min(duration));
    if let Some((from, to)) = span {
        state.time = state.time.clamp(from.min(duration), to.min(duration));
    }
    if state.playing {
        state.time += ui.input(|i| i.stable_dt) * state.speed;
        if state.time >= end {
            state.time = end;
            state.playing = false;
        }
        ui.ctx().request_repaint();
    }
    out.changed |= canvas(ui, state, project, p, blocking);
    out.changed |= camera_type_dialog(ui.ctx(), project, p, blocking);
    out.changed |= timeline(ui, state, project, p, blocking);
    out
}

/// Al poner la primera cámara, pregunta qué tipo de cámara (sensor) se usa: queda como la de fábrica de las cámaras nuevas, así no
/// hay que configurarla cada vez. Devuelve si cambió el proyecto.
fn camera_type_dialog(ctx: &egui::Context, project: &mut Project, p: &Palette, blocking: u64) -> bool {
    let has_camera = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .is_some_and(|b| b.items.iter().any(|i| i.kind == BlockKind::Camera));
    if project.camera_body_asked || !has_camera {
        return false;
    }
    let pick: std::cell::Cell<Option<Option<u64>>> = std::cell::Cell::new(None);
    let mut preset: Option<(&'static str, f32)> = None;
    let closed = crate::ui::kit::dialog(
        ctx,
        p,
        "camera_type",
        "¿Qué cámara usás?",
        crate::ui::kit::DialogWidth::Normal,
        |ui| {
            ui.label(
                RichText::new("Define el sensor de las cámaras nuevas, así no hay que configurarlo en cada una. Se puede cambiar después.")
                    .color(p.dim),
            );
            ui.add_space(crate::ui::tokens::SP_S);
            for (name, crop) in crate::optics::BODY_PRESETS {
                let existing = project.camera_bodies.iter().find(|b| b.name == name).map(|b| b.id);
                if ui
                    .add(egui::Button::new(format!("{name}  (×{crop})")).min_size(vec2(ui.available_width(), 0.0)))
                    .clicked()
                {
                    match (existing, crop) {
                        (Some(id), _) => pick.set(Some(Some(id))),
                        (None, c) if c == 1.0 => pick.set(Some(None)),
                        _ => preset = Some((name, crop)),
                    }
                }
            }
            let others: Vec<(u64, String)> = project
                .camera_bodies
                .iter()
                .filter(|b| !crate::optics::BODY_PRESETS.iter().any(|(n, _)| *n == b.name))
                .map(|b| (b.id, b.name.clone()))
                .collect();
            for (id, name) in others {
                if ui
                    .add(egui::Button::new(name).min_size(vec2(ui.available_width(), 0.0)))
                    .clicked()
                {
                    pick.set(Some(Some(id)));
                }
            }
        },
        |ui| {
            if ui.button("Más tarde").clicked() {
                pick.set(Some(None));
            }
        },
    );
    if let Some((name, crop)) = preset {
        pick.set(Some(Some(project.add_camera_body(name, crop))));
    }
    if closed && pick.get().is_none() {
        pick.set(Some(None));
    }
    let Some(body) = pick.get() else {
        return false;
    };
    project.camera_body_asked = true;
    project.camera_body_default = body;
    // La cámara que ya está puesta toma el mismo sensor.
    for b in &mut project.blockings {
        for i in b.items.iter_mut().filter(|i| i.kind == BlockKind::Camera && i.body.is_none()) {
            i.body = body;
        }
    }
    true
}

/// El guion técnico de la escena a la derecha de la planta (solo lectura): cada plano con su tipo, su descripción y su cámara.
/// Un clic en un plano resalta su tramo en la línea de tiempo y va a su cámara.
fn tech_panel(
    ui: &mut Ui,
    area: egui::Rect,
    state: &mut BlockState,
    project: &Project,
    p: &Palette,
    scene: u64,
) {
    let numbers = project.shot_numbers();
    let mut go: Option<(u64, u64)> = None;
    let mut pick: Option<u64> = None;
    ui.painter().rect_filled(area, 0, p.panel);
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(area.shrink(crate::ui::tokens::SP_S)),
        |ui| {
            ui.set_clip_rect(area.intersect(ui.clip_rect()));
            ui.label(RichText::new("Guion técnico").font(egui::FontId::new(
                crate::ui::tokens::FONT_CARD_TITLE,
                theme::bold_family(),
            )));
            ui.add_space(crate::ui::tokens::SP_XS);
            let shots: Vec<&crate::model::Shot> =
                project.shots.iter().filter(|s| s.scene == scene).collect();
            if shots.is_empty() {
                ui.label(
                    RichText::new("Esta escena todavía no tiene planos. Armalos en el Guion técnico.")
                        .size(crate::ui::tokens::FONT_SMALL)
                        .color(p.dim),
                );
            }
            egui::ScrollArea::vertical()
                .id_salt("blocking_tech_panel_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for shot in shots {
                        let n = numbers.get(&shot.id).copied().unwrap_or(0);
                        let selected = state.sel_shot == Some(shot.id);
                        let head = [shot.size.as_str(), shot.movement.as_str(), shot.angle.as_str()]
                            .iter()
                            .filter(|t| !t.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" · ");
                        let linked = shot.blocking.zip(shot.camera);
                        let frame = egui::Frame::new()
                            .fill(if selected { p.selection } else { p.container })
                            .corner_radius(crate::ui::tokens::R_CONTROL)
                            .inner_margin(egui::Margin::same(crate::ui::tokens::SP_S as i8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new(format!("{n}  {head}"))
                                        .size(crate::ui::tokens::FONT_SMALL)
                                        .color(p.accent),
                                );
                                if !shot.description.trim().is_empty() {
                                    ui.label(RichText::new(shot.description.trim()).color(p.text));
                                }
                                for (text, color) in [(&shot.dialogue, p.text), (&shot.sound, p.dim)] {
                                    if !text.trim().is_empty() {
                                        ui.label(
                                            RichText::new(text.trim())
                                                .size(crate::ui::tokens::FONT_SMALL)
                                                .color(color),
                                        );
                                    }
                                }
                            });
                        let resp = ui
                            .interact(
                                frame.response.rect,
                                ui.scope_id().with(("tech_shot", shot.id)),
                                egui::Sense::click(),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if resp.clicked() {
                            pick = Some(shot.id);
                            go = linked;
                        }
                        ui.add_space(crate::ui::tokens::SP_XS);
                    }
                });
        });
    if let Some(id) = pick {
        state.sel_shot = Some(id);
    }
    if let Some((blocking, camera)) = go {
        state.go_to(project, blocking, camera);
    }
}

/// El lienzo: cuadrícula, cosas y las interacciones (mover, girar, estirar, animar, arrastrar la vista y acercar).
fn canvas(
    ui: &mut Ui,
    state: &mut BlockState,
    project: &mut Project,
    p: &Palette,
    blocking: u64,
) -> bool {
    let mut changed = false;
    let size = ui.available_size() - vec2(0.0, state.tl_h);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(size.x, size.y.max(60.0)),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0, p.canvas);
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("blocking_canvas"), rect));

    let raw = |project: &Project| {
        project
            .blockings
            .iter()
            .find(|b| b.id == blocking)
            .map(|b| b.items.clone())
            .unwrap_or_default()
    };
    if let Some(target) = state.center_on.take() {
        if let Some(item) = raw(project).iter().find(|i| i.id == target) {
            let (x, y, _) = item.pose_at(state.time);
            state.offset = rect.size() / 2.0 - vec2(x, y) * state.scale;
        }
    }

    // Acercar con la rueda (o pellizcando), siempre hacia el punto donde está el mouse.
    let hover = resp.hover_pos();
    let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
    if let (Some(pos), true) = (hover, scroll != 0.0 || pinch != 1.0) {
        let world = (
            (pos.x - rect.min.x - state.offset.x) / state.scale,
            (pos.y - rect.min.y - state.offset.y) / state.scale,
        );
        state.scale = (state.scale * (scroll * 0.0015).exp() * pinch).clamp(MIN_SCALE, MAX_SCALE);
        state.offset = vec2(
            pos.x - rect.min.x - world.0 * state.scale,
            pos.y - rect.min.y - world.1 * state.scale,
        );
    }
    let scale = state.scale;
    let transforms = move |offset: egui::Vec2| {
        (
            move |(x, y): (f32, f32)| {
                pos2(
                    rect.min.x + offset.x + x * scale,
                    rect.min.y + offset.y + y * scale,
                )
            },
            move |pos: egui::Pos2| {
                (
                    (pos.x - rect.min.x - offset.x) / scale,
                    (pos.y - rect.min.y - offset.y) / scale,
                )
            },
        )
    };
    let (to_screen, to_world) = transforms(state.offset);

    // Cuadrícula de 1 m, con una línea más firme cada 5.
    let (x0, y0) = to_world(rect.min);
    let (x1, y1) = to_world(rect.max);
    let faint = Stroke::new(1.0, theme::mix(p.canvas, p.grid, 0.5));
    let firm = Stroke::new(1.0, p.grid);
    if scale >= 24.0 {
        for gx in x0.floor() as i32..=x1.ceil() as i32 {
            let x = to_screen((gx as f32, 0.0)).x;
            painter.line_segment(
                [pos2(x, rect.min.y), pos2(x, rect.max.y)],
                if gx % 5 == 0 { firm } else { faint },
            );
        }
        for gy in y0.floor() as i32..=y1.ceil() as i32 {
            let y = to_screen((0.0, gy as f32)).y;
            painter.line_segment(
                [pos2(rect.min.x, y), pos2(rect.max.x, y)],
                if gy % 5 == 0 { firm } else { faint },
            );
        }
    }

    // El plano de la locación de rodaje, de fondo y a escala (su ancho en metros se carga en la ficha del lugar).
    if state.show_plan {
        draw_floor_plan(
            ui,
            p,
            &painter,
            project,
            blocking,
            to_screen((0.0, 0.0)),
            scale,
        );
    }

    // --- interacción (sobre las cosas tal como están en el instante que se ve)
    let time = state.time;
    let pad = 6.0 / scale;
    let before = raw(project);
    let items = posed(&before, time, &|i| project.camera_fov(i));
    let layer = state.layer;
    let top_hit = |point: (f32, f32)| {
        items
            .iter()
            .rev()
            .filter(|i| layer.owns(i.kind) && !i.hidden)
            .find(|i| hit(i, point, pad))
            .map(|i| i.id)
    };
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1) <= GRAB / scale;
    // Las posiciones guardadas que se ven como fantasma (menos la del instante que se ve, que es la figura misma).
    let pose_ghosts: Vec<(BlockItem, motion::Position)> = before
        .iter()
        .filter(|i| layer.owns(i.kind) && !i.hidden && !i.locked)
        .filter_map(|i| items.iter().find(|p| p.id == i.id).map(|now| (i, now)))
        .flat_map(|(i, now)| {
            motion::ghosts_for(i, state.ghosts, state.selected)
                .into_iter()
                .filter(|pos| (pos.t - time).abs() >= crate::model::KEY_EPSILON)
                .map(|pos| (pose_item(now, &pos), pos))
                .collect::<Vec<_>>()
        })
        .collect();
    let ghost_at = |point: (f32, f32)| {
        pose_ghosts
            .iter()
            .rev()
            .find(|(g, _)| hit(g, point, pad))
            .map(|(g, pos)| (g.id, pos.clone()))
    };
    let sel_pose = state.sel_pose;
    // Lo del trayecto de lo elegido que está bajo el mouse: un punto intermedio, una posición o el trayecto mismo.
    let path_item = state
        .selected
        .filter(|_| state.multi.len() == 1)
        .and_then(|id| {
            before
                .iter()
                .find(|i| i.id == id && !i.locked && !i.keys.is_empty())
        });
    let on_path = |point: (f32, f32)| -> Option<PathHit> {
        let item = path_item?;
        if item.smooth_handles().into_iter().any(|(_, at, h)| {
            near((at.0 + h.0, at.1 + h.1), point) || near((at.0 - h.0, at.1 - h.1), point)
        }) {
            return Some(PathHit::Handle);
        }
        if let Some((k, n, ..)) = motion::waypoints(item)
            .into_iter()
            .find(|(_, _, at, _)| near(*at, point))
        {
            return Some(PathHit::Via(k, n));
        }
        if let Some(k) = item.keys.iter().position(|k| near((k.x, k.y), point)) {
            return Some(PathHit::Key(k));
        }
        motion::nearest_on_path(item, point, GRAB / scale).map(|(k, n, q)| PathHit::Line(k, n, q))
    };
    // La selección múltiple: manda `selected`; si cambió a algo que no está en `multi`, queda solo eso.
    state.sync_multi();
    state.multi.retain(|id| before.iter().any(|i| i.id == *id));
    let single = state.multi.len() == 1;
    let shift = ui.input(|i| i.modifiers.shift);
    // Los tiradores y posiciones de lo elegido, de más a menos prioritario: giro, tamaño, posiciones de la animación.
    let grabbed = |point: (f32, f32), selected: Option<u64>| -> Option<Drag> {
        let item = items
            .iter()
            .find(|i| Some(i.id) == selected && !i.locked && !i.hidden)?;
        // Las manijas son de una sola cosa elegida.
        if !single {
            return None;
        }
        if near(handle(item, handle_pad(item.kind) / scale), point) {
            return Some(Drag::Rotate { item: item.id });
        }
        // Los puntos del trayecto y, en un boom, la figura de su operador.
        if let Some(raw) = before.iter().find(|i| i.id == item.id) {
            // Los tiradores de curvatura de los nodos suaves.
            for (node, at, h) in raw.smooth_handles() {
                if near((at.0 + h.0, at.1 + h.1), point) {
                    return Some(Drag::Handle {
                        item: item.id,
                        node,
                        back: false,
                    });
                }
                if near((at.0 - h.0, at.1 - h.1), point) {
                    return Some(Drag::Handle {
                        item: item.id,
                        node,
                        back: true,
                    });
                }
            }
            if let Some((k, n, _, _)) = motion::waypoints(raw)
                .into_iter()
                .find(|(_, _, at, _)| near(*at, point))
            {
                return Some(Drag::Via {
                    item: item.id,
                    key: k,
                    index: n,
                });
            }
            if let Some(spec) = item.mic.as_ref().filter(|m| m.kind == MicKind::Boom) {
                let operator = spec.operator.unwrap_or_else(|| {
                    let back = rotate((-1.2, 0.0), item.rot);
                    (item.x + back.0, item.y + back.1)
                });
                if (operator.0 - point.0).hypot(operator.1 - point.1) <= 0.25_f32.max(GRAB / scale)
                {
                    return Some(Drag::Operator { item: item.id });
                }
            }
        }
        // Apertura y alcance, en el borde del haz, del patrón o del cono.
        if let Some((which, _)) = plan_checks::range_handles(item)
            .into_iter()
            .find(|(_, at)| near(*at, point))
        {
            return Some(match which {
                Range::Reach => Drag::Reach { item: item.id },
                Range::Beam => Drag::Beam { item: item.id },
            });
        }
        // La manija de giro de la posición elegida, en su fantasma.
        if let Some((sid, index)) = sel_pose.filter(|(sid, _)| *sid == item.id)
            && let Some((g, _)) = pose_ghosts
                .iter()
                .find(|(g, pos)| g.id == sid && pos.index == index)
            && near(handle(g, handle_pad(g.kind) / scale), point)
        {
            return Some(Drag::PoseRotate { item: sid, index });
        }
        if let Some(side) = size_sides(item.kind)
            .into_iter()
            .find(|s| near(size_handle(item, *s), point))
        {
            return Some(Drag::Size {
                item: item.id,
                side,
            });
        }
        let index = before
            .iter()
            .find(|i| i.id == item.id)?
            .keys
            .iter()
            .position(|k| near((k.x, k.y), point))?;
        Some(Drag::Key {
            item: item.id,
            index,
        })
    };
    state.cursor = hover.map(to_world);
    // Con el botón del medio o con Espacio apretado, se mueve la vista en cualquier herramienta.
    let panning =
        ui.input(|i| i.key_down(egui::Key::Space)) || resp.dragged_by(egui::PointerButton::Middle);
    if panning {
        state.offset += resp.drag_delta();
        state.view_target = None;
        // Si se movió la vista con Espacio apretado, al soltarlo no se reproduce.
        state.space_panned |= resp.dragged();
    }
    let mut ghost: Option<Ghost> = None;
    let tool_at_start = state.tool;
    tool_keys(ui, state, has_people(project, Some(blocking)));
    match state.tool {
        Tool::Path if !panning => {
            // Cada clic suma una posición, un segundo después de la anterior.
            if let (true, Some(pos), Some(sel)) =
                (resp.clicked(), resp.interact_pointer_pos(), state.selected)
            {
                let point = to_world(pos);
                let snap = state.snap;
                let mut new_duration = None;
                if let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) {
                    let duration = b.duration;
                    if let Some(i) = b
                        .items
                        .iter_mut()
                        .find(|i| i.id == sel)
                        .filter(|i| i.animatable())
                    {
                        let t = (i.keys.last().map_or(0.0, |k| k.t) + 1.0)
                            .max(crate::model::KEY_EPSILON * 2.0);
                        let rot = i.pose_at(t).2;
                        i.set_pose_at(
                            t,
                            snapped(point.0, 0.25, snap),
                            snapped(point.1, 0.25, snap),
                            rot,
                        );
                        if t > duration {
                            new_duration = Some(t);
                        }
                        changed = true;
                    }
                    if let Some(d) = new_duration {
                        b.duration = d;
                    }
                }
            }
        }
        Tool::Paste if !panning => {
            let (placed, shown) = paste_step(ui, state, project, blocking, &resp, &to_world);
            changed |= placed;
            ghost = shown;
        }
        Tool::Measure if !panning => measure_step(ui, state, &resp, &to_world),
        Tool::Place(spec) if !panning => {
            let (placed, shown) = placement_step(
                ui, state, project, spec, blocking, &resp, &items, &to_world, scale,
            );
            changed |= placed;
            ghost = shown;
        }
        _ => {}
    }
    // La herramienta con que empezó el cuadro: si acaba de soltarse al colocar, ese mismo clic no vuelve a elegir.
    if let (Tool::Select, false, Some(pos)) = (tool_at_start, panning, resp.interact_pointer_pos())
    {
        let point = to_world(pos);
        if resp.drag_started() {
            // Lo que se agarra es lo que había bajo el mouse al apretar, no donde está cuando se nota el arrastre.
            let point = ui
                .input(|i| i.pointer.press_origin())
                .map_or(point, to_world);
            state.drag = Some(match (grabbed(point, state.selected), top_hit(point)) {
                (Some(drag), _) => drag,
                (None, Some(id)) => {
                    let item = items
                        .iter()
                        .find(|i| i.id == id)
                        .expect("lo que se tocó existe");
                    // Agarrar algo que ya está elegido mueve todo lo elegido; si no, se elige (con Shift, se suma).
                    if !state.multi.contains(&id) {
                        if shift {
                            state.multi.push(id);
                        } else {
                            state.multi = vec![id];
                        }
                    }
                    state.selected = Some(id);
                    state.sel_pose = None;
                    // Lo bloqueado se elige pero no se mueve.
                    if item.locked {
                        Drag::Idle
                    } else {
                        Drag::Move {
                            item: id,
                            grab: (point.0 - item.x, point.1 - item.y),
                        }
                    }
                }
                (None, None) => match ghost_at(point) {
                    // Agarrar un fantasma elige esa posición y la mueve.
                    Some((id, pos)) => {
                        state.selected = Some(id);
                        state.sel_pose = Some((id, pos.index));
                        Drag::Pose {
                            item: id,
                            index: pos.index,
                            grab: (point.0 - pos.x, point.1 - pos.y),
                        }
                    }
                    // En un lugar vacío, un rectángulo de selección (la vista se mueve con Espacio o el botón del medio).
                    None => Drag::Marquee { start: point },
                },
            });
        }
        if resp.dragged() {
            let snap = state.snap;
            let (now_time, auto) = (state.time, state.auto_record);
            if let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) {
                match state.drag {
                    Some(Drag::Move { item, grab }) => {
                        // Todo lo elegido se mueve junto: lo mismo que se corre la que se agarró.
                        let target = (
                            snapped(point.0 - grab.0, 0.25, snap),
                            snapped(point.1 - grab.1, 0.25, snap),
                        );
                        // El corrimiento sale de donde se ve la que se agarró; cada una se edita en su instante.
                        if let Some((px, py, _)) = b
                            .items
                            .iter()
                            .find(|i| i.id == item)
                            .map(|i| i.pose_at(now_time))
                        {
                            let (dx, dy) = (target.0 - px, target.1 - py);
                            for id in &state.multi {
                                if let Some(i) = b
                                    .items
                                    .iter_mut()
                                    .find(|i| i.id == *id && !i.locked && !i.is_attached_lavalier())
                                {
                                    let at = edit_instant(i, now_time, auto);
                                    let (x, y, rot) = i.pose_at(at);
                                    // Si se edita otro instante que el que se ve, se parte de lo que se ve.
                                    let (x, y) = if at == now_time {
                                        (x, y)
                                    } else {
                                        let shown = i.pose_at(now_time);
                                        (shown.0, shown.1)
                                    };
                                    i.set_pose_at(at, x + dx, y + dy, rot);
                                    changed = true;
                                }
                            }
                        }
                    }
                    Some(Drag::Rotate { item }) => {
                        // La principal queda mirando hacia el mouse; las demás giran lo mismo.
                        let turn = b.items.iter().find(|i| i.id == item).map(|i| {
                            let (x, y, rot) = i.pose_at(now_time);
                            let angle =
                                (point.1 - y).atan2(point.0 - x).to_degrees() - handle_dir(i.kind);
                            snapped(angle, 15.0, snap).rem_euclid(360.0) - rot
                        });
                        if let Some(turn) = turn {
                            for id in &state.multi {
                                if let Some(i) = b
                                    .items
                                    .iter_mut()
                                    .find(|i| i.id == *id && !i.locked && !i.is_attached_lavalier())
                                {
                                    let at = edit_instant(i, now_time, auto);
                                    let (x, y, rot) = i.pose_at(at);
                                    i.set_pose_at(at, x, y, (rot + turn).rem_euclid(360.0));
                                    changed = true;
                                }
                            }
                        }
                    }
                    Some(Drag::Reach { item }) => {
                        if let Some(i) = b.items.iter_mut().find(|i| i.id == item)
                            && let Some(now) = items.iter().find(|n| n.id == item)
                        {
                            let value = snapped(
                                plan_checks::range_value(now, Range::Reach, point),
                                0.25,
                                snap,
                            )
                            .max(0.25);
                            match i.kind {
                                BlockKind::Light => {
                                    i.light.get_or_insert_with(Default::default).reach_m = value
                                }
                                BlockKind::Mic => {
                                    i.mic.get_or_insert_with(Default::default).reach_m = value
                                }
                                _ => i.cone_len_m = value,
                            }
                            changed = true;
                        }
                    }
                    Some(Drag::Beam { item }) => {
                        if let Some(i) = b.items.iter_mut().find(|i| i.id == item)
                            && let Some(now) = items.iter().find(|n| n.id == item)
                        {
                            let value = snapped(
                                plan_checks::range_value(now, Range::Beam, point),
                                5.0,
                                snap,
                            )
                            .clamp(5.0, 360.0);
                            i.light.get_or_insert_with(Default::default).beam_deg = value;
                            i.fov = value;
                            changed = true;
                        }
                    }
                    Some(Drag::Marquee { start }) => state.marquee = Some((start, point)),
                    Some(Drag::Via { item, key, index }) => {
                        if let Some(v) = b
                            .items
                            .iter_mut()
                            .find(|i| i.id == item)
                            .and_then(|i| i.keys.get_mut(key))
                            .and_then(|k| k.via.get_mut(index))
                        {
                            (v.x, v.y) =
                                (snapped(point.0, 0.25, snap), snapped(point.1, 0.25, snap));
                            changed = true;
                        }
                    }
                    Some(Drag::Handle { item, node, back }) => {
                        if let Some(i) = b.items.iter_mut().find(|i| i.id == item) {
                            // El tirador va de la posición del nodo al punto; el de atrás es el simétrico.
                            let centre = match node {
                                crate::model::PathNodeRef::Via(k, n) => i
                                    .keys
                                    .get(k)
                                    .and_then(|key| key.via.get(n))
                                    .map(|v| (v.x, v.y)),
                                crate::model::PathNodeRef::Key(k) => {
                                    i.keys.get(k).map(|key| (key.x, key.y))
                                }
                            };
                            if let Some(c) = centre {
                                let dir = if back { -1.0 } else { 1.0 };
                                let h = (dir * (point.0 - c.0), dir * (point.1 - c.1));
                                match node {
                                    crate::model::PathNodeRef::Via(k, n) => {
                                        if let Some(v) =
                                            i.keys.get_mut(k).and_then(|key| key.via.get_mut(n))
                                        {
                                            v.handle = Some(h);
                                        }
                                    }
                                    crate::model::PathNodeRef::Key(k) => {
                                        if let Some(key) = i.keys.get_mut(k) {
                                            key.handle = Some(h);
                                        }
                                    }
                                }
                                changed = true;
                            }
                        }
                    }
                    Some(Drag::Operator { item }) => {
                        if let Some(i) = b.items.iter_mut().find(|i| i.id == item)
                            && i.mic.is_some()
                        {
                            let at = edit_instant(i, now_time, auto);
                            i.set_operator_at(
                                at,
                                (snapped(point.0, 0.25, snap), snapped(point.1, 0.25, snap)),
                            );
                            changed = true;
                        }
                    }
                    Some(Drag::Size { item, side }) => {
                        if let (Some(i), Some(current)) = (
                            b.items.iter_mut().find(|i| i.id == item),
                            items.iter().find(|i| i.id == item),
                        ) {
                            let (x, y, w, h) = resized(current, side, point, snap);
                            (i.w, i.h) = (w, h);
                            if let Some(m) = i.modifier.as_mut() {
                                m.w = w;
                            }
                            let at = edit_instant(i, now_time, auto);
                            i.set_pose_at(at, x, y, current.rot);
                            changed = true;
                        }
                    }
                    Some(Drag::Key { item, index }) => {
                        if let Some(k) = b
                            .items
                            .iter_mut()
                            .find(|i| i.id == item)
                            .and_then(|i| i.keys.get_mut(index))
                        {
                            (k.x, k.y) =
                                (snapped(point.0, 0.25, snap), snapped(point.1, 0.25, snap));
                            changed = true;
                        }
                    }
                    Some(Drag::Pose { item, index, grab }) => {
                        if let Some(i) = b.items.iter_mut().find(|i| i.id == item) {
                            let at = (
                                snapped(point.0 - grab.0, 0.25, snap),
                                snapped(point.1 - grab.1, 0.25, snap),
                            );
                            match index {
                                Some(n) => {
                                    if let Some(k) = i.keys.get_mut(n) {
                                        (k.x, k.y) = at;
                                    }
                                }
                                None => (i.x, i.y) = at,
                            }
                            changed = true;
                        }
                    }
                    Some(Drag::PoseRotate { item, index }) => {
                        if let Some(i) = b.items.iter_mut().find(|i| i.id == item) {
                            let (x, y, _) = match index {
                                Some(n) => i
                                    .keys
                                    .get(n)
                                    .map_or((i.x, i.y, i.rot), |k| (k.x, k.y, k.rot)),
                                None => (i.x, i.y, i.rot),
                            };
                            let angle =
                                (point.1 - y).atan2(point.0 - x).to_degrees() - handle_dir(i.kind);
                            let angle = snapped(angle, 15.0, snap).rem_euclid(360.0);
                            match index {
                                Some(n) => {
                                    if let Some(k) = i.keys.get_mut(n) {
                                        k.rot = angle;
                                    }
                                }
                                None => i.rot = angle,
                            }
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        if resp.clicked() {
            // Tocar una posición de la animación (su punto o su fantasma) lleva el tiempo hasta ella y la elige.
            let key = state
                .selected
                .and_then(|id| before.iter().find(|i| i.id == id))
                .and_then(|i| {
                    i.keys
                        .iter()
                        .position(|k| near((k.x, k.y), point))
                        .map(|n| (i.id, n, i.keys[n].t))
                });
            match (key, top_hit(point)) {
                (Some((id, n, t)), None) => {
                    state.time = t;
                    state.playing = false;
                    state.sel_pose = Some((id, Some(n)));
                }
                (_, Some(id)) => {
                    // Con Shift, un clic suma la cosa a la selección o la quita; sin Shift, queda sola.
                    if shift {
                        if let Some(n) = state.multi.iter().position(|m| *m == id) {
                            state.multi.remove(n);
                            state.selected = state.multi.last().copied();
                        } else {
                            state.multi.push(id);
                            state.selected = Some(id);
                        }
                    } else {
                        state.selected = Some(id);
                        state.multi = vec![id];
                    }
                    state.sel_pose = None;
                }
                (None, None) => match ghost_at(point) {
                    Some((id, pos)) => {
                        state.selected = Some(id);
                        state.sel_pose = Some((id, pos.index));
                        state.time = pos.t;
                        state.playing = false;
                    }
                    // Un clic sobre un punto del trayecto lo elige; sobre el trayecto mismo no suelta la selección.
                    None if on_path(point).is_some() => {
                        if let Some(PathHit::Via(k, n)) = on_path(point)
                            && let Some(id) = state.selected
                        {
                            state.sel_via = Some((id, k, n));
                        }
                    }
                    None if shift => {}
                    None => {
                        state.selected = None;
                        state.multi.clear();
                        state.sel_pose = None;
                    }
                },
            }
        }
    }
    // Doble clic sobre el trayecto de lo elegido (como en Inkscape): un punto nuevo, o alterna un punto o una posición entre
    // esquina (recta) y curva suave.
    if tool_at_start == Tool::Select
        && resp.double_clicked()
        && let (Some(pos), Some(id)) = (resp.interact_pointer_pos(), state.selected)
        && let Some(hit) = on_path(to_world(pos))
        && let Some(item) = project
            .blockings
            .iter_mut()
            .find(|b| b.id == blocking)
            .and_then(|b| b.items.iter_mut().find(|i| i.id == id))
    {
        match hit {
            PathHit::Via(k, n) => {
                if let Some(v) = item.keys.get_mut(k).and_then(|key| key.via.get_mut(n)) {
                    v.smooth = !v.smooth;
                    // Al volver a esquina se pierde la curvatura que se le había dado.
                    v.handle = None;
                    state.sel_via = Some((id, k, n));
                }
            }
            PathHit::Key(k) => {
                if let Some(key) = item.keys.get_mut(k) {
                    key.smooth = !key.smooth;
                    key.handle = None;
                }
            }
            PathHit::Handle => {}
            PathHit::Line(k, n, (x, y)) => {
                if let Some(key) = item.keys.get_mut(k) {
                    key.via.insert(
                        n.min(key.via.len()),
                        crate::model::Waypoint {
                            x,
                            y,
                            smooth: false,
                            handle: None,
                        },
                    );
                    state.sel_via = Some((id, k, n.min(key.via.len() - 1)));
                }
            }
        }
        changed = true;
    }
    if resp.drag_stopped() {
        if let Some(Drag::Marquee { start }) = state.drag {
            // Se elige lo que quedó dentro del rectángulo (con Shift, se suma a lo que ya estaba).
            let end = ui.input(|i| i.pointer.latest_pos()).map_or(start, to_world);
            let area =
                egui::Rect::from_two_pos(egui::pos2(start.0, start.1), egui::pos2(end.0, end.1));
            let inside: Vec<u64> = items
                .iter()
                .filter(|i| layer.owns(i.kind) && !i.hidden)
                .filter(|i| area.contains(egui::pos2(i.x, i.y)))
                .map(|i| i.id)
                .collect();
            if !shift {
                state.multi.clear();
            }
            for id in inside {
                if !state.multi.contains(&id) {
                    state.multi.push(id);
                }
            }
            state.selected = state.multi.last().copied();
            state.sel_pose = None;
        }
        state.marquee = None;
        state.drag = None;
    }
    if state.selected.is_some()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete))
    {
        let now = state.time;
        if let Some((id, k, n)) = state.sel_via {
            // Con un punto del trayecto elegido, Supr lo borra.
            if let Some(key) = project
                .blockings
                .iter_mut()
                .find(|b| b.id == blocking)
                .and_then(|b| b.items.iter_mut().find(|i| i.id == id))
                .and_then(|i| i.keys.get_mut(k))
                && n < key.via.len()
            {
                key.via.remove(n);
                changed = true;
            }
            state.sel_via = None;
        } else if let Some((id, index)) = state.sel_pose {
            // Con una posición elegida, Supr borra esa posición (la de partida la cede a la siguiente) y no la figura.
            if let Some(i) = project
                .blockings
                .iter_mut()
                .find(|b| b.id == blocking)
                .and_then(|b| b.items.iter_mut().find(|i| i.id == id))
            {
                let t = index.and_then(|n| i.keys.get(n)).map_or(0.0, |k| k.t);
                changed |= i.delete_position_at(t);
            }
            state.sel_pose = None;
        } else {
            // Sin posición elegida: lo que tiene una posición en el instante que se ve pierde esa y conserva las demás; si
            // no tiene ninguna ahí (está entre dos posiciones) o no tiene posiciones, se borra la cosa.
            let ids = state.multi.clone();
            for id in ids {
                let removed_position = project
                    .blockings
                    .iter_mut()
                    .find(|b| b.id == blocking)
                    .and_then(|b| b.items.iter_mut().find(|i| i.id == id))
                    .is_some_and(|i| !i.keys.is_empty() && i.delete_position_at(now));
                if !removed_position {
                    project.remove_block_item(blocking, id);
                    state.multi.retain(|m| *m != id);
                }
                changed = true;
            }
            if state.multi.is_empty() {
                state.selected = None;
            }
        }
    }
    changed |= motion_keys(ui, state, project, blocking, &resp);
    changed |= edit_keys(ui, state, project, blocking);

    // El menú contextual (clic derecho): primero se elige lo que hay bajo el mouse.
    if tool_at_start == Tool::Select {
        if resp.secondary_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            match top_hit(to_world(pos)) {
                Some(id) if !state.multi.contains(&id) => {
                    state.selected = Some(id);
                    state.multi = vec![id];
                    state.sel_pose = None;
                }
                Some(_) => {}
                None => {
                    state.selected = None;
                    state.multi.clear();
                }
            }
        }
        let mut act: Option<EditAct> = None;
        let primary = chosen_items(state, project, blocking)
            .into_iter()
            .find(|i| Some(i.id) == state.selected)
            .cloned();
        let has_clipboard = !state.clipboard.is_empty();
        resp.context_menu(|ui| {
            let Some(primary) = &primary else {
                if has_clipboard && ui.button("Pegar").clicked() {
                    act = Some(EditAct::Paste);
                    ui.close();
                } else if !has_clipboard {
                    ui.label(RichText::new("No hay nada elegido").color(p.dim));
                }
                return;
            };
            let mut item = |ui: &mut Ui, label: &str, what: EditAct| {
                if ui.button(label).clicked() {
                    act = Some(what);
                    ui.close();
                }
            };
            item(ui, "Duplicar (Ctrl+D)", EditAct::Duplicate);
            item(ui, "Copiar (Ctrl+C)", EditAct::Copy);
            if has_clipboard {
                item(ui, "Pegar (Ctrl+V)", EditAct::Paste);
            }
            ui.separator();
            item(
                ui,
                if primary.hidden {
                    "Mostrar (H)"
                } else {
                    "Ocultar (H)"
                },
                EditAct::ToggleHidden,
            );
            item(
                ui,
                if primary.locked {
                    "Desbloquear (Ctrl+L)"
                } else {
                    "Bloquear (Ctrl+L)"
                },
                EditAct::ToggleLocked,
            );
            ui.separator();
            item(ui, "Traer al frente", EditAct::Front);
            item(ui, "Llevar atrás", EditAct::Back);
            if primary.kind == BlockKind::Camera {
                ui.separator();
                item(ui, "Ver planos vinculados", EditAct::ShowShots);
            }
            ui.separator();
            if ui
                .button(RichText::new("Eliminar (Supr)").color(p.danger))
                .clicked()
            {
                act = Some(EditAct::Delete);
                ui.close();
            }
        });
        if let Some(act) = act {
            changed |= apply_act(state, project, blocking, act);
        }
    }

    // La vista pudo moverse al arrastrar el fondo: el dibujo usa la nueva.
    let (to_screen, _) = transforms(state.offset);

    // --- dibujo (ver `draw_scene`)
    let originals = raw(project);
    let items = posed(&originals, time, &|i| project.camera_fov(i));
    // Los avisos «en cuadro»: en el instante que se ve y en cada posición de cada cámara.
    let codes = project.shot_codes();
    let shots_of = |camera: u64| -> Vec<String> {
        project
            .shots
            .iter()
            .filter(|sh| sh.blocking == Some(blocking) && sh.camera == Some(camera))
            .filter_map(|sh| codes.get(&sh.id).cloned())
            .collect()
    };
    state.warnings = plan_checks::in_frame(&originals, &|i| project.camera_fov(i), time, &shots_of);
    let warnings = plan_checks::warning_texts(&state.warnings);
    let distances = state
        .selected
        .filter(|_| state.multi.len() == 1)
        .and_then(|id| items.iter().find(|i| i.id == id))
        .map(|i| plan_checks::distances(i, &items))
        .unwrap_or_default();
    let hovered =
        ghost
            .as_ref()
            .and_then(|g| g.highlight)
            .or(if state.drag.is_none() && ghost.is_none() {
                hover.map(to_world).and_then(top_hit)
            } else {
                None
            });
    draw_scene(
        &Scene {
            ui,
            painter: &painter,
            p,
            project,
            originals: &originals,
            items: &items,
            layer,
            hovered,
            show_others: state.show_others,
            layers: state.layers,
            other_shot_items: match state.shot_filter {
                Some(f) => project
                    .shots
                    .iter()
                    .filter(|sh| sh.id != f)
                    .flat_map(|sh| sh.items.iter().copied())
                    .filter(|id| !project.shots.iter().any(|sh| sh.id == f && sh.items.contains(id)))
                    .collect(),
                None => Vec::new(),
            },
            focus_camera: state
                .shot_filter
                .and_then(|id| project.shots.iter().find(|sh| sh.id == id))
                .and_then(|sh| sh.camera),
            time,
            scale,
            warnings: &warnings,
            multi: &state.multi,
            distances: &distances,
            ghost_more: ghost.as_ref().map_or(&[], |g| &g.more),
            measure: state.measure.or(state.measure_a.zip(hover.map(to_world))),
            marquee: state.marquee,
            ghost: ghost.as_ref().and_then(|g| g.item.as_ref()),
            ghosts: state.ghosts,
            sel_pose: state.sel_pose,
            sel_via: state.sel_via,
            shot_span: shot_span_of(project, state, blocking),
        },
        &to_screen,
    );
    if let Some(g) = &ghost {
        draw_overlay(&painter, p, g, &to_screen);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(if g.ok {
                egui::CursorIcon::Crosshair
            } else {
                egui::CursorIcon::NotAllowed
            });
        }
    }
    // La escala, para tener una idea de las distancias.
    let bar = rect.left_bottom() + vec2(16.0, -18.0);
    painter.line_segment([bar, bar + vec2(scale, 0.0)], Stroke::new(2.0, p.dim));
    painter.text(
        bar + vec2(scale + 8.0, 0.0),
        Align2::LEFT_CENTER,
        "1 m",
        FontId::proportional(11.0),
        p.dim,
    );
    if resp.hovered() {
        let over = hover
            .map(to_world)
            .and_then(|point| grabbed(point, state.selected));
        ui.ctx().set_cursor_icon(match (state.drag, over) {
            (Some(Drag::Size { side, .. }), _) | (None, Some(Drag::Size { side, .. })) => {
                resize_cursor(side)
            }
            (Some(Drag::Rotate { .. } | Drag::Key { .. }), _)
            | (None, Some(Drag::Rotate { .. } | Drag::Key { .. })) => egui::CursorIcon::Grab,
            _ => egui::CursorIcon::Default,
        });
    }
    changed
}

// ------------------------------------------------------------------------------------------------
// Dibujo de las figuras (docs/plantas.md §3)
// ------------------------------------------------------------------------------------------------

/// Todo lo que hace falta para dibujar la escena: se arma una vez por cuadro.
struct Scene<'a> {
    ui: &'a Ui,
    painter: &'a egui::Painter,
    p: &'a Palette,
    project: &'a Project,
    /// Las cosas tal como se guardaron (con sus posiciones) y en el instante que se ve.
    originals: &'a [BlockItem],
    items: &'a [BlockItem],
    layer: Layer,
    hovered: Option<u64>,
    show_others: bool,
    /// Cuáles de las otras plantas se ven (por `Layer::index`).
    layers: [bool; 4],
    /// La cámara del plano al que se filtra: las demás cámaras se atenúan.
    focus_camera: Option<u64>,
    /// Las cosas anotadas en otros planos y no en el filtrado: también se atenúan.
    other_shot_items: Vec<u64>,
    /// El instante que se ve, para marcar la posición actual en un recorrido.
    time: f32,
    scale: f32,
    /// Qué cosas pueden verse en cuadro de una cámara, y de cuáles.
    warnings: &'a std::collections::HashMap<(u64, Seen), String>,
    /// Todo lo elegido (la principal incluida) y las distancias de la principal a las personas de su cono o haz.
    multi: &'a [u64],
    distances: &'a [plan_checks::Distance],
    /// Lo que se pegaría (más de una cosa) y las ayudas de medir y de seleccionar.
    ghost_more: &'a [BlockItem],
    measure: Option<((f32, f32), (f32, f32))>,
    marquee: Option<((f32, f32), (f32, f32))>,
    /// La figura que sigue al mouse en el modo de colocación.
    ghost: Option<&'a BlockItem>,
    /// Cuántos fantasmas de posiciones se ven, y cuál posición está elegida.
    ghosts: GhostMode,
    sel_pose: Option<(u64, Option<usize>)>,
    sel_via: Option<(u64, usize, usize)>,
    /// El tramo de un plano elegido en la línea de tiempo: la cámara y desde cuándo hasta cuándo.
    shot_span: Option<(u64, f32, f32)>,
}

/// La insignia de una posición guardada: dónde va, qué dice y de qué color.
struct Badge {
    at: egui::Pos2,
    text: String,
    /// La marca de la posición y su pausa, una por línea debajo.
    notes: Vec<String>,
    fill: egui::Color32,
    opacity: f32,
    chosen: bool,
}

impl Scene<'_> {
    /// ¿Está elegida esta cosa (sea la principal o una de varias)?
    fn is_sel(&self, id: u64) -> bool {
        self.multi.contains(&id)
    }
}

/// Un texto que se dibuja encima de todas las figuras.
struct LabelJob {
    at: egui::Pos2,
    anchor: Align2,
    text: String,
    font: FontId,
    color: egui::Color32,
    max_width: f32,
}

fn is_base(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Actor | BlockKind::Object | BlockKind::Opening | BlockKind::Wall
    )
}

/// Las iniciales de un nombre: hasta dos letras.
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// El icono de Phosphor de una figura, si lo lleva.
fn figure_icon(item: &BlockItem) -> Option<&'static str> {
    match item.kind {
        BlockKind::Actor => Some(icon::PERSON),
        BlockKind::Camera => Some(icon::VIDEO_CAMERA),
        BlockKind::Light => Some(
            if item
                .light
                .as_ref()
                .is_some_and(|l| l.fixture == LightFixture::Practical)
            {
                icon::LIGHTBULB
            } else {
                icon::LAMP
            },
        ),
        BlockKind::Modifier => item
            .modifier
            .as_ref()
            .is_some_and(|m| m.kind == ModifierKind::Flag)
            .then_some(icon::FLAG),
        BlockKind::Mic => Some(icon::MICROPHONE),
        BlockKind::Opening => (item.opening == OpeningKind::Door).then_some(icon::DOOR),
        BlockKind::Object | BlockKind::Wall => None,
    }
}

/// Los colores de una figura: del cuerpo, del detalle, del hueco y de las líneas. Salen de la paleta, salvo los que
/// vienen de los datos (el color del personaje y la temperatura de una luz).
struct Paints {
    body: egui::Color32,
    detail: egui::Color32,
    hollow: egui::Color32,
    line: egui::Color32,
}

fn paints(s: &Scene, item: &BlockItem) -> Paints {
    let p = s.p;
    let canvas = p.canvas;
    let mut body = match item.kind {
        BlockKind::Wall => theme::mix(canvas, p.text, 0.65),
        BlockKind::Object => theme::mix(canvas, p.text, 0.18),
        BlockKind::Opening => canvas,
        BlockKind::Actor => item
            .character
            .and_then(|id| s.project.characters.iter().find(|c| c.id == id))
            .map_or(p.accent, |c| theme::character_color(&c.id.to_string())),
        BlockKind::Camera => p.plan_camera,
        BlockKind::Light => {
            theme::beam_color(p, item.kelvin, item.light.as_ref().and_then(|l| l.gel))
        }
        BlockKind::Modifier => match item.modifier.as_ref().map(|m| m.kind) {
            Some(ModifierKind::Flag) => p.bg,
            Some(ModifierKind::Diffuser) => theme::mix(canvas, p.plan_light, 0.25),
            _ => theme::mix(canvas, p.text, 0.45),
        },
        BlockKind::Mic => p.plan_sound,
    };
    let mut detail = theme::mix(body, theme::ink_on(body, p), 0.4);
    let mut line = match item.kind {
        BlockKind::Mic => p.plan_sound,
        BlockKind::Opening => theme::mix(canvas, p.text, 0.7),
        BlockKind::Modifier => p.plan_light,
        _ => theme::ink_on(body, p),
    };
    if let (BlockKind::Modifier, Some(m)) = (item.kind, item.modifier.as_ref()) {
        if m.kind == ModifierKind::Bounce {
            detail = match m.face {
                ModifierFace::White => p.text,
                ModifierFace::Silver => theme::mix(p.text, canvas, 0.35),
                ModifierFace::Gold => p.warning,
                ModifierFace::Black => p.bg,
            };
        } else if m.kind == ModifierKind::Flag {
            detail = theme::mix(p.bg, p.text, 0.3);
            line = theme::mix(p.bg, p.text, 0.5);
        }
    }
    if item.kind == BlockKind::Light
        && item
            .light
            .as_ref()
            .is_some_and(|l| l.fixture == LightFixture::Practical)
    {
        body = theme::mix(body, p.text, 0.1);
    }
    Paints {
        body,
        detail,
        hollow: canvas,
        line,
    }
}

/// Un abanico de triángulos desde el primer punto (el origen).
fn fan(painter: &egui::Painter, points: &[egui::Pos2], color: egui::Color32) {
    if points.len() < 3 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for point in points {
        mesh.colored_vertex(*point, color);
    }
    for i in 1..points.len() as u32 - 1 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Recorta un texto con «…» hasta que entre en `max_width`; si no entran ni tres letras, no se dibuja.
fn fit_text(painter: &egui::Painter, text: &str, font: &FontId, max_width: f32) -> Option<String> {
    let width = |t: &str| {
        painter
            .layout_no_wrap(t.to_string(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    };
    if width(text) <= max_width {
        return Some(text.to_string());
    }
    let chars: Vec<char> = text.chars().collect();
    (3..chars.len()).rev().find_map(|n| {
        let shown = format!("{}…", chars[..n].iter().collect::<String>());
        (width(&shown) <= max_width).then_some(shown)
    })
}

/// Dibuja la escena en el orden de §3: la base, las zonas de cobertura, los recorridos, los fantasmas (próxima etapa),
/// las figuras, las etiquetas y las manijas.
fn draw_scene(s: &Scene, to_screen: &dyn Fn((f32, f32)) -> egui::Pos2) {
    let (p, painter) = (s.p, s.painter);
    let detail = figures::detail_for(s.scale);
    // Lo de otra planta solo se ve, en transparencia, si se pidió; lo oculto no se dibuja.
    // «Otras plantas» (§6.3): la de luces muestra los conos de las cámaras, la de sonido los conos y las luces, y la de
    // cámara las luces y los micrófonos; todo tenue, solo como referencia.
    let referenced = |kind: BlockKind| match s.layer {
        Layer::Camera => matches!(
            kind,
            BlockKind::Light | BlockKind::Modifier | BlockKind::Mic
        ),
        Layer::Lights => kind == BlockKind::Camera,
        Layer::Sound => matches!(
            kind,
            BlockKind::Camera | BlockKind::Light | BlockKind::Modifier
        ),
        Layer::Set => matches!(kind, BlockKind::Camera | BlockKind::Light | BlockKind::Mic),
    };
    let opacity = |kind: BlockKind| {
        if is_base(kind) || s.layer.owns(kind) {
            Some(1.0)
        } else if s.show_others && referenced(kind) && s.layers[layer_of(kind).index()] {
            Some(0.3)
        } else {
            None
        }
    };
    // Lo de otra planta no se rellena: solo el contorno de su cobertura.
    let detail_of = |kind: BlockKind| figures::Detail {
        fills: detail.fills && s.layer.owns(kind),
        ..detail
    };
    let dim = |item: &BlockItem| -> f32 {
        match s.focus_camera {
            Some(c) if item.kind == BlockKind::Camera && item.id != c => 0.25,
            _ if s.other_shot_items.contains(&item.id) => 0.25,
            _ => 1.0,
        }
    };
    let shown = |kind: BlockKind| {
        s.items
            .iter()
            .filter(move |i| i.kind == kind && !i.hidden)
            .filter_map(move |i| opacity(kind).map(|o| (i, o * dim(i))))
    };
    let mut labels: Vec<LabelJob> = Vec::new();
    let mut handles: Vec<&BlockItem> = Vec::new();

    // 1. La base: paredes, muebles y aberturas.
    for kind in [BlockKind::Wall, BlockKind::Object, BlockKind::Opening] {
        for (item, o) in shown(kind) {
            draw_figure(s, item, o, detail, to_screen, &mut labels, false);
            if s.is_sel(item.id) && s.multi.len() == 1 && s.layer.owns(kind) && !item.locked {
                handles.push(item);
            }
        }
    }

    // 2. Las zonas de cobertura: el cono de cada cámara, el haz de cada luz y el patrón de cada micrófono.
    for kind in [BlockKind::Camera, BlockKind::Light, BlockKind::Mic] {
        for (item, o) in shown(kind) {
            draw_coverage(s, item, o, detail_of(kind), to_screen);
        }
    }

    // La sombra suave que proyecta una bandera hacia atrás, si una luz la alcanza con su haz.
    for (flag, o) in shown(BlockKind::Modifier).filter(|(i, _)| {
        i.modifier
            .as_ref()
            .is_some_and(|m| m.kind == ModifierKind::Flag)
    }) {
        for light in s
            .items
            .iter()
            .filter(|l| l.kind == BlockKind::Light && !l.hidden)
        {
            if let Some(shadow) = plan_checks::flag_shadow(flag, light) {
                let points: Vec<egui::Pos2> = shadow.into_iter().map(to_screen).collect();
                painter.add(egui::Shape::convex_polygon(
                    points,
                    p.shadow.gamma_multiply(0.35 * o),
                    Stroke::NONE,
                ));
            }
        }
    }

    for g in s.ghost.into_iter().chain(s.ghost_more) {
        draw_coverage(s, g, 0.5, detail, to_screen);
    }

    // 3. Los recorridos de lo que se mueve: una línea del color de la figura, con flechas que muestran el sentido; recta o,
    //    si `curved`, una curva suave. El tramo de un plano elegido se resalta.
    for item in s
        .originals
        .iter()
        .filter(|i| !i.keys.is_empty() && !i.hidden)
    {
        let Some(o) = opacity(item.kind).map(|o| o * dim(item)) else {
            continue;
        };
        let chosen = s.is_sel(item.id);
        let color = paints(s, item).body;
        let path: Vec<egui::Pos2> = motion::path_points(item)
            .into_iter()
            .map(to_screen)
            .collect();
        painter.add(egui::Shape::dashed_line(
            &path,
            Stroke::new(
                if chosen { 2.0 } else { 1.0 },
                color.gamma_multiply(if chosen { 0.9 } else { 0.5 } * o),
            ),
            7.0,
            5.0,
        ));
        for (at, angle) in motion::path_arrows(item) {
            let c = to_screen(at);
            let dir = vec2(angle.to_radians().cos(), angle.to_radians().sin());
            let side = vec2(-dir.y, dir.x);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + dir * 6.0,
                    c - dir * 4.0 + side * 4.0,
                    c - dir * 4.0 - side * 4.0,
                ],
                color.gamma_multiply(0.85 * o),
                Stroke::NONE,
            ));
        }
        if let Some((cam, from, to)) = s.shot_span.filter(|(cam, ..)| *cam == item.id) {
            let _ = cam;
            let along: Vec<egui::Pos2> = motion::path_between(item, from, to)
                .into_iter()
                .map(to_screen)
                .collect();
            painter.add(egui::Shape::line(
                along,
                Stroke::new(4.0, p.accent.gamma_multiply(0.85 * o)),
            ));
        }
        if chosen {
            // Los puntos del trayecto: cuadrados si son esquinas, círculos si son curvas; el elegido, de acento.
            // Los tiradores de curvatura de los nodos suaves: una línea a cada lado y un punto en cada extremo.
            for (_, at, h) in item.smooth_handles() {
                let (a, b) = (
                    to_screen((at.0 + h.0, at.1 + h.1)),
                    to_screen((at.0 - h.0, at.1 - h.1)),
                );
                painter.line_segment([b, a], Stroke::new(1.0, p.accent.gamma_multiply(0.7 * o)));
                for end in [a, b] {
                    painter.circle(end, 3.5, p.accent, Stroke::new(1.0, p.canvas));
                }
            }
            for (k, n, at, smooth) in motion::waypoints(item) {
                let c = to_screen(at);
                let on = s.sel_via == Some((item.id, k, n));
                let stroke = Stroke::new(2.0, if on { p.accent } else { color });
                if smooth {
                    painter.circle(c, 5.0, p.canvas, stroke);
                } else {
                    painter.rect(
                        egui::Rect::from_center_size(c, vec2(9.0, 9.0)),
                        1,
                        p.canvas,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
            for key in &item.keys {
                let on = (key.t - s.time).abs() < crate::model::KEY_EPSILON;
                painter.circle(
                    to_screen((key.x, key.y)),
                    if on { 6.0 } else { 4.5 },
                    if on { p.accent } else { p.canvas },
                    Stroke::new(2.0, color.gamma_multiply(o)),
                );
            }
        }
    }

    // 4. Los fantasmas: la figura entera (cuerpo, orientación y cono, haz o patrón) en cada posición guardada, al 35 %, con
    //    su insignia. La del instante que se ve es la figura misma y no se repite.
    let mut pose_handle: Option<BlockItem> = None;
    let mut badges: Vec<Badge> = Vec::new();
    for item in s.originals.iter().filter(|i| !i.hidden) {
        let Some(o) = opacity(item.kind).map(|o| o * dim(item)) else {
            continue;
        };
        let Some(now) = s.items.iter().find(|i| i.id == item.id) else {
            continue;
        };
        let all = motion::positions(item);
        let tag = match item.kind {
            BlockKind::Actor => s
                .project
                .characters
                .iter()
                .find(|c| Some(c.id) == item.character)
                .map_or(item.label.clone(), |c| {
                    initials(&clean_character_name(&c.name))
                }),
            BlockKind::Camera => item.label.clone(),
            _ => item.label.chars().take(3).collect(),
        };
        let picked = s.is_sel(item.id).then_some(item.id);
        for pos in motion::ghosts_for(item, s.ghosts, picked) {
            let mut g = pose_item(now, &pos);
            if item.kind == BlockKind::Camera {
                // El cono de cada fantasma tiene el zoom de su instante (un efecto Vértigo cambia la lente).
                g.fov = s.project.camera_fov(&BlockItem {
                    lens_mm: item.lens_at(pos.t),
                    ..item.clone()
                });
            }
            let here = (pos.t - s.time).abs() < crate::model::KEY_EPSILON;
            if !here {
                draw_coverage(s, &g, 0.35 * o, detail, to_screen);
                draw_figure(s, &g, 0.35 * o, detail, to_screen, &mut labels, true);
            }
            let chosen = s.sel_pose == Some((item.id, pos.index));
            if chosen && !here && !item.locked && s.layer.owns(item.kind) {
                pose_handle = Some(g.clone());
            }
            // La insignia (número, letra o iniciales, tiempo; debajo, la marca y la pausa) se dibuja con las etiquetas.
            let colors = paints(s, &g);
            let half = figures::body_extent(&figures::local_figure(&g))
                * s.scale
                * figures::body_scale(figures::body_extent(&figures::local_figure(&g)), s.scale)
                / 2.0
                + 4.0;
            badges.push(Badge {
                at: to_screen((pos.x, pos.y)) + vec2(half, -half),
                text: motion::badge_text(&tag, &pos),
                notes: [
                    (!pos.label.is_empty()).then(|| pos.label.clone()),
                    motion::hold_text(&pos),
                ]
                .into_iter()
                .flatten()
                .collect(),
                fill: colors.body,
                opacity: o,
                chosen,
            });
        }
        // El arco de un paneo: cuando entre dos posiciones solo cambia el giro.
        if !matches!(s.ghosts, GhostMode::Nothing)
            && (s.ghosts == GhostMode::All || s.is_sel(item.id))
        {
            let color = paints(s, item).body;
            let local = figures::local_figure(item);
            let radius_m = figures::body_extent(&local) / 2.0 + 10.0 / s.scale;
            for w in all.windows(2) {
                if let Some((from, sweep)) = motion::pan_arc(&w[0], &w[1]) {
                    let steps = ((sweep.abs() / 6.0).ceil() as usize).max(2);
                    let arc: Vec<egui::Pos2> = (0..=steps)
                        .map(|n| {
                            let a = (from + sweep * n as f32 / steps as f32).to_radians();
                            to_screen((w[0].x + radius_m * a.cos(), w[0].y + radius_m * a.sin()))
                        })
                        .collect();
                    painter.add(egui::Shape::line(
                        arc.clone(),
                        Stroke::new(1.5, color.gamma_multiply(0.8 * o)),
                    ));
                    if let [.., a, b] = arc[..] {
                        let dir = (b - a).normalized();
                        let side = vec2(-dir.y, dir.x);
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                b + dir * 5.0,
                                b - dir * 3.0 + side * 3.5,
                                b - dir * 3.0 - side * 3.5,
                            ],
                            color.gamma_multiply(0.8 * o),
                            Stroke::NONE,
                        ));
                    }
                }
            }
        }
    }

    // 5. Las figuras: personas, cámaras, luces, modificadores y micrófonos.
    for kind in [
        BlockKind::Actor,
        BlockKind::Camera,
        BlockKind::Light,
        BlockKind::Modifier,
        BlockKind::Mic,
    ] {
        for (item, o) in shown(kind) {
            draw_figure(s, item, o, detail, to_screen, &mut labels, false);
            if s.is_sel(item.id) && s.multi.len() == 1 && s.layer.owns(kind) && !item.locked {
                handles.push(item);
            }
        }
    }

    // El fantasma, al 50 %, con las figuras.
    for g in s.ghost.into_iter().chain(s.ghost_more) {
        draw_figure(s, g, 0.5, detail, to_screen, &mut labels, true);
    }

    // Las distancias de lo elegido a las personas de su cono o su haz (§6.2).
    for d in s.distances {
        let (a, b) = (to_screen(d.from), to_screen(d.to));
        painter.line_segment([a, b], Stroke::new(1.0, p.accent.gamma_multiply(0.6)));
        let mid = a + (b - a) * 0.5;
        let text = format!("{:.1} m", d.meters).replace('.', ",");
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), p.text);
        let back = egui::Rect::from_center_size(mid, galley.size() + vec2(6.0, 2.0));
        painter.rect_filled(back, 3, p.canvas.gamma_multiply(0.9));
        painter.galley(back.center() - galley.size() / 2.0, galley, p.text);
    }
    // El operador de un boom que puede verse en cuadro lleva su propio aviso.
    for (item, o) in shown(BlockKind::Mic) {
        if let (Some(op), Some(why)) = (
            item.mic.as_ref().and_then(|m| m.operator),
            s.warnings.get(&(item.id, Seen::Operator)),
        ) {
            let at = to_screen(op);
            painter.circle_filled(at, 8.0, p.canvas.gamma_multiply(o));
            painter.text(
                at,
                Align2::CENTER_CENTER,
                icon::WARNING,
                FontId::proportional(13.0),
                p.warning.gamma_multiply(o),
            );
            s.ui.interact(
                egui::Rect::from_center_size(at, vec2(16.0, 16.0)),
                s.ui.scope_id().with(("plan_operator_warning", item.id)),
                egui::Sense::hover(),
            )
            .on_hover_text(why);
        }
    }
    // La herramienta Medir y el rectángulo de selección.
    if let Some((a, b)) = s.measure {
        let (pa, pb) = (to_screen(a), to_screen(b));
        painter.add(egui::Shape::dashed_line(
            &[pa, pb],
            Stroke::new(1.5, p.accent),
            6.0,
            4.0,
        ));
        painter.circle_filled(pa, 3.5, p.accent);
        painter.circle_filled(pb, 3.5, p.accent);
        let meters = ((a.0 - b.0).hypot(a.1 - b.1)).max(0.0);
        let galley = painter.layout_no_wrap(
            format!("{meters:.2} m").replace('.', ","),
            FontId::proportional(12.0),
            p.text,
        );
        let back = egui::Rect::from_center_size(
            pa + (pb - pa) * 0.5 + vec2(0.0, -12.0),
            galley.size() + vec2(8.0, 4.0),
        );
        painter.rect_filled(back, 3, p.canvas.gamma_multiply(0.9));
        painter.galley(back.center() - galley.size() / 2.0, galley, p.text);
    }
    if let Some((a, b)) = s.marquee {
        let rect = egui::Rect::from_two_pos(to_screen(a), to_screen(b));
        painter.rect(
            rect,
            0,
            p.accent.gamma_multiply(0.12),
            Stroke::new(1.0, p.accent),
            egui::StrokeKind::Inside,
        );
    }

    // 6. Las insignias de las posiciones y las etiquetas, encima de todas las figuras.
    for badge in &badges {
        let galley = painter.layout_no_wrap(badge.text.clone(), FontId::proportional(10.0), p.text);
        let pill = egui::Rect::from_min_size(badge.at, galley.size() + vec2(8.0, 4.0));
        painter.rect_filled(pill, 4, badge.fill.gamma_multiply(0.9 * badge.opacity));
        if badge.chosen {
            painter.rect_stroke(
                pill,
                4,
                Stroke::new(2.0, p.accent),
                egui::StrokeKind::Outside,
            );
        }
        painter.text(
            pill.center(),
            Align2::CENTER_CENTER,
            &badge.text,
            FontId::proportional(10.0),
            theme::ink_on(badge.fill, p).gamma_multiply(badge.opacity),
        );
        let mut under = pill.left_bottom() + vec2(0.0, 2.0);
        for note in &badge.notes {
            if let Some(shown) = fit_text(painter, note, &FontId::proportional(10.0), 140.0) {
                painter.text(
                    under,
                    Align2::LEFT_TOP,
                    shown,
                    FontId::proportional(10.0),
                    p.text.gamma_multiply(badge.opacity),
                );
                under.y += 12.0;
            }
        }
    }
    for job in &labels {
        if let Some(text) = fit_text(painter, &job.text, &job.font, job.max_width) {
            painter.text(job.at, job.anchor, text, job.font.clone(), job.color);
        }
    }

    // 7. Las manijas de lo elegido: giro y tamaño. Lo bloqueado no las tiene. La posición elegida gira con la de su fantasma.
    if let Some(g) = &pose_handle {
        let h = to_screen(handle(g, handle_pad(g.kind) / s.scale));
        painter.line_segment([to_screen(handle(g, 0.0)), h], Stroke::new(1.0, p.accent));
        painter.circle(h, 5.0, p.canvas, Stroke::new(2.0, p.accent));
    }
    for item in handles
        .iter()
        .filter(|i| i.mic.as_ref().is_some_and(|m| m.kind == MicKind::Boom))
    {
        // El operador de un boom se arrastra: un aro alrededor de su figura.
        let operator = item
            .mic
            .as_ref()
            .and_then(|m| m.operator)
            .unwrap_or_else(|| {
                let back = rotate((-1.2, 0.0), item.rot);
                (item.x + back.0, item.y + back.1)
            });
        painter.circle_stroke(
            to_screen(operator),
            0.3 * s.scale.max(30.0),
            Stroke::new(1.5, p.accent.gamma_multiply(0.8)),
        );
    }
    for item in handles {
        let h = to_screen(handle(item, handle_pad(item.kind) / s.scale));
        painter.line_segment(
            [to_screen(handle(item, 0.0)), h],
            Stroke::new(1.0, p.accent),
        );
        painter.circle(h, 5.0, p.canvas, Stroke::new(2.0, p.accent));
        // Apertura y alcance: en el borde del haz, del patrón o del cono.
        for (_, at) in plan_checks::range_handles(item) {
            let c = to_screen(at);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(0.0, -6.0),
                    c + vec2(6.0, 0.0),
                    c + vec2(0.0, 6.0),
                    c + vec2(-6.0, 0.0),
                ],
                p.canvas,
                Stroke::new(2.0, p.accent),
            ));
        }
        for side in size_sides(item.kind) {
            let square =
                egui::Rect::from_center_size(to_screen(size_handle(item, side)), vec2(8.0, 8.0));
            painter.rect(
                square,
                1,
                p.canvas,
                Stroke::new(2.0, p.accent),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// La zona que cubre una cámara, una luz o un micrófono.
fn draw_coverage(
    s: &Scene,
    item: &BlockItem,
    o: f32,
    detail: figures::Detail,
    to_screen: &dyn Fn((f32, f32)) -> egui::Pos2,
) {
    let (p, painter) = (s.p, s.painter);
    let at = (item.x, item.y);
    match item.kind {
        BlockKind::Camera => {
            let points: Vec<egui::Pos2> = cone(item).into_iter().map(to_screen).collect();
            let color = p.plan_camera;
            if detail.fills {
                fan(painter, &points, color.gamma_multiply(0.16 * o));
            }
            painter.add(egui::Shape::closed_line(
                points.clone(),
                Stroke::new(1.0, color.gamma_multiply(0.35 * o)),
            ));
            // Los bordes del cuadro, más marcados que el relleno.
            if let (Some(first), Some(a), Some(b)) = (points.first(), points.get(1), points.last())
            {
                let edge = Stroke::new(1.5, color.gamma_multiply(0.9 * o));
                painter.line_segment([*first, *a], edge);
                painter.line_segment([*first, *b], edge);
            }
        }
        BlockKind::Light => {
            let spec = item.light.clone().unwrap_or_default();
            let color = theme::beam_color(p, item.kelvin, spec.gel);
            let bands = figures::beam_bands(
                at,
                item.rot,
                spec.beam_deg,
                spec.reach_m,
                figures::band_count(spec.fixture),
                spec.intensity,
            );
            for band in &bands {
                let points: Vec<egui::Pos2> = band.points.iter().map(|q| to_screen(*q)).collect();
                if detail.fills {
                    fan(painter, &points, color.gamma_multiply(band.alpha * o));
                }
            }
            if let Some(outer) = bands.first() {
                let points: Vec<egui::Pos2> = outer.points.iter().map(|q| to_screen(*q)).collect();
                painter.add(egui::Shape::closed_line(
                    points,
                    Stroke::new(1.0, color.gamma_multiply(0.6 * o)),
                ));
            }
        }
        BlockKind::Mic => {
            let spec = item.mic.clone().unwrap_or_default();
            let color = p.plan_sound;
            for lobe in figures::polar_lobes(at, item.rot, spec.pattern, spec.reach_m) {
                let points: Vec<egui::Pos2> = lobe.points.iter().map(|q| to_screen(*q)).collect();
                // El lóbulo de atrás capta en contrafase: más tenue.
                let k = if lobe.rear { 0.5 } else { 1.0 };
                if detail.fills {
                    fan(painter, &points, color.gamma_multiply(0.18 * k * o));
                }
                painter.add(egui::Shape::closed_line(
                    points,
                    Stroke::new(1.0, color.gamma_multiply(0.6 * k * o)),
                ));
            }
        }
        _ => {}
    }
}

/// El cuerpo de una figura con sus estados, su icono, su insignia y las etiquetas que van encima.
fn draw_figure(
    s: &Scene,
    item: &BlockItem,
    o: f32,
    detail: figures::Detail,
    to_screen: &dyn Fn((f32, f32)) -> egui::Pos2,
    labels: &mut Vec<LabelJob>,
    ghost: bool,
) {
    use figures::Paint;
    let (p, painter) = (s.p, s.painter);
    let local = figures::local_figure(item);
    let grow = figures::body_scale(figures::body_extent(&local), s.scale);
    let colors = paints(s, item);
    let chosen = !ghost && s.is_sel(item.id) && s.layer.owns(item.kind);
    // El borde: más claro al pasar el mouse (con un fundido), de acento y de 2 puntos si está elegido.
    let hover = kit::fade(
        s.ui,
        s.ui.scope_id().with(("plan_hover", item.id)),
        !ghost && s.hovered == Some(item.id) && s.layer.owns(item.kind),
        tokens::ANIM_FAST,
    );
    let rest = theme::mix(p.canvas, p.text, 0.5);
    let outline = if chosen {
        Stroke::new(2.0, p.accent.gamma_multiply(o))
    } else {
        Stroke::new(
            1.0 + 0.5 * hover,
            theme::mix(rest, p.text, hover).gamma_multiply(o),
        )
    };
    let mut lo = egui::pos2(f32::MAX, f32::MAX);
    let mut hi = egui::pos2(f32::MIN, f32::MIN);
    for part in figures::figure(item, grow) {
        let points: Vec<egui::Pos2> = part.points.iter().map(|q| to_screen(*q)).collect();
        match part.paint {
            Paint::Body | Paint::Void => {
                for q in &points {
                    lo = lo.min(*q);
                    hi = hi.max(*q);
                }
                let fill = if part.paint == Paint::Body {
                    colors.body
                } else {
                    colors.hollow
                };
                painter.add(egui::Shape::convex_polygon(
                    points,
                    fill.gamma_multiply(o),
                    outline,
                ));
            }
            Paint::Detail => {
                painter.add(egui::Shape::convex_polygon(
                    points,
                    colors.detail.gamma_multiply(o),
                    Stroke::NONE,
                ));
            }
            Paint::Muted => {
                painter.add(egui::Shape::convex_polygon(
                    points,
                    theme::mix(p.canvas, p.dim, 0.8).gamma_multiply(o),
                    Stroke::NONE,
                ));
            }
            Paint::Line => {
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(1.5, colors.line.gamma_multiply(o)),
                ));
            }
            Paint::Dashed => {
                let mut points = points;
                if part.closed {
                    points.extend(points.first().copied());
                }
                painter.add(egui::Shape::dashed_line(
                    &points,
                    Stroke::new(1.5, colors.line.gamma_multiply(o)),
                    4.0,
                    3.0,
                ));
            }
        }
    }
    if lo.x > hi.x || ghost {
        return;
    }
    let body = egui::Rect::from_min_max(lo, hi);
    let size = body.width().max(body.height());

    // El icono (o las iniciales de la persona) adentro del cuerpo, si mide 18 px o más.
    let ink = theme::ink_on(colors.body, p).gamma_multiply(o);
    let linked = item
        .character
        .and_then(|id| s.project.characters.iter().find(|c| c.id == id));
    if detail.icons && size >= figures::ICON_MIN_PX {
        let glyph_size = (size * 0.5).clamp(10.0, 26.0);
        match (item.kind, linked) {
            (BlockKind::Actor, Some(c)) => {
                painter.text(
                    body.center(),
                    Align2::CENTER_CENTER,
                    initials(&clean_character_name(&c.name)),
                    FontId::new(glyph_size.min(14.0), theme::bold_family()),
                    ink,
                );
            }
            _ => {
                if let Some(glyph) = figure_icon(item) {
                    painter.text(
                        body.center(),
                        Align2::CENTER_CENTER,
                        glyph,
                        FontId::proportional(glyph_size),
                        ink,
                    );
                }
            }
        }
    }

    // La insignia con la letra de la cámara.
    if item.kind == BlockKind::Camera && !item.label.is_empty() {
        let badge =
            egui::Rect::from_center_size(body.left_top() - vec2(2.0, 2.0), vec2(14.0, 14.0));
        painter.rect_filled(badge, 3, p.plan_camera.gamma_multiply(o));
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            item.label.chars().take(2).collect::<String>(),
            FontId::new(10.0, theme::bold_family()),
            theme::ink_on(p.plan_camera, p).gamma_multiply(o),
        );
    }

    // Etiqueta: debajo del cuerpo; la de un mueble va adentro, si entra.
    if detail.labels {
        let name = match item.kind {
            BlockKind::Actor => {
                linked.map_or(item.label.clone(), |c| clean_character_name(&c.name))
            }
            _ => item.label.clone(),
        };
        if !name.is_empty() {
            let (at, anchor, max_width) = match item.kind {
                BlockKind::Object | BlockKind::Opening => {
                    (body.center(), Align2::CENTER_CENTER, body.width() - 4.0)
                }
                BlockKind::Wall => (body.center(), Align2::CENTER_CENTER, 0.0),
                _ => (
                    body.center_bottom() + vec2(0.0, 3.0),
                    Align2::CENTER_TOP,
                    (size * 3.0).max(72.0),
                ),
            };
            if max_width > 0.0 {
                labels.push(LabelJob {
                    at,
                    anchor,
                    text: name,
                    font: FontId::proportional(12.0),
                    color: p.text.gamma_multiply(o),
                    max_width,
                });
            }
        }
    }

    // Estados: candado (bloqueado) y advertencia (puede verse en cuadro).
    if item.locked {
        let at = body.right_bottom();
        painter.circle_filled(at, 7.0, p.canvas.gamma_multiply(o));
        painter.text(
            at,
            Align2::CENTER_CENTER,
            icon::LOCK_SIMPLE,
            FontId::proportional(10.0),
            p.text.gamma_multiply(o),
        );
    }
    let why: Vec<&str> = [Seen::Item, Seen::Pole]
        .iter()
        .filter_map(|k| s.warnings.get(&(item.id, *k)).map(String::as_str))
        .collect();
    if !why.is_empty() {
        let why = why.join("\n");
        let at = body.right_top();
        painter.circle_filled(at, 8.0, p.canvas.gamma_multiply(o));
        painter.text(
            at,
            Align2::CENTER_CENTER,
            icon::WARNING,
            FontId::proportional(13.0),
            p.warning.gamma_multiply(o),
        );
        s.ui.interact(
            egui::Rect::from_center_size(at, vec2(16.0, 16.0)),
            s.ui.scope_id().with(("plan_warning", item.id)),
            egui::Sense::hover(),
        )
        .on_hover_text(why);
    }
}

/// Dibuja el plano de la locación de rodaje detrás de la cuadrícula, con su ancho real en metros.
fn draw_floor_plan(
    ui: &Ui,
    p: &Palette,
    painter: &egui::Painter,
    project: &Project,
    blocking: u64,
    origin: egui::Pos2,
    scale: f32,
) {
    let Some(b) = project.blockings.iter().find(|b| b.id == blocking) else {
        return;
    };
    let Some(place) = project
        .scene_shoot_location(b.scene)
        .and_then(|l| project.shoot_locations.iter().find(|s| s.id == l))
    else {
        return;
    };
    let (Some(path), Some(width_m)) = (&place.floor_plan, place.plan_width_m) else {
        return;
    };
    let image = egui::Image::new(format!("file://{}", project.resolve_path(path).display()));
    let Ok(egui::load::TexturePoll::Ready { texture }) =
        image.load_for_size(ui.ctx(), vec2(512.0, 512.0))
    else {
        return;
    };
    let w = width_m * scale;
    let h = w * texture.size.y / texture.size.x.max(1.0);
    let rect = egui::Rect::from_min_size(origin, vec2(w, h));
    painter.image(
        texture.id,
        rect,
        egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        p.text.gamma_multiply(0.45),
    );
}

fn resize_cursor(side: (i8, i8)) -> egui::CursorIcon {
    match side {
        (0, _) => egui::CursorIcon::ResizeVertical,
        (_, 0) => egui::CursorIcon::ResizeHorizontal,
        (a, b) if a == b => egui::CursorIcon::ResizeNwSe,
        _ => egui::CursorIcon::ResizeNeSw,
    }
}

/// Los elementos con fila en la línea de tiempo: lo que se mueve, lo elegido y las cámaras con planos vinculados.
fn timeline_rows<'a>(
    project: &'a Project,
    state: &BlockState,
    blocking: u64,
) -> Vec<&'a BlockItem> {
    let Some(b) = project.blockings.iter().find(|b| b.id == blocking) else {
        return Vec::new();
    };
    b.items
        .iter()
        .filter(|i| {
            !i.keys.is_empty()
                || state.selected == Some(i.id)
                || (i.kind == BlockKind::Camera
                    && project
                        .shots
                        .iter()
                        .any(|s| s.blocking == Some(blocking) && s.camera == Some(i.id)))
        })
        .collect()
}

/// El alto de la línea de tiempo: los controles, la regla y una fila por elemento (hasta `TL_MAX_ROWS`; el resto se
/// desplaza).
fn timeline_height(project: &Project, state: &BlockState, blocking: u64) -> f32 {
    let rows = timeline_rows(project, state, blocking)
        .len()
        .clamp(1, TL_MAX_ROWS);
    TL_HEADER_H + TL_RULER_H + rows as f32 * TL_ROW_H + tokens::SP_S
}

/// Un plano vinculado a una cámara: su id, la cámara, su tramo y su código.
type ShotRow = (u64, u64, Option<(f32, f32)>, String);
/// Un cambio a una posición de una cosa, que dice si cambió algo.
type KeyEdit = Box<dyn FnOnce(&mut BlockItem) -> bool>;

/// Lo que pide la línea de tiempo en un cuadro; se aplica al final, cuando ya no hay nada prestado.
enum TlEdit {
    Select(u64),
    GoTo {
        item: u64,
        t: f32,
    },
    KeyTime {
        item: u64,
        index: usize,
        t: f32,
    },
    Label {
        item: u64,
        index: usize,
        text: String,
    },
    Hold {
        item: u64,
        index: usize,
        seconds: f32,
    },
    EaseIn {
        item: u64,
        index: usize,
        on: bool,
    },
    EaseOut {
        item: u64,
        index: usize,
        on: bool,
    },
    Lens {
        item: u64,
        index: usize,
        mm: Option<f32>,
    },
    Delete {
        item: u64,
        index: usize,
    },
    ShotRange {
        shot: u64,
        range: (f32, f32),
    },
    SelectShot(u64),
}

/// El color con que se marca cada tipo en la línea de tiempo.
fn kind_color(p: &Palette, kind: BlockKind) -> egui::Color32 {
    match kind {
        BlockKind::Camera => p.plan_camera,
        BlockKind::Light | BlockKind::Modifier => p.plan_light,
        BlockKind::Mic => p.plan_sound,
        _ => p.accent,
    }
}

/// Línea de tiempo (plantas.md §5.3): reproducir, velocidad, grabación automática y duración; la regla con la posición
/// actual; y una fila por elemento que se mueve, con un rombo por posición (se arrastran, con imán) y los tramos de los
/// planos sobre la fila de cada cámara.
fn timeline(
    ui: &mut Ui,
    state: &mut BlockState,
    project: &mut Project,
    p: &Palette,
    blocking: u64,
) -> bool {
    use crate::ui::kit;
    let Some(duration) = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .map(|b| b.duration)
    else {
        return false;
    };
    let duration = duration.max(0.5);
    let height = timeline_height(project, state, blocking);
    state.tl_h = height;
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("blocking_timeline"), rect));
    let mut edits: Vec<TlEdit> = Vec::new();
    let mut duration_delta = 0;
    let last_key = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .map_or(0.0, |b| {
            b.items
                .iter()
                .flat_map(|i| i.keys.iter().map(|k| k.t))
                .fold(0.0, f32::max)
        });

    // --- Los controles.
    let header = egui::Rect::from_min_size(rect.min, vec2(rect.width(), TL_HEADER_H));
    let mut head = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(header.shrink2(vec2(tokens::SP_L, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    kit::sides(
        &mut head,
        |ui| {
            let (button, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
            let fill = if resp.hovered() {
                p.container_hi
            } else {
                p.container
            };
            ui.painter()
                .circle(button.center(), 14.0, fill, Stroke::new(1.0, p.border));
            let c = button.center();
            if state.playing {
                for dx in [-3.5, 3.5] {
                    ui.painter().rect_filled(
                        egui::Rect::from_center_size(c + vec2(dx, 0.0), vec2(3.5, 11.0)),
                        1,
                        p.text,
                    );
                }
            } else {
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        c + vec2(-4.0, -6.5),
                        c + vec2(-4.0, 6.5),
                        c + vec2(6.5, 0.0),
                    ],
                    p.text,
                    Stroke::NONE,
                ));
            }
            resp.clone().on_hover_text(if state.playing {
                "Pausar (Espacio)"
            } else {
                "Reproducir el movimiento (Espacio)"
            });
            if resp.clicked() {
                state.toggle_play(duration);
            }
            ui.label(
                RichText::new(format!("{:.1} / {:.0} s", state.time, duration).replace('.', ","))
                    .font(FontId::new(13.0, theme::bold_family()))
                    .color(p.text),
            );
            let speeds = [0.5_f32, 1.0, 2.0];
            let active = speeds
                .iter()
                .position(|v| (*v - state.speed).abs() < 0.01)
                .unwrap_or(1);
            if let Some(i) = kit::segmented(ui, p, &["0,5×", "1×", "2×"], active) {
                state.speed = speeds[i];
            }
            // La grabación automática: encendida, mover o girar algo en otro instante crea o corrige su posición.
            let rec = ui
                .add(egui::Button::selectable(state.auto_record, "     Grabar"))
                .on_hover_text(
                    "Grabación automática: con la línea de tiempo en otro instante, mover o girar algo crea o corrige su posición ahí. Apagada, los cambios van a la posición de partida.",
                );
            let dot = if state.auto_record { p.danger } else { p.dim };
            ui.painter().circle_filled(
                egui::pos2(rec.rect.left() + 14.0, rec.rect.center().y),
                5.0,
                dot,
            );
            if rec.clicked() {
                state.auto_record = !state.auto_record;
            }
        },
        |ui| {
            // La duración total, en un campo de número (reglas §2.10); no baja de la última posición.
            let min = last_key.ceil().max(1.0);
            let mut seconds = duration;
            if kit::number_field(ui, &mut seconds, min..=600.0, " s").changed() {
                duration_delta = (seconds - duration).round() as i32;
            }
            ui.label(RichText::new("Duración").size(12.0).color(p.dim));
        },
    );

    // --- La regla y la posición actual.
    let track_l = rect.left() + TL_LABEL_W;
    let track_r = rect.right() - tokens::SP_L;
    let px_per_s = ((track_r - track_l) / duration).max(1.0);
    let x_of = |t: f32| track_l + (track_r - track_l) * (t / duration).clamp(0.0, 1.0);
    let t_of = |x: f32| ((x - track_l) / (track_r - track_l)).clamp(0.0, 1.0) * duration;
    let ruler = egui::Rect::from_min_max(
        egui::pos2(track_l - 6.0, header.bottom()),
        egui::pos2(track_r + 6.0, header.bottom() + TL_RULER_H),
    );
    // Los instantes de todas las posiciones: lo que atrae a lo que se arrastra.
    let all_times: Vec<f32> = timeline_rows(project, state, blocking)
        .iter()
        .flat_map(|i| i.keys.iter().map(|k| k.t))
        .collect();
    let radius = 6.0 / px_per_s;
    let ruler_resp = ui.interact(
        ruler,
        ui.make_persistent_id("plant_ruler"),
        egui::Sense::click_and_drag(),
    );
    if let (true, Some(pos)) = (
        ruler_resp.is_pointer_button_down_on() || ruler_resp.dragged(),
        ruler_resp.interact_pointer_pos(),
    ) {
        state.time = motion::snap_time(t_of(pos.x), &all_times, radius, 0.1).clamp(0.0, duration);
        state.playing = false;
    }
    let base_y = ruler.bottom();
    painter.line_segment(
        [
            egui::pos2(track_l, base_y - 1.0),
            egui::pos2(track_r, base_y - 1.0),
        ],
        Stroke::new(1.0, p.border),
    );
    let tick_every = (28.0 / px_per_s).ceil().max(1.0) as i32;
    for second in (0..=duration.floor() as i32).step_by(tick_every as usize) {
        let x = x_of(second as f32);
        painter.line_segment(
            [egui::pos2(x, base_y - 5.0), egui::pos2(x, base_y - 1.0)],
            Stroke::new(1.0, p.dim),
        );
        painter.text(
            egui::pos2(x + 2.0, ruler.top() + 1.0),
            Align2::LEFT_TOP,
            second.to_string(),
            FontId::proportional(9.0),
            p.dim,
        );
    }
    // La marca roja de la grabación automática.
    if state.auto_record {
        painter.circle_filled(
            egui::pos2(rect.left() + tokens::SP_L + 4.0, ruler.center().y),
            4.0,
            p.danger,
        );
        painter.text(
            egui::pos2(rect.left() + tokens::SP_L + 14.0, ruler.center().y),
            Align2::LEFT_CENTER,
            "Grabando",
            FontId::proportional(10.0),
            p.danger,
        );
    }

    // --- Una fila por elemento.
    let rows: Vec<BlockItem> = timeline_rows(project, state, blocking)
        .into_iter()
        .cloned()
        .collect();
    let codes = project.shot_codes();
    let shots: Vec<ShotRow> = project
        .shots
        .iter()
        .filter(|s| s.blocking == Some(blocking))
        .filter_map(|s| {
            s.camera.map(|c| {
                (
                    s.id,
                    c,
                    s.range,
                    codes.get(&s.id).cloned().unwrap_or_default(),
                )
            })
        })
        .collect();
    let rows_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), ruler.bottom()),
        egui::pos2(rect.right(), rect.bottom() - tokens::SP_S),
    );
    let mut rows_ui = ui.new_child(egui::UiBuilder::new().max_rect(rows_rect));
    egui::ScrollArea::vertical()
        .id_salt("plant_timeline_rows")
        .auto_shrink([false, false])
        .show(&mut rows_ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if rows.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Movelo en otro instante, o apretá K, para crearle posiciones.")
                        .size(12.0)
                        .color(p.dim),
                );
            }
            for item in &rows {
                let (row, _) =
                    ui.allocate_exact_size(vec2(rows_rect.width(), TL_ROW_H), egui::Sense::hover());
                let rp = ui.painter_at(row);
                let cy = row.center().y - 2.0;
                let color = kind_color(p, item.kind);
                if state.selected == Some(item.id) {
                    rp.rect_filled(row, 0, p.selection.gamma_multiply(0.5));
                }
                // El nombre: un clic elige el elemento.
                let label_rect =
                    egui::Rect::from_min_size(row.min, vec2(TL_LABEL_W - 6.0, TL_ROW_H));
                let label_resp = ui.interact(
                    label_rect,
                    ui.make_persistent_id(("plant_tl_label", item.id)),
                    egui::Sense::click(),
                );
                rp.circle_filled(
                    egui::pos2(row.left() + tokens::SP_L, row.center().y),
                    4.0,
                    color,
                );
                if let Some(glyph) = figure_icon(item) {
                    rp.text(
                        egui::pos2(row.left() + tokens::SP_L + 12.0, row.center().y),
                        Align2::LEFT_CENTER,
                        glyph,
                        FontId::proportional(13.0),
                        p.dim,
                    );
                }
                let name = if item.label.is_empty() {
                    kind_name(item.kind).to_string()
                } else {
                    item.label.clone()
                };
                if let Some(shown) =
                    fit_text(&rp, &name, &FontId::proportional(12.0), TL_LABEL_W - 52.0)
                {
                    rp.text(
                        egui::pos2(row.left() + tokens::SP_L + 30.0, row.center().y),
                        Align2::LEFT_CENTER,
                        shown,
                        FontId::proportional(12.0),
                        p.text,
                    );
                }
                if label_resp.clicked() {
                    edits.push(TlEdit::Select(item.id));
                }
                label_resp.on_hover_text(&name);
                rp.line_segment(
                    [
                        egui::pos2(track_l, row.center().y),
                        egui::pos2(track_r, row.center().y),
                    ],
                    Stroke::new(1.0, theme::mix(p.bg, p.border, 0.6)),
                );

                // Los tramos de los planos de una cámara, en una franja de abajo.
                if item.kind == BlockKind::Camera {
                    for (shot, _, range, code) in
                        shots.iter().filter(|(_, cam, ..)| *cam == item.id)
                    {
                        let (a, b) = motion::shot_span(*range, duration);
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(x_of(a), row.bottom() - 8.0),
                            egui::pos2(x_of(b), row.bottom() - 2.0),
                        );
                        let chosen = state.sel_shot == Some(*shot);
                        rp.rect_filled(
                            bar,
                            2,
                            p.plan_camera
                                .gamma_multiply(if chosen { 0.95 } else { 0.45 }),
                        );
                        if chosen {
                            rp.rect_stroke(
                                bar,
                                2,
                                Stroke::new(1.0, p.accent),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if bar.width() > 22.0 {
                            rp.text(
                                bar.left_center() + vec2(3.0, 0.0),
                                Align2::LEFT_CENTER,
                                code,
                                FontId::proportional(9.0),
                                theme::ink_on(p.plan_camera, p),
                            );
                        }
                        let id = ui.make_persistent_id(("plant_tl_shot", *shot));
                        let zone = bar.expand2(vec2(0.0, 3.0));
                        let body =
                            ui.interact(zone, id.with("body"), egui::Sense::click_and_drag());
                        let left = ui.interact(
                            egui::Rect::from_center_size(
                                bar.left_center(),
                                vec2(8.0, zone.height()),
                            ),
                            id.with("l"),
                            egui::Sense::drag(),
                        );
                        let right = ui.interact(
                            egui::Rect::from_center_size(
                                bar.right_center(),
                                vec2(8.0, zone.height()),
                            ),
                            id.with("r"),
                            egui::Sense::drag(),
                        );
                        let pointer_t = ui
                            .input(|i| i.pointer.latest_pos())
                            .map_or(0.0, |pos| t_of(pos.x));
                        let snap = |t: f32| motion::snap_time(t, &all_times, radius, 0.1);
                        if body.clicked() || body.drag_started() {
                            edits.push(TlEdit::SelectShot(*shot));
                        }
                        if left.dragged() {
                            edits.push(TlEdit::SelectShot(*shot));
                            edits.push(TlEdit::ShotRange {
                                shot: *shot,
                                range: (snap(pointer_t).clamp(0.0, b - 0.2), b),
                            });
                        } else if right.dragged() {
                            edits.push(TlEdit::SelectShot(*shot));
                            edits.push(TlEdit::ShotRange {
                                shot: *shot,
                                range: (a, snap(pointer_t).clamp(a + 0.2, duration)),
                            });
                        } else if body.dragged() {
                            // Se mueve entero, agarrado desde donde se apretó.
                            let grab_id = id.with("grab");
                            if body.drag_started() {
                                // Desde donde se apretó, no desde donde se nota el arrastre.
                                let pressed_t = ui
                                    .input(|i| i.pointer.press_origin())
                                    .map_or(pointer_t, |pos| t_of(pos.x));
                                ui.data_mut(|d| d.insert_temp(grab_id, pressed_t - a));
                            }
                            let grab: f32 = ui.data(|d| d.get_temp(grab_id)).unwrap_or(0.0);
                            let span = b - a;
                            let start =
                                snap(pointer_t - grab).clamp(0.0, (duration - span).max(0.0));
                            edits.push(TlEdit::ShotRange {
                                shot: *shot,
                                range: (start, start + span),
                            });
                        }
                        if left.hovered() || right.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        }
                        body.on_hover_text(format!(
                            "Plano {code}: de {} a {}. Arrastrá para moverlo o estirá sus bordes.",
                            motion::seconds_text(a),
                            motion::seconds_text(b)
                        ));
                    }
                }

                // Las posiciones: un rombo cada una; la de partida no se arrastra.
                let positions = motion::positions(item);
                for pos in &positions {
                    if pos.hold > 0.0 {
                        rp.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(x_of(pos.t), cy - 1.5),
                                egui::pos2(x_of(pos.t + pos.hold), cy + 1.5),
                            ),
                            1,
                            color.gamma_multiply(0.6),
                        );
                    }
                    let c = egui::pos2(x_of(pos.t), cy);
                    let on = state.sel_pose == Some((item.id, pos.index));
                    let size = if on { 7.5 } else { 6.0 };
                    let resp = ui.interact(
                        egui::Rect::from_center_size(c, vec2(14.0, 16.0)),
                        ui.make_persistent_id(("plant_tl_key", item.id, pos.number)),
                        if pos.index.is_some() {
                            egui::Sense::click_and_drag()
                        } else {
                            egui::Sense::click()
                        },
                    );
                    let fill = if pos.index.is_none() {
                        p.canvas
                    } else if on {
                        p.accent
                    } else {
                        color
                    };
                    rp.add(egui::Shape::convex_polygon(
                        vec![
                            c + vec2(0.0, -size),
                            c + vec2(size, 0.0),
                            c + vec2(0.0, size),
                            c + vec2(-size, 0.0),
                        ],
                        fill,
                        Stroke::new(
                            if on { 2.0 } else { 1.0 },
                            if on { p.accent } else { color },
                        ),
                    ));
                    if let Some(n) = pos.index.filter(|_| resp.dragged()) {
                        // Entre sus vecinas, y pegándose a otras posiciones si queda cerca.
                        let lo = positions[pos.number - 2].t + 2.0 * crate::model::KEY_EPSILON;
                        let hi = positions
                            .get(pos.number)
                            .map_or(duration, |next| next.t - 2.0 * crate::model::KEY_EPSILON);
                        let others: Vec<f32> = all_times
                            .iter()
                            .copied()
                            .filter(|t| (*t - pos.t).abs() > 1e-4)
                            .chain([state.time])
                            .collect();
                        let pointer_t = ui
                            .input(|i| i.pointer.latest_pos())
                            .map_or(pos.t, |pp| t_of(pp.x));
                        let t = motion::snap_time(pointer_t, &others, radius, 0.1)
                            .clamp(lo.min(hi), hi.max(lo));
                        edits.push(TlEdit::KeyTime {
                            item: item.id,
                            index: n,
                            t,
                        });
                    }
                    if resp.clicked() {
                        edits.push(TlEdit::GoTo {
                            item: item.id,
                            t: pos.t,
                        });
                    }
                    let tip = format!(
                        "{} · {}",
                        motion::badge_text("", pos),
                        if pos.label.is_empty() {
                            "sin marca"
                        } else {
                            pos.label.as_str()
                        }
                    );
                    let resp = resp.on_hover_text(tip);
                    if let Some(n) = pos.index {
                        resp.context_menu(|ui| {
                            if ui.button("Ir a esta posición").clicked() {
                                edits.push(TlEdit::GoTo {
                                    item: item.id,
                                    t: pos.t,
                                });
                                ui.close();
                            }
                            ui.label(RichText::new("Marca").size(12.0).color(p.dim));
                            let mut text = pos.label.clone();
                            if ui
                                .add(
                                    TextEdit::singleline(&mut text)
                                        .desired_width(180.0)
                                        .hint_text("Juana se sienta"),
                                )
                                .changed()
                            {
                                edits.push(TlEdit::Label {
                                    item: item.id,
                                    index: n,
                                    text,
                                });
                            }
                            let mut hold = pos.hold;
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Pausa").size(12.0).color(p.dim));
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut hold)
                                            .range(0.0..=60.0)
                                            .speed(0.1)
                                            .suffix(" s"),
                                    )
                                    .changed()
                                {
                                    edits.push(TlEdit::Hold {
                                        item: item.id,
                                        index: n,
                                        seconds: hold,
                                    });
                                }
                            });
                            let key = &item.keys[n];
                            let (mut ease_in, mut ease_out) = (key.ease_in, key.ease_out);
                            if ui.checkbox(&mut ease_in, "Frenar al llegar").changed() {
                                edits.push(TlEdit::EaseIn {
                                    item: item.id,
                                    index: n,
                                    on: ease_in,
                                });
                            }
                            if ui.checkbox(&mut ease_out, "Acelerar al salir").changed() {
                                edits.push(TlEdit::EaseOut {
                                    item: item.id,
                                    index: n,
                                    on: ease_out,
                                });
                            }
                            // El zoom de una cámara en esta posición (un efecto Vértigo, por ejemplo).
                            if item.kind == BlockKind::Camera {
                                let key = &item.keys[n];
                                let mut has = key.lens_mm.is_some();
                                let mut mm = key.lens_mm.unwrap_or_else(|| item.lens_at(pos.t));
                                if ui.checkbox(&mut has, "Cambia el zoom").changed() {
                                    edits.push(TlEdit::Lens {
                                        item: item.id,
                                        index: n,
                                        mm: has.then_some(mm),
                                    });
                                }
                                if has
                                    && ui
                                        .add(
                                            egui::DragValue::new(&mut mm)
                                                .range(8.0..=600.0)
                                                .speed(0.5)
                                                .suffix(" mm"),
                                        )
                                        .changed()
                                {
                                    edits.push(TlEdit::Lens {
                                        item: item.id,
                                        index: n,
                                        mm: Some(mm),
                                    });
                                }
                            }
                            ui.separator();
                            if ui
                                .button(RichText::new("Eliminar posición").color(p.danger))
                                .clicked()
                            {
                                edits.push(TlEdit::Delete {
                                    item: item.id,
                                    index: n,
                                });
                                ui.close();
                            }
                        });
                    }
                }
            }
        });

    // La posición actual cruza la regla y todas las filas; roja mientras se graba.
    let head_color = if state.auto_record {
        p.danger
    } else {
        p.accent
    };
    let x = x_of(state.time);
    painter.line_segment(
        [
            egui::pos2(x, ruler.top()),
            egui::pos2(x, rect.bottom() - tokens::SP_S),
        ],
        Stroke::new(1.5, head_color.gamma_multiply(0.8)),
    );
    painter.circle(
        egui::pos2(x, ruler.center().y),
        5.0,
        p.panel,
        Stroke::new(2.0, head_color),
    );

    // --- Se aplica lo pedido.
    let mut changed = false;
    if duration_delta != 0
        && let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking)
    {
        let min = last_key.ceil().max(1.0);
        b.duration = (b.duration + duration_delta as f32).clamp(min, 600.0);
        changed = true;
    }
    for edit in edits {
        match edit {
            TlEdit::Select(id) => {
                state.selected = Some(id);
                state.sel_pose = None;
            }
            TlEdit::GoTo { item, t } => {
                state.selected = Some(item);
                state.go_to_time(project, item, t);
            }
            TlEdit::SelectShot(id) => state.sel_shot = Some(id),
            TlEdit::ShotRange { shot, range } => {
                if let Some(s) = project.shots.iter_mut().find(|s| s.id == shot)
                    && s.range != Some(range)
                {
                    s.range = Some(range);
                    changed = true;
                }
            }
            other => {
                let (item, f): (u64, KeyEdit) = match other {
                    TlEdit::KeyTime { item, index, t } => (
                        item,
                        Box::new(move |i| motion::move_key_time(i, index, t).is_some()),
                    ),
                    TlEdit::Label { item, index, text } => (
                        item,
                        Box::new(move |i| i.keys.get_mut(index).map(|k| k.label = text).is_some()),
                    ),
                    TlEdit::Hold {
                        item,
                        index,
                        seconds,
                    } => (
                        item,
                        Box::new(move |i| {
                            i.keys.get_mut(index).map(|k| k.hold = seconds).is_some()
                        }),
                    ),
                    TlEdit::EaseIn { item, index, on } => (
                        item,
                        Box::new(move |i| i.keys.get_mut(index).map(|k| k.ease_in = on).is_some()),
                    ),
                    TlEdit::Lens { item, index, mm } => (
                        item,
                        Box::new(move |i| i.keys.get_mut(index).map(|k| k.lens_mm = mm).is_some()),
                    ),
                    TlEdit::EaseOut { item, index, on } => (
                        item,
                        Box::new(move |i| i.keys.get_mut(index).map(|k| k.ease_out = on).is_some()),
                    ),
                    TlEdit::Delete { item, index } => (
                        item,
                        Box::new(move |i| {
                            let ok = index < i.keys.len();
                            if ok {
                                i.keys.remove(index);
                            }
                            ok
                        }),
                    ),
                    _ => continue,
                };
                let deleted = matches!(&state.sel_pose, Some((id, Some(_))) if *id == item);
                if let Some(target) = project
                    .blockings
                    .iter_mut()
                    .find(|b| b.id == blocking)
                    .and_then(|b| b.items.iter_mut().find(|i| i.id == item))
                {
                    changed |= f(target);
                }
                // Una posición que ya no está deja de estar elegida.
                if deleted
                    && let Some((id, Some(n))) = state.sel_pose
                    && project
                        .blockings
                        .iter()
                        .flat_map(|b| b.items.iter())
                        .find(|i| i.id == id)
                        .is_none_or(|i| n >= i.keys.len())
                {
                    state.sel_pose = None;
                }
            }
        }
    }
    changed
}

// ------------------------------------------------------------------------------------------------
// Panel derecho
// ------------------------------------------------------------------------------------------------

fn kind_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Actor => "Persona",
        BlockKind::Object | BlockKind::Opening => "Mueble u objeto",
        BlockKind::Wall => "Pared",
        BlockKind::Camera => "Cámara",
        BlockKind::Light | BlockKind::Modifier => "Luz",
        BlockKind::Mic => "Micrófono",
    }
}

fn properties(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) -> bool {
    inspector::selection_tab(ui, state, project, p)
}

/// Los cuerpos de cámara del proyecto, para ajustar su sensor.
pub(crate) fn bodies_editor(ui: &mut Ui, p: &Palette, project: &mut Project) -> bool {
    let mut changed = false;
    if project.camera_bodies.is_empty() {
        return false;
    }
    ui.add_space(tokens::SP_M);
    ui.label(
        RichText::new("CUERPOS DE CÁMARA DEL PROYECTO")
            .size(11.0)
            .color(p.dim),
    );
    ui.add_space(tokens::SP_S);
    let mut remove: Option<u64> = None;
    for body in &mut project.camera_bodies {
        egui::Frame::new().fill(p.field).stroke(Stroke::new(1.0, p.border)).corner_radius(4).inner_margin(8).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                changed |= ui.add(TextEdit::singleline(&mut body.name).desired_width(crate::ui::kit::beside(ui, 1, 0.0, 60.0))).changed();
                if widgets::icon_button(ui, p, icon::TRASH, "Quitar este cuerpo de cámara (las cámaras vuelven a full frame)", true).clicked() {
                    remove = Some(body.id);
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Factor de recorte").size(12.0).color(p.dim));
                changed |= ui.add(egui::DragValue::new(&mut body.crop_factor).range(0.5..=4.0).speed(0.01).prefix("×")).changed();
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Ancho del sensor").size(12.0).color(p.dim));
                let mut mm = body.sensor_w_mm.unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut mm).range(0.0..=60.0).speed(0.1).suffix(" mm")).on_hover_text("0: se calcula con el factor de recorte").changed() {
                    body.sensor_w_mm = (mm > 0.0).then_some(mm);
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Recorte del video").size(12.0).color(p.dim));
                changed |= ui.add(egui::DragValue::new(&mut body.video_crop).range(1.0..=3.0).speed(0.01).prefix("×")).on_hover_text("Algunas cámaras recortan más al grabar en 4K o con estabilización digital").changed();
            });
        });
        ui.add_space(tokens::SP_S);
    }
    if let Some(id) = remove {
        project.remove_camera_body(id);
        changed = true;
    }
    changed
}

/// A qué planta pertenece cada cosa.
fn layer_of(kind: BlockKind) -> Layer {
    match kind {
        BlockKind::Light | BlockKind::Modifier => Layer::Lights,
        BlockKind::Mic => Layer::Sound,
        _ => Layer::Camera,
    }
}

/// Pestaña ELEMENTOS: todo lo que hay en la planta, agrupado. Un clic lo selecciona en el lienzo.
fn items_tab(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) -> bool {
    let Some(b) = state
        .blocking
        .and_then(|id| project.blockings.iter().find(|b| b.id == id))
    else {
        crate::ui::kit::empty_state(ui, p, icon::VIDEO_CAMERA, "Todavía no hay planta.", None);
        return false;
    };
    if b.items.is_empty() {
        crate::ui::kit::empty_state(
            ui,
            p,
            icon::VIDEO_CAMERA,
            "Todavía no hay nada. Usá las herramientas de la barra de contexto.",
            None,
        );
        return false;
    }
    let groups: [(&str, &[BlockKind]); 5] = [
        ("Cámaras activas", &[BlockKind::Camera]),
        ("Personas", &[BlockKind::Actor]),
        (
            "Objetos",
            &[BlockKind::Object, BlockKind::Opening, BlockKind::Wall],
        ),
        ("Fuentes", &[BlockKind::Light, BlockKind::Modifier]),
        ("Micrófonos", &[BlockKind::Mic]),
    ];
    // Ocultar y bloquear desde la lista; se aplican al final, cuando ya no hay nada prestado.
    let mut toggles: Vec<(u64, bool)> = Vec::new();
    let Some(blocking) = state.blocking else {
        return false;
    };
    for (title, kinds) in groups {
        let members: Vec<&BlockItem> = b.items.iter().filter(|i| kinds.contains(&i.kind)).collect();
        if members.is_empty() {
            continue;
        }
        crate::ui::kit::section(ui, p, title, None);
        for item in members {
            let name = match item.kind {
                BlockKind::Actor => item
                    .character
                    .and_then(|id| project.characters.iter().find(|c| c.id == id))
                    .map_or(item.label.clone(), |c| clean_character_name(&c.name)),
                _ => item.label.clone(),
            };
            let shown = if name.is_empty() {
                kind_name(item.kind).to_string()
            } else {
                name
            };
            let code = match item.kind {
                BlockKind::Camera => icon::VIDEO_CAMERA,
                BlockKind::Actor => icon::USER,
                BlockKind::Object | BlockKind::Opening => icon::CUBE,
                BlockKind::Wall => icon::WALL,
                BlockKind::Light | BlockKind::Modifier => icon::SUN,
                BlockKind::Mic => icon::MICROPHONE,
            };
            let selected = state.multi.contains(&item.id) || state.selected == Some(item.id);
            let row = crate::ui::kit::tree_row(
                ui,
                p,
                crate::ui::kit::Row {
                    depth: 0,
                    caret: None,
                    icon: Some(code),
                    label: &shown,
                    selected,
                },
            );
            // El ojo (ocultar) y el candado (bloquear) contra el borde derecho de la fila.
            let lock_rect = egui::Rect::from_center_size(
                egui::pos2(row.rect.right() - tokens::SP_L - 2.0, row.rect.center().y),
                vec2(22.0, 22.0),
            );
            let eye_rect = lock_rect.translate(vec2(-24.0, 0.0));
            let eye = ui.interact(
                eye_rect,
                ui.make_persistent_id(("plant_eye", item.id)),
                egui::Sense::click(),
            );
            let lock = ui.interact(
                lock_rect,
                ui.make_persistent_id(("plant_lock", item.id)),
                egui::Sense::click(),
            );
            let painter = ui.painter();
            painter.text(
                eye_rect.center(),
                Align2::CENTER_CENTER,
                if item.hidden {
                    icon::EYE_SLASH
                } else {
                    icon::EYE
                },
                FontId::proportional(14.0),
                if item.hidden { p.accent } else { p.dim },
            );
            painter.text(
                lock_rect.center(),
                Align2::CENTER_CENTER,
                if item.locked {
                    icon::LOCK_SIMPLE
                } else {
                    icon::LOCK_SIMPLE_OPEN
                },
                FontId::proportional(14.0),
                if item.locked { p.accent } else { p.dim },
            );
            if item.hidden {
                // Lo oculto figura tachado.
                let left = row.rect.left() + 12.0 + 20.0 + 26.0;
                let width = ui
                    .painter()
                    .layout_no_wrap(shown.clone(), FontId::proportional(14.0), p.text)
                    .size()
                    .x
                    .min(eye_rect.left() - left - 4.0);
                ui.painter().line_segment(
                    [
                        egui::pos2(left, row.rect.center().y),
                        egui::pos2(left + width, row.rect.center().y),
                    ],
                    Stroke::new(1.0, p.dim),
                );
            }
            eye.clone().on_hover_text(if item.hidden {
                "Mostrar"
            } else {
                "Ocultar (H)"
            });
            lock.clone().on_hover_text(if item.locked {
                "Desbloquear"
            } else {
                "Bloquear (Ctrl+L)"
            });
            if eye.clicked() {
                toggles.push((item.id, true));
            } else if lock.clicked() {
                toggles.push((item.id, false));
            } else if row.clicked() {
                state.selected = Some(item.id);
                state.multi = vec![item.id];
                state.layer = layer_of(item.kind);
                state.tab_request = Some(0);
                // Elegir desde la lista centra la vista en la cosa.
                state.center_on = Some(item.id);
            }
        }
    }
    let mut changed = false;
    if let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) {
        for (id, hide) in toggles {
            if let Some(i) = b.items.iter_mut().find(|i| i.id == id) {
                if hide {
                    i.hidden = !i.hidden;
                    if i.hidden {
                        state.multi.retain(|m| *m != id);
                        if state.selected == Some(id) {
                            state.selected = state.multi.last().copied();
                        }
                    }
                } else {
                    i.locked = !i.locked;
                }
                changed = true;
            }
        }
    }
    changed
}

/// Pestaña PLANTA: nombre y variantes, escala, plano de fondo, grilla, duración del movimiento y el resumen.
fn plant_tab(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) -> bool {
    use crate::ui::kit;
    let mut changed = false;
    let Some(blocking) = state
        .blocking
        .filter(|id| project.blockings.iter().any(|b| b.id == *id))
    else {
        kit::empty_state(
            ui,
            p,
            icon::VIDEO_CAMERA,
            "Todavía no hay planta en esta escena.",
            None,
        );
        return false;
    };
    let scene = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .map_or(0, |b| b.scene);
    kit::section(ui, p, "Planta", None);
    if let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) {
        let width = kit::fill_width(ui, 80.0, 400.0);
        changed |= ui
            .add(
                TextEdit::singleline(&mut b.name)
                    .desired_width(width)
                    .hint_text("Nombre"),
            )
            .changed();
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(format!("{}  Nueva variante", icon::PLUS))
            .on_hover_text("Otra planta para esta escena")
            .clicked()
        {
            state.blocking = Some(project.add_blocking(scene));
            state.selected = None;
            changed = true;
        }
        if ui
            .button(RichText::new(format!("{}  Eliminar", icon::TRASH)).color(p.danger))
            .clicked()
        {
            state.pending_delete = Some(blocking);
        }
    });
    kit::section(ui, p, "Escala", None);
    let ctx = ui.ctx().clone();
    kit::prop_row(ui, p, "Zoom", |ui| {
        if kit::icon_button(ui, p, icon::MINUS, "Alejar", true).clicked() {
            state.zoom_by(&ctx, 0.8);
        }
        ui.label(format!("{} %", state.percent()));
        if kit::icon_button(ui, p, icon::PLUS, "Acercar", true).clicked() {
            state.zoom_by(&ctx, 1.25);
        }
    });
    ui.add(egui::Label::new(
        RichText::new(format!("1 m = {:.0} px", state.scale))
            .size(crate::ui::tokens::FONT_SMALL)
            .color(p.dim),
    ));
    kit::section(ui, p, "Plano de fondo", None);
    let place = project
        .scene_shoot_location(scene)
        .and_then(|l| project.shoot_locations.iter().find(|s| s.id == l));
    match place.map(|l| (l.name.clone(), l.floor_plan.is_some(), l.plan_width_m)) {
        Some((name, true, Some(width))) => {
            ui.checkbox(&mut state.show_plan, "Mostrar el plano de la locación");
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{name} · {} m de ancho",
                        format!("{width:.1}").replace('.', ",")
                    ))
                    .size(crate::ui::tokens::FONT_SMALL)
                    .color(p.dim),
                )
                .truncate(),
            );
        }
        Some((name, ..)) => {
            ui.add(egui::Label::new(RichText::new(format!("«{name}» todavía no tiene plano con su ancho en metros. Cargalo en Locaciones de rodaje.")).size(crate::ui::tokens::FONT_SMALL).color(p.dim)).wrap());
        }
        None => {
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Asigná un lugar de rodaje a la escena para usar su plano de fondo.",
                    )
                    .size(crate::ui::tokens::FONT_SMALL)
                    .color(p.dim),
                )
                .wrap(),
            );
        }
    }
    kit::section(ui, p, "Grilla", None);
    ui.checkbox(&mut state.snap, "Ajustar a la grilla");
    kit::section(ui, p, "Movimiento", None);
    if let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) {
        let mut seconds = b.duration;
        kit::prop_row(ui, p, "Duración", |ui| {
            kit::number_field(ui, &mut seconds, 1.0..=600.0, " s");
        });
        if seconds != b.duration {
            b.duration = seconds;
            changed = true;
        }
    }
    if let Some(b) = project.blockings.iter().find(|b| b.id == blocking) {
        summary(ui, state, project, p, b);
    }
    // Todo lo que puede verse en cuadro de alguna cámara (plantas.md §6.2).
    kit::section(ui, p, "Avisos en cuadro", None);
    if state.warnings.is_empty() {
        ui.label(RichText::new("Sin avisos").color(p.dim));
    }
    let kinds: std::collections::HashMap<u64, BlockKind> = project
        .blockings
        .iter()
        .flat_map(|b| b.items.iter().map(|i| (i.id, i.kind)))
        .collect();
    for w in state.warnings.clone() {
        let text = w.text();
        let resp = ui
            .add(
                egui::Label::new(
                    RichText::new(format!("{}  {text}", icon::WARNING))
                        .size(12.0)
                        .color(p.warning),
                )
                .truncate()
                .sense(egui::Sense::click()),
            )
            .on_hover_text(format!(
                "{text}\nEn el instante {}",
                motion::seconds_text(w.at)
            ));
        if resp.clicked() {
            state.selected = Some(w.item);
            state.multi = vec![w.item];
            if let Some(kind) = kinds.get(&w.item) {
                state.layer = layer_of(*kind);
            }
            state.tab_request = Some(0);
        }
    }
    changed
}

/// Pie fijo del inspector: la acción principal de la planta y exportarla a PDF. Devuelve (cambió, mensaje).
fn footer_ui(
    ui: &mut Ui,
    p: &Palette,
    state: &mut BlockState,
    project: &mut Project,
    paper: crate::storage::Paper,
) -> (bool, Option<String>) {
    let (spec, label) = match state.layer {
        Layer::Camera => (Spec::Camera, "Nueva cámara"),
        Layer::Lights => (Spec::Light(state.light_fixture), "Nueva fuente"),
        Layer::Sound => (Spec::Mic(state.mic_kind), "Nuevo micrófono"),
        Layer::Set => (Spec::Furniture(crate::model::ObjectShape::Table), "Nuevo mueble"),
    };
    let width = ui.available_width();
    let mut changed = false;
    let mut status = None;
    let ready = state.blocking.is_some();
    if ui
        .add_enabled(
            ready,
            egui::Button::new(format!("{}  {label}", icon::PLUS)).min_size(vec2(width, 0.0)),
        )
        .clicked()
    {
        // Elige la herramienta: la figura aparece donde hagas clic, no en el centro de la vista.
        state.set_tool(Tool::Place(spec));
    }
    // Lo que actúa sobre lo elegido vive en el pie del inspector (reglas §6.8).
    if let (Some(blocking), false) = (state.blocking, state.multi.is_empty())
        && ui
            .add(
                egui::Button::new(
                    RichText::new(format!("{}  Eliminar", icon::TRASH)).color(p.danger),
                )
                .min_size(vec2(width, 0.0)),
            )
            .clicked()
    {
        changed |= apply_act(state, project, blocking, EditAct::Delete);
    }
    if ui
        .add_enabled(
            ready,
            egui::Button::new(format!("{}  Exportar plantas…", icon::EXPORT))
                .min_size(vec2(width, 0.0)),
        )
        .clicked()
    {
        state.export.blocking = state.blocking;
        // Parte de la planta que se está trabajando.
        state.export.layers = match state.layer {
            Layer::Camera => [true, false, false],
            Layer::Lights => [false, true, false],
            Layer::Sound => [false, false, true],
            Layer::Set => state.export.layers,
        };
        state.export_open = true;
    }
    if let Some(done) = export_dialog(ui.ctx(), state, project, p, paper) {
        status = Some(done);
    }
    (changed, status)
}

/// Exporta las plantas pedidas a un solo PDF: paredes, muebles, personas, cámaras, luces o micrófonos, los recorridos de los
/// movimientos, una escala de 1 m y, si se pide, la leyenda con los planos de cada cámara y las posiciones.
fn export_pdf(
    project: &Project,
    opts: &plan_pdf::PlanExport,
    paper: crate::storage::Paper,
) -> Option<String> {
    let bytes = plan_pdf::plan_pdf(project, opts, paper);
    let name = if opts.blocking.is_some() { "planta" } else { "plantas" };
    let path = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name(format!("{} - {name}.pdf", project.title))
        .save_file()?;
    Some(
        match bytes.and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string())) {
            Ok(()) => "Plantas exportadas".to_string(),
            Err(e) => format!("Error al exportar: {e}"),
        },
    )
}

/// La ventana de exportar plantas: de uno o de todos los bloqueos, qué plantas, cada una en su hoja o todas juntas, con o sin
/// conos y leyenda. La escenografía va siempre. Devuelve el aviso si se exportó.
fn export_dialog(
    ctx: &egui::Context,
    state: &mut BlockState,
    project: &Project,
    p: &Palette,
    paper: crate::storage::Paper,
) -> Option<String> {
    if !state.export_open {
        return None;
    }
    let (mut go, mut cancel) = (false, false);
    let mut opts = state.export.clone();
    let current = state.blocking;
    let closed = crate::ui::kit::dialog(
        ctx,
        p,
        "export_plants",
        "Exportar plantas",
        crate::ui::kit::DialogWidth::Normal,
        |ui| {
            crate::ui::kit::section(ui, p, "Bloqueos", None);
            ui.radio_value(&mut opts.blocking, current, "Solo este bloqueo");
            ui.radio_value(&mut opts.blocking, None, "Todos los bloqueos del proyecto");
            crate::ui::kit::section(ui, p, "Plantas", None);
            for (n, name) in ["Cámara", "Luces", "Sonido"].iter().enumerate() {
                ui.checkbox(&mut opts.layers[n], *name);
            }
            ui.add_space(crate::ui::tokens::SP_XS);
            ui.radio_value(&mut opts.combine, false, "Cada planta en su hoja");
            ui.radio_value(&mut opts.combine, true, "Todas en una sola hoja");
            crate::ui::kit::section(ui, p, "Incluir", None);
            ui.checkbox(&mut opts.cones, "El cono de las cámaras");
            ui.checkbox(&mut opts.legend, "Leyenda y tablas (planos por cámara y posiciones)");
        },
        |ui| {
            if crate::ui::kit::primary_button(ui, p, &format!("{}  Exportar", icon::EXPORT), None).clicked() {
                go = true;
            }
            if ui.button("Cancelar").clicked() {
                cancel = true;
            }
        },
    );
    state.export = opts.clone();
    if go {
        state.export_open = false;
        return export_pdf(project, &opts, paper);
    }
    if cancel || closed {
        state.export_open = false;
    }
    None
}

/// El PDF de una planta de un bloqueo (sin pedir dónde guardarlo); la de escenografía lleva las tres plantas en una hoja.
#[cfg(test)]
fn plan_pdf(
    project: &Project,
    blocking: u64,
    layer: Layer,
    paper: crate::storage::Paper,
    cones: bool,
) -> Result<Vec<u8>, String> {
    let (layers, combine) = match layer {
        Layer::Camera => ([true, false, false], false),
        Layer::Lights => ([false, true, false], false),
        Layer::Sound => ([false, false, true], false),
        Layer::Set => ([true; 3], true),
    };
    plan_pdf::plan_pdf(
        project,
        &plan_pdf::PlanExport {
            blocking: Some(blocking),
            layers,
            combine,
            cones,
            legend: true,
        },
        paper,
    )
}


/// El resumen de la planta de luces (consumo contra lo que da el lugar) o de la de sonido (pistas y micrófonos).
fn summary(
    ui: &mut Ui,
    state: &BlockState,
    project: &Project,
    p: &Palette,
    b: &crate::model::Blocking,
) {
    match state.layer {
        Layer::Lights => {
            let lights: Vec<&BlockItem> = b
                .items
                .iter()
                .filter(|i| i.kind == BlockKind::Light)
                .collect();
            let used: u32 = lights.iter().map(|l| l.watts).sum();
            let limit = project
                .scene_shoot_location(b.scene)
                .and_then(|l| project.shoot_locations.iter().find(|s| s.id == l))
                .and_then(|l| l.power_watts);
            kit::section(ui, p, "Resumen de luces", None);
            if lights.is_empty() {
                ui.label(RichText::new("Todavía no hay luces en esta planta.").color(p.dim));
                return;
            }
            for l in &lights {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} · {} · {} W · {} K",
                            l.label,
                            l.light.as_ref().map_or("", |spec| spec.role.label()),
                            l.watts,
                            l.kelvin
                        ))
                        .size(12.0)
                        .color(p.text),
                    )
                    .truncate(),
                );
            }
            ui.add_space(tokens::SP_S);
            let over = limit.is_some_and(|l| used > l);
            let text = match limit {
                Some(l) => format!("Consumo: {used} W de {l} W disponibles en el lugar"),
                None => format!("Consumo: {used} W (el lugar no tiene la potencia cargada)"),
            };
            ui.add(
                egui::Label::new(RichText::new(text).color(if over { p.warning } else { p.text }))
                    .wrap(),
            );
            if let Some(l) = limit {
                ui.add(
                    egui::ProgressBar::new((used as f32 / l.max(1) as f32).min(1.0))
                        .fill(if over { p.warning } else { p.accent }),
                );
            }
        }
        Layer::Sound => {
            let mics: Vec<&BlockItem> = b
                .items
                .iter()
                .filter(|i| i.kind == BlockKind::Mic)
                .collect();
            kit::section(ui, p, "Pistas del grabador", None);
            if mics.is_empty() {
                ui.label(RichText::new("Todavía no hay micrófonos en esta planta.").color(p.dim));
                return;
            }
            let mut sorted = mics.clone();
            sorted.sort_by_key(|m| m.track);
            for m in sorted {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "Pista {} · {} · {}",
                            m.track,
                            m.label,
                            m.mic.as_ref().map_or("", |spec| spec.kind.label())
                        ))
                        .size(12.0)
                        .color(p.text),
                    )
                    .truncate(),
                );
            }
            let wireless = mics
                .iter()
                .filter(|m| {
                    m.mic
                        .as_ref()
                        .is_some_and(|spec| spec.wireless || spec.kind == MicKind::Lavalier)
                })
                .count();
            ui.add_space(tokens::SP_S);
            ui.label(RichText::new(format!("Inalámbricos hacen falta: {wireless}")).color(p.dim));
        }
        Layer::Camera | Layer::Set => {}
    }
}

// ------------------------------------------------------------------------------------------------
// La pantalla dentro del marco (reglas §1.2)
// ------------------------------------------------------------------------------------------------

/// Plantas de cámara, luces y sonido: pantalla de tipo D (lienzo + inspector).
pub struct BlockScreen<'a> {
    pub state: &'a mut BlockState,
}

impl Screen for BlockScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Canvas
    }

    fn title(&self, _cx: &ScreenCx) -> String {
        layer_title(self.state.layer).into()
    }

    fn enter(&mut self, cx: &mut ScreenCx) {
        self.state.record(cx.project);
        // La escena actual es compartida: si otra pantalla la cambió, las plantas la siguen.
        if let Some(scene) = *cx.current_scene
            && self.state.scene != Some(scene)
            && cx
                .project
                .script_scenes()
                .iter()
                .any(|(id, _)| *id == scene)
        {
            self.state.show_scene(scene);
        }
        if let Some(tab) = self.state.tab_request.take() {
            cx.inspector_tab = Some(tab);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= toolbar_ui(ui, self.state, cx.project, cx.palette);
    }

    fn can_undo(&self, _cx: &ScreenCx) -> bool {
        self.state.can_undo()
    }

    fn can_redo(&self, _cx: &ScreenCx) -> bool {
        self.state.can_redo()
    }

    fn undo(&mut self, cx: &mut ScreenCx) {
        cx.dirty |= self.state.undo(cx.project);
    }

    fn redo(&mut self, cx: &mut ScreenCx) {
        cx.dirty |= self.state.redo(cx.project);
    }

    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        true
    }

    fn context_bar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= context_ui(ui, self.state, cx.project, cx.palette);
        *cx.current_scene = self.state.scene;
    }

    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = show(ui, self.state, cx.project, cx.palette);
        cx.dirty |= out.changed;
        // Lo que hay en las plantas se refleja en el Desglose.
        cx.dirty |= cx.project.sync_equipment();
        if let Some(id) = out.delete_blocking {
            match cx.project.blockings.iter().find(|b| b.id == id) {
                Some(b) if !b.items.is_empty() => {
                    let linked = cx
                        .project
                        .shots
                        .iter()
                        .filter(|s| s.blocking == Some(id))
                        .count();
                    cx.confirm = Some(Confirm::DeleteBlocking(
                        id,
                        b.name.clone(),
                        b.items.len(),
                        linked,
                    ));
                }
                _ => {
                    cx.project.remove_blocking(id);
                    cx.dirty = true;
                }
            }
        }
    }

    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Selección", "Elementos", "Planta"]
    }

    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        match tab {
            0 => cx.dirty |= properties(ui, self.state, cx.project, cx.palette),
            1 => cx.dirty |= items_tab(ui, self.state, cx.project, cx.palette),
            _ => cx.dirty |= plant_tab(ui, self.state, cx.project, cx.palette),
        }
    }

    fn has_inspector_footer(&self, _tab: usize, _cx: &ScreenCx) -> bool {
        true
    }

    fn inspector_footer(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        let (changed, status) =
            footer_ui(ui, cx.palette, self.state, cx.project, cx.settings.paper);
        cx.dirty |= changed;
        if status.is_some() {
            cx.status = status;
        }
    }

    fn status(&self, _cx: &ScreenCx) -> Vec<String> {
        self.state.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toolbar(ui: &mut Ui, state: &mut BlockState, project: &mut Project, p: &Palette) -> bool {
        toolbar_ui(ui, state, project, p)
    }

    fn context_bar(
        ui: &mut Ui,
        state: &mut BlockState,
        project: &mut Project,
        p: &Palette,
    ) -> bool {
        context_ui(ui, state, project, p)
    }

    /// Dibuja la pestaña 0 (Selección) o 1 (Elementos) del inspector.
    fn right_panel(
        ui: &mut Ui,
        state: &mut BlockState,
        project: &mut Project,
        p: &Palette,
        tab: usize,
    ) -> bool {
        match tab {
            0 => properties(ui, state, project, p),
            1 => items_tab(ui, state, project, p),
            _ => plant_tab(ui, state, project, p),
        }
    }

    fn item(kind: BlockKind, x: f32, y: f32, w: f32, h: f32, rot: f32) -> BlockItem {
        BlockItem {
            id: 1,
            kind,
            x,
            y,
            w,
            h,
            rot,
            label: String::new(),
            character: None,
            fov: 40.0,
            lens_mm: 0.0,
            body: None,
            keys: Vec::new(),
            watts: 0,
            kelvin: 0,
            ..BlockItem::default()
        }
    }

    #[test]
    fn things_are_picked_by_their_turned_shape() {
        // Una mesa de 2 × 0,5 m girada 90° ocupa 0,5 de ancho y 2 de alto.
        let table = item(BlockKind::Object, 5.0, 5.0, 2.0, 0.5, 90.0);
        assert!(hit(&table, (5.0, 5.9), 0.0));
        assert!(!hit(&table, (5.9, 5.0), 0.0));
        // Una persona se agarra por su círculo.
        let actor = item(BlockKind::Actor, 1.0, 1.0, 0.6, 0.6, 0.0);
        assert!(hit(&actor, (1.2, 1.2), 0.0) && !hit(&actor, (1.5, 1.5), 0.0));
        // El tirador de giro está delante, y el cono se abre a ambos lados de la dirección.
        let h = handle(&actor, 0.3);
        assert!((h.0 - 1.6).abs() < 1e-4 && (h.1 - 1.0).abs() < 1e-4);
        let camera = item(BlockKind::Camera, 0.0, 0.0, 0.5, 0.35, 0.0);
        let c = cone(&camera);
        assert_eq!(c[0], (0.0, 0.0));
        assert!(
            c[1].1 < 0.0 && c.last().unwrap().1 > 0.0,
            "el cono se abre a los dos lados"
        );
        assert!(
            c.iter()
                .skip(1)
                .all(|(x, y)| (x.hypot(*y) - camera.cone_len_m).abs() < 1e-3)
        );
        // Ajustar a la cuadrícula.
        assert_eq!(snapped(0.37, 0.25, true), 0.25);
        assert_eq!(snapped(0.37, 0.25, false), 0.37);
    }

    #[test]
    fn blockings_hold_cameras_that_shots_link_to_and_survive_removals() {
        use crate::model::ParagraphKind::SceneHeading;
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let blocking = project.add_blocking(scene);
        assert_eq!(project.blockings[0].name, "Bloqueo 1");
        let a = project
            .add_block_item(blocking, BlockKind::Camera, 1.0, 1.0)
            .unwrap();
        let b = project
            .add_block_item(blocking, BlockKind::Camera, 4.0, 1.0)
            .unwrap();
        let labels: Vec<_> = project.blockings[0]
            .items
            .iter()
            .map(|i| i.label.clone())
            .collect();
        assert_eq!(labels, ["A", "B"]);
        assert_eq!(
            project
                .scene_cameras(scene)
                .iter()
                .map(|(_, _, n)| n.clone())
                .collect::<Vec<_>>(),
            ["A · Bloqueo 1", "B · Bloqueo 1"]
        );

        let shot = project.add_shot(scene, None);
        project.shots[0].blocking = Some(blocking);
        project.shots[0].camera = Some(a);
        // Quitar la cámara suelta el vínculo del plano; quitar el bloqueo, también.
        project.remove_block_item(blocking, a);
        assert_eq!(project.shots[0].camera, None);
        project.shots[0].camera = Some(b);
        project.remove_blocking(blocking);
        assert_eq!(
            (project.shots[0].blocking, project.shots[0].camera),
            (None, None)
        );
        let _ = shot;

        // La letra libre más chica se reutiliza.
        let again = project.add_blocking(scene);
        let first = project
            .add_block_item(again, BlockKind::Camera, 0.0, 0.0)
            .unwrap();
        project.add_block_item(again, BlockKind::Camera, 1.0, 0.0);
        project.remove_block_item(again, first);
        let third = project
            .add_block_item(again, BlockKind::Camera, 2.0, 0.0)
            .unwrap();
        assert_eq!(
            project
                .blockings
                .last()
                .unwrap()
                .items
                .iter()
                .find(|i| i.id == third)
                .unwrap()
                .label,
            "A"
        );

        // Un proyecto viejo, sin columna de cámara, la gana antes del storyboard.
        project.shot_columns.clear();
        project.ensure_shot_columns();
        project
            .shot_columns
            .retain(|c| c.kind != crate::model::ColumnKind::Camera);
        project.ensure_shot_columns();
        let kinds: Vec<_> = project.shot_columns.iter().map(|c| c.kind).collect();
        let at = kinds
            .iter()
            .position(|k| *k == crate::model::ColumnKind::Camera)
            .unwrap();
        assert_eq!(kinds[at + 1], crate::model::ColumnKind::Storyboard);
    }

    fn setup() -> (egui::Context, BlockState, Project, u64, u64, u64) {
        use crate::model::ParagraphKind::SceneHeading;
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        // La pregunta por el tipo de cámara es modal: las pruebas de la planta no la necesitan.
        project.camera_body_asked = true;
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - DÍA")];
        let scene = project.paragraphs[0].id;
        let blocking = project.add_blocking(scene);
        let camera = project
            .add_block_item(blocking, BlockKind::Camera, 3.0, 2.0)
            .unwrap();
        (ctx, BlockState::default(), project, blocking, camera, scene)
    }

    fn frame(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
    ) -> egui::Rect {
        frame_with(ctx, state, project, events, t, egui::Modifiers::NONE)
    }

    fn frame_with(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
        modifiers: egui::Modifiers,
    ) -> egui::Rect {
        let p = crate::storage::Settings::default().scheme.palette();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1200.0, 800.0),
            )),
            time: Some(t),
            focused: true,
            events: std::iter::once(egui::Event::ModifiersChanged(modifiers))
                .chain(events)
                .collect(),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            show(ui, state, project, &p);
        });
        out.textures_delta.clear();
        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("blocking_canvas")))
            .expect("lienzo dibujado")
    }

    fn drag(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        from: egui::Pos2,
        to: egui::Pos2,
        t: &mut f64,
    ) {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut step = |events: Vec<egui::Event>| {
            *t += 0.1;
            frame(ctx, state, project, events, *t);
        };
        step(vec![egui::Event::PointerMoved(from)]);
        step(vec![button(from, true)]);
        step(vec![egui::Event::PointerMoved(from + (to - from) * 0.5)]);
        step(vec![egui::Event::PointerMoved(to)]);
        step(vec![button(to, false)]);
        step(vec![]);
    }

    #[test]
    fn dragging_moves_a_camera_and_its_handle_turns_it() {
        let (ctx, mut state, mut project, blocking, camera, _) = setup();
        let mut t = 0.0;
        frame(&ctx, &mut state, &mut project, vec![], 0.01);
        frame(&ctx, &mut state, &mut project, vec![], 0.02);
        let canvas = frame(&ctx, &mut state, &mut project, vec![], 0.03);
        let (offset, scale) = (state.offset, state.scale);
        let at = move |x: f32, y: f32| canvas.min + offset + vec2(x, y) * scale;

        // Arrastrar la cámara 2 m a la derecha y 1 m hacia abajo.
        drag(
            &ctx,
            &mut state,
            &mut project,
            at(3.0, 2.0),
            at(5.0, 3.0),
            &mut t,
        );
        let item = |project: &Project| {
            project
                .blockings
                .iter()
                .find(|b| b.id == blocking)
                .unwrap()
                .items
                .iter()
                .find(|i| i.id == camera)
                .unwrap()
                .clone()
        };
        let moved = item(&project);
        assert!(
            (moved.x - 5.0).abs() < 0.3 && (moved.y - 3.0).abs() < 0.3,
            "{moved:?}"
        );
        assert_eq!(state.selected, Some(camera));

        // Con la cámara elegida, el círculo de adelante la hace girar: hacia abajo son 90°.
        let handle_at = at(handle(&moved, 14.0 / scale).0, moved.y);
        drag(
            &ctx,
            &mut state,
            &mut project,
            handle_at,
            at(moved.x, moved.y + 2.0),
            &mut t,
        );
        assert!(
            (item(&project).rot - 90.0).abs() < 1.0,
            "{:?}",
            item(&project)
        );
        assert!(
            (item(&project).x - moved.x).abs() < 1e-3,
            "girar no la mueve"
        );

        // Arrastrar en un lugar vacío dibuja un rectángulo de selección: no mueve la vista ni las cosas (la vista se mueve con
        // Espacio o con el botón del medio).
        let before = (state.offset, item(&project).x);
        drag(
            &ctx,
            &mut state,
            &mut project,
            at(10.0, 9.0),
            at(10.0, 9.0) + vec2(50.0, 20.0),
            &mut t,
        );
        assert!(state.offset == before.0 && item(&project).x == before.1);
        assert!(
            state.marquee.is_none(),
            "el rectángulo desaparece al soltar"
        );
    }

    #[test]
    fn the_blocking_page_panel_and_toolbar_draw_with_everything_in_the_plan() {
        let (ctx, mut state, mut project, blocking, _, scene) = setup();
        let actor = project
            .add_block_item(blocking, BlockKind::Actor, 1.0, 1.0)
            .unwrap();
        project.add_block_item(blocking, BlockKind::Object, 2.0, 4.0);
        project.add_block_item(blocking, BlockKind::Wall, 0.0, 6.0);
        let lucas = project.new_character("Lucas");
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == actor)
            .unwrap()
            .character = Some(lucas);
        let shot = project.add_shot(scene, None);
        project.shots[0].blocking = Some(blocking);
        project.shots[0].camera = Some(project.scene_cameras(scene)[0].1);
        let _ = shot;
        let p = crate::storage::Settings::default().scheme.palette();
        for tab in 0..3 {
            for selected in [None, Some(actor), Some(project.scene_cameras(scene)[0].1)] {
                state.selected = selected;
                for n in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(1200.0, 800.0),
                        )),
                        time: Some(n as f64 * 0.1),
                        focused: true,
                        ..Default::default()
                    };
                    let mut out = ctx.run_ui(input, |ui| {
                        egui::Panel::right("p").show(ui, |ui| {
                            right_panel(ui, &mut state, &mut project, &p, tab);
                        });
                        show(ui, &mut state, &mut project, &p);
                        toolbar(ui, &mut state, &mut project, &p);
                        context_bar(ui, &mut state, &mut project, &p);
                    });
                    out.textures_delta.clear();
                }
            }
        }
        assert_eq!(project.blockings[0].items.len(), 4);
    }

    fn find(project: &Project, blocking: u64, id: u64) -> BlockItem {
        project
            .blockings
            .iter()
            .find(|b| b.id == blocking)
            .unwrap()
            .items
            .iter()
            .find(|i| i.id == id)
            .unwrap()
            .clone()
    }

    #[test]
    fn stretching_from_a_corner_or_an_end_keeps_the_opposite_side_in_place() {
        // Mesa de 2 × 1 m en (5, 5): arrastrar la esquina de abajo a la derecha 1 m hacia cada lado.
        let table = item(BlockKind::Object, 5.0, 5.0, 2.0, 1.0, 0.0);
        let (x, y, w, h) = resized(&table, (1, 1), (7.0, 6.5), false);
        assert_eq!((w, h), (3.0, 2.0));
        assert_eq!(
            (x - w / 2.0, y - h / 2.0),
            (4.0, 4.5),
            "la esquina de arriba a la izquierda no se movió"
        );
        // Un lado solo cambia un eje.
        let (_, _, w, h) = resized(&table, (0, -1), (5.0, 3.5), false);
        assert_eq!((w, h), (2.0, 2.0));
        // Girada 90°, el «ancho» de la mesa va hacia abajo en pantalla.
        let turned = item(BlockKind::Object, 5.0, 5.0, 2.0, 1.0, 90.0);
        assert!((size_handle(&turned, (1, 0)).1 - 6.0).abs() < 1e-4);
        let (x, y, w, _) = resized(&turned, (1, 0), (5.0, 7.0), false);
        assert!(
            (w - 3.0).abs() < 1e-4 && (x - 5.0).abs() < 1e-4 && (y - 5.5).abs() < 1e-4,
            "{x} {y} {w}"
        );
        // No se puede achicar hasta nada.
        assert_eq!(resized(&table, (1, 0), (-9.0, 5.0), false).2, MIN_SIZE);
        // Una pared se estira por sus puntas.
        let wall = item(BlockKind::Wall, 0.0, 0.0, 3.0, 0.15, 0.0);
        assert_eq!(size_sides(BlockKind::Wall), vec![(-1, 0), (1, 0)]);
        assert!(
            size_sides(BlockKind::Actor).is_empty() && size_sides(BlockKind::Camera).is_empty()
        );
        let (x, _, w, _) = resized(&wall, (-1, 0), (-3.5, 0.0), true);
        assert_eq!((x, w), (-1.0, 5.0));
    }

    #[test]
    fn the_turn_handle_of_furniture_sits_above_it_clear_of_the_size_handles() {
        let table = item(BlockKind::Object, 5.0, 5.0, 2.0, 1.0, 0.0);
        let h = handle(&table, 0.5);
        assert!(
            (h.0 - 5.0).abs() < 1e-4 && (h.1 - 4.0).abs() < 1e-4,
            "{h:?}"
        );
        assert!(size_sides(table.kind).iter().all(|s| {
            let at = size_handle(&table, *s);
            (at.0 - h.0).hypot(at.1 - h.1) > 0.4
        }));
    }

    #[test]
    fn positions_in_time_are_walked_in_a_straight_line_and_turn_the_short_way() {
        let mut camera = item(BlockKind::Camera, 0.0, 0.0, 0.5, 0.35, 350.0);
        // En el segundo 0 se cambia el comienzo; en otro instante, se crea una posición.
        camera.set_pose_at(0.0, 0.0, 0.0, 350.0);
        assert!(camera.keys.is_empty());
        camera.set_pose_at(4.0, 8.0, 4.0, 10.0);
        assert_eq!(camera.keys.len(), 1);
        assert_eq!(camera.pose_at(0.0), (0.0, 0.0, 350.0));
        let (x, y, rot) = camera.pose_at(2.0);
        assert!((x - 4.0).abs() < 1e-4 && (y - 2.0).abs() < 1e-4, "{x} {y}");
        assert!(
            rot.min(360.0 - rot) < 1e-3,
            "de 350° a 10° pasa por 0°, no por 180°: {rot}"
        );
        assert_eq!(
            camera.pose_at(9.0),
            (8.0, 4.0, 10.0),
            "después de la última se queda quieta"
        );
        // Corregir una posición ya existente no crea otra; las nuevas quedan en orden.
        camera.set_pose_at(4.02, 9.0, 4.0, 10.0);
        camera.set_pose_at(1.0, 1.0, 1.0, 0.0);
        assert_eq!(
            camera.keys.iter().map(|k| k.t).collect::<Vec<_>>(),
            vec![1.0, 4.0]
        );
        assert_eq!(camera.keys[1].x, 9.0);
        // Las paredes no se animan.
        let mut wall = item(BlockKind::Wall, 0.0, 0.0, 3.0, 0.15, 0.0);
        wall.set_pose_at(5.0, 2.0, 2.0, 0.0);
        assert!(wall.keys.is_empty() && wall.x == 2.0);
    }

    #[test]
    fn furniture_and_walls_are_resized_by_dragging_their_handles() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        let table = project
            .add_block_item(blocking, BlockKind::Object, 5.0, 5.0)
            .unwrap();
        let wall = project
            .add_block_item(blocking, BlockKind::Wall, 0.0, 8.0)
            .unwrap();
        let mut t = 0.0;
        frame(&ctx, &mut state, &mut project, vec![], 0.01);
        frame(&ctx, &mut state, &mut project, vec![], 0.02);
        let canvas = frame(&ctx, &mut state, &mut project, vec![], 0.03);
        let (offset, scale) = (state.offset, state.scale);
        let at = move |(x, y): (f32, f32)| canvas.min + offset + vec2(x, y) * scale;

        state.selected = Some(table);
        let before = find(&project, blocking, table);
        let corner = size_handle(&before, (1, 1));
        drag(
            &ctx,
            &mut state,
            &mut project,
            at(corner),
            at((corner.0 + 1.0, corner.1 + 1.0)),
            &mut t,
        );
        let after = find(&project, blocking, table);
        assert!(
            (after.w - (before.w + 1.0)).abs() < 0.3 && (after.h - (before.h + 1.0)).abs() < 0.3,
            "{after:?}"
        );
        assert!(
            (after.x - after.w / 2.0 - (before.x - before.w / 2.0)).abs() < 0.2,
            "la esquina opuesta quedó donde estaba"
        );
        assert!(state.drag.is_none());

        state.selected = Some(wall);
        let before = find(&project, blocking, wall);
        let end = size_handle(&before, (1, 0));
        drag(
            &ctx,
            &mut state,
            &mut project,
            at(end),
            at((end.0 + 2.0, end.1)),
            &mut t,
        );
        let after = find(&project, blocking, wall);
        assert!(
            (after.w - (before.w + 2.0)).abs() < 0.3 && (after.y - before.y).abs() < 1e-3,
            "{after:?}"
        );
    }

    #[test]
    fn moving_at_a_later_time_adds_a_position_and_playing_runs_the_clock() {
        let (ctx, mut state, mut project, blocking, camera, _) = setup();
        let mut t = 0.0;
        frame(&ctx, &mut state, &mut project, vec![], 0.01);
        frame(&ctx, &mut state, &mut project, vec![], 0.02);
        let canvas = frame(&ctx, &mut state, &mut project, vec![], 0.03);
        let (offset, scale) = (state.offset, state.scale);
        let at = move |(x, y): (f32, f32)| canvas.min + offset + vec2(x, y) * scale;

        state.time = 4.0;
        drag(
            &ctx,
            &mut state,
            &mut project,
            at((3.0, 2.0)),
            at((7.0, 2.0)),
            &mut t,
        );
        let moved = find(&project, blocking, camera);
        assert_eq!((moved.x, moved.y), (3.0, 2.0), "el comienzo no cambia");
        assert_eq!(moved.keys.len(), 1);
        assert!(
            (moved.keys[0].x - 7.0).abs() < 0.3 && (moved.keys[0].t - 4.0).abs() < 1e-4,
            "{moved:?}"
        );
        assert!((moved.pose_at(2.0).0 - 5.0).abs() < 0.2);

        // Las posiciones también se arrastran, y tocar una lleva el tiempo a ella.
        state.time = 0.0;
        let key = (moved.keys[0].x, moved.keys[0].y);
        drag(
            &ctx,
            &mut state,
            &mut project,
            at(key),
            at((key.0, key.1 + 2.0)),
            &mut t,
        );
        let moved = find(&project, blocking, camera);
        assert!(
            (moved.keys[0].y - 4.0).abs() < 0.3 && moved.keys.len() == 1,
            "{moved:?}"
        );
        assert_eq!(state.time, 0.0, "arrastrar una posición no mueve el tiempo");

        // Reproducir: el tiempo avanza y se detiene al llegar al final.
        state.playing = true;
        let start = state.time;
        for n in 0..5 {
            frame(
                &ctx,
                &mut state,
                &mut project,
                vec![],
                10.0 + n as f64 * 0.1,
            );
        }
        assert!(state.time > start && state.playing);
        state.time = project.blockings[0].duration - 0.001;
        for n in 0..3 {
            frame(
                &ctx,
                &mut state,
                &mut project,
                vec![],
                20.0 + n as f64 * 0.1,
            );
        }
        assert!(!state.playing && state.time == project.blockings[0].duration);

        // La duración no baja de la última posición.
        project.normalize();
        assert!(project.blockings[0].duration >= 4.0);
    }

    #[test]
    fn a_camera_cone_follows_its_lens_and_sensor_and_the_panel_draws_the_camera_controls() {
        let (ctx, mut state, mut project, blocking, camera, _) = setup();
        let canon = project.add_camera_body("Canon", 1.6);
        let cone_angle = |project: &Project| {
            let item = project.blockings[0]
                .items
                .iter()
                .find(|i| i.id == camera)
                .unwrap();
            project.camera_fov(item)
        };
        let wide = cone_angle(&project);
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .body = Some(canon);
        assert!(
            cone_angle(&project) < wide,
            "el mismo lente en un sensor chico ve menos"
        );
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .lens_mm = 0.0;
        assert_eq!(
            cone_angle(&project),
            40.0,
            "una cámara sin lente vuelve a su ángulo"
        );
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .lens_mm = 24.0;
        let p = crate::storage::Settings::default().scheme.palette();
        state.selected = Some(camera);
        for lens in [24.0, 0.0] {
            project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == camera)
                .unwrap()
                .lens_mm = lens;
            for n in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1200.0, 900.0),
                    )),
                    time: Some(n as f64 * 0.1),
                    focused: true,
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    egui::Panel::right("p").show(ui, |ui| {
                        right_panel(ui, &mut state, &mut project, &p, 0);
                    });
                    show(ui, &mut state, &mut project, &p);
                });
                out.textures_delta.clear();
            }
        }
        let _ = blocking;
    }

    #[test]
    fn lights_and_microphones_live_in_their_own_plan_and_the_others_show_faded_on_request() {
        let (ctx, mut state, mut project, blocking, _, scene) = setup();
        let light = project
            .add_block_item(blocking, BlockKind::Light, 1.0, 1.0)
            .unwrap();
        let mic = project
            .add_block_item(blocking, BlockKind::Mic, 2.0, 1.0)
            .unwrap();
        project.add_block_item(blocking, BlockKind::Actor, 3.0, 1.0);
        let lamp = find(&project, blocking, light);
        assert_eq!(
            (
                lamp.watts,
                lamp.kelvin,
                lamp.light.as_ref().map(|l| l.role.label()),
                lamp.label.as_str()
            ),
            (650, 5600, Some("Principal"), "L1")
        );
        let boom = find(&project, blocking, mic);
        assert_eq!(
            (boom.track, boom.mic.as_ref().map(|m| m.kind)),
            (1, Some(MicKind::Boom))
        );
        // Cada planta toca solo lo suyo y la base.
        assert!(
            Layer::Lights.owns(BlockKind::Light)
                && !Layer::Lights.owns(BlockKind::Camera)
                && !Layer::Lights.owns(BlockKind::Wall)
        );
        assert!(Layer::Camera.owns(BlockKind::Wall) && Layer::Camera.owns(BlockKind::Camera));
        assert!(Layer::Sound.owns(BlockKind::Mic) && !Layer::Sound.owns(BlockKind::Light));
        // La potencia de las luces se suma contra la del lugar de rodaje.
        let place = project.add_shoot_location("Casa");
        project.scenes.entry(scene).or_default().shoot_location = Some(place);
        project
            .blockings
            .iter_mut()
            .find(|b| b.id == blocking)
            .unwrap()
            .items
            .iter_mut()
            .find(|i| i.id == light)
            .unwrap()
            .watts = 1200;
        project.shoot_locations[0].power_watts = Some(1000);
        let p = crate::storage::Settings::default().scheme.palette();
        for layer in Layer::ALL {
            for others in [false, true] {
                state.layer = layer;
                state.show_others = others;
                state.selected = Some(match layer {
                    Layer::Lights => light,
                    Layer::Sound => mic,
                    Layer::Camera | Layer::Set => project.scene_cameras(scene).first().map_or(light, |c| c.1),
                });
                for n in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(1200.0, 800.0),
                        )),
                        time: Some(n as f64 * 0.1),
                        focused: true,
                        ..Default::default()
                    };
                    let mut out = ctx.run_ui(input, |ui| {
                        toolbar(ui, &mut state, &mut project, &p);
                        egui::Panel::right("p").show(ui, |ui| {
                            right_panel(ui, &mut state, &mut project, &p, 0);
                        });
                        show(ui, &mut state, &mut project, &p);
                    });
                    out.textures_delta.clear();
                }
            }
        }
        assert_eq!(crate::shoot::lights_for_test(&project, place), 1200);
    }

    #[test]
    fn undo_and_redo_walk_the_plant_history_and_a_drag_counts_as_one_change() {
        let (_, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        state.record(&project);
        assert!(!state.can_undo());
        state.selected = project.add_block_item(blocking, BlockKind::Light, 0.0, 0.0);
        state.record(&project);
        assert_eq!(state.undo.len(), 1);
        // Los cambios seguidos (un arrastre) no suman un paso por cada cuadro.
        let id = state.selected.unwrap();
        for n in 0..5 {
            project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .unwrap()
                .x = n as f32;
            state.record(&project);
        }
        assert_eq!(state.undo.len(), 1, "el arrastre es un solo cambio");
        let count = project.blockings[0].items.len();
        assert!(state.undo(&mut project));
        assert!(
            project.blockings[0].items.len() < count
                || project.blockings[0].items.iter().all(|i| i.id != id
                    || i.x == 0.0
                    || i.x
                        == project.blockings[0]
                            .items
                            .iter()
                            .find(|j| j.id == id)
                            .unwrap()
                            .x)
        );
        assert!(state.can_redo());
        assert!(state.redo(&mut project));
        assert_eq!(project.blockings[0].items.len(), count);
    }

    #[test]
    fn the_plant_exports_to_a_pdf_and_an_empty_one_says_so() {
        let (_, mut state, mut project, _, _, scene) = setup();
        let blocking = project.add_blocking(scene);
        state.blocking = Some(blocking);
        assert!(
            plan_pdf(&project, blocking, Layer::Camera, crate::storage::Paper::A4, true)
                .is_err_and(|e| e.contains("vacía"))
        );
        for kind in [
            BlockKind::Wall,
            BlockKind::Object,
            BlockKind::Actor,
            BlockKind::Camera,
            BlockKind::Light,
            BlockKind::Mic,
        ] {
            project.add_block_item(blocking, kind, 1.0, 1.0);
        }
        for layer in Layer::ALL {
            let bytes = plan_pdf(&project, blocking, layer, crate::storage::Paper::A4, true)
                .expect("el PDF se arma");
            assert!(bytes.starts_with(b"%PDF"));
        }
    }

    #[test]
    fn the_zoom_buttons_and_fit_set_a_target_the_view_reaches() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        project.add_block_item(blocking, BlockKind::Wall, 10.0, 10.0);
        let before = state.scale;
        state.zoom_by(&ctx, 1.25);
        assert!(state.view_target.is_some());
        for _ in 0..3 {
            state.step_view_animation(&ctx);
        }
        assert!(
            (state.scale - before * 1.25).abs() < 0.5,
            "el zoom llegó a {}",
            state.scale
        );
        assert_eq!(
            state.percent(),
            (state.scale / BASE_SCALE * 100.0).round() as u32
        );
        state.fit(&ctx, &project);
        for _ in 0..3 {
            state.step_view_animation(&ctx);
        }
        assert!(state.scale >= MIN_SCALE && state.scale <= MAX_SCALE);
    }

    #[test]
    fn every_figure_draws_in_every_theme_zoom_and_state() {
        use crate::model::{LightSpec, MicSpec, ModifierSpec};
        for scheme in crate::theme::Scheme::ALL {
            let (ctx, mut state, mut project, blocking, camera, _) = setup();
            let mut add = |kind: BlockKind, x: f32, edit: &dyn Fn(&mut BlockItem)| {
                let id = project.add_block_item(blocking, kind, x, 2.0).unwrap();
                let item = project.blockings[0]
                    .items
                    .iter_mut()
                    .find(|i| i.id == id)
                    .unwrap();
                edit(item);
                id
            };
            let person = add(BlockKind::Actor, 1.0, &|_| {});
            for fixture in LightFixture::ALL {
                add(BlockKind::Light, 4.0, &|i| {
                    i.light = Some(LightSpec::new(fixture))
                });
            }
            for kind in ModifierKind::ALL {
                add(BlockKind::Modifier, 5.0, &|i| {
                    i.modifier = Some(ModifierSpec {
                        kind,
                        ..ModifierSpec::default()
                    })
                });
            }
            for kind in MicKind::ALL {
                add(BlockKind::Mic, 6.0, &|i| {
                    i.mic = Some(crate::model::MicSpec::new(kind))
                });
            }
            for pattern in MicPattern::ALL {
                add(BlockKind::Mic, 7.0, &|i| {
                    i.mic = Some(MicSpec {
                        pattern,
                        ..MicSpec::default()
                    })
                });
            }
            for opening in [OpeningKind::Door, OpeningKind::Window] {
                add(BlockKind::Opening, 2.0, &|i| i.opening = opening);
            }
            add(BlockKind::Object, 3.0, &|i| {
                i.shape = crate::model::ObjectShape::Round
            });
            let hidden = add(BlockKind::Light, 8.0, &|i| i.hidden = true);
            let locked = add(BlockKind::Actor, 9.0, &|i| i.locked = true);
            add(BlockKind::Mic, 3.2, &|i| {
                i.mic = Some(MicSpec {
                    attached_to: Some(person),
                    ..MicSpec::new(MicKind::Lavalier)
                })
            });
            let curved = add(BlockKind::Actor, 1.0, &|i| {
                i.keys = vec![
                    crate::model::BlockKey::new(2.0, 3.0, 3.0, 0.0),
                    crate::model::BlockKey::new(4.0, 0.0, 5.0, 90.0),
                ];
            });
            let p = scheme.palette();
            theme::apply(&ctx, &p, 100);
            for layer in Layer::ALL {
                state.layer = layer;
                for scale in [18.0, 24.0, 40.0, 220.0] {
                    state.scale = scale;
                    for (selected, others) in [
                        (None, false),
                        (Some(camera), true),
                        (Some(locked), false),
                        (Some(curved), true),
                        (Some(hidden), false),
                    ] {
                        (state.selected, state.show_others) = (selected, others);
                        for n in 0..2 {
                            frame(&ctx, &mut state, &mut project, Vec::new(), n as f64 * 0.1);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_hidden_figure_cannot_be_picked_and_a_locked_one_does_not_move() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        let id = project
            .add_block_item(blocking, BlockKind::Actor, 5.0, 5.0)
            .unwrap();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .unwrap()
            .locked = true;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), 0.0);
        let at = canvas.min + state.offset + vec2(5.0, 5.0) * state.scale;
        let mut t = 1.0;
        drag(
            &ctx,
            &mut state,
            &mut project,
            at,
            at + vec2(80.0, 0.0),
            &mut t,
        );
        assert_eq!(state.selected, Some(id), "lo bloqueado se elige");
        let item = find(&project, blocking, id);
        assert_eq!((item.x, item.y), (5.0, 5.0), "pero no se mueve");
    }

    #[test]
    fn initials_take_up_to_two_letters() {
        assert_eq!(initials("Juana Pérez"), "JP");
        assert_eq!(initials("lucas"), "L");
        assert_eq!(initials(""), "");
    }

    // ---- Modo de colocación (plantas.md §4) ----

    /// Dónde cae un punto del plano (metros) en la pantalla.
    fn at(canvas: egui::Rect, state: &BlockState, world: (f32, f32)) -> egui::Pos2 {
        canvas.min + state.offset + vec2(world.0, world.1) * state.scale
    }

    fn mouse(pos: egui::Pos2, pressed: bool, mods: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: mods,
        }
    }

    /// Un clic en el punto `world` del plano (con `mods` apretadas), en cuatro cuadros.
    fn click(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        world: (f32, f32),
        mods: egui::Modifiers,
        t: &mut f64,
    ) {
        let canvas = frame_with(ctx, state, project, Vec::new(), *t, mods);
        let pos = at(canvas, state, world);
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![mouse(pos, true, mods)],
            vec![mouse(pos, false, mods)],
            vec![],
        ] {
            *t += 0.04;
            frame_with(ctx, state, project, events, *t, mods);
        }
    }

    /// Mueve el mouse al punto `world` sin apretar nada.
    fn hover_at(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        world: (f32, f32),
        t: &mut f64,
    ) {
        let canvas = frame(ctx, state, project, Vec::new(), *t);
        let pos = at(canvas, state, world);
        for _ in 0..3 {
            *t += 0.04;
            frame(
                ctx,
                state,
                project,
                vec![egui::Event::PointerMoved(pos)],
                *t,
            );
        }
    }

    fn key(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        key: egui::Key,
        t: &mut f64,
    ) {
        for pressed in [true, false] {
            *t += 0.04;
            let event = egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            frame(ctx, state, project, vec![event], *t);
        }
    }

    fn count(project: &Project) -> usize {
        project.blockings[0].items.len()
    }

    #[test]
    fn choosing_a_tool_and_moving_the_mouse_creates_nothing() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        state.record(&project);
        let before = count(&project);
        let mut t = 0.0;
        for spec in [
            Spec::Camera,
            Spec::Person,
            Spec::Light(LightFixture::Panel),
            Spec::Mic(MicKind::Boom),
        ] {
            state.set_tool(Tool::Place(spec));
            for point in [(1.0, 1.0), (2.5, 3.0), (4.0, 4.0)] {
                hover_at(&ctx, &mut state, &mut project, point, &mut t);
            }
            assert_eq!(state.tool, Tool::Place(spec), "sigue en modo de colocación");
        }
        assert_eq!(count(&project), before);
        state.record(&project);
        assert!(!state.can_undo(), "el fantasma no entra en deshacer");
    }

    #[test]
    fn a_click_creates_exactly_one_element_there_with_and_without_the_grid() {
        for (snap, expected) in [(true, (3.25, 2.5)), (false, (3.13, 2.4))] {
            let (ctx, mut state, mut project, blocking, _, _) = setup();
            state.blocking = Some(blocking);
            state.snap = snap;
            state.record(&project);
            let before = count(&project);
            state.set_tool(Tool::Place(Spec::Person));
            let mut t = 0.0;
            click(
                &ctx,
                &mut state,
                &mut project,
                (3.13, 2.4),
                egui::Modifiers::NONE,
                &mut t,
            );
            assert_eq!(count(&project), before + 1, "snap {snap}");
            let item = project.blockings[0].items.last().unwrap();
            assert_eq!(item.kind, BlockKind::Actor);
            assert!(
                (item.x - expected.0).abs() < 0.03 && (item.y - expected.1).abs() < 0.03,
                "snap {snap}: ({}, {})",
                item.x,
                item.y
            );
            assert_eq!(state.selected, Some(item.id), "lo nuevo queda elegido");
            assert_eq!(state.tool, Tool::Select, "la herramienta se suelta");
            state.record(&project);
            assert_eq!(state.undo.len(), 1, "un solo paso de deshacer");
        }
    }

    #[test]
    fn alt_turns_the_grid_off_while_placing() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        state.set_tool(Tool::Place(Spec::Person));
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (3.13, 2.4),
            egui::Modifiers::ALT,
            &mut t,
        );
        let item = project.blockings[0].items.last().unwrap();
        assert!(
            (item.x - 3.13).abs() < 0.03 && (item.y - 2.4).abs() < 0.03,
            "({}, {})",
            item.x,
            item.y
        );
    }

    #[test]
    fn escape_and_the_right_click_cancel_without_creating_anything() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        let before = count(&project);
        let mut t = 0.0;
        state.set_tool(Tool::Place(Spec::Camera));
        hover_at(&ctx, &mut state, &mut project, (2.0, 2.0), &mut t);
        key(&ctx, &mut state, &mut project, egui::Key::Escape, &mut t);
        assert_eq!((state.tool, count(&project)), (Tool::Select, before));
        state.set_tool(Tool::Place(Spec::Camera));
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let pos = at(canvas, &state, (2.0, 2.0));
        for pressed in [true, false] {
            t += 0.04;
            let event = egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(
                &ctx,
                &mut state,
                &mut project,
                vec![egui::Event::PointerMoved(pos), event],
                t,
            );
        }
        assert_eq!((state.tool, count(&project)), (Tool::Select, before));
    }

    #[test]
    fn shift_click_keeps_the_tool_active_and_the_cameras_take_the_next_letter() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        state.set_tool(Tool::Place(Spec::Camera));
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (6.0, 3.0),
            egui::Modifiers::SHIFT,
            &mut t,
        );
        assert_eq!(
            state.tool,
            Tool::Place(Spec::Camera),
            "con Shift sigue activa"
        );
        click(
            &ctx,
            &mut state,
            &mut project,
            (8.0, 3.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(state.tool, Tool::Select, "sin Shift se suelta");
        let labels: Vec<&str> = project.blockings[0]
            .items
            .iter()
            .filter(|i| i.kind == BlockKind::Camera)
            .map(|i| i.label.as_str())
            .collect();
        assert_eq!(labels, ["A", "B", "C"], "la letra siguiente libre");
    }

    #[test]
    fn pressing_and_dragging_places_and_aims_and_shift_jumps_by_fifteen_degrees() {
        for (mods, expected) in [
            (egui::Modifiers::NONE, 90.0),
            (egui::Modifiers::SHIFT, 90.0),
        ] {
            let (ctx, mut state, mut project, blocking, _, _) = setup();
            state.blocking = Some(blocking);
            state.set_tool(Tool::Place(Spec::Person));
            let mut t = 0.0;
            let canvas = frame_with(&ctx, &mut state, &mut project, Vec::new(), t, mods);
            let (from, to) = (
                at(canvas, &state, (5.0, 5.0)),
                at(canvas, &state, (5.0, 7.0)),
            );
            for events in [
                vec![egui::Event::PointerMoved(from)],
                vec![mouse(from, true, mods)],
                vec![egui::Event::PointerMoved(from + (to - from) * 0.5)],
                vec![egui::Event::PointerMoved(to)],
                vec![mouse(to, false, mods)],
                vec![],
            ] {
                t += 0.04;
                frame_with(&ctx, &mut state, &mut project, events, t, mods);
            }
            let item = project.blockings[0].items.last().unwrap();
            assert_eq!(item.kind, BlockKind::Actor);
            assert!(
                (item.x - 5.0).abs() < 0.03 && (item.y - 5.0).abs() < 0.03,
                "se coloca donde se apretó"
            );
            assert!(
                (item.rot - expected).abs() < 1.0,
                "mira hacia donde se arrastró: {}",
                item.rot
            );
        }
        // Con Shift, 26° salta a 30°.
        assert_eq!(placement::angle_between((0.0, 0.0), (1.0, 0.5), true), 30.0);
    }

    #[test]
    fn walls_chain_with_clicks_stick_to_other_ends_and_a_double_click_ends_the_chain() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        let base = project
            .add_block_item(blocking, BlockKind::Wall, 2.0, 0.0)
            .unwrap();
        {
            let w = project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == base)
                .unwrap();
            (w.w, w.rot) = (4.0, 0.0);
        }
        let walls = |p: &Project| {
            p.blockings[0]
                .items
                .iter()
                .filter(|i| i.kind == BlockKind::Wall)
                .count()
        };
        state.set_tool(Tool::Place(Spec::Wall));
        let mut t = 0.0;
        // El primer clic solo marca el inicio, pegado al extremo de la pared que ya está (4, 0).
        click(
            &ctx,
            &mut state,
            &mut project,
            (4.1, 0.1),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(walls(&project), 1);
        assert_eq!(state.wall_start, Some((4.0, 0.0)));
        click(
            &ctx,
            &mut state,
            &mut project,
            (4.0, 3.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(walls(&project), 2);
        let second = project.blockings[0].items.last().unwrap();
        assert!(
            (second.w - 3.0).abs() < 0.05 && (second.rot - 90.0).abs() < 0.5,
            "w {} rot {}",
            second.w,
            second.rot
        );
        assert_eq!(
            state.wall_start,
            Some((4.0, 3.0)),
            "la cadena sigue desde donde cerró"
        );
        click(
            &ctx,
            &mut state,
            &mut project,
            (1.0, 3.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(walls(&project), 3);
        // Esc termina la cadena pero deja la herramienta; la segunda vez la cancela.
        key(&ctx, &mut state, &mut project, egui::Key::Escape, &mut t);
        assert_eq!(
            (state.wall_start, state.tool),
            (None, Tool::Place(Spec::Wall))
        );
        key(&ctx, &mut state, &mut project, egui::Key::Escape, &mut t);
        assert_eq!(state.tool, Tool::Select);
        assert_eq!(walls(&project), 3, "Esc no crea nada");
        // Doble clic: la pared que cierra el primer clic queda y la cadena termina.
        t += 0.5;
        state.set_tool(Tool::Place(Spec::Wall));
        click(
            &ctx,
            &mut state,
            &mut project,
            (8.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let pos = at(canvas, &state, (10.0, 8.0));
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![mouse(pos, true, egui::Modifiers::NONE)],
            vec![mouse(pos, false, egui::Modifiers::NONE)],
            vec![mouse(pos, true, egui::Modifiers::NONE)],
            vec![mouse(pos, false, egui::Modifiers::NONE)],
            vec![],
        ] {
            t += 0.03;
            frame(&ctx, &mut state, &mut project, events, t);
        }
        assert_eq!(
            walls(&project),
            4,
            "una sola pared nueva: el doble clic no suma otra"
        );
        assert_eq!((state.wall_start, state.tool), (None, Tool::Select));
    }

    #[test]
    fn a_door_only_lands_on_a_wall_and_takes_its_angle_and_id() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        let wall = project
            .add_block_item(blocking, BlockKind::Wall, 4.0, 6.0)
            .unwrap();
        {
            let w = project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == wall)
                .unwrap();
            (w.w, w.rot) = (6.0, 0.0);
        }
        let openings = |p: &Project| {
            p.blockings[0]
                .items
                .iter()
                .filter(|i| i.kind == BlockKind::Opening)
                .count()
        };
        state.set_tool(Tool::Place(Spec::Opening(OpeningKind::Door)));
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (4.0, 3.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(openings(&project), 0, "lejos de toda pared no se crea nada");
        assert!(
            matches!(state.tool, Tool::Place(_)),
            "y la herramienta sigue"
        );
        click(
            &ctx,
            &mut state,
            &mut project,
            (3.0, 6.1),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(openings(&project), 1);
        let door = project.blockings[0]
            .items
            .iter()
            .find(|i| i.kind == BlockKind::Opening)
            .unwrap();
        assert_eq!(
            (door.wall, door.rot, door.opening),
            (Some(wall), 0.0, OpeningKind::Door)
        );
        assert!(
            (door.y - 6.0).abs() < 0.05 && (door.x - 3.0).abs() < 0.3,
            "({}, {})",
            door.x,
            door.y
        );
    }

    #[test]
    fn a_lavalier_only_lands_on_a_person_and_stays_attached_to_them() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        state.layer = Layer::Sound;
        let mics = |p: &Project| {
            p.blockings[0]
                .items
                .iter()
                .filter(|i| i.kind == BlockKind::Mic)
                .count()
        };
        // Sin personas, la tecla M no entra en el modo del corbatero (la primera herramienta es el boom).
        let person = project
            .add_block_item(blocking, BlockKind::Actor, 5.0, 5.0)
            .unwrap();
        state.set_tool(Tool::Place(Spec::Mic(MicKind::Lavalier)));
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (8.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(mics(&project), 0, "fuera de toda persona no se crea nada");
        click(
            &ctx,
            &mut state,
            &mut project,
            (5.05, 5.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(mics(&project), 1);
        let mic = project.blockings[0]
            .items
            .iter()
            .find(|i| i.kind == BlockKind::Mic)
            .unwrap();
        assert_eq!(mic.mic.as_ref().unwrap().attached_to, Some(person));
        assert!(mic.mic.as_ref().unwrap().wireless);
        assert!(!has_people(&Project::default(), None));
        assert!(has_people(&project, Some(blocking)));
    }

    #[test]
    fn a_boom_takes_two_clicks_first_the_operator_then_the_capsule() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        state.layer = Layer::Sound;
        state.set_tool(Tool::Place(Spec::Mic(MicKind::Boom)));
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (2.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(
            project.blockings[0]
                .items
                .iter()
                .filter(|i| i.kind == BlockKind::Mic)
                .count(),
            0
        );
        assert_eq!(state.boom_operator, Some((2.0, 8.0)));
        assert!(state.status()[0].contains("cápsula"));
        click(
            &ctx,
            &mut state,
            &mut project,
            (4.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        let boom = project.blockings[0]
            .items
            .iter()
            .find(|i| i.kind == BlockKind::Mic)
            .expect("el boom");
        let spec = boom.mic.as_ref().unwrap();
        assert_eq!(
            (spec.kind, spec.operator),
            (MicKind::Boom, Some((2.0, 8.0)))
        );
        assert!(
            (boom.x - 4.0).abs() < 0.03 && (boom.y - 8.0).abs() < 0.03 && boom.rot.abs() < 1.0,
            "la cápsula mira hacia donde apunta la caña"
        );
        assert_eq!((state.tool, state.boom_operator), (Tool::Select, None));
    }

    #[test]
    fn an_object_is_one_meter_by_default_and_a_drag_draws_its_rectangle() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        state.set_tool(Tool::Place(Spec::Object { round: false }));
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (6.0, 6.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        let one = project.blockings[0].items.last().unwrap();
        assert_eq!((one.kind, one.w, one.h), (BlockKind::Object, 1.0, 1.0));
        state.set_tool(Tool::Place(Spec::Object { round: true }));
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (8.0, 2.0)),
            at(canvas, &state, (10.0, 3.0)),
        );
        for events in [
            vec![egui::Event::PointerMoved(from)],
            vec![mouse(from, true, egui::Modifiers::NONE)],
            vec![egui::Event::PointerMoved(from + (to - from) * 0.5)],
            vec![egui::Event::PointerMoved(to)],
            vec![mouse(to, false, egui::Modifiers::NONE)],
            vec![],
        ] {
            t += 0.04;
            frame(&ctx, &mut state, &mut project, events, t);
        }
        let drawn = project.blockings[0].items.last().unwrap();
        assert_eq!(drawn.shape, crate::model::ObjectShape::Round);
        assert!(
            (drawn.w - 2.0).abs() < 0.05
                && (drawn.h - 1.0).abs() < 0.05
                && (drawn.x - 9.0).abs() < 0.05,
            "{}×{} en ({}, {})",
            drawn.w,
            drawn.h,
            drawn.x,
            drawn.y
        );
    }

    #[test]
    fn each_plant_only_answers_to_the_keys_of_its_own_tools() {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        let mut t = 0.0;
        key(&ctx, &mut state, &mut project, egui::Key::P, &mut t);
        assert_eq!(state.tool, Tool::Place(Spec::Person));
        key(&ctx, &mut state, &mut project, egui::Key::V, &mut t);
        assert_eq!(state.tool, Tool::Select);
        key(&ctx, &mut state, &mut project, egui::Key::O, &mut t);
        key(&ctx, &mut state, &mut project, egui::Key::O, &mut t);
        assert_eq!(
            state.tool,
            Tool::Place(Spec::Object { round: true }),
            "otra vez O alterna a redondo"
        );
        key(&ctx, &mut state, &mut project, egui::Key::L, &mut t);
        assert_eq!(
            state.tool,
            Tool::Place(Spec::Object { round: true }),
            "L no es de la planta de cámara"
        );
        state.layer = Layer::Lights;
        state.set_tool(Tool::Select);
        key(&ctx, &mut state, &mut project, egui::Key::P, &mut t);
        assert_eq!(state.tool, Tool::Select, "P no es de la planta de luces");
        key(&ctx, &mut state, &mut project, egui::Key::L, &mut t);
        assert_eq!(state.tool, Tool::Place(Spec::Light(LightFixture::Fresnel)));
        state.layer = Layer::Sound;
        state.set_tool(Tool::Select);
        // Sin personas, el corbatero no se puede elegir: M cicla boom → (salta el corbatero) …
        key(&ctx, &mut state, &mut project, egui::Key::M, &mut t);
        assert_eq!(state.tool, Tool::Place(Spec::Mic(MicKind::Boom)));
        key(&ctx, &mut state, &mut project, egui::Key::M, &mut t);
        assert_eq!(
            state.tool,
            Tool::Place(Spec::Mic(MicKind::Plant)),
            "sin personas, M salta el corbatero"
        );
    }

    #[test]
    fn the_status_bar_explains_how_to_place_while_a_tool_is_active() {
        let mut state = BlockState::default();
        assert!(!state.status().iter().any(|s| s.contains("cancelar")));
        state.set_tool(Tool::Place(Spec::Camera));
        assert_eq!(
            state.status()[0],
            "Clic para colocar · Arrastrá para orientar · Shift para seguir colocando · Esc para cancelar"
        );
        state.set_tool(Tool::Place(Spec::Wall));
        assert!(state.status()[0].contains("Esc"));
    }

    // ---- Movimiento (plantas.md §5) ----

    fn timeline_rect(ctx: &egui::Context) -> egui::Rect {
        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("blocking_timeline")))
            .expect("línea de tiempo dibujada")
    }

    /// Una cámara en (3; 2) con una posición guardada en (7; 2) a los 4 s, elegida, con la línea de tiempo en 0.
    fn walker() -> (egui::Context, BlockState, Project, u64, u64) {
        let (ctx, mut state, mut project, blocking, camera, _) = setup();
        state.blocking = Some(blocking);
        state.selected = Some(camera);
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .set_pose_at(4.0, 7.0, 2.0, 0.0);
        (ctx, state, project, blocking, camera)
    }

    #[test]
    fn clicking_a_ghost_goes_to_its_instant_and_selects_that_position() {
        let (ctx, mut state, mut project, _, camera) = walker();
        let mut t = 0.0;
        // Dentro del cuerpo del fantasma, lejos del punto de la posición.
        click(
            &ctx,
            &mut state,
            &mut project,
            (7.3, 2.1),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(state.time, 4.0, "va al instante de la posición");
        assert_eq!(state.sel_pose, Some((camera, Some(0))));
        assert_eq!(state.selected, Some(camera));
        // La figura de ese instante es la de la posición: tocarla elige la cosa y suelta la posición.
        click(
            &ctx,
            &mut state,
            &mut project,
            (7.0, 2.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(state.sel_pose, None);
    }

    #[test]
    fn dragging_a_ghost_moves_that_position_and_not_the_clock() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (7.3, 2.1)),
            at(canvas, &state, (7.3, 4.1)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let moved = find(&project, blocking, camera);
        assert_eq!(moved.keys.len(), 1);
        assert!(
            (moved.keys[0].y - 4.0).abs() < 0.3 && (moved.keys[0].x - 7.0).abs() < 0.3,
            "{:?}",
            moved.keys[0]
        );
        assert_eq!((moved.x, moved.y), (3.0, 2.0), "el inicio no se mueve");
        assert_eq!(state.time, 0.0);
        assert_eq!(state.sel_pose, Some((camera, Some(0))));
    }

    #[test]
    fn with_no_ghosts_the_positions_cannot_be_grabbed() {
        let (ctx, mut state, mut project, _, _) = walker();
        state.ghosts = GhostMode::Nothing;
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (7.3, 2.1),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!((state.time, state.sel_pose), (0.0, None));
    }

    #[test]
    fn delete_removes_the_selected_position_and_not_the_figure() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (7.3, 2.1),
            egui::Modifiers::NONE,
            &mut t,
        );
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        let item = find(&project, blocking, camera);
        assert!(item.keys.is_empty(), "la posición se borró");
        assert_eq!(state.sel_pose, None);
        assert_eq!(
            state.selected,
            Some(camera),
            "la cámara sigue ahí y elegida"
        );
        // Sin posición elegida, Supr borra la figura, como siempre.
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        assert!(project.blockings[0].items.iter().all(|i| i.id != camera));
    }

    #[test]
    fn with_recording_off_the_changes_go_to_the_starting_position() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        state.auto_record = false;
        // A los 2 s la figura va por la mitad del camino, en (5; 2).
        state.time = 2.0;
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (5.0, 2.0)),
            at(canvas, &state, (5.0, 5.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let item = find(&project, blocking, camera);
        assert_eq!(item.keys.len(), 1, "no se crea ninguna posición");
        assert_eq!(
            (item.keys[0].x, item.keys[0].y),
            (7.0, 2.0),
            "la guardada no cambia"
        );
        assert!((item.y - 5.0).abs() < 0.3, "cambió el inicio: {}", item.y);
    }

    #[test]
    fn k_comma_period_and_space_drive_the_timeline() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let mut t = 0.0;
        // K: una posición en el instante que se ve, donde está la figura entonces.
        state.time = 2.0;
        key(&ctx, &mut state, &mut project, egui::Key::K, &mut t);
        let item = find(&project, blocking, camera);
        assert_eq!(item.keys.len(), 2);
        assert!(
            (item.keys[0].t - 2.0).abs() < 1e-4 && (item.keys[0].x - 5.0).abs() < 1e-3,
            "{:?}",
            item.keys[0]
        );
        assert_eq!(state.sel_pose, Some((camera, Some(0))));
        // En 0 no crea nada (ahí está la posición de partida).
        state.time = 0.0;
        key(&ctx, &mut state, &mut project, egui::Key::K, &mut t);
        assert_eq!(find(&project, blocking, camera).keys.len(), 2);
        // Punto: a la siguiente; coma: a la anterior.
        key(&ctx, &mut state, &mut project, egui::Key::Period, &mut t);
        assert_eq!(state.time, 2.0);
        key(&ctx, &mut state, &mut project, egui::Key::Period, &mut t);
        assert_eq!(state.time, 4.0);
        key(&ctx, &mut state, &mut project, egui::Key::Comma, &mut t);
        assert_eq!(state.time, 2.0);
        key(&ctx, &mut state, &mut project, egui::Key::Comma, &mut t);
        assert_eq!((state.time, state.sel_pose), (0.0, Some((camera, None))));
        // Espacio reproduce y pausa.
        assert!(!state.playing);
        key(&ctx, &mut state, &mut project, egui::Key::Space, &mut t);
        assert!(state.playing);
        key(&ctx, &mut state, &mut project, egui::Key::Space, &mut t);
        assert!(!state.playing);
    }

    #[test]
    fn playing_runs_at_the_chosen_speed() {
        let (ctx, mut state, mut project, _, _) = walker();
        project.blockings[0].duration = 100.0;
        let mut t = 1.0;
        frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let mut elapsed = |speed: f32| {
            state.speed = speed;
            state.time = 0.0;
            state.playing = true;
            for _ in 0..5 {
                t += 0.1;
                frame(&ctx, &mut state, &mut project, Vec::new(), t);
            }
            state.time
        };
        let (slow, normal, fast) = (elapsed(0.5), elapsed(1.0), elapsed(2.0));
        assert!(slow < normal && normal < fast, "{slow} {normal} {fast}");
        assert!(
            (normal / slow - 2.0).abs() < 0.2 && (fast / normal - 2.0).abs() < 0.2,
            "{slow} {normal} {fast}"
        );
    }

    #[test]
    fn the_timeline_has_a_row_per_moving_element_and_a_diamond_drags_with_a_magnet() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let person = project
            .add_block_item(blocking, BlockKind::Actor, 1.0, 5.0)
            .unwrap();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == person)
            .unwrap()
            .set_pose_at(6.0, 2.0, 5.0, 0.0);
        project.blockings[0].duration = 10.0;
        assert_eq!(
            timeline_rows(&project, &state, blocking).len(),
            2,
            "la cámara y la persona"
        );
        let one_row = BlockState::default();
        let still = timeline_height(&Project::default(), &one_row, 0);
        assert!(timeline_height(&project, &state, blocking) > still);
        let mut t = 0.0;
        // Un par de cuadros para que el lienzo y la línea de tiempo se acomoden al alto de las filas.
        for _ in 0..3 {
            t += 0.01;
            frame(&ctx, &mut state, &mut project, Vec::new(), t);
        }
        let line = timeline_rect(&ctx);
        let (track_l, track_r) = (line.left() + TL_LABEL_W, line.right() - tokens::SP_L);
        let x_of = |s: f32| track_l + (track_r - track_l) * s / 10.0;
        // El rombo de la cámara (4 s) está en la primera fila.
        let y = line.top() + TL_HEADER_H + TL_RULER_H + TL_ROW_H / 2.0 - 2.0;
        let from = egui::pos2(x_of(4.0), y);
        // Se arrastra hasta 5,96 s: queda pegado a los 6 s de la persona.
        let to = egui::pos2(x_of(5.96), y);
        for events in [
            vec![egui::Event::PointerMoved(from)],
            vec![mouse(from, true, egui::Modifiers::NONE)],
            vec![egui::Event::PointerMoved(from + (to - from) * 0.5)],
            vec![egui::Event::PointerMoved(to)],
            vec![mouse(to, false, egui::Modifiers::NONE)],
            vec![],
        ] {
            t += 0.05;
            frame(&ctx, &mut state, &mut project, events, t);
        }
        let key_t = find(&project, blocking, camera).keys[0].t;
        assert!(
            (key_t - 6.0).abs() < 1e-3,
            "se pegó a los 6 s de la persona: {key_t}"
        );
    }

    #[test]
    fn with_a_shot_filtered_what_is_added_is_related_to_it_and_all_plants_export_in_one_pdf() {
        let (_ctx, mut state, mut project, blocking, camera) = walker();
        let scene = project.blockings[0].scene;
        let shot = project.add_shot(scene, None);
        state.shot_filter = Some(shot);
        // Un micrófono nuevo se anota en el plano; la cámara que ya había no cambia.
        assert!(place_item(&mut state, &mut project, blocking, Spec::Mic(MicKind::Stand), (3.0, 3.0), &Extra::default()));
        let mic = state.selected.unwrap();
        assert_eq!(project.shots[0].items, vec![mic]);
        assert_eq!(project.shots[0].camera, None);
        // Una cámara nueva pasa a ser la del plano.
        assert!(place_item(&mut state, &mut project, blocking, Spec::Camera, (5.0, 3.0), &Extra::default()));
        let cam = state.selected.unwrap();
        assert_eq!((project.shots[0].blocking, project.shots[0].camera), (Some(blocking), Some(cam)));
        assert_ne!(cam, camera);
        // Todas las plantas de todos los bloqueos, en un PDF: una hoja por planta, o una sola con todas.
        let all = |combine: bool, legend: bool| plan_pdf::PlanExport {
            blocking: None,
            layers: [true, true, true],
            combine,
            cones: false,
            legend,
        };
        let paper = crate::storage::Paper::A4;
        assert!(plan_pdf::plan_pdf(&project, &all(false, true), paper).is_ok());
        assert!(plan_pdf::plan_pdf(&project, &all(true, false), paper).is_ok());
        let none = plan_pdf::PlanExport { layers: [false; 3], ..all(false, true) };
        assert!(plan_pdf::plan_pdf(&project, &none, paper).is_err());
    }

    #[test]
    fn filtering_by_shot_keeps_the_time_inside_its_span_and_stops_playback_at_its_end() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let scene = project.blockings[0].scene;
        let shot = project.add_shot(scene, None);
        project.shots[0].blocking = Some(blocking);
        project.shots[0].camera = Some(camera);
        project.shots[0].range = Some((2.0, 4.0));
        project.blockings[0].duration = 10.0;
        state.shot_filter = Some(shot);
        state.time = 9.0;
        let mut t = 0.0;
        for _ in 0..3 {
            t += 0.01;
            frame(&ctx, &mut state, &mut project, Vec::new(), t);
        }
        assert!((2.0..=4.0).contains(&state.time), "{}", state.time);
        state.time = 3.5;
        state.playing = true;
        for _ in 0..40 {
            t += 0.1;
            frame(&ctx, &mut state, &mut project, Vec::new(), t);
        }
        assert!(!state.playing && (state.time - 4.0).abs() < 0.01, "{}", state.time);
        // Un plano que ya no existe deja de filtrar.
        project.shots.clear();
        frame(&ctx, &mut state, &mut project, Vec::new(), t + 0.1);
        assert_eq!(state.shot_filter, None);
    }

    #[test]
    fn a_shot_span_sits_on_the_camera_row_and_can_be_moved_and_highlights_the_path() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let scene = project.blockings[0].scene;
        project.add_shot(scene, None);
        project.shots[0].blocking = Some(blocking);
        project.shots[0].camera = Some(camera);
        project.shots[0].range = Some((2.0, 4.0));
        project.blockings[0].duration = 10.0;
        let mut t = 0.0;
        // Un par de cuadros para que el lienzo y la línea de tiempo se acomoden al alto de las filas.
        for _ in 0..3 {
            t += 0.01;
            frame(&ctx, &mut state, &mut project, Vec::new(), t);
        }
        let line = timeline_rect(&ctx);
        let (track_l, track_r) = (line.left() + TL_LABEL_W, line.right() - tokens::SP_L);
        let x_of = |s: f32| track_l + (track_r - track_l) * s / 10.0;
        let y = line.top() + TL_HEADER_H + TL_RULER_H + TL_ROW_H - 5.0;
        let from = egui::pos2(x_of(3.0), y);
        let to = egui::pos2(x_of(6.0), y);
        for events in [
            vec![egui::Event::PointerMoved(from)],
            vec![mouse(from, true, egui::Modifiers::NONE)],
            vec![egui::Event::PointerMoved(from + (to - from) * 0.5)],
            vec![egui::Event::PointerMoved(to)],
            vec![mouse(to, false, egui::Modifiers::NONE)],
            vec![],
        ] {
            t += 0.05;
            frame(&ctx, &mut state, &mut project, events, t);
        }
        let (a, b) = project.shots[0].range.unwrap();
        assert!(
            (b - a - 2.0).abs() < 0.05,
            "el tramo no cambia de largo: {a}..{b}"
        );
        assert!((a - 5.0).abs() < 0.15, "se corrió 3 s: {a}");
        assert_eq!(state.sel_shot, Some(project.shots[0].id));
        assert_eq!(
            shot_span_of(&project, &state, blocking),
            Some((camera, a, b))
        );
    }

    #[test]
    fn every_motion_view_draws_with_ghosts_curves_pans_and_holds() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        {
            let item = project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == camera)
                .unwrap();
            item.set_pose_at(6.0, 7.0, 2.0, 90.0);
            item.set_pose_at(8.0, 9.0, 5.0, 90.0);
            item.keys[0].hold = 1.5;
            item.keys[0].label = "Llega a la mesa".into();
            item.keys[0].ease_in = true;
        }
        let person = project
            .add_block_item(blocking, BlockKind::Actor, 1.0, 5.0)
            .unwrap();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == person)
            .unwrap()
            .set_pose_at(5.0, 2.0, 6.0, 40.0);
        for mode in GhostMode::ALL {
            state.ghosts = mode;
            for time in [0.0, 3.0, 4.0, 7.0] {
                state.time = time;
                for record in [true, false] {
                    state.auto_record = record;
                    for n in 0..2 {
                        frame(&ctx, &mut state, &mut project, Vec::new(), n as f64 * 0.1);
                    }
                }
            }
        }
    }

    // ---- Selección, manijas, menú, copiar y pegar, medidas y avisos (plantas.md §5.4 y §6) ----

    /// Tres cosas en fila: una persona, otra y una luz, y la línea de tiempo en 0.
    fn crowd() -> (egui::Context, BlockState, Project, u64, [u64; 3]) {
        let (ctx, mut state, mut project, blocking, _, _) = setup();
        state.blocking = Some(blocking);
        let a = project
            .add_block_item(blocking, BlockKind::Actor, 8.0, 3.0)
            .unwrap();
        let b = project
            .add_block_item(blocking, BlockKind::Actor, 10.0, 3.0)
            .unwrap();
        let c = project
            .add_block_item(blocking, BlockKind::Object, 12.0, 3.0)
            .unwrap();
        (ctx, state, project, blocking, [a, b, c])
    }

    fn mods_click(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        world: (f32, f32),
        mods: egui::Modifiers,
        t: &mut f64,
    ) {
        click(ctx, state, project, world, mods, t);
    }

    #[test]
    fn shift_click_adds_and_removes_things_from_the_selection() {
        let (ctx, mut state, mut project, _, [a, b, _]) = crowd();
        let mut t = 0.0;
        click(
            &ctx,
            &mut state,
            &mut project,
            (8.0, 3.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(state.multi, vec![a]);
        mods_click(
            &ctx,
            &mut state,
            &mut project,
            (10.0, 3.0),
            egui::Modifiers::SHIFT,
            &mut t,
        );
        assert_eq!(state.multi, vec![a, b]);
        assert_eq!(state.selected, Some(b), "la última es la principal");
        mods_click(
            &ctx,
            &mut state,
            &mut project,
            (8.0, 3.0),
            egui::Modifiers::SHIFT,
            &mut t,
        );
        assert_eq!(
            state.multi,
            vec![b],
            "Shift+clic sobre una elegida la quita"
        );
        click(
            &ctx,
            &mut state,
            &mut project,
            (14.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert!(
            state.multi.is_empty() && state.selected.is_none(),
            "un clic en el vacío suelta todo"
        );
    }

    #[test]
    fn a_rectangle_selects_what_is_inside_it_and_shift_adds_to_it() {
        let (ctx, mut state, mut project, _, [a, b, c]) = crowd();
        state.ghosts = GhostMode::Nothing;
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (7.0, 2.0)),
            at(canvas, &state, (11.0, 4.5)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let mut chosen = state.multi.clone();
        chosen.sort();
        assert_eq!(chosen, vec![a, b], "las dos personas, no el mueble");
        // Con Shift se suma lo del nuevo rectángulo.
        let (from, to) = (
            at(canvas, &state, (11.5, 2.0)),
            at(canvas, &state, (13.0, 4.0)),
        );
        let events = |pos: egui::Pos2, pressed: bool| mouse(pos, pressed, egui::Modifiers::SHIFT);
        for evs in [
            vec![egui::Event::PointerMoved(from)],
            vec![events(from, true)],
            vec![egui::Event::PointerMoved(from + (to - from) * 0.5)],
            vec![egui::Event::PointerMoved(to)],
            vec![events(to, false)],
            vec![],
        ] {
            t += 0.05;
            frame_with(
                &ctx,
                &mut state,
                &mut project,
                evs,
                t,
                egui::Modifiers::SHIFT,
            );
        }
        assert!(
            state.multi.contains(&c) && state.multi.contains(&a) && state.multi.contains(&b),
            "{:?}",
            state.multi
        );
    }

    #[test]
    fn several_things_move_and_turn_together() {
        let (ctx, mut state, mut project, blocking, [a, b, _]) = crowd();
        state.multi = vec![a, b];
        state.selected = Some(b);
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (10.0, 3.0)),
            at(canvas, &state, (10.0, 6.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let (pa, pb) = (find(&project, blocking, a), find(&project, blocking, b));
        assert!(
            (pa.y - 6.0).abs() < 0.3 && (pb.y - 6.0).abs() < 0.3,
            "las dos bajaron 3 m: {} {}",
            pa.y,
            pb.y
        );
        assert!(
            ((pb.x - pa.x) - 2.0).abs() < 1e-3,
            "siguen separadas lo mismo"
        );
        // Con R giran las dos 15°; con Shift+R, al revés.
        key(&ctx, &mut state, &mut project, egui::Key::R, &mut t);
        assert!(
            (find(&project, blocking, a).rot - 15.0).abs() < 1e-3
                && (find(&project, blocking, b).rot - 15.0).abs() < 1e-3
        );
        for pressed in [true, false] {
            t += 0.04;
            let event = egui::Event::Key {
                key: egui::Key::R,
                physical_key: Some(egui::Key::R),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            };
            frame_with(
                &ctx,
                &mut state,
                &mut project,
                vec![event],
                t,
                egui::Modifiers::SHIFT,
            );
        }
        assert!(find(&project, blocking, a).rot.abs() < 1e-3);
    }

    #[test]
    fn the_arrow_keys_nudge_the_selection_a_tenth_of_a_meter_and_shift_half_a_meter() {
        let (ctx, mut state, mut project, blocking, [a, _, _]) = crowd();
        state.selected = Some(a);
        let mut t = 0.0;
        key(
            &ctx,
            &mut state,
            &mut project,
            egui::Key::ArrowRight,
            &mut t,
        );
        assert!((find(&project, blocking, a).x - 8.1).abs() < 1e-4);
        for pressed in [true, false] {
            t += 0.04;
            let event = egui::Event::Key {
                key: egui::Key::ArrowDown,
                physical_key: Some(egui::Key::ArrowDown),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            };
            frame_with(
                &ctx,
                &mut state,
                &mut project,
                vec![event],
                t,
                egui::Modifiers::SHIFT,
            );
        }
        assert!((find(&project, blocking, a).y - 3.5).abs() < 1e-4);
    }

    fn command_key(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        key: egui::Key,
        t: &mut f64,
    ) {
        for pressed in [true, false] {
            *t += 0.04;
            let event = egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            };
            frame_with(
                ctx,
                state,
                project,
                vec![event],
                *t,
                egui::Modifiers::COMMAND,
            );
        }
    }

    #[test]
    fn hide_lock_front_and_back_act_on_everything_chosen() {
        let (ctx, mut state, mut project, blocking, [a, b, c]) = crowd();
        state.multi = vec![a, b];
        state.selected = Some(b);
        let mut t = 0.0;
        // Bloquear (Ctrl+L): las dos; otra vez, las desbloquea.
        command_key(&ctx, &mut state, &mut project, egui::Key::L, &mut t);
        assert!(find(&project, blocking, a).locked && find(&project, blocking, b).locked);
        command_key(&ctx, &mut state, &mut project, egui::Key::L, &mut t);
        assert!(!find(&project, blocking, a).locked);
        // Ocultar (H): dejan de estar elegidas y no se pueden tocar.
        key(&ctx, &mut state, &mut project, egui::Key::H, &mut t);
        assert!(find(&project, blocking, a).hidden && find(&project, blocking, b).hidden);
        assert!(state.multi.is_empty() && state.selected.is_none());
        click(
            &ctx,
            &mut state,
            &mut project,
            (8.0, 3.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert!(state.selected.is_none(), "lo oculto no se elige");
        // Traer al frente y llevar atrás reordenan.
        state.multi = vec![c];
        state.selected = Some(c);
        assert!(apply_act(&mut state, &mut project, blocking, EditAct::Back));
        assert_eq!(project.blockings[0].items.first().unwrap().id, c);
        assert!(apply_act(
            &mut state,
            &mut project,
            blocking,
            EditAct::Front
        ));
        assert_eq!(project.blockings[0].items.last().unwrap().id, c);
    }

    #[test]
    fn duplicate_copies_next_to_the_original_and_paste_follows_the_mouse_until_the_click() {
        let (ctx, mut state, mut project, blocking, [a, _, _]) = crowd();
        let camera = project.blockings[0]
            .items
            .iter()
            .find(|i| i.kind == BlockKind::Camera)
            .unwrap()
            .id;
        state.multi = vec![a, camera];
        state.selected = Some(a);
        let before = project.blockings[0].items.len();
        let mut t = 0.0;
        // Ctrl+D: dos copias a 0,5 m; la cámara toma la letra siguiente libre.
        command_key(&ctx, &mut state, &mut project, egui::Key::D, &mut t);
        assert_eq!(project.blockings[0].items.len(), before + 2);
        assert_eq!(state.multi.len(), 2, "quedan elegidas las copias");
        let copy = find(&project, blocking, a);
        assert!(state.multi.iter().all(|id| *id != a && *id != camera));
        let copied_camera = project.blockings[0]
            .items
            .iter()
            .find(|i| i.kind == BlockKind::Camera && i.id != camera)
            .unwrap();
        assert_eq!(copied_camera.label, "B");
        let _ = copy;
        // Ctrl+C, Ctrl+V: entra en modo de pegar y no crea nada hasta el clic.
        command_key(&ctx, &mut state, &mut project, egui::Key::C, &mut t);
        assert_eq!(state.clipboard.len(), 2);
        let count = project.blockings[0].items.len();
        command_key(&ctx, &mut state, &mut project, egui::Key::V, &mut t);
        assert_eq!(state.tool, Tool::Paste);
        hover_at(&ctx, &mut state, &mut project, (6.0, 8.0), &mut t);
        assert_eq!(
            project.blockings[0].items.len(),
            count,
            "mover el mouse no pega"
        );
        assert!(state.status()[0].contains("pegar"));
        click(
            &ctx,
            &mut state,
            &mut project,
            (6.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(project.blockings[0].items.len(), count + 2);
        assert_eq!(state.tool, Tool::Select);
        let pasted = state.multi.clone();
        let near: Vec<(f32, f32)> = pasted
            .iter()
            .map(|id| {
                let i = find(&project, blocking, *id);
                (i.x, i.y)
            })
            .collect();
        let centre = ((near[0].0 + near[1].0) / 2.0, (near[0].1 + near[1].1) / 2.0);
        assert!(
            (centre.0 - 6.0).abs() < 0.3 && (centre.1 - 8.0).abs() < 0.3,
            "queda centrado donde se hizo clic: {centre:?}"
        );
        // Esc cancela sin pegar.
        command_key(&ctx, &mut state, &mut project, egui::Key::V, &mut t);
        key(&ctx, &mut state, &mut project, egui::Key::Escape, &mut t);
        assert_eq!(
            (state.tool, project.blockings[0].items.len()),
            (Tool::Select, count + 2)
        );
    }

    #[test]
    fn the_context_menu_actions_and_delete_remove_everything_chosen_in_one_step() {
        let (ctx, mut state, mut project, blocking, [a, b, c]) = crowd();
        state.multi = vec![a, b];
        state.selected = Some(a);
        let mut t = 0.0;
        state.record(&project);
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        let ids: Vec<u64> = project.blockings[0].items.iter().map(|i| i.id).collect();
        assert!(!ids.contains(&a) && !ids.contains(&b) && ids.contains(&c));
        state.record(&project);
        assert_eq!(state.undo.len(), 1, "un solo paso de deshacer para las dos");
        assert!(state.undo(&mut project));
        assert!(
            project.blockings[0].items.iter().any(|i| i.id == a)
                && project.blockings[0].items.iter().any(|i| i.id == b)
        );
        // Ver planos vinculados pide el inspector.
        state.selected = Some(blocking);
        assert!(!apply_act(
            &mut state,
            &mut project,
            blocking,
            EditAct::ShowShots
        ));
        assert_eq!(state.tab_request, Some(0));
    }

    #[test]
    fn the_light_and_microphone_range_handles_drag_the_edge_of_the_beam_and_the_pattern() {
        let (ctx, mut state, mut project, blocking, _) = crowd();
        let lamp = project
            .add_block_item(blocking, BlockKind::Light, 4.0, 8.0)
            .unwrap();
        let mic = project
            .add_block_item(blocking, BlockKind::Mic, 4.0, 6.0)
            .unwrap();
        state.layer = Layer::Lights;
        state.selected = Some(lamp);
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let handles = plan_checks::range_handles(&find(&project, blocking, lamp));
        let reach = handles.iter().find(|h| h.0 == Range::Reach).unwrap().1;
        let beam = handles.iter().find(|h| h.0 == Range::Beam).unwrap().1;
        // El alcance: se arrastra el frente del haz hasta 5 m.
        let (from, to) = (
            at(canvas, &state, reach),
            at(canvas, &state, (4.0 + 5.0, 8.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        assert!((find(&project, blocking, lamp).light.unwrap().reach_m - 5.0).abs() < 0.3);
        // La apertura: el borde del haz se abre hasta 60°.
        let handles = plan_checks::range_handles(&find(&project, blocking, lamp));
        let beam_now = handles.iter().find(|h| h.0 == Range::Beam).unwrap().1;
        assert_ne!(beam, beam_now, "el borde del haz se corrió con el alcance");
        let r = 5.0_f32;
        let target = (
            4.0 + r * 30.0_f32.to_radians().cos(),
            8.0 + r * 30.0_f32.to_radians().sin(),
        );
        let (from, to) = (at(canvas, &state, beam_now), at(canvas, &state, target));
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let spec = find(&project, blocking, lamp).light.unwrap();
        assert!(
            (spec.beam_deg - 60.0).abs() < 8.0,
            "apertura {}",
            spec.beam_deg
        );
        assert_eq!(find(&project, blocking, lamp).fov, spec.beam_deg);
        // Un micrófono solo tiene alcance.
        state.layer = Layer::Sound;
        state.selected = Some(mic);
        let handles = plan_checks::range_handles(&find(&project, blocking, mic));
        assert_eq!(handles.len(), 1);
        let (from, to) = (
            at(canvas, &state, handles[0].1),
            at(canvas, &state, (4.0 + 3.0, 6.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        assert!((find(&project, blocking, mic).mic.unwrap().reach_m - 3.0).abs() < 0.3);
    }

    #[test]
    fn openings_and_modifiers_resize_only_along_their_length_and_modifiers_keep_their_spec() {
        assert_eq!(size_sides(BlockKind::Opening), vec![(-1, 0), (1, 0)]);
        assert_eq!(size_sides(BlockKind::Modifier), vec![(-1, 0), (1, 0)]);
        assert_eq!(size_sides(BlockKind::Object).len(), 8);
        assert!(size_sides(BlockKind::Light).is_empty());
        let (ctx, mut state, mut project, blocking, _) = crowd();
        let flag = project
            .add_block_item(blocking, BlockKind::Modifier, 4.0, 8.0)
            .unwrap();
        state.layer = Layer::Lights;
        state.selected = Some(flag);
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let item = find(&project, blocking, flag);
        let end = size_handle(&item, (1, 0));
        let (from, to) = (
            at(canvas, &state, end),
            at(canvas, &state, (end.0 + 1.0, end.1)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let wider = find(&project, blocking, flag);
        assert!(wider.w > item.w + 0.5, "{} → {}", item.w, wider.w);
        assert_eq!(
            wider.modifier.unwrap().w,
            wider.w,
            "la medida del modificador sigue a la de la figura"
        );
    }

    #[test]
    fn the_measure_tool_shows_the_distance_and_creates_nothing() {
        let (ctx, mut state, mut project, _, _) = crowd();
        let count = count(&project);
        state.set_tool(Tool::Measure);
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (2.0, 10.0)),
            at(canvas, &state, (5.0, 14.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let (a, b) = state.measure.expect("quedó la medida");
        assert!(
            ((a.0 - b.0).hypot(a.1 - b.1) - 5.0).abs() < 0.1,
            "3-4-5: {a:?} {b:?}"
        );
        assert_eq!(count, super::tests::count(&project), "no crea nada");
        assert_eq!(state.tool, Tool::Measure, "sigue midiendo");
        key(&ctx, &mut state, &mut project, egui::Key::Escape, &mut t);
        assert_eq!((state.tool, state.measure), (Tool::Select, None));
    }

    #[test]
    fn the_selected_camera_measures_the_people_in_its_cone_and_frame_warnings_name_camera_and_shots()
     {
        let (ctx, mut state, mut project, blocking, camera, scene) = {
            let (ctx, state, project, blocking, camera, scene) = setup();
            (ctx, state, project, blocking, camera, scene)
        };
        state.blocking = Some(blocking);
        {
            let cam = project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == camera)
                .unwrap();
            (cam.rot, cam.fov) = (0.0, 40.0);
        }
        let near = project
            .add_block_item(blocking, BlockKind::Actor, 5.0, 2.0)
            .unwrap();
        project.add_block_item(blocking, BlockKind::Actor, 3.0, 9.0);
        let lamp = project
            .add_block_item(blocking, BlockKind::Light, 6.0, 2.0)
            .unwrap();
        let far = project
            .add_block_item(blocking, BlockKind::Light, 3.0, 12.0)
            .unwrap();
        project.add_shot(scene, None);
        project.shots[0].blocking = Some(blocking);
        project.shots[0].camera = Some(camera);
        state.selected = Some(camera);
        for n in 0..3 {
            frame(&ctx, &mut state, &mut project, Vec::new(), n as f64 * 0.1);
        }
        let seen: Vec<u64> = state.warnings.iter().map(|w| w.item).collect();
        assert!(seen.contains(&lamp) && !seen.contains(&far), "{seen:?}");
        let text = state
            .warnings
            .iter()
            .find(|w| w.item == lamp)
            .unwrap()
            .text();
        assert!(
            text.contains("Cámara A") && text.contains("plano"),
            "{text}"
        );
        assert!(
            !state.warnings.iter().any(|w| w.item == near),
            "una persona no avisa"
        );
        // Y la lista de la pestaña PLANTA la dibuja.
        let p = crate::storage::Settings::default().scheme.palette();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(400.0, 900.0),
            )),
            time: Some(5.0),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                plant_tab(ui, &mut state, &mut project, &p);
            });
        });
        out.textures_delta.clear();
    }

    #[test]
    fn other_plants_show_only_their_references_and_cannot_be_picked() {
        let (ctx, mut state, mut project, blocking, _) = crowd();
        let lamp = project
            .add_block_item(blocking, BlockKind::Light, 4.0, 8.0)
            .unwrap();
        state.layer = Layer::Camera;
        state.show_others = true;
        let mut t = 0.0;
        // Desde la planta de cámara se ve la luz tenue, pero un clic encima no la elige.
        click(
            &ctx,
            &mut state,
            &mut project,
            (4.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_ne!(state.selected, Some(lamp));
        state.layer = Layer::Lights;
        let cam = project.blockings[0]
            .items
            .iter()
            .find(|i| i.kind == BlockKind::Camera)
            .unwrap();
        let (cam_id, cx, cy) = (cam.id, cam.x, cam.y);
        click(
            &ctx,
            &mut state,
            &mut project,
            (cx, cy),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_ne!(
            state.selected,
            Some(cam_id),
            "la cámara es solo referencia en la planta de luces"
        );
        // La planta de luces no muestra personas como lo suyo, pero sí la base.
        click(
            &ctx,
            &mut state,
            &mut project,
            (4.0, 8.0),
            egui::Modifiers::NONE,
            &mut t,
        );
        assert_eq!(state.selected, Some(lamp));
    }

    #[test]
    fn hiding_and_locking_from_the_elements_tab_work_and_a_hidden_thing_can_be_shown_again() {
        let (ctx, mut state, mut project, blocking, [a, _, _]) = crowd();
        let p = crate::storage::Settings::default().scheme.palette();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == a)
            .unwrap()
            .hidden = true;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(400.0, 800.0),
                )),
                time: Some(1.0),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    items_tab(ui, &mut state, &mut project, &p);
                });
            },
        );
        out.textures_delta.clear();
        assert!(find(&project, blocking, a).hidden);
        // Mostrarlo es lo mismo que hacerlo desde el menú.
        state.multi = vec![a];
        assert!(apply_act(
            &mut state,
            &mut project,
            blocking,
            EditAct::ToggleHidden
        ));
        assert!(!find(&project, blocking, a).hidden);
    }

    // ---- Inspector (§7) y PDF (§8) ----

    /// Dibuja `add` y devuelve todo el texto que pintó.
    fn drawn_text(ctx: &egui::Context, add: impl FnOnce(&mut Ui)) -> String {
        let mut add = Some(add);
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(380.0, 1600.0),
                )),
                time: Some(1.0),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if let Some(add) = add.take() {
                        add(ui);
                    }
                });
            },
        );
        out.textures_delta.clear();
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_selection_tab_draws_the_fields_of_every_kind_and_shows_dashes_for_differing_values() {
        let (ctx, mut state, mut project, blocking, camera, _) = setup();
        theme::apply(
            &ctx,
            &crate::storage::Settings::default().scheme.palette(),
            100,
        );
        state.blocking = Some(blocking);
        let p = crate::storage::Settings::default().scheme.palette();
        let a = project
            .add_block_item(blocking, BlockKind::Light, 1.0, 1.0)
            .unwrap();
        let b = project
            .add_block_item(blocking, BlockKind::Light, 2.0, 1.0)
            .unwrap();
        {
            let items = &mut project.blockings[0].items;
            items.iter_mut().find(|i| i.id == a).unwrap().watts = 300;
            items.iter_mut().find(|i| i.id == b).unwrap().watts = 650;
            items.iter_mut().find(|i| i.id == b).unwrap().kelvin =
                items.iter().find(|i| i.id == a).unwrap().kelvin;
        }
        // Dos luces con distinta potencia: «—» en la potencia y el valor común en la temperatura.
        state.multi = vec![a, b];
        state.selected = Some(b);
        let text = drawn_text(&ctx, |ui| {
            inspector::selection_tab(ui, &mut state, &mut project, &p);
        });
        assert!(text.contains("—"), "{text}");
        assert!(
            text.contains("Potencia") && text.contains("Temperatura") && text.contains("Aparato"),
            "{text}"
        );
        assert!(
            text.contains("5.600 K") || text.contains("5600 K"),
            "el valor igual se muestra: {text}"
        );
        assert!(
            text.contains("2 elegidas") || text.contains("Varias cosas"),
            "{text}"
        );
        // Cada tipo, de a uno: sus campos.
        let wall = project
            .add_block_item(blocking, BlockKind::Wall, 0.0, 0.0)
            .unwrap();
        let door = project
            .add_block_item(blocking, BlockKind::Opening, 0.0, 0.0)
            .unwrap();
        let flag = project
            .add_block_item(blocking, BlockKind::Modifier, 0.0, 0.0)
            .unwrap();
        let boom = project
            .add_block_item(blocking, BlockKind::Mic, 0.0, 0.0)
            .unwrap();
        let lav = project
            .add_block_item(blocking, BlockKind::Mic, 0.0, 0.0)
            .unwrap();
        let person = project
            .add_block_item(blocking, BlockKind::Actor, 3.0, 3.0)
            .unwrap();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == lav)
            .unwrap()
            .mic
            .as_mut()
            .unwrap()
            .kind = MicKind::Lavalier;
        let expectations: [(u64, &[&str]); 8] = [
            (
                camera,
                &[
                    "Cuerpo",
                    "Lente",
                    "Soporte",
                    "Altura",
                    "Ángulo",
                    "Largo del cono",
                    "Planos vinculados",
                ],
            ),
            (
                a,
                &[
                    "Aparato",
                    "Función",
                    "Apertura",
                    "Alcance",
                    "Potencia",
                    "Temperatura",
                    "Intensidad",
                    "Gelatina",
                    "Motivación",
                ],
            ),
            (wall, &["Largo", "Grosor"]),
            (door, &["Tipo", "Ancho"]),
            (flag, &["Tipo", "Largo"]),
            (
                boom,
                &["Patrón", "Alcance", "Pista", "Inalámbrico", "Operador"],
            ),
            (lav, &["Lo lleva"]),
            (person, &["Personaje", "Tamaño"]),
        ];
        for (id, fields) in expectations {
            state.selected = Some(id);
            state.multi = vec![id];
            let text = drawn_text(&ctx, |ui| {
                inspector::selection_tab(ui, &mut state, &mut project, &p);
            });
            let lower = text.to_lowercase();
            for field in fields {
                assert!(
                    lower.contains(&field.to_lowercase()),
                    "{id}: falta «{field}»: {text}"
                );
            }
            assert!(
                text.contains("Giro") && text.contains("Oculto") && text.contains("Bloqueado"),
                "{text}"
            );
        }
    }

    #[test]
    fn the_positions_list_shows_time_and_mark_and_the_curved_switch_and_edits_apply_to_everything_chosen()
     {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let p = crate::storage::Settings::default().scheme.palette();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .keys[0]
            .label = "Llega a la mesa".into();
        let text = drawn_text(&ctx, |ui| {
            inspector::selection_tab(ui, &mut state, &mut project, &p);
        });
        assert!(
            text.contains("POSICIONES") && !text.contains("Recorrido curvo"),
            "{text}"
        );
        assert!(
            text.contains("1 · 0 s")
                && text.contains("2 · 4 s")
                && text.contains("Llega a la mesa")
                && text.contains("inicio"),
            "{text}"
        );
        assert!(
            !text.contains("Fijar posición"),
            "en 0 s no hay nada que fijar: {text}"
        );
        // `common` distingue lo igual de lo distinto.
        let items: Vec<BlockItem> = project.blockings[0].items.clone();
        assert_eq!(
            inspector::common(&items[..1], |i| i.kind),
            Some(BlockKind::Camera)
        );
        let two = [
            items[0].clone(),
            BlockItem {
                kind: BlockKind::Light,
                ..items[0].clone()
            },
        ];
        assert_eq!(inspector::common(&two, |i| i.kind), None);
        assert_eq!(inspector::common(&two, |i| i.x), Some(items[0].x));
        let _ = blocking;
    }

    #[test]
    fn the_pdf_uses_the_same_shapes_with_legend_numbered_ghosts_and_a_positions_table() {
        let (_, _, mut project, blocking, camera, scene) = setup();
        project.add_shot(scene, None);
        project.shots[0].blocking = Some(blocking);
        project.shots[0].camera = Some(camera);
        let person = project
            .add_block_item(blocking, BlockKind::Actor, 1.0, 1.0)
            .unwrap();
        let lamp = project
            .add_block_item(blocking, BlockKind::Light, 5.0, 3.0)
            .unwrap();
        let mic = project
            .add_block_item(blocking, BlockKind::Mic, 4.0, 1.0)
            .unwrap();
        project.add_block_item(blocking, BlockKind::Wall, 2.0, 5.0);
        project.add_block_item(blocking, BlockKind::Opening, 2.0, 5.0);
        project.add_block_item(blocking, BlockKind::Modifier, 6.0, 3.0);
        {
            let items = &mut project.blockings[0].items;
            let walker = items.iter_mut().find(|i| i.id == person).unwrap();
            walker.keys.iter_mut().for_each(|k| k.smooth = true);
            for n in 1..=40 {
                walker.set_pose_at(
                    n as f32 * 0.5,
                    1.0 + n as f32 * 0.1,
                    1.0 + (n % 5) as f32 * 0.2,
                    (n * 20) as f32,
                );
            }
            walker.keys[0].label = "Se levanta".into();
            walker.keys[1].hold = 2.0;
            items
                .iter_mut()
                .find(|i| i.id == lamp)
                .unwrap()
                .light
                .as_mut()
                .unwrap()
                .fixture = LightFixture::Softbox;
            items
                .iter_mut()
                .find(|i| i.id == mic)
                .unwrap()
                .mic
                .as_mut()
                .unwrap()
                .pattern = MicPattern::Figure8;
        }
        project.blockings[0].duration = 30.0;
        for layer in Layer::ALL {
            let bytes = plan_pdf(&project, blocking, layer, crate::storage::Paper::A4, true)
                .unwrap_or_else(|e| panic!("{layer:?}: {e}"));
            assert!(bytes.starts_with(b"%PDF"), "{layer:?}");
            assert!(bytes.len() > 2000, "{layer:?}: {} bytes", bytes.len());
        }
        // Lo oculto no se imprime y una planta sin nada que imprimir lo dice.
        for i in &mut project.blockings[0].items {
            i.hidden = true;
        }
        assert!(plan_pdf(&project, blocking, Layer::Camera, crate::storage::Paper::A4, true).is_err());
    }

    #[test]
    fn choosing_from_the_elements_list_centers_the_view_on_it() {
        let (ctx, mut state, mut project, blocking, _) = walker();
        let far = project
            .add_block_item(blocking, BlockKind::Actor, 40.0, 30.0)
            .unwrap();
        let p = crate::storage::Settings::default().scheme.palette();
        // Dibuja la lista y hace clic en la fila de la persona lejana.
        let mut rows: Vec<(egui::Rect, String)> = Vec::new();
        for n in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(380.0, 800.0),
                    )),
                    time: Some(1.0 + n as f64 * 0.1),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        items_tab(ui, &mut state, &mut project, &p);
                    });
                },
            );
            out.textures_delta.clear();
            rows = out
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == "Persona" => Some((
                        egui::Rect::from_min_size(t.pos, t.galley.size()),
                        t.galley.text().to_string(),
                    )),
                    _ => None,
                })
                .collect();
        }
        let (rect, _) = rows.first().cloned().expect("la fila de la persona");
        let pos = rect.center();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![mouse(pos, true, egui::Modifiers::NONE)],
            vec![mouse(pos, false, egui::Modifiers::NONE)],
        ] {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(380.0, 800.0),
                    )),
                    time: Some(2.0),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        items_tab(ui, &mut state, &mut project, &p);
                    });
                },
            );
            out.textures_delta.clear();
        }
        assert_eq!(state.selected, Some(far));
        assert_eq!(
            state.center_on,
            Some(far),
            "la vista se centra en lo elegido"
        );
    }

    // ---- Zoom por posición, puntos del trayecto, operador, Supr y la caja de posiciones ----

    /// Un doble clic en el punto `world` del plano.
    fn double_click(
        ctx: &egui::Context,
        state: &mut BlockState,
        project: &mut Project,
        world: (f32, f32),
        t: &mut f64,
    ) {
        let canvas = frame(ctx, state, project, Vec::new(), *t);
        let pos = at(canvas, state, world);
        let none = egui::Modifiers::NONE;
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![mouse(pos, true, none)],
            vec![mouse(pos, false, none)],
            vec![mouse(pos, true, none)],
            vec![mouse(pos, false, none)],
            vec![],
        ] {
            *t += 0.03;
            frame(ctx, state, project, events, *t);
        }
        // Pasa un rato para que el próximo doble clic no cuente como un triple.
        *t += 0.6;
    }

    fn at_screen(canvas: egui::Rect, state: &BlockState, world: (f32, f32)) -> egui::Pos2 {
        at(canvas, state, world)
    }

    fn camera_of(project: &Project, id: u64) -> BlockItem {
        project.blockings[0]
            .items
            .iter()
            .find(|i| i.id == id)
            .unwrap()
            .clone()
    }

    #[test]
    fn a_double_click_on_the_path_adds_a_point_and_a_double_click_on_a_point_switches_corner_and_curve()
     {
        let (ctx, mut state, mut project, _, camera) = walker();
        let mut t = 0.0;
        double_click(&ctx, &mut state, &mut project, (5.0, 2.0), &mut t);
        let key = &camera_of(&project, camera).keys[0];
        assert_eq!(key.via.len(), 1, "un punto nuevo sobre el trayecto");
        assert!(
            (key.via[0].x - 5.0).abs() < 0.3
                && (key.via[0].y - 2.0).abs() < 0.3
                && !key.via[0].smooth
        );
        assert_eq!(state.sel_via, Some((camera, 0, 0)));
        assert_eq!(state.selected, Some(camera), "no se soltó la selección");
        assert_eq!(
            camera_of(&project, camera).keys.len(),
            1,
            "y no es una posición"
        );
        // Doble clic sobre el punto: pasa a curva; otra vez, a esquina.
        let via = (key.via[0].x, key.via[0].y);
        double_click(&ctx, &mut state, &mut project, via, &mut t);
        assert!(camera_of(&project, camera).keys[0].via[0].smooth);
        double_click(&ctx, &mut state, &mut project, via, &mut t);
        assert!(!camera_of(&project, camera).keys[0].via[0].smooth);
        // Un doble clic lejos del trayecto no agrega nada.
        double_click(&ctx, &mut state, &mut project, (5.0, 6.0), &mut t);
        assert_eq!(camera_of(&project, camera).keys[0].via.len(), 1);
    }

    #[test]
    fn a_path_point_drags_and_supr_deletes_it_and_not_the_camera() {
        let (ctx, mut state, mut project, _, camera) = walker();
        let mut t = 0.0;
        double_click(&ctx, &mut state, &mut project, (5.0, 2.0), &mut t);
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let via = camera_of(&project, camera).keys[0].via[0];
        let (from, to) = (
            at(canvas, &state, (via.x, via.y)),
            at(canvas, &state, (via.x, via.y + 2.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let moved = camera_of(&project, camera);
        assert!(
            (moved.keys[0].via[0].y - (via.y + 2.0)).abs() < 0.3,
            "el punto se movió: {:?}",
            moved.keys[0].via[0]
        );
        assert_eq!(
            (moved.keys[0].x, moved.keys[0].y),
            (7.0, 2.0),
            "la posición no"
        );
        // El movimiento ahora pasa por el punto.
        let mid = moved.pose_at(2.0);
        assert!(
            mid.1 > 3.0,
            "a mitad de camino la cámara va por el punto bajo: {mid:?}"
        );
        state.sel_via = Some((camera, 0, 0));
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        assert!(
            camera_of(&project, camera).keys[0].via.is_empty(),
            "Supr borra el punto"
        );
        assert_eq!(camera_of(&project, camera).keys.len(), 1);
    }

    #[test]
    fn supr_on_a_camera_with_positions_deletes_the_one_at_this_instant_and_keeps_the_rest() {
        let (ctx, mut state, mut project, _blocking, camera) = walker();
        {
            let cam = project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == camera)
                .unwrap();
            cam.set_pose_at(8.0, 9.0, 5.0, 90.0);
        }
        let mut t = 0.0;
        // Parada en la posición de los 4 s: se borra esa y quedan la de partida y la de los 8 s.
        state.time = 4.0;
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        let cam = camera_of(&project, camera);
        assert_eq!(cam.keys.iter().map(|k| k.t).collect::<Vec<_>>(), [8.0]);
        assert_eq!(state.selected, Some(camera), "la cámara sigue elegida");
        // En 0 s, la de partida la toma la siguiente.
        state.time = 0.0;
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        let cam = camera_of(&project, camera);
        assert!(
            cam.keys.is_empty() && (cam.x, cam.y) == (9.0, 5.0),
            "{cam:?}"
        );
        // Sin posiciones, Supr borra la cámara.
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        assert!(project.blockings[0].items.iter().all(|i| i.id != camera));
    }

    #[test]
    fn supr_on_a_camera_between_two_positions_deletes_the_camera() {
        let (ctx, mut state, mut project, _, camera) = walker();
        let mut t = 0.0;
        state.time = 2.0;
        key(&ctx, &mut state, &mut project, egui::Key::Delete, &mut t);
        assert!(
            project.blockings[0].items.iter().all(|i| i.id != camera),
            "entre dos posiciones también se borra"
        );
        assert_eq!(state.selected, None);
    }

    #[test]
    fn without_recording_the_edits_go_to_the_keyframe_where_the_timeline_is_and_not_to_the_first_one()
     {
        let (ctx, mut state, mut project, _, camera) = walker();
        state.auto_record = false;
        let mut t = 0.0;
        // Parado sobre el keyframe de los 4 s, se mueve la cámara elegida: cambia ESE keyframe y no la partida.
        state.time = 4.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (7.0, 2.0)),
            at(canvas, &state, (7.0, 5.0)),
        );
        // La figura está sobre el punto de la posición: se agarra desde un costado del cuerpo.
        let (from, to) = (from + vec2(12.0, 0.0), to + vec2(12.0, 0.0));
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let cam = camera_of(&project, camera);
        assert_eq!((cam.x, cam.y), (3.0, 2.0), "la partida no se tocó: {cam:?}");
        assert!(
            (cam.keys[0].y - 5.0).abs() < 0.3,
            "el keyframe sí: {:?}",
            cam.keys[0]
        );
        // Con las flechas pasa lo mismo.
        key(
            &ctx,
            &mut state,
            &mut project,
            egui::Key::ArrowRight,
            &mut t,
        );
        let cam = camera_of(&project, camera);
        assert_eq!((cam.x, cam.y), (3.0, 2.0));
        assert!((cam.keys[0].x - 7.1).abs() < 0.3, "{:?}", cam.keys[0]);
        // Parado donde no hay keyframe, se sigue editando la partida.
        state.time = 2.0;
        key(&ctx, &mut state, &mut project, egui::Key::ArrowDown, &mut t);
        let cam = camera_of(&project, camera);
        assert!((cam.y - 2.1).abs() < 1e-3, "{cam:?}");
        assert_eq!(cam.keys.len(), 1);
    }

    #[test]
    fn ctrl_z_undoes_and_ctrl_shift_z_and_ctrl_y_redo_in_the_plants() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        let mut t = 0.0;
        state.record(&project);
        let start = camera_of(&project, camera).x;
        // Dos cambios separados en el tiempo: dos pasos.
        for n in 0..2 {
            key(
                &ctx,
                &mut state,
                &mut project,
                egui::Key::ArrowRight,
                &mut t,
            );
            std::thread::sleep(std::time::Duration::from_millis(450));
            state.record(&project);
            assert!(camera_of(&project, camera).x > start + 0.05 * (n + 1) as f32);
        }
        assert_eq!(state.undo.len(), 2);
        command_key(&ctx, &mut state, &mut project, egui::Key::Z, &mut t);
        assert!((camera_of(&project, camera).x - (start + 0.1)).abs() < 1e-3);
        command_key(&ctx, &mut state, &mut project, egui::Key::Z, &mut t);
        assert!((camera_of(&project, camera).x - start).abs() < 1e-3);
        // Rehacer con Ctrl+Y y con Ctrl+Shift+Z.
        command_key(&ctx, &mut state, &mut project, egui::Key::Y, &mut t);
        assert!((camera_of(&project, camera).x - (start + 0.1)).abs() < 1e-3);
        for pressed in [true, false] {
            t += 0.04;
            let event = egui::Event::Key {
                key: egui::Key::Z,
                physical_key: Some(egui::Key::Z),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            };
            frame_with(
                &ctx,
                &mut state,
                &mut project,
                vec![event],
                t,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            );
        }
        assert!((camera_of(&project, camera).x - (start + 0.2)).abs() < 1e-3);
        // También se deshace borrar y el vínculo de un plano.
        let scene = project.blockings[0].scene;
        project.add_shot(scene, None);
        state.record(&project);
        std::thread::sleep(std::time::Duration::from_millis(450));
        project.shots[0].blocking = Some(blocking);
        project.shots[0].camera = Some(camera);
        project.shots[0].range = Some((1.0, 3.0));
        state.record(&project);
        command_key(&ctx, &mut state, &mut project, egui::Key::Z, &mut t);
        assert_eq!(
            (project.shots[0].camera, project.shots[0].range),
            (None, None)
        );
    }

    #[test]
    fn playing_with_recording_on_and_a_zoom_between_two_keyframes_creates_no_keyframes() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .set_lens_at(4.0, 120.0);
        project.blockings[0].duration = 10.0;
        state.auto_record = true;
        state.selected = Some(camera);
        let p = crate::storage::Settings::default().scheme.palette();
        let keys_before = camera_of(&project, camera).keys.clone();
        state.time = 0.0;
        state.playing = true;
        let mut t = 1.0;
        for _ in 0..60 {
            t += 0.05;
            // El lienzo avanza el reloj y el inspector dibuja la lente interpolada de cada cuadro.
            frame(&ctx, &mut state, &mut project, Vec::new(), t);
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(380.0, 1200.0),
                    )),
                    time: Some(t),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        inspector::selection_tab(ui, &mut state, &mut project, &p);
                    });
                },
            );
            out.textures_delta.clear();
        }
        assert!(state.time > 1.0, "el reloj avanzó");
        assert_eq!(
            camera_of(&project, camera).keys,
            keys_before,
            "no se creó ninguna posición"
        );
        let _ = blocking;
    }
    #[test]
    fn the_operator_of_a_boom_can_be_dragged() {
        let (ctx, mut state, mut project, blocking, _) = crowd();
        let boom = project
            .add_block_item(blocking, BlockKind::Mic, 4.0, 6.0)
            .unwrap();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == boom)
            .unwrap()
            .mic
            .as_mut()
            .unwrap()
            .operator = Some((2.0, 6.0));
        state.layer = Layer::Sound;
        state.selected = Some(boom);
        let mut t = 0.0;
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let (from, to) = (
            at(canvas, &state, (2.0, 6.0)),
            at(canvas, &state, (1.0, 8.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let moved = camera_of(&project, boom);
        let op = moved.mic.unwrap().operator.unwrap();
        assert!(
            (op.0 - 1.0).abs() < 0.3 && (op.1 - 8.0).abs() < 0.3,
            "{op:?}"
        );
        assert_eq!((moved.x, moved.y), (4.0, 6.0), "la cápsula no se movió");
    }

    #[test]
    fn the_zoom_of_a_position_moves_the_cone_the_warnings_and_the_angle_preview() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .set_lens_at(4.0, 200.0);
        let fov_at = |project: &Project, time: f32| {
            let items = project.blockings[0].items.clone();
            posed(&items, time, &|i| project.camera_fov(i))
                .into_iter()
                .find(|i| i.id == camera)
                .unwrap()
                .fov
        };
        let (start, end) = (fov_at(&project, 0.0), fov_at(&project, 4.0));
        assert!(
            end < start / 3.0,
            "un 200 mm es mucho más cerrado que un 35 mm: {start}° → {end}°"
        );
        assert!(
            fov_at(&project, 2.0) < start && fov_at(&project, 2.0) > end,
            "el zoom avanza con el tiempo"
        );
        // El inspector muestra la lente del instante y la vista previa del ángulo sale del mismo ángulo.
        let cam = camera_of(&project, camera);
        assert!(
            inspector::camera_fov(&project, &cam, 4.0) < inspector::camera_fov(&project, &cam, 0.0)
        );
        // Y el aviso «en cuadro» mira con el zoom de cada posición: a 200 mm la luz de al lado ya no entra.
        let lamp = project
            .add_block_item(blocking, BlockKind::Light, 5.0, 2.9)
            .unwrap();
        let items = project.blockings[0].items.clone();
        let wide_only = |lens_end: f32| {
            let mut items = items.clone();
            items.iter_mut().find(|i| i.id == camera).unwrap().keys[0].lens_mm = Some(lens_end);
            plan_checks::in_frame(&items, &|i| project.camera_fov(i), 0.0, &|_| Vec::new())
                .iter()
                .filter(|w| w.item == lamp)
                .count()
        };
        assert_eq!(wide_only(35.0), 1, "con 35 mm la luz entra");
        let _ = (&ctx, &mut state);
    }

    #[test]
    fn the_positions_box_goes_last_after_the_camera_bodies_and_scrolls_instead_of_pushing_things() {
        let (ctx, mut state, mut project, blocking, camera) = walker();
        project.add_camera_body("Canon", 1.6);
        let p = crate::storage::Settings::default().scheme.palette();
        {
            let cam = project.blockings[0]
                .items
                .iter_mut()
                .find(|i| i.id == camera)
                .unwrap();
            for n in 1..=20 {
                cam.set_pose_at(4.0 + n as f32, 3.0, 3.0, 0.0);
            }
        }
        let text = drawn_text(&ctx, |ui| {
            inspector::selection_tab(ui, &mut state, &mut project, &p);
        });
        let bodies = text
            .find("CUERPOS DE CÁMARA")
            .expect("los cuerpos de cámara");
        let positions = text.find("POSICIONES").expect("las posiciones");
        assert!(positions > bodies, "las posiciones van al final");
        // Con 21 posiciones, la altura del inspector es la misma que con 2: la caja se desplaza.
        let height = |project: &mut Project, state: &mut BlockState| {
            let mut used = 0.0;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(380.0, 3000.0),
                    )),
                    time: Some(9.0),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        inspector::selection_tab(ui, state, project, &p);
                        used = ui.min_rect().height();
                    });
                },
            );
            out.textures_delta.clear();
            used
        };
        let many = height(&mut project, &mut state);
        project.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .keys
            .truncate(2);
        let few = height(&mut project, &mut state);
        assert!(
            many - few < 7.0 * tokens::ROW_H,
            "la caja no crece sin tope: {few} → {many}"
        );
        let _ = blocking;
    }

    #[test]
    fn a_smooth_point_shows_curvature_handles_and_dragging_one_reshapes_the_curve() {
        let (ctx, mut state, mut project, _, camera) = walker();
        let mut t = 0.0;
        double_click(&ctx, &mut state, &mut project, (5.0, 2.0), &mut t);
        let via = camera_of(&project, camera).keys[0].via[0];
        // Un punto en esquina no tiene tiradores; al pasar a curva, sí.
        assert!(camera_of(&project, camera).smooth_handles().is_empty());
        double_click(&ctx, &mut state, &mut project, (via.x, via.y), &mut t);
        let item = camera_of(&project, camera);
        let handles = item.smooth_handles();
        assert_eq!(handles.len(), 1, "un nodo suave, un par de tiradores");
        let (node, at, h) = handles[0];
        assert_eq!(node, crate::model::PathNodeRef::Via(0, 0));
        let before = item.segment_curve(0);
        // Se agarra el tirador de adelante y se lo sube 2 m: la curva cambia y el punto no se mueve.
        let canvas = frame(&ctx, &mut state, &mut project, Vec::new(), t);
        let front = (at.0 + h.0, at.1 + h.1);
        let (from, to) = (
            at_screen(canvas, &state, front),
            at_screen(canvas, &state, (front.0, front.1 + 2.0)),
        );
        drag(&ctx, &mut state, &mut project, from, to, &mut t);
        let after = camera_of(&project, camera);
        let moved = after.keys[0].via[0];
        assert_eq!(
            (moved.x, moved.y),
            (via.x, via.y),
            "el punto sigue donde estaba"
        );
        let handle = moved.handle.expect("quedó con tirador propio");
        assert!(handle.1 > h.1 + 1.5, "el tirador subió: {h:?} → {handle:?}");
        assert_ne!(before, after.segment_curve(0), "la curva cambió");
        // El tirador de atrás es el simétrico.
        let (_, _, now) = after.smooth_handles()[0];
        assert!((now.0 - handle.0).abs() < 1e-4 && (now.1 - handle.1).abs() < 1e-4);
        assert_eq!(
            state.selected,
            Some(camera),
            "agarrar un tirador no suelta la selección"
        );
        // Si el punto vuelve a esquina, pierde la curvatura.
        double_click(&ctx, &mut state, &mut project, (via.x, via.y), &mut t);
        let back = camera_of(&project, camera).keys[0].via[0];
        assert!(!back.smooth && back.handle.is_none());
    }
}
