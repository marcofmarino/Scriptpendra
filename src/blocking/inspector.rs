//! La pestaña SELECCIÓN del inspector de las plantas (docs/plantas.md §7): los campos de cada tipo en filas de propiedad y
//! campos apilados del kit, la lista de posiciones y, con varias cosas elegidas, solo los campos en común (con «—» donde los
//! valores difieren). Cada control anota lo que cambió como una edición que después se aplica a todo lo elegido.

use super::*;
use crate::model::Role;

/// Un cambio que se aplica a cada cosa elegida.
type Edit = Box<dyn Fn(&mut BlockItem)>;

/// Lo que pide la pestaña y toca al proyecto, no a una cosa.
enum Action {
    AddBody(&'static str, f32),
    LinkShot { shot: u64, on: bool },
    /// Registrar esta cosa como un equipo de la lista de su rol y vincularla.
    Register { role: Role, kind: &'static str, name: String },
}

/// Cómo se sostiene una cámara.
const SUPPORTS: [&str; 7] = [
    "Trípode", "Mano", "Gimbal", "Slider", "Dolly", "Grúa", "Dron",
];

/// A qué lista de equipos pertenece una cosa de la planta, con su nombre y los tipos de fila que le corresponden.
fn equipment_kind(kind: BlockKind) -> Option<(Role, &'static str, &'static [&'static str])> {
    match kind {
        BlockKind::Mic => Some((
            Role::SoundGear,
            "Micrófono",
            &["Micrófono", "Boom", "Inalámbrico"],
        )),
        BlockKind::Light => Some((Role::PhotoGear, "Luz", &["Luz"])),
        BlockKind::Modifier => Some((Role::PhotoGear, "Modificador", &["Modificador"])),
        BlockKind::Camera => Some((Role::PhotoGear, "Cámara", &["Cámara"])),
        // Un mueble u objeto único: el mismo sillón en varias escenas del mismo lugar es un solo elemento de arte.
        BlockKind::Object => Some((
            Role::Art,
            "Mueble u objeto",
            &["Escenografía", "Utilería", "Vestuario", "Vehículo", "Maquillaje", "Otro"],
        )),
        _ => None,
    }
}

/// El valor que comparten todas las cosas elegidas, o `None` si difieren.
pub(super) fn common<T: PartialEq>(
    items: &[BlockItem],
    get: impl Fn(&BlockItem) -> T,
) -> Option<T> {
    let first = get(items.first()?);
    items.iter().all(|i| get(i) == first).then_some(first)
}

fn small_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(tokens::FONT_SMALL).color(p.dim)).truncate());
}

/// Un número de ancho `W_NUM` en una fila de propiedad; con valores distintos muestra «—».
#[allow(clippy::too_many_arguments)]
fn num(
    ui: &mut Ui,
    p: &Palette,
    edits: &mut Vec<Edit>,
    items: &[BlockItem],
    label: &str,
    get: impl Fn(&BlockItem) -> f32,
    set: impl Fn(&mut BlockItem, f32) + Copy + 'static,
    range: std::ops::RangeInclusive<f32>,
    suffix: &'static str,
    decimals: usize,
) {
    let shared = common(items, |i| (get(i) * 1000.0).round() as i64);
    let mixed = shared.is_none();
    let mut value = shared.map_or_else(|| get(&items[0]), |v| v as f32 / 1000.0);
    kit::prop_row(ui, p, label, |ui| {
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.x = tokens::W_NUM;
            let speed = ((range.end() - range.start()) / 200.0).max(0.01);
            let r = ui.add(
                egui::DragValue::new(&mut value)
                    .range(range.clone())
                    .speed(speed)
                    .custom_formatter(move |n, _| {
                        if mixed {
                            "—".to_string()
                        } else {
                            format!("{n:.decimals$}{suffix}").replace('.', ",")
                        }
                    })
                    .custom_parser(move |s| {
                        s.trim()
                            .trim_end_matches(suffix.trim())
                            .trim()
                            .replace(',', ".")
                            .parse()
                            .ok()
                    }),
            );
            if r.changed() {
                let v = value;
                edits.push(Box::new(move |i| set(i, v)));
            }
        });
    });
}

/// Un deslizador con su valor y su nota (si es una sola cosa); con varias, un número con «—».
#[allow(clippy::too_many_arguments)]
fn slide(
    ui: &mut Ui,
    p: &Palette,
    edits: &mut Vec<Edit>,
    items: &[BlockItem],
    label: &str,
    get: impl Fn(&BlockItem) -> f32 + Copy,
    set: impl Fn(&mut BlockItem, f32) + Copy + 'static,
    range: std::ops::RangeInclusive<f32>,
    unit: &'static str,
    decimals: usize,
    note: Option<&str>,
) {
    if items.len() == 1 {
        let mut value = get(&items[0]);
        if kit::slider_value(ui, p, label, &mut value, range, unit, decimals, note) {
            edits.push(Box::new(move |i| set(i, value)));
        }
    } else {
        let suffix: &'static str = if unit.is_empty() {
            ""
        } else if unit.starts_with('°') {
            "°"
        } else {
            Box::leak(format!(" {unit}").into_boxed_str())
        };
        num(
            ui, p, edits, items, label, get, set, range, suffix, decimals,
        );
    }
}

