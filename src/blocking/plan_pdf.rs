//! El PDF de una planta (docs/plantas.md §8): las mismas formas que el lienzo (`figures`), la cobertura de cámaras, luces y
//! micrófonos, los recorridos con flechas, los fantasmas numerados de cada posición, una leyenda con la escala y los planos
//! de cada cámara, y la tabla de posiciones.

use super::*;
use crate::figures::{Paint, Part};

type Rgb = [u8; 3];

const CAMERA: Rgb = [70, 140, 220];
const SOUND: Rgb = [220, 130, 40];
const DARK: Rgb = [40, 40, 40];
const GRAY: Rgb = [150, 150, 150];

fn rgb(c: egui::Color32) -> Rgb {
    let [r, g, b, _] = c.to_array();
    [r, g, b]
}

/// El color con que se imprime el cuerpo de una cosa, y el de sus detalles (más oscuro).
fn body_color(project: &Project, item: &BlockItem) -> Rgb {
    match item.kind {
        BlockKind::Wall => [90, 90, 90],
        BlockKind::Object => [225, 225, 225],
        BlockKind::Opening => [255, 255, 255],
        BlockKind::Actor => item
            .character
            .and_then(|id| project.characters.iter().find(|c| c.id == id))
            .map_or([200, 200, 200], |c| {
                rgb(theme::character_color(&c.id.to_string()))
            }),
        BlockKind::Camera => CAMERA,
        BlockKind::Light => rgb(theme::beam_color_plain(
            item.kelvin,
            item.light.as_ref().and_then(|l| l.gel),
        )),
        BlockKind::Modifier => match item.modifier.as_ref().map(|m| m.kind) {
            Some(ModifierKind::Flag) => [60, 60, 60],
            Some(ModifierKind::Diffuser) => [255, 255, 255],
            _ => [190, 190, 190],
        },
        BlockKind::Mic => SOUND,
    }
}

fn darker(c: Rgb) -> Rgb {
    c.map(|v| (v as f32 * 0.65) as u8)
}

/// Qué cosas se imprimen a todo color en cada planta y cuáles solo de referencia (plantas.md §6.3).
fn role(layer: Layer, kind: BlockKind) -> Option<f32> {
    if is_base(kind) || layer.owns(kind) {
        return Some(1.0);
    }
    let referenced = match layer {
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
    referenced.then_some(0.3)
}

/// Como `role`, para varias plantas en una misma hoja: lo que una de ellas imprime a todo color va a todo color.
fn role_in(layers: &[Layer], kind: BlockKind) -> Option<f32> {
    layers
        .iter()
        .filter_map(|l| role(*l, kind))
        .fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v))))
}

/// Qué se exporta: las plantas de uno o de todos los bloqueos, cada una en su hoja o todas en una, con o sin leyenda y conos.
#[derive(Clone)]
pub(super) struct PlanExport {
    /// `None`: todos los bloqueos del proyecto.
    pub blocking: Option<u64>,
    /// Cámara, luces y sonido; la escenografía va siempre.
    pub layers: [bool; 3],
    /// Todas las plantas marcadas en una sola hoja (si no, una hoja por planta).
    pub combine: bool,
    pub cones: bool,
    /// La leyenda de símbolos, los planos por cámara y la tabla de posiciones.
    pub legend: bool,
}

/// Una línea de trazos entre puntos.
fn dashed(pen: &mut crate::pdf::Pen, points: &[(f32, f32)], color: Rgb, width: f32) {
    let mut on = true;
    let mut left: f32 = 4.0;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len < 1e-3 {
            continue;
        }
        let mut walked = 0.0;
        while walked < len {
            let step = left.min(len - walked);
            if on {
                let p0 = (
                    a.0 + (b.0 - a.0) * walked / len,
                    a.1 + (b.1 - a.1) * walked / len,
                );
                let p1 = (
                    a.0 + (b.0 - a.0) * (walked + step) / len,
                    a.1 + (b.1 - a.1) * (walked + step) / len,
                );
                pen.line(p0, p1, color, width);
            }
            walked += step;
            left -= step;
            if left <= 1e-3 {
                on = !on;
                left = if on { 4.0 } else { 3.0 };
            }
        }
    }
}