/// Un desplegable a todo el ancho con la etiqueta arriba; con valores distintos muestra «—».
#[allow(clippy::too_many_arguments)]
fn pick(
    ui: &mut Ui,
    p: &Palette,
    edits: &mut Vec<Edit>,
    items: &[BlockItem],
    label: &str,
    salt: &'static str,
    options: &[String],
    get: fn(&BlockItem) -> Option<usize>,
    set: fn(&mut BlockItem, usize),
) {
    let shared = common(items, get);
    let shown = match shared {
        Some(Some(n)) => options.get(n).cloned().unwrap_or_default(),
        Some(None) => "Sin elegir".to_string(),
        None => "—".to_string(),
    };
    kit::combo_field(ui, p, label, salt, &shown, |ui| {
        for (n, option) in options.iter().enumerate() {
            if ui
                .selectable_label(shared == Some(Some(n)), option)
                .clicked()
            {
                edits.push(Box::new(move |i| set(i, n)));
            }
        }
    });
}

/// Un interruptor; con valores distintos queda indeterminado.
fn toggle(
    ui: &mut Ui,
    p: &Palette,
    edits: &mut Vec<Edit>,
    items: &[BlockItem],
    label: &str,
    get: fn(&BlockItem) -> bool,
    set: fn(&mut BlockItem, bool),
) {
    let shared = common(items, get);
    let mut on = shared.unwrap_or(false);
    kit::prop_row(ui, p, label, |ui| {
        let r = ui.add(egui::Checkbox::without_text(&mut on).indeterminate(shared.is_none()));
        if r.changed() {
            let v = on;
            edits.push(Box::new(move |i| set(i, v)));
        }
    });
}

/// El selector de patrón de un micrófono: cada uno con el dibujo de su curva polar, apuntando hacia arriba.
fn pattern_picker(ui: &mut Ui, p: &Palette, edits: &mut Vec<Edit>, items: &[BlockItem]) {
    let shared = common(items, |i| i.mic.as_ref().map(|m| m.pattern));
    small_label(ui, p, "Patrón");
    ui.horizontal_wrapped(|ui| {
        for pattern in MicPattern::ALL {
            let (rect, resp) = ui.allocate_exact_size(vec2(48.0, 48.0), egui::Sense::click());
            let chosen = shared == Some(Some(pattern));
            let hover = kit::fade(ui, resp.id.with("hover"), resp.hovered(), tokens::ANIM_FAST);
            ui.painter().rect(
                rect,
                tokens::R_CONTROL,
                theme::mix(p.field, p.container_hi, hover),
                Stroke::new(
                    if chosen { 2.0 } else { 1.0 },
                    if chosen { p.accent } else { p.border },
                ),
                egui::StrokeKind::Inside,
            );
            let centre = rect.center() + vec2(0.0, 6.0);
            let reach = 17.0;
            for lobe in figures::polar_lobes((0.0, 0.0), -90.0, pattern, 1.0) {
                let points: Vec<egui::Pos2> = lobe
                    .points
                    .iter()
                    .map(|q| centre + vec2(q.0, q.1) * reach)
                    .collect();
                let k = if lobe.rear { 0.5 } else { 1.0 };
                if points.len() >= 3 {
                    let mut mesh = egui::Mesh::default();
                    for q in &points {
                        mesh.colored_vertex(*q, p.plan_sound.gamma_multiply(0.45 * k));
                    }
                    for n in 1..points.len() as u32 - 1 {
                        mesh.add_triangle(0, n, n + 1);
                    }
                    ui.painter().add(egui::Shape::mesh(mesh));
                }
                ui.painter().add(egui::Shape::closed_line(
                    points,
                    Stroke::new(1.0, p.plan_sound.gamma_multiply(k)),
                ));
            }
            ui.painter().circle_filled(centre, 2.0, p.plan_sound);
            if resp.on_hover_text(pattern.label()).clicked() {
                edits.push(Box::new(move |i| {
                    if let Some(m) = i.mic.as_mut() {
                        m.pattern = pattern;
                    }
                }));
            }
        }
    });
    if let Some(Some(pattern)) = shared {
        ui.label(
            RichText::new(pattern.label())
                .size(tokens::FONT_SMALL)
                .color(p.dim),
        );
    } else if shared.is_none() {
        ui.label(RichText::new("—").size(tokens::FONT_SMALL).color(p.dim));
    }
}