/// Dibuja las partes de una figura ya puestas en la hoja.
fn draw_parts(
    pen: &mut crate::pdf::Pen,
    parts: &[Part],
    body: Rgb,
    alpha: f32,
    to_page: &dyn Fn((f32, f32)) -> (f32, f32),
) {
    for part in parts {
        let points: Vec<(f32, f32)> = part.points.iter().map(|q| to_page(*q)).collect();
        match part.paint {
            Paint::Body => pen.poly(&points, Some((body, alpha)), Some((DARK, 0.5))),
            Paint::Detail => pen.poly(&points, Some((darker(body), alpha)), None),
            Paint::Muted => pen.poly(&points, Some(([170, 170, 170], alpha)), None),
            Paint::Void => pen.poly(&points, Some(([255, 255, 255], 1.0)), Some((GRAY, 0.5))),
            Paint::Line => {
                for pair in points.windows(2) {
                    pen.line(pair[0], pair[1], DARK, 0.7);
                }
            }
            Paint::Dashed => {
                let mut points = points;
                if part.closed {
                    points.extend(points.first().copied());
                }
                dashed(pen, &points, DARK, 0.7);
            }
        }
    }
}

/// La cobertura de algo: el cono de una cámara, el haz de una luz o el patrón de un micrófono.
fn draw_coverage(
    pen: &mut crate::pdf::Pen,
    project: &Project,
    item: &BlockItem,
    alpha: f32,
    cones: bool,
    to_page: &dyn Fn((f32, f32)) -> (f32, f32),
) {
    match item.kind {
        // El cono de las cámaras es opcional al exportar: tapa mucho de la planta.
        BlockKind::Camera if !cones => {}
        BlockKind::Camera => {
            let seen = BlockItem {
                fov: project.camera_fov(item),
                ..item.clone()
            };
            let pts: Vec<_> = cone(&seen).into_iter().map(to_page).collect();
            pen.poly(&pts, Some((CAMERA, 0.18 * alpha)), Some((CAMERA, 0.6)));
        }
        BlockKind::Light => {
            let spec = item.light.clone().unwrap_or_default();
            let color = body_color(project, item);
            for band in figures::beam_bands(
                (item.x, item.y),
                item.rot,
                spec.beam_deg,
                spec.reach_m,
                figures::band_count(spec.fixture),
                spec.intensity,
            ) {
                let pts: Vec<_> = band.points.iter().map(|q| to_page(*q)).collect();
                pen.poly(&pts, Some((color, (band.alpha * alpha).min(1.0))), None);
            }
        }
        BlockKind::Mic => {
            let spec = item.mic.clone().unwrap_or_default();
            for lobe in figures::polar_lobes((item.x, item.y), item.rot, spec.pattern, spec.reach_m)
            {
                let pts: Vec<_> = lobe.points.iter().map(|q| to_page(*q)).collect();
                let k = if lobe.rear { 0.5 } else { 1.0 };
                pen.poly(&pts, Some((SOUND, 0.2 * k * alpha)), Some((SOUND, 0.6)));
            }
        }
        _ => {}
    }
}

/// Un nombre para la leyenda de cada tipo de figura que hay en la planta.
fn legend_name(item: &BlockItem) -> String {
    match item.kind {
        BlockKind::Actor => "Persona".into(),
        BlockKind::Object => if item.shape == crate::model::ObjectShape::Round {
            "Objeto redondo"
        } else {
            "Mueble u objeto"
        }
        .into(),
        BlockKind::Wall => "Pared".into(),
        BlockKind::Opening => if item.opening == OpeningKind::Window {
            "Ventana"
        } else {
            "Puerta"
        }
        .into(),
        BlockKind::Camera => "Cámara".into(),
        BlockKind::Light => item
            .light
            .as_ref()
            .map_or("Luz", |l| l.fixture.label())
            .to_string(),
        BlockKind::Modifier => item
            .modifier
            .as_ref()
            .map_or("Modificador", |m| m.kind.label())
            .to_string(),
        BlockKind::Mic => item
            .mic
            .as_ref()
            .map_or("Micrófono", |m| m.kind.label())
            .to_string(),
    }
}

/// El PDF de las plantas pedidas (sin pedir dónde guardarlo): por cada bloqueo, una hoja por planta marcada o una sola con
/// todas. La escenografía (paredes, aberturas, muebles y personas) va en todas.
pub(super) fn plan_pdf(
    project: &Project,
    opts: &PlanExport,
    paper: crate::storage::Paper,
) -> Result<Vec<u8>, String> {
    let blockings: Vec<&crate::model::Blocking> = project
        .blockings
        .iter()
        .filter(|b| opts.blocking.is_none_or(|id| b.id == id))
        .collect();
    if opts.blocking.is_some() && blockings.is_empty() {
        return Err("La planta ya no existe.".to_string());
    }
    let selected: Vec<Layer> = [Layer::Camera, Layer::Lights, Layer::Sound]
        .into_iter()
        .zip(opts.layers)
        .filter_map(|(l, on)| on.then_some(l))
        .collect();
    if selected.is_empty() {
        return Err("Marcá al menos una planta para exportar.".to_string());
    }
    let groups: Vec<Vec<Layer>> = if opts.combine {
        vec![selected]
    } else {
        selected.into_iter().map(|l| vec![l]).collect()
    };
    let mut pages = Vec::new();
    for b in blockings {
        for layers in &groups {
            if let Some(page) = plan_page(project, b, layers, opts, paper) {
                pages.push(page);
            }
        }
    }
    if pages.is_empty() {
        return Err("No hay nada para exportar: las plantas están vacías.".to_string());
    }
    crate::pdf::render_vector_pages(&project.title, &project.author, pages)
}