/// La vista previa del ángulo vertical de una cámara: una cuña que sube o baja desde la cámara.
fn tilt_preview(ui: &mut Ui, p: &Palette, fov: f32, tilt: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, tokens::R_CONTROL, p.field);
    let origin = egui::pos2(rect.left() + tokens::SP_L, rect.center().y);
    let len = rect.width() - 2.0 * tokens::SP_L;
    // La apertura vertical sale del ángulo de visión de la lente (con un cuadro de 16:9): al cambiar de lente, cambia la cuña.
    let vertical = 2.0 * ((fov.clamp(5.0, 170.0) / 2.0).to_radians().tan() * 9.0 / 16.0).atan();
    let half = (vertical / 2.0).clamp(0.03, 1.4);
    let a = tilt.to_radians();
    let tip = |off: f32| origin + vec2((a + off).cos(), -(a + off).sin()) * len;
    painter.add(egui::Shape::convex_polygon(
        vec![origin, tip(-half), tip(half)],
        p.plan_camera.gamma_multiply(0.25),
        Stroke::new(1.0, p.plan_camera.gamma_multiply(0.7)),
    ));
    painter.line_segment([origin, tip(0.0)], Stroke::new(1.5, p.plan_camera));
    painter.line_segment(
        [
            egui::pos2(rect.left(), origin.y),
            egui::pos2(rect.right(), origin.y),
        ],
        Stroke::new(1.0, p.border),
    );
    painter.circle_filled(origin, 3.0, p.plan_camera);
}

/// El ángulo de visión de una cámara con la lente que tiene en el instante `t`.
pub(super) fn camera_fov(project: &Project, item: &BlockItem, t: f32) -> f32 {
    project.camera_fov(&BlockItem {
        lens_mm: item.lens_at(t),
        ..item.clone()
    })
}

/// Las posiciones guardadas de lo elegido, en una caja con su propio desplazamiento: crece hasta cierto alto y después se
/// desplaza, y va al final de la pestaña, así que no mueve nada de lo que está arriba. Devuelve si cambió el proyecto.
fn positions_box(
    ui: &mut Ui,
    state: &mut BlockState,
    project: &mut Project,
    p: &Palette,
    blocking: u64,
) -> bool {
    let Some(item) = state
        .selected
        .filter(|_| state.multi.len() == 1)
        .and_then(|id| {
            project
                .blockings
                .iter()
                .find(|b| b.id == blocking)
                .and_then(|b| b.items.iter().find(|i| i.id == id))
        })
        .filter(|i| i.animatable())
        .cloned()
    else {
        return false;
    };
    let time = state.time;
    let mut changed = false;
    kit::section(ui, p, "Posiciones", None);
    let all = motion::positions(&item);
    let mut goto: Option<f32> = None;
    let mut label: Option<(usize, String)> = None;
    let mut delete: Option<Option<usize>> = None;
    egui::Frame::new()
        .fill(p.field)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(tokens::R_CONTROL)
        .inner_margin(tokens::SP_S)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if all.is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new("Todavía no tiene posiciones. Con el tiempo en otro instante, movelo o apretá K.")
                            .size(tokens::FONT_SMALL)
                            .color(p.dim),
                    )
                    .wrap(),
                );
            }
            egui::ScrollArea::vertical()
                .id_salt("plant_positions_box")
                .max_height(6.0 * tokens::ROW_H)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for pos in &all {
                        kit::sides(
                            ui,
                            |ui| {
                                let text = format!("{} · {}", pos.number, motion::seconds_text(pos.t));
                                let current = (time - pos.t).abs() < crate::model::KEY_EPSILON;
                                if ui.add(egui::Button::selectable(current, text)).clicked() {
                                    goto = Some(pos.t);
                                }
                                if let Some(n) = pos.index {
                                    let mut text = pos.label.clone();
                                    let width = (kit::fill_width(ui, 60.0, 400.0) - tokens::ICON_BTN - tokens::SP_S).max(40.0);
                                    if ui
                                        .add(TextEdit::singleline(&mut text).desired_width(width).hint_text("Marca"))
                                        .changed()
                                    {
                                        label = Some((n, text));
                                    }
                                } else {
                                    ui.label(RichText::new("inicio").size(tokens::FONT_SMALL).color(p.dim));
                                }
                            },
                            |ui| {
                                if kit::icon_button(ui, p, icon::TRASH, "Borrar esta posición", true).clicked() {
                                    delete = Some(pos.index);
                                }
                            },
                        );
                    }
                });
        });
    if time >= crate::model::KEY_EPSILON
        && item.key_at(time).is_none()
        && ui
            .button(format!(
                "{}  Fijar posición en {}",
                icon::PLUS,
                motion::seconds_text(time)
            ))
            .clicked()
        && let Some(i) = project
            .blockings
            .iter_mut()
            .find(|b| b.id == blocking)
            .and_then(|b| b.items.iter_mut().find(|i| i.id == item.id))
    {
        let (x, y, rot) = i.pose_at(time);
        i.set_pose_at(time, x, y, rot);
        changed = true;
    }
    if let Some(t) = goto {
        state.go_to_time(project, item.id, t);
    }
    if let Some(i) = project
        .blockings
        .iter_mut()
        .find(|b| b.id == blocking)
        .and_then(|b| b.items.iter_mut().find(|i| i.id == item.id))
    {
        if let Some((n, text)) = label
            && let Some(k) = i.keys.get_mut(n)
        {
            k.label = text;
            changed = true;
        }
        if let Some(index) = delete {
            // Borrar la posición de partida deja a la siguiente en su lugar.
            let t = index.and_then(|n| i.keys.get(n)).map_or(0.0, |k| k.t);
            changed |= i.delete_position_at(t);
            state.sel_pose = None;
        }
    }
    changed
}