/// Una hoja: un bloqueo con las plantas `layers`. `None` si no tiene nada propio de ellas.
fn plan_page<'a>(
    project: &'a Project,
    b: &'a crate::model::Blocking,
    layers: &[Layer],
    opts: &PlanExport,
    paper: crate::storage::Paper,
) -> Option<crate::pdf::VectorPage<'a>> {
    let layers: Vec<Layer> = layers.to_vec();
    let (cones, legend, blocking) = (opts.cones, opts.legend, b.id);
    let items: Vec<&BlockItem> = b
        .items
        .iter()
        .filter(|i| !i.hidden && role_in(&layers, i.kind).is_some())
        .collect();
    // Sin nada propio de esas plantas, la hoja serían solo paredes.
    let own = items.iter().any(|i| {
        layers.iter().any(|l| l.owns(i.kind)) && (!is_base(i.kind) || layers.contains(&Layer::Camera))
    });
    if !own {
        return None;
    }
    let sheet = paper.sheet();
    let (w, h) = (sheet.width * 72.0, sheet.height * 72.0);
    let margin = 36.0;
    let codes = project.shot_codes();
    let names: HashMap<u64, String> = project
        .characters
        .iter()
        .map(|c| (c.id, clean_character_name(&c.name)))
        .collect();
    let name_of = move |i: &BlockItem| -> String {
        match i.kind {
            BlockKind::Actor => i
                .character
                .and_then(|c| names.get(&c).cloned())
                .unwrap_or_else(|| i.label.clone()),
            _ => i.label.clone(),
        }
    };

    // Lo que va en la hoja de abajo: la leyenda y la tabla de posiciones (con tope, para que entren).
    let mut legend_kinds: Vec<String> = Vec::new();
    for i in &items {
        let n = legend_name(i);
        if !legend_kinds.contains(&n) {
            legend_kinds.push(n);
        }
    }
    let cameras: Vec<&BlockItem> = items
        .iter()
        .copied()
        .filter(|i| i.kind == BlockKind::Camera)
        .collect();
    let moving: Vec<&BlockItem> = items.iter().copied().filter(|i| !i.keys.is_empty()).collect();
    let rows: usize = moving.iter().map(|i| i.keys.len() + 1).sum();
    let max_rows = (((h * 0.22) / 9.0) as usize).max(4);
    let shown_rows = rows.min(max_rows);
    let columns = 2usize;
    let legend_lines = legend_kinds.len().div_ceil(columns);
    let legend_h = 26.0
        + if legend {
            legend_lines as f32 * 14.0
                + 14.0
                + cameras.len().max(1) as f32 * 10.0
                + if rows > 0 {
                    24.0 + shown_rows as f32 * 9.0
                } else {
                    0.0
                }
        } else {
            0.0
        };

    // Se ajusta todo lo dibujado a la hoja: figuras, posiciones y lo que alcanzan sus conos, haces y patrones.
    let (mut min, mut max) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    let mut grow = |p: (f32, f32)| {
        min = (min.0.min(p.0), min.1.min(p.1));
        max = (max.0.max(p.0), max.1.max(p.1));
    };
    for item in &items {
        for part in figures::figure(item, 1.0) {
            part.points.iter().for_each(|q| grow(*q));
        }
        for pos in motion::positions(item) {
            grow((pos.x, pos.y));
        }
        if let Some(reach) = plan_checks::reach_of(item) {
            for a in [0.0f32, 90.0, 180.0, 270.0] {
                let v = rotate((reach, 0.0), a);
                grow((item.x + v.0, item.y + v.1));
            }
        }
    }
    let (bw, bh) = ((max.0 - min.0).max(1.0), (max.1 - min.1).max(1.0));
    let scale = ((w - margin * 2.0) / bw).min((h - margin * 2.0 - legend_h - 20.0) / bh);
    let to_page = move |(x, y): (f32, f32)| {
        (
            margin + (x - min.0) * scale,
            margin + 20.0 + (y - min.1) * scale,
        )
    };
    let title = format!(
        "{} — {} · {}",
        project.title,
        if layers.len() > 1 { "Todas las plantas" } else { layer_title(layers[0]) },
        b.name
    );
    let fov = |i: &BlockItem| project.camera_fov(i);

    let draw = move |surface: &mut krilla::surface::Surface, regular: &krilla::text::Font, bold: &krilla::text::Font| {
        let mut pen = crate::pdf::Pen {
            surface,
            regular,
            bold,
        };
        pen.text((margin, margin - 8.0), &title, 11.0, true);
        let posed: Vec<BlockItem> = items
            .iter()
            .map(|i| BlockItem {
                fov: fov(i),
                ..(**i).clone()
            })
            .collect();
        let kinds_in_order = [
            BlockKind::Wall,
            BlockKind::Object,
            BlockKind::Opening,
            BlockKind::Actor,
            BlockKind::Camera,
            BlockKind::Light,
            BlockKind::Modifier,
            BlockKind::Mic,
        ];
        // 1. La base.
        for item in posed.iter().filter(|i| {
            matches!(
                i.kind,
                BlockKind::Wall | BlockKind::Object | BlockKind::Opening
            )
        }) {
            draw_parts(
                &mut pen,
                &figures::figure(item, 1.0),
                body_color(project, item),
                1.0,
                &to_page,
            );
        }
        // 2. La cobertura.
        for item in posed.iter().filter(|i| {
            matches!(
                i.kind,
                BlockKind::Camera | BlockKind::Light | BlockKind::Mic
            )
        }) {
            draw_coverage(
                &mut pen,
                project,
                item,
                role_in(&layers, item.kind).unwrap_or(1.0),
                cones,
                &to_page,
            );
        }
        // 3. Los recorridos, con flechas.
        for item in posed.iter().filter(|i| !i.keys.is_empty()) {
            let color = darker(body_color(project, item));
            let path: Vec<_> = motion::path_points(item).into_iter().map(to_page).collect();
            dashed(&mut pen, &path, color, 0.8);
            for (at, angle) in motion::path_arrows(item) {
                let c = to_page(at);
                let dir = (angle.to_radians().cos(), angle.to_radians().sin());
                let side = (-dir.1, dir.0);
                let tri = [
                    (c.0 + dir.0 * 4.0, c.1 + dir.1 * 4.0),
                    (
                        c.0 - dir.0 * 3.0 + side.0 * 3.0,
                        c.1 - dir.1 * 3.0 + side.1 * 3.0,
                    ),
                    (
                        c.0 - dir.0 * 3.0 - side.0 * 3.0,
                        c.1 - dir.1 * 3.0 - side.1 * 3.0,
                    ),
                ];
                pen.poly(&tri, Some((color, 1.0)), None);
            }
        }
        // 4. Los fantasmas numerados de cada posición (la 1 es el inicio).
        for item in &posed {
            let all = motion::positions(item);
            let tag: String = match item.kind {
                BlockKind::Actor => name_of(item)
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .collect::<String>()
                    .to_uppercase(),
                _ => item.label.chars().take(3).collect(),
            };
            for pos in &all {
                let ghost = BlockItem {
                    x: pos.x,
                    y: pos.y,
                    rot: pos.rot,
                    keys: Vec::new(),
                    ..item.clone()
                };
                draw_parts(
                    &mut pen,
                    &figures::figure(&ghost, 1.0),
                    body_color(project, item),
                    0.35 * role_in(&layers, item.kind).unwrap_or(1.0),
                    &to_page,
                );
                let at = to_page((pos.x, pos.y));
                pen.text(
                    (at.0 + 4.0, at.1 - 9.0),
                    &motion::badge_text(&tag, pos),
                    6.5,
                    true,
                );
            }
        }
        // 5. Las figuras en su lugar de partida, con su nombre.
        for kind in kinds_in_order {
            for item in posed.iter().filter(|i| {
                i.kind == kind
                    && !matches!(
                        kind,
                        BlockKind::Wall | BlockKind::Object | BlockKind::Opening
                    )
            }) {
                let alpha = role_in(&layers, kind).unwrap_or(1.0);
                draw_parts(
                    &mut pen,
                    &figures::figure(item, 1.0),
                    body_color(project, item),
                    alpha,
                    &to_page,
                );
                let at = to_page((item.x, item.y));
                let name = name_of(item);
                let text = match kind {
                    BlockKind::Light => format!("{} · {} W", name, item.watts),
                    BlockKind::Mic => format!("{} · pista {}", name, item.track),
                    _ => name,
                };
                pen.text(
                    (at.0 - 10.0, at.1 + 0.35 * scale + 2.0),
                    &text,
                    if kind == BlockKind::Camera { 9.0 } else { 7.0 },
                    kind == BlockKind::Camera,
                );
            }
        }

        // --- La leyenda.
        let mut y = h - margin - legend_h + 6.0;
        let bar = (margin, y);
        pen.line(bar, (bar.0 + scale, bar.1), [0, 0, 0], 1.5);
        pen.text((bar.0 + scale + 6.0, bar.1 - 4.0), "1 m", 8.0, false);
        if !legend {
            return;
        }
        y += 14.0;
        pen.text((margin, y), "Leyenda", 9.0, true);
        y += 13.0;
        let column_w = (w - 2.0 * margin) / columns as f32;
        for (n, name) in legend_kinds.iter().enumerate() {
            let sample = items.iter().find(|i| legend_name(i) == *name).copied();
            let (col, row) = (n % columns, n / columns);
            let at = (margin + col as f32 * column_w, y + row as f32 * 14.0);
            if let Some(item) = sample {
                let local = figures::local_figure(&BlockItem {
                    x: 0.0,
                    y: 0.0,
                    rot: 0.0,
                    keys: Vec::new(),
                    ..item.clone()
                });
                let extent = local
                    .iter()
                    .flat_map(|p| p.points.iter())
                    .fold(0.01f32, |m, q| m.max(q.0.abs()).max(q.1.abs()))
                    * 2.0;
                let k = 10.0 / extent;
                let centre = (at.0 + 7.0, at.1 + 5.0);
                let parts: Vec<Part> = local
                    .into_iter()
                    .map(|mut p| {
                        p.points
                            .iter_mut()
                            .for_each(|q| *q = (centre.0 + q.0 * k, centre.1 + q.1 * k));
                        p
                    })
                    .collect();
                draw_parts(&mut pen, &parts, body_color(project, item), 1.0, &|q| q);
            }
            pen.text((at.0 + 18.0, at.1 + 1.0), name, 7.5, false);
        }
        y += legend_lines as f32 * 14.0 + 6.0;
        pen.text((margin, y), "Planos por cámara", 9.0, true);
        y += 12.0;
        if cameras.is_empty() {
            pen.text((margin, y), "No hay cámaras en esta planta.", 8.0, false);
            y += 10.0;
        }
        for item in &cameras {
            let shots: Vec<String> = project
                .shots
                .iter()
                .filter(|s| s.blocking == Some(blocking) && s.camera == Some(item.id))
                .map(|s| codes.get(&s.id).cloned().unwrap_or_default())
                .collect();
            let line = format!(
                "Cámara {}: {}",
                item.label,
                if shots.is_empty() {
                    "sin planos".to_string()
                } else {
                    shots.join(", ")
                }
            );
            pen.text((margin, y), &line, 8.0, false);
            y += 10.0;
        }
        // --- La tabla de posiciones: una fila por posición, en la letra de ancho fijo del documento.
        if rows > 0 {
            y += 8.0;
            pen.text((margin, y), "Posiciones", 9.0, true);
            y += 12.0;
            pen.text(
                (margin, y),
                &format!(
                    "{:<16} {:>3} {:>7}  {:<28} {}",
                    "Elemento", "N.º", "Tiempo", "Marca", "Pausa"
                ),
                7.0,
                true,
            );
            y += 9.0;
            let mut left = shown_rows.saturating_sub(1);
            'table: for item in &moving {
                for pos in motion::positions(item) {
                    if left == 0 {
                        pen.text(
                            (margin, y),
                            &format!("… y {} posiciones más", rows - shown_rows),
                            7.0,
                            false,
                        );
                        break 'table;
                    }
                    let name: String = name_of(item).chars().take(16).collect();
                    let note: String = pos.label.chars().take(28).collect();
                    let hold = motion::hold_text(&pos).unwrap_or_default();
                    pen.text(
                        (margin, y),
                        &format!(
                            "{:<16} {:>3} {:>7}  {:<28} {}",
                            name,
                            pos.number,
                            motion::seconds_text(pos.t),
                            note,
                            hold
                        ),
                        7.0,
                        false,
                    );
                    y += 9.0;
                    left -= 1;
                }
            }
        }
    };
    Some(crate::pdf::VectorPage {
        width: w,
        height: h,
        draw: Box::new(draw),
    })
}