/// La pestaña SELECCIÓN. Devuelve si cambió el proyecto.
pub(super) fn selection_tab(
    ui: &mut Ui,
    state: &mut BlockState,
    project: &mut Project,
    p: &Palette,
) -> bool {
    let Some(blocking) = state.blocking else {
        return false;
    };
    state.sync_multi();
    let ids: Vec<u64> = state.multi.clone();
    let items: Vec<BlockItem> = project
        .blockings
        .iter()
        .find(|b| b.id == blocking)
        .map(|b| {
            b.items
                .iter()
                .filter(|i| ids.contains(&i.id))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if items.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::ARROWS_OUT_CARDINAL,
            "Elegí algo del lienzo para ver sus propiedades. Arrastrá para moverlo, usá el círculo para girarlo y los cuadrados para cambiar su tamaño.",
            None,
        );
        return false;
    }
    let time = state.time;
    let auto = state.auto_record;
    let playing = state.playing;
    let kind = common(&items, |i| i.kind);
    let single = items.len() == 1;
    let mut edits: Vec<Edit> = Vec::new();
    let mut actions: Vec<Action> = Vec::new();

    // --- Quién es.
    kit::section(
        ui,
        p,
        if single {
            kind_name(items[0].kind)
        } else {
            "Varias cosas"
        },
        None,
    );
    if single {
        let mut label = items[0].label.clone();
        let hint = if items[0].kind == BlockKind::Camera {
            "Letra o nombre"
        } else {
            "Nombre"
        };
        if kit::field(ui, p, hint, &mut label, 1) {
            edits.push(Box::new(move |i| i.label = label.clone()));
        }
        // El equipo registrado: el mismo micrófono, luz o cámara se usa en distintos lugares y escenas.
        if let Some((role, kind_name, kinds)) = equipment_kind(items[0].kind) {
            let current = items[0].equipment;
            let entries: Vec<(u64, String)> = project
                .role_entries(role)
                .iter()
                .filter(|e| e.plan_key.is_none() && kinds.contains(&e.kind.as_str()))
                .map(|e| (e.id, if e.title.trim().is_empty() { e.kind.clone() } else { e.title.trim().to_string() }))
                .collect();
            let shown = current
                .and_then(|id| entries.iter().find(|(e, _)| *e == id))
                .map_or("Sin registrar".to_string(), |(_, n)| n.clone());
            kit::combo_field(ui, p, if role == Role::Art { "Elemento de arte" } else { "Equipo registrado" }, "block_equipment", &shown, |ui| {
                if ui.selectable_label(current.is_none(), "Sin registrar").clicked() {
                    edits.push(Box::new(|i| i.equipment = None));
                }
                for (id, name) in &entries {
                    if ui.selectable_label(current == Some(*id), name).clicked() {
                        let id = *id;
                        edits.push(Box::new(move |i| i.equipment = Some(id)));
                    }
                }
                ui.separator();
                if ui.button(if role == Role::Art { format!("{}  Registrar como elemento único", icon::PLUS) } else { format!("{}  Registrar como equipo nuevo", icon::PLUS) }).clicked() {
                    let name = if items[0].label.trim().is_empty() {
                        kind_name.to_string()
                    } else {
                        items[0].label.trim().to_string()
                    };
                    actions.push(Action::Register { role, kind: kinds[0], name });
                    ui.close();
                }
            });
        }
    } else {
        ui.label(
            RichText::new(format!(
                "{} elegidas. Solo se muestran los campos que tienen en común.",
                items.len()
            ))
            .size(tokens::FONT_SMALL)
            .color(p.dim),
        );
    }

    let characters: Vec<(u64, String)> = project
        .characters
        .iter()
        .map(|c| (c.id, clean_character_name(&c.name)))
        .collect();
    let bodies = project.camera_bodies.clone();

    match kind {
        Some(BlockKind::Actor) => {
            let mut names = vec!["Sin vincular".to_string()];
            names.extend(characters.iter().map(|c| c.1.clone()));
            let ids_of: Vec<u64> = characters.iter().map(|c| c.0).collect();
            let pick_ids = ids_of.clone();
            let shared = common(&items, |i| i.character);
            let shown = match shared {
                Some(None) => "Sin vincular".to_string(),
                Some(Some(id)) => characters
                    .iter()
                    .find(|c| c.0 == id)
                    .map_or("Sin vincular".to_string(), |c| c.1.clone()),
                None => "—".to_string(),
            };
            kit::combo_field(ui, p, "Personaje", "block_character", &shown, |ui| {
                if ui
                    .selectable_label(shared == Some(None), "Sin vincular")
                    .clicked()
                {
                    edits.push(Box::new(|i| i.character = None));
                }
                for (n, name) in names.iter().enumerate().skip(1) {
                    let id = pick_ids[n - 1];
                    if ui
                        .selectable_label(shared == Some(Some(id)), name)
                        .clicked()
                    {
                        edits.push(Box::new(move |i| i.character = Some(id)));
                    }
                }
            });
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Tamaño",
                |i| i.w,
                |i, v| (i.w, i.h) = (v, v),
                0.3..=1.2,
                " m",
                1,
            );
        }
        Some(BlockKind::Object) => {
            use crate::model::ObjectShape;
            fn all_shapes() -> Vec<ObjectShape> {
                [ObjectShape::Rect, ObjectShape::Round]
                    .into_iter()
                    .chain(ObjectShape::FURNITURE.iter().map(|(s, ..)| *s))
                    .collect()
            }
            let shapes: Vec<String> = all_shapes().iter().map(|s| s.label().to_string()).collect();
            pick(
                ui,
                p,
                &mut edits,
                &items,
                "Forma",
                "block_shape",
                &shapes,
                |i| all_shapes().iter().position(|s| *s == i.shape),
                |i, n| {
                    let shape = all_shapes()[n];
                    i.shape = shape;
                    // El nombre sigue al mueble mientras sea el de fábrica.
                    if let Some(name) = shape.furniture_name()
                        && (i.label.is_empty() || ObjectShape::FURNITURE.iter().any(|(_, n, ..)| *n == i.label))
                    {
                        i.label = name.to_string();
                    }
                },
            );
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Ancho",
                |i| i.w,
                |i, v| i.w = v,
                0.2..=30.0,
                " m",
                2,
            );
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Alto",
                |i| i.h,
                |i, v| i.h = v,
                0.2..=30.0,
                " m",
                2,
            );
        }
        Some(BlockKind::Wall) => {
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Largo",
                |i| i.w,
                |i, v| i.w = v,
                0.25..=60.0,
                " m",
                2,
            );
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Grosor",
                |i| i.h,
                |i, v| i.h = v,
                0.1..=1.0,
                " m",
                2,
            );
        }
        Some(BlockKind::Opening) => {
            let kinds = vec!["Puerta".to_string(), "Ventana".to_string()];
            pick(
                ui,
                p,
                &mut edits,
                &items,
                "Tipo",
                "block_opening",
                &kinds,
                |i| Some(usize::from(i.opening == OpeningKind::Window)),
                |i, n| {
                    i.opening = if n == 1 {
                        OpeningKind::Window
                    } else {
                        OpeningKind::Door
                    }
                },
            );
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Ancho",
                |i| i.w,
                |i, v| i.w = v,
                0.4..=6.0,
                " m",
                2,
            );
        }
        Some(BlockKind::Camera) => {
            kit::section(ui, p, "Configuración de cámara", None);
            let names: Vec<String> = std::iter::once("Full frame (36 mm)".to_string())
                .chain(bodies.iter().map(|b| b.name.clone()))
                .collect();
            let body_ids: Vec<u64> = bodies.iter().map(|b| b.id).collect();
            let shared_body = common(&items, |i| i.body);
            let shown = match shared_body {
                Some(None) => "Full frame".to_string(),
                Some(Some(id)) => bodies
                    .iter()
                    .find(|b| b.id == id)
                    .map_or("Full frame".to_string(), |b| b.name.clone()),
                None => "—".to_string(),
            };
            kit::combo_field(ui, p, "Cuerpo (sensor)", "camera_body", &shown, |ui| {
                for (n, name) in names.iter().enumerate() {
                    let id = n.checked_sub(1).map(|k| body_ids[k]);
                    if ui.selectable_label(shared_body == Some(id), name).clicked() {
                        edits.push(Box::new(move |i| i.body = id));
                    }
                }
                ui.separator();
                small_label(ui, p, "Agregar al proyecto");
                for (name, crop) in crate::optics::BODY_PRESETS {
                    if ui
                        .selectable_label(false, format!("{name} (×{crop})"))
                        .clicked()
                    {
                        actions.push(Action::AddBody(name, crop));
                    }
                }
            });
            let legacy = items.iter().any(|i| i.lens_mm <= 0.0);
            if legacy && single {
                num(
                    ui,
                    p,
                    &mut edits,
                    &items,
                    "Ángulo de visión",
                    |i| i.fov,
                    |i, v| i.fov = v,
                    10.0..=120.0,
                    "°",
                    0,
                );
                if ui.button("Definir por lente (mm)").clicked() {
                    edits.push(Box::new(|i| {
                        i.lens_mm = (crate::optics::FULL_FRAME_MM
                            / 2.0
                            / (i.fov.clamp(10.0, 120.0) / 2.0).to_radians().tan())
                        .round()
                        .clamp(8.0, 600.0);
                    }));
                }
            } else {
                let note = single.then(|| {
                    let sensor = items[0]
                        .body
                        .and_then(|b| bodies.iter().find(|x| x.id == b))
                        .map_or(
                            crate::optics::FULL_FRAME_MM,
                            crate::optics::CameraBody::sensor_width,
                        );
                    format!(
                        "Eq. full frame: {}",
                        crate::optics::fmt_mm(crate::optics::equivalent_lens(
                            sensor,
                            items[0].lens_at(time)
                        ))
                    )
                });
                slide(
                    ui,
                    p,
                    &mut edits,
                    &items,
                    "Lente",
                    // La lente es la del instante que se ve: en otro que el 0, cambiarla fija el zoom de esa posición.
                    move |i| i.lens_at(time),
                    move |i, v| {
                        let at = edit_instant(i, time, auto);
                        i.set_lens_at(at, v)
                    },
                    8.0..=600.0,
                    "mm",
                    0,
                    note.as_deref(),
                );
            }
            let supports: Vec<String> = SUPPORTS.iter().map(|s| s.to_string()).collect();
            pick(
                ui,
                p,
                &mut edits,
                &items,
                "Soporte",
                "camera_support",
                &supports,
                |i| SUPPORTS.iter().position(|s| *s == i.support),
                |i, n| i.support = SUPPORTS[n].to_string(),
            );
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Altura",
                |i| i.cam_height_m,
                |i, v| i.cam_height_m = v,
                0.0..=10.0,
                "m",
                1,
                None,
            );
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Ángulo",
                |i| i.tilt_deg,
                |i, v| i.tilt_deg = v,
                -90.0..=90.0,
                "°",
                0,
                None,
            );
            tilt_preview(
                ui,
                p,
                camera_fov(project, &items[0], time),
                common(&items, |i| (i.tilt_deg * 10.0).round() as i32)
                    .map_or(0.0, |v| v as f32 / 10.0),
            );
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Largo del cono",
                |i| i.cone_len_m,
                |i, v| i.cone_len_m = v,
                0.5..=30.0,
                "m",
                1,
                None,
            );
            // Los planos de la escena: se vinculan con una casilla (solo con una cámara).
            if single {
                kit::section(ui, p, "Planos vinculados", None);
                let scene = project
                    .blockings
                    .iter()
                    .find(|b| b.id == blocking)
                    .map_or(0, |b| b.scene);
                let codes = project.shot_codes();
                let mut any = false;
                for shot in project.shots.iter().filter(|s| s.scene == scene) {
                    any = true;
                    let mut on =
                        shot.blocking == Some(blocking) && shot.camera == Some(items[0].id);
                    let label = format!(
                        "{} ({})",
                        codes.get(&shot.id).cloned().unwrap_or_default(),
                        shot.size
                    );
                    if ui.checkbox(&mut on, label).changed() {
                        actions.push(Action::LinkShot { shot: shot.id, on });
                    }
                }
                if !any {
                    ui.add(egui::Label::new(RichText::new("Esta escena todavía no tiene planos. Se crean en el guion técnico.").size(tokens::FONT_SMALL).color(p.dim)).wrap());
                }
            }
        }
        Some(BlockKind::Light) => {
            kit::section(ui, p, "Fuente de luz", None);
            let fixtures: Vec<String> = LightFixture::ALL
                .iter()
                .map(|f| f.label().to_string())
                .collect();
            pick(
                ui,
                p,
                &mut edits,
                &items,
                "Aparato",
                "light_fixture",
                &fixtures,
                |i| {
                    i.light
                        .as_ref()
                        .and_then(|l| LightFixture::ALL.iter().position(|f| *f == l.fixture))
                },
                |i, n| {
                    let fixture = LightFixture::ALL[n];
                    let spec = i.light.get_or_insert_with(Default::default);
                    spec.fixture = fixture;
                    (spec.beam_deg, spec.reach_m) = (fixture.beam_deg(), fixture.reach_m());
                    i.fov = spec.beam_deg;
                    (i.watts, i.kelvin) = fixture.watts_kelvin();
                },
            );
            let roles: Vec<String> = LightRole::ALL
                .iter()
                .map(|r| r.label().to_string())
                .collect();
            pick(
                ui,
                p,
                &mut edits,
                &items,
                "Función",
                "light_function",
                &roles,
                |i| {
                    i.light
                        .as_ref()
                        .and_then(|l| LightRole::ALL.iter().position(|r| *r == l.role))
                },
                |i, n| i.light.get_or_insert_with(Default::default).role = LightRole::ALL[n],
            );
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Apertura",
                |i| i.light.as_ref().map_or(0.0, |l| l.beam_deg),
                |i, v| {
                    i.light.get_or_insert_with(Default::default).beam_deg = v;
                    i.fov = v;
                },
                5.0..=360.0,
                "°",
                0,
                None,
            );
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Alcance",
                |i| i.light.as_ref().map_or(0.0, |l| l.reach_m),
                |i, v| i.light.get_or_insert_with(Default::default).reach_m = v,
                0.5..=20.0,
                "m",
                1,
                None,
            );
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Potencia",
                |i| i.watts as f32,
                |i, v| i.watts = v as u32,
                0.0..=20000.0,
                " W",
                0,
            );
            // La temperatura, con una muestra de su color al lado.
            let kelvin = common(&items, |i| i.kelvin);
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Temperatura",
                |i| i.kelvin as f32,
                |i, v| i.kelvin = v as u32,
                1000.0..=10000.0,
                " K",
                0,
            );
            if let Some(k) = kelvin {
                let gel = common(&items, |i| i.light.as_ref().and_then(|l| l.gel)).flatten();
                let color = theme::beam_color(p, k, gel);
                let (swatch, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 10.0), egui::Sense::hover());
                ui.painter().rect_filled(swatch, 3, color);
                resp.on_hover_text("El color que tiene el haz con esta temperatura");
            }
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Intensidad",
                |i| i.light.as_ref().map_or(0.0, |l| l.intensity),
                |i, v| i.light.get_or_insert_with(Default::default).intensity = v,
                0.0..=100.0,
                "%",
                0,
                None,
            );
            if single {
                let gel = items[0].light.as_ref().and_then(|l| l.gel);
                let mut on = gel.is_some();
                kit::prop_row(ui, p, "Gelatina", |ui| {
                    if ui.checkbox(&mut on, "").changed() {
                        let color = if on { Some([255u8, 140, 60]) } else { None };
                        edits.push(Box::new(move |i| {
                            i.light.get_or_insert_with(Default::default).gel = color
                        }));
                    }
                    if let Some(mut rgb) = gel
                        && ui.color_edit_button_srgb(&mut rgb).changed()
                    {
                        edits.push(Box::new(move |i| {
                            i.light.get_or_insert_with(Default::default).gel = Some(rgb)
                        }));
                    }
                });
            }
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Altura",
                |i| i.light.as_ref().map_or(0.0, |l| l.height_m),
                |i, v| i.light.get_or_insert_with(Default::default).height_m = v,
                0.0..=10.0,
                "m",
                1,
                None,
            );
            if single {
                let mut motivation = items[0]
                    .light
                    .as_ref()
                    .map(|l| l.motivation.clone())
                    .unwrap_or_default();
                if kit::field(ui, p, "Motivación", &mut motivation, 1) {
                    edits.push(Box::new(move |i| {
                        i.light.get_or_insert_with(Default::default).motivation = motivation.clone()
                    }));
                }
            }
        }
        Some(BlockKind::Modifier) => {
            kit::section(ui, p, "Modificador", None);
            let kinds: Vec<String> = ModifierKind::ALL
                .iter()
                .map(|k| k.label().to_string())
                .collect();
            pick(
                ui,
                p,
                &mut edits,
                &items,
                "Tipo",
                "modifier_kind",
                &kinds,
                |i| {
                    i.modifier
                        .as_ref()
                        .and_then(|m| ModifierKind::ALL.iter().position(|k| *k == m.kind))
                },
                |i, n| i.modifier.get_or_insert_with(Default::default).kind = ModifierKind::ALL[n],
            );
            if items.iter().all(|i| {
                i.modifier
                    .as_ref()
                    .is_some_and(|m| m.kind == ModifierKind::Bounce)
            }) {
                let faces: Vec<String> = ModifierFace::ALL
                    .iter()
                    .map(|f| f.label().to_string())
                    .collect();
                pick(
                    ui,
                    p,
                    &mut edits,
                    &items,
                    "Cara",
                    "modifier_face",
                    &faces,
                    |i| {
                        i.modifier
                            .as_ref()
                            .and_then(|m| ModifierFace::ALL.iter().position(|f| *f == m.face))
                    },
                    |i, n| {
                        i.modifier.get_or_insert_with(Default::default).face = ModifierFace::ALL[n]
                    },
                );
            }
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Largo",
                |i| i.w,
                |i, v| {
                    i.w = v;
                    i.modifier.get_or_insert_with(Default::default).w = v;
                },
                0.2..=12.0,
                " m",
                1,
            );
        }
        Some(BlockKind::Mic) => {
            kit::section(ui, p, "Micrófono", None);
            let kinds: Vec<String> = MicKind::ALL.iter().map(|k| k.label().to_string()).collect();
            pick(
                ui,
                p,
                &mut edits,
                &items,
                "Tipo",
                "mic_type",
                &kinds,
                |i| {
                    i.mic
                        .as_ref()
                        .and_then(|m| MicKind::ALL.iter().position(|k| *k == m.kind))
                },
                |i, n| {
                    let kind = MicKind::ALL[n];
                    let spec = i.mic.get_or_insert_with(Default::default);
                    spec.kind = kind;
                    (spec.pattern, spec.reach_m) = (kind.default_pattern(), kind.default_reach_m());
                    spec.wireless = kind == MicKind::Lavalier;
                },
            );
            pattern_picker(ui, p, &mut edits, &items);
            slide(
                ui,
                p,
                &mut edits,
                &items,
                "Alcance",
                |i| i.mic.as_ref().map_or(0.0, |m| m.reach_m),
                |i, v| i.mic.get_or_insert_with(Default::default).reach_m = v,
                0.25..=10.0,
                "m",
                2,
                None,
            );
            num(
                ui,
                p,
                &mut edits,
                &items,
                "Pista",
                |i| i.track as f32,
                |i, v| i.track = v as u32,
                0.0..=32.0,
                "",
                0,
            );
            toggle(
                ui,
                p,
                &mut edits,
                &items,
                "Inalámbrico",
                |i| i.mic.as_ref().is_some_and(|m| m.wireless),
                |i, v| i.mic.get_or_insert_with(Default::default).wireless = v,
            );
            if single {
                let spec = items[0].mic.clone().unwrap_or_default();
                match spec.kind {
                    MicKind::Lavalier => {
                        let people: Vec<(u64, String)> = project
                            .blockings
                            .iter()
                            .find(|b| b.id == blocking)
                            .map(|b| {
                                b.items
                                    .iter()
                                    .filter(|i| i.kind == BlockKind::Actor)
                                    .map(|i| {
                                        (
                                            i.id,
                                            i.character
                                                .and_then(|c| characters.iter().find(|x| x.0 == c))
                                                .map_or(
                                                    if i.label.is_empty() {
                                                        "Persona".to_string()
                                                    } else {
                                                        i.label.clone()
                                                    },
                                                    |x| x.1.clone(),
                                                ),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let names: Vec<String> = people.iter().map(|p| p.1.clone()).collect();
                        let cur = spec
                            .attached_to
                            .and_then(|a| people.iter().position(|x| x.0 == a));
                        let ppl = people.clone();
                        let shown = cur.map_or("Sin persona".to_string(), |n| names[n].clone());
                        kit::combo_field(ui, p, "Lo lleva", "mic_person", &shown, |ui| {
                            for (n, name) in names.iter().enumerate() {
                                if ui.selectable_label(cur == Some(n), name).clicked() {
                                    let id = ppl[n].0;
                                    edits.push(Box::new(move |i| {
                                        i.mic.get_or_insert_with(Default::default).attached_to =
                                            Some(id)
                                    }));
                                }
                            }
                        });
                    }
                    MicKind::Boom => {
                        let op = spec.operator;
                        kit::kv_row(
                            ui,
                            p,
                            "Operador",
                            &op.map_or("Sin ubicar".to_string(), |o| {
                                format!("{:.1} · {:.1} m", o.0, o.1).replace('.', ",")
                            }),
                        );
                    }
                    _ => {}
                }
            }
        }
        None if !items.is_empty() => {}
        _ => {}
    }

    // --- Dónde está y cómo se ve.
    kit::section(ui, p, "Posición", None);
    num(
        ui,
        p,
        &mut edits,
        &items,
        "Giro",
        |i| i.pose_at(time).2,
        move |i, v| {
            let at = edit_instant(i, time, auto);
            let (x, y, _) = i.pose_at(at);
            i.set_pose_at(at, x, y, v.rem_euclid(360.0));
        },
        -360.0..=720.0,
        "°",
        0,
    );
    toggle(
        ui,
        p,
        &mut edits,
        &items,
        "Oculto",
        |i| i.hidden,
        |i, v| i.hidden = v,
    );
    toggle(
        ui,
        p,
        &mut edits,
        &items,
        "Bloqueado",
        |i| i.locked,
        |i, v| i.locked = v,
    );

    // --- Se aplica todo ahora que nada está prestado. Mientras se reproduce, los campos muestran lo que va pasando pero no
    // editan nada: si no, el valor interpolado de cada cuadro se grabaría como posiciones nuevas.
    if playing {
        edits.clear();
    }
    let mut changed = false;
    if let Some(b) = project.blockings.iter_mut().find(|b| b.id == blocking) {
        for item in b.items.iter_mut().filter(|i| ids.contains(&i.id)) {
            for edit in &edits {
                edit(item);
                changed = true;
            }
        }
    }
    for action in actions {
        match action {
            Action::AddBody(name, crop) => {
                let id = project.add_camera_body(name, crop);
                if let Some(i) = project
                    .blockings
                    .iter_mut()
                    .find(|b| b.id == blocking)
                    .and_then(|b| b.items.iter_mut().find(|i| Some(i.id) == state.selected))
                {
                    i.body = Some(id);
                }
                changed = true;
            }
            Action::Register { role, kind, name } => {
                let scene = project.blockings.iter().find(|b| b.id == blocking).map(|b| b.scene);
                let id = project.add_role_entry(role, scene);
                if let Some(e) = project.role_entries_mut(role).iter_mut().find(|e| e.id == id) {
                    e.kind = kind.to_string();
                    e.title = name;
                }
                if let Some(i) = project
                    .blockings
                    .iter_mut()
                    .find(|b| b.id == blocking)
                    .and_then(|b| b.items.iter_mut().find(|i| Some(i.id) == state.selected))
                {
                    i.equipment = Some(id);
                }
                changed = true;
            }
            Action::LinkShot { shot, on } => {
                let camera = state.selected;
                if let Some(s) = project.shots.iter_mut().find(|s| s.id == shot) {
                    (s.blocking, s.camera) = if on {
                        (Some(blocking), camera)
                    } else {
                        (None, None)
                    };
                    changed = true;
                }
            }
        }
    }
    if kind == Some(BlockKind::Camera) {
        changed |= bodies_editor(ui, p, project);
    }
    // Las posiciones van al final, en su caja: crecen ahí sin desplazar nada.
    changed |= positions_box(ui, state, project, p, blocking);
    changed
}
