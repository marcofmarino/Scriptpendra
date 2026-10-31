//! Lo que la planta revisa y mide (docs/plantas.md §6): el cono de una cámara, qué cae dentro, las distancias a las
//! personas, los avisos «en cuadro» y las manijas de apertura y alcance de luces y micrófonos. Todo es puro y en metros.

use std::collections::HashMap;

use crate::model::{BlockItem, BlockKind, MicKind, pose_in};
use crate::motion;

pub type Pt = (f32, f32);

fn rotate(v: Pt, degrees: f32) -> Pt {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (v.0 * cos - v.1 * sin, v.0 * sin + v.1 * cos)
}

fn dist(a: Pt, b: Pt) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// El cono de visión de una cámara (con el ángulo en `fov` y el largo en `cone_len_m`): su centro y el arco.
pub fn cone(item: &BlockItem) -> Vec<Pt> {
    let half = item.fov.clamp(1.0, 170.0) / 2.0;
    let mut out = vec![(item.x, item.y)];
    for i in 0..=16 {
        let angle = item.rot - half + 2.0 * half * i as f32 / 16.0;
        let r = rotate((item.cone_len_m.max(0.5), 0.0), angle);
        out.push((item.x + r.0, item.y + r.1));
    }
    out
}

pub fn point_in(poly: &[Pt], point: Pt) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > point.1) != (b.1 > point.1)
            && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// ¿Cae `p` dentro del haz de una luz (el sector de `beam_deg` y `reach_m` hacia donde mira)?
pub fn in_beam(light: &BlockItem, p: Pt) -> bool {
    let spec = light.light.clone().unwrap_or_default();
    let d = dist((light.x, light.y), p);
    if d > spec.reach_m {
        return false;
    }
    if spec.beam_deg >= 359.0 || d < 1e-4 {
        return true;
    }
    let toward = (p.1 - light.y).atan2(p.0 - light.x).to_degrees();
    motion::turn_between(light.rot, toward).abs() <= spec.beam_deg / 2.0
}

/// La sombra que proyecta una bandera hacia atrás cuando el haz de una luz la alcanza: el cuadrilátero entre sus dos puntas
/// y esas mismas puntas llevadas, en la dirección que viene de la luz, un largo y medio de bandera (como mínimo, 1 m).
pub fn flag_shadow(flag: &BlockItem, light: &BlockItem) -> Option<Vec<Pt>> {
    if !in_beam(light, (flag.x, flag.y)) {
        return None;
    }
    let half = rotate((flag.w / 2.0, 0.0), flag.rot);
    let ends = [
        (flag.x - half.0, flag.y - half.1),
        (flag.x + half.0, flag.y + half.1),
    ];
    let length = (flag.w * 1.5).max(1.0);
    let far = ends.map(|e| {
        let d = (e.0 - light.x, e.1 - light.y);
        let n = d.0.hypot(d.1).max(1e-4);
        (e.0 + d.0 / n * length, e.1 + d.1 / n * length)
    });
    Some(vec![ends[0], ends[1], far[1], far[0]])
}

/// Una distancia que se dibuja: de lo elegido a una persona dentro de su cono o de su haz.
#[derive(Clone, Debug, PartialEq)]
pub struct Distance {
    pub from: Pt,
    pub to: Pt,
    pub meters: f32,
    pub person: u64,
}

/// Las distancias de lo elegido a cada persona dentro de su cono (una cámara) o de su haz (una luz); sirve para el foco y la
/// iluminación. Cualquier otra cosa no mide nada.
pub fn distances(selected: &BlockItem, items: &[BlockItem]) -> Vec<Distance> {
    let origin = (selected.x, selected.y);
    let inside: Box<dyn Fn(Pt) -> bool> = match selected.kind {
        BlockKind::Camera => {
            let poly = cone(selected);
            Box::new(move |p| point_in(&poly, p))
        }
        BlockKind::Light => {
            let light = selected.clone();
            Box::new(move |p| in_beam(&light, p))
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter(|i| i.kind == BlockKind::Actor && !i.hidden)
        .filter(|i| inside((i.x, i.y)))
        .map(|i| Distance {
            from: origin,
            to: (i.x, i.y),
            meters: dist(origin, (i.x, i.y)),
            person: i.id,
        })
        .collect()
}

/// Qué parte de lo que se ve en cuadro: la cosa misma, la caña de un boom o su operador.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Seen {
    Item,
    Pole,
    Operator,
}

/// Un aviso «en cuadro»: algo que puede verse dentro del cono de una cámara.
#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub item: u64,
    pub what: Seen,
    pub camera: u64,
    pub camera_label: String,
    /// Los códigos de los planos vinculados a esa cámara («12A», «12B»).
    pub shots: Vec<String>,
    /// El instante en que se ve.
    pub at: f32,
}

impl Warning {
    /// «Puede verse en cuadro de la Cámara A (planos 12A, 12B)».
    pub fn text(&self) -> String {
        let who = match self.what {
            Seen::Item => "Puede verse",
            Seen::Pole => "La caña puede verse",
            Seen::Operator => "El operador puede verse",
        };
        let shots = match self.shots.len() {
            0 => String::new(),
            1 => format!(" (plano {})", self.shots[0]),
            _ => format!(" (planos {})", self.shots.join(", ")),
        };
        format!("{who} en cuadro de la Cámara {}{shots}", self.camera_label)
    }
}

/// Los avisos de todo lo que puede verse en cuadro: una luz, un modificador, un micrófono, la caña o el operador de un boom
/// dentro del cono de alguna cámara. Se revisa en el instante `time` y en cada posición guardada de cada cámara (cuando ahí
/// está también lo que se mueve). `fov_of` da el ángulo de cada cámara y `shots_of`, los planos vinculados a ella.
pub fn in_frame(
    originals: &[BlockItem],
    fov_of: &dyn Fn(&BlockItem) -> f32,
    time: f32,
    shots_of: &dyn Fn(u64) -> Vec<String>,
) -> Vec<Warning> {
    let mut out: Vec<Warning> = Vec::new();
    for camera in originals
        .iter()
        .filter(|i| i.kind == BlockKind::Camera && !i.hidden)
    {
        let mut times = vec![time];
        for pos in motion::positions(camera) {
            if !times
                .iter()
                .any(|t| (t - pos.t).abs() < crate::model::KEY_EPSILON)
            {
                times.push(pos.t);
            }
        }
        let shots = shots_of(camera.id);
        for t in times {
            let (x, y, rot) = camera.pose_at(t);
            // Con el zoom que tiene la cámara en ese instante.
            let lensed = BlockItem {
                lens_mm: camera.lens_at(t),
                ..camera.clone()
            };
            let seen_by = BlockItem {
                x,
                y,
                rot,
                fov: fov_of(&lensed),
                ..lensed
            };
            let poly = cone(&seen_by);
            for target in originals.iter().filter(|i| {
                matches!(
                    i.kind,
                    BlockKind::Light | BlockKind::Modifier | BlockKind::Mic
                ) && !i.hidden
            }) {
                let (tx, ty, trot) = pose_in(originals, target, t);
                let mut seen = Vec::new();
                if point_in(&poly, (tx, ty)) {
                    seen.push(Seen::Item);
                }
                if let Some(spec) = target.mic.as_ref().filter(|m| m.kind == MicKind::Boom)
                    && let Some(op) = spec.operator
                {
                    // La caña va de la cápsula al operador; el operador es un punto.
                    let capsule = (tx, ty);
                    if (1..8).any(|n| {
                        let f = n as f32 / 8.0;
                        point_in(
                            &poly,
                            (
                                capsule.0 + (op.0 - capsule.0) * f,
                                capsule.1 + (op.1 - capsule.1) * f,
                            ),
                        )
                    }) {
                        seen.push(Seen::Pole);
                    }
                    if point_in(&poly, op) {
                        seen.push(Seen::Operator);
                    }
                }
                let _ = trot;
                for what in seen {
                    if !out
                        .iter()
                        .any(|w| w.item == target.id && w.what == what && w.camera == camera.id)
                    {
                        out.push(Warning {
                            item: target.id,
                            what,
                            camera: camera.id,
                            camera_label: camera.label.clone(),
                            shots: shots.clone(),
                            at: t,
                        });
                    }
                }
            }
        }
    }
    out
}

/// Los avisos de cada cosa, uno por línea: lo que se ve en el tooltip de su icono. La clave es la cosa y qué parte.
pub fn warning_texts(warnings: &[Warning]) -> HashMap<(u64, Seen), String> {
    let mut out: HashMap<(u64, Seen), Vec<String>> = HashMap::new();
    for w in warnings {
        out.entry((w.item, w.what)).or_default().push(w.text());
    }
    out.into_iter().map(|(k, v)| (k, v.join("\n"))).collect()
}

/// Una manija para cambiar la apertura o el alcance de algo arrastrando el borde de su haz, su patrón o su cono.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    /// Hasta dónde llega (el alcance de una luz o un micrófono, o el largo del cono de una cámara).
    Reach,
    /// Qué tan abierto es el haz de una luz.
    Beam,
}

/// Cuánto llega lo que proyecta una cosa, en metros: el alcance de la luz o del micrófono, o el largo del cono.
pub fn reach_of(item: &BlockItem) -> Option<f32> {
    match item.kind {
        BlockKind::Light => Some(item.light.clone().unwrap_or_default().reach_m),
        BlockKind::Mic => Some(item.mic.clone().unwrap_or_default().reach_m),
        BlockKind::Camera => Some(item.cone_len_m),
        _ => None,
    }
}

/// Las manijas de apertura y alcance de `item` y dónde están: el alcance, al frente; y la apertura de una luz, en el borde
/// del haz. Una práctica (que ilumina en círculo) no tiene apertura.
pub fn range_handles(item: &BlockItem) -> Vec<(Range, Pt)> {
    let Some(reach) = reach_of(item) else {
        return Vec::new();
    };
    let at = |angle: f32| {
        let v = rotate((reach, 0.0), angle);
        (item.x + v.0, item.y + v.1)
    };
    let mut out = vec![(Range::Reach, at(item.rot))];
    if item.kind == BlockKind::Light {
        let beam = item.light.clone().unwrap_or_default().beam_deg;
        if beam < 359.0 {
            out.push((Range::Beam, at(item.rot + beam / 2.0)));
        }
    }
    out
}

/// El valor nuevo de una manija llevada hasta `point`: el alcance en metros, o la apertura total en grados.
pub fn range_value(item: &BlockItem, handle: Range, point: Pt) -> f32 {
    match handle {
        Range::Reach => dist((item.x, item.y), point).clamp(0.25, 30.0),
        Range::Beam => {
            let toward = (point.1 - item.y).atan2(point.0 - item.x).to_degrees();
            (2.0 * motion::turn_between(item.rot, toward).abs()).clamp(5.0, 360.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockKey, LightSpec, MicSpec};

    fn camera(id: u64, label: &str, x: f32, y: f32, rot: f32) -> BlockItem {
        BlockItem {
            id,
            kind: BlockKind::Camera,
            x,
            y,
            rot,
            fov: 40.0,
            label: label.into(),
            w: 0.5,
            h: 0.35,
            ..BlockItem::default()
        }
    }

    fn light(id: u64, x: f32, y: f32) -> BlockItem {
        BlockItem {
            id,
            kind: BlockKind::Light,
            x,
            y,
            w: 0.4,
            h: 0.4,
            light: Some(LightSpec::default()),
            ..BlockItem::default()
        }
    }

    fn actor(id: u64, x: f32, y: f32) -> BlockItem {
        BlockItem {
            id,
            kind: BlockKind::Actor,
            x,
            y,
            w: 0.6,
            h: 0.6,
            ..BlockItem::default()
        }
    }

    fn warn(items: &[BlockItem], time: f32) -> Vec<Warning> {
        in_frame(items, &|i| i.fov, time, &|c| {
            if c == 1 {
                vec!["12A".into(), "12B".into()]
            } else {
                Vec::new()
            }
        })
    }

    #[test]
    fn a_light_inside_a_camera_cone_is_warned_about_and_one_outside_is_not() {
        let items = [
            camera(1, "A", 0.0, 0.0, 0.0),
            light(2, 3.0, 0.0),
            light(3, 3.0, 4.0),
            light(4, -2.0, 0.0),
            light(5, 6.0, 0.0),
        ];
        let w = warn(&items, 0.0);
        let ids: Vec<u64> = w.iter().map(|w| w.item).collect();
        assert_eq!(
            ids,
            [2],
            "solo la que está dentro del cono (la de 6 m queda más allá de los 5 m)"
        );
        assert_eq!(
            w[0].text(),
            "Puede verse en cuadro de la Cámara A (planos 12A, 12B)"
        );
    }

    #[test]
    fn hidden_things_and_hidden_cameras_do_not_warn_and_with_one_plan_the_text_says_plano() {
        let mut items = [camera(1, "A", 0.0, 0.0, 0.0), light(2, 3.0, 0.0)];
        items[1].hidden = true;
        assert!(warn(&items, 0.0).is_empty());
        items[1].hidden = false;
        items[0].hidden = true;
        assert!(warn(&items, 0.0).is_empty());
        items[0].hidden = false;
        let one = in_frame(&items, &|i| i.fov, 0.0, &|_| vec!["3C".into()]);
        assert!(one[0].text().ends_with("(plano 3C)"));
        let none = in_frame(&items, &|i| i.fov, 0.0, &|_| Vec::new());
        assert!(none[0].text().ends_with("Cámara A"));
    }

    #[test]
    fn it_also_checks_every_saved_position_of_the_camera() {
        // La cámara arranca mirando hacia otro lado y a los 4 s gira hacia la luz: el aviso sale aunque ahora no se vea.
        let mut cam = camera(1, "A", 0.0, 0.0, 180.0);
        cam.keys = vec![BlockKey::new(4.0, 0.0, 0.0, 0.0)];
        let items = [cam, light(2, 3.0, 0.0)];
        let w = warn(&items, 0.0);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].at, 4.0);
    }

    #[test]
    fn it_follows_what_moves_when_the_camera_looks_at_a_later_instant() {
        let mut lamp = light(2, 3.0, 8.0);
        lamp.keys = vec![BlockKey::new(4.0, 3.0, 0.0, 0.0)];
        let mut cam = camera(1, "A", 0.0, 0.0, 0.0);
        cam.keys = vec![BlockKey::new(4.0, 0.0, 0.0, 0.0)];
        let w = warn(&[cam, lamp], 0.0);
        assert_eq!(w.len(), 1, "a los 4 s la luz ya está en el cuadro");
        assert_eq!(w[0].at, 4.0);
    }

    #[test]
    fn the_boom_pole_and_the_operator_warn_apart_from_the_microphone() {
        let boom = |capsule: Pt, operator: Pt| BlockItem {
            id: 2,
            kind: BlockKind::Mic,
            x: capsule.0,
            y: capsule.1,
            w: 0.3,
            h: 0.3,
            mic: Some(MicSpec {
                kind: MicKind::Boom,
                operator: Some(operator),
                ..MicSpec::default()
            }),
            ..BlockItem::default()
        };
        let cam = camera(1, "A", 0.0, 0.0, 0.0);
        // La cápsula fuera de cuadro (atrás de la cámara), la caña y el operador dentro del cono.
        let seen: Vec<(Seen, u64)> = warn(&[cam.clone(), boom((-1.0, 0.0), (3.0, 0.0))], 0.0)
            .iter()
            .map(|w| (w.what, w.item))
            .collect();
        assert!(
            seen.contains(&(Seen::Pole, 2))
                && seen.contains(&(Seen::Operator, 2))
                && !seen.contains(&(Seen::Item, 2)),
            "{seen:?}"
        );
        // Todo fuera de cuadro: nada.
        assert!(warn(&[cam.clone(), boom((-1.0, 0.0), (-3.0, 0.0))], 0.0).is_empty());
        let w = warn(&[cam, boom((-1.0, 0.0), (3.0, 0.0))], 0.0);
        let texts = warning_texts(&w);
        assert!(texts[&(2, Seen::Operator)].starts_with("El operador puede verse"));
        assert!(texts[&(2, Seen::Pole)].starts_with("La caña puede verse"));
    }

    #[test]
    fn distances_go_to_the_people_inside_the_cone_or_the_beam() {
        let cam = camera(1, "A", 0.0, 0.0, 0.0);
        let items = [actor(2, 2.0, 0.0), actor(3, 2.0, 3.0), actor(4, -2.0, 0.0)];
        let d = distances(&cam, &items);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].person, 2);
        assert!((d[0].meters - 2.0).abs() < 1e-4);
        let mut lamp = light(9, 0.0, 0.0);
        lamp.light.as_mut().unwrap().beam_deg = 40.0;
        let near: Vec<u64> = distances(&lamp, &items).iter().map(|d| d.person).collect();
        assert_eq!(near, [2]);
        // Una persona más allá del alcance del haz no cuenta.
        assert!(distances(&lamp, &[actor(5, 6.0, 0.0)]).is_empty());
        // Una persona no mide nada.
        assert!(distances(&items[0], &items).is_empty());
        // Una práctica ilumina en círculo.
        lamp.light = Some(LightSpec::new(crate::model::LightFixture::Practical));
        assert_eq!(distances(&lamp, &[actor(6, -1.0, 0.0)]).len(), 1);
    }

    #[test]
    fn a_flag_casts_a_soft_shadow_behind_it_only_when_a_light_reaches_it() {
        let mut lamp = light(1, 0.0, 0.0);
        lamp.light.as_mut().unwrap().reach_m = 6.0;
        lamp.light.as_mut().unwrap().beam_deg = 60.0;
        let flag = BlockItem {
            id: 2,
            kind: BlockKind::Modifier,
            x: 3.0,
            y: 0.0,
            w: 1.0,
            h: 0.1,
            rot: 90.0,
            ..BlockItem::default()
        };
        let shadow = flag_shadow(&flag, &lamp).expect("la luz la alcanza");
        assert_eq!(shadow.len(), 4);
        assert!(
            shadow[2].0 > 3.4 && shadow[3].0 > 3.4,
            "la sombra cae del lado opuesto a la luz: {shadow:?}"
        );
        let off = BlockItem {
            y: 5.0,
            ..flag.clone()
        };
        assert!(
            flag_shadow(&off, &lamp).is_none(),
            "fuera del haz no hay sombra"
        );
        let far = BlockItem { x: 9.0, ..flag };
        assert!(
            flag_shadow(&far, &lamp).is_none(),
            "más allá del alcance tampoco"
        );
    }

    #[test]
    fn the_range_handles_sit_on_the_edge_of_the_beam_and_edit_it() {
        let mut lamp = light(1, 1.0, 1.0);
        lamp.rot = 90.0;
        {
            let spec = lamp.light.as_mut().unwrap();
            spec.beam_deg = 60.0;
            spec.reach_m = 4.0;
        }
        let handles = range_handles(&lamp);
        assert_eq!(handles.len(), 2);
        let reach = handles.iter().find(|h| h.0 == Range::Reach).unwrap().1;
        assert!(
            (reach.0 - 1.0).abs() < 1e-4 && (reach.1 - 5.0).abs() < 1e-4,
            "al frente, a 4 m: {reach:?}"
        );
        let beam = handles.iter().find(|h| h.0 == Range::Beam).unwrap().1;
        // En el borde del haz: a 30° del frente y a 4 m.
        assert!((((beam.0 - 1.0).powi(2) + (beam.1 - 1.0).powi(2)).sqrt() - 4.0).abs() < 1e-4);
        assert!((range_value(&lamp, Range::Beam, beam) - 60.0).abs() < 0.1);
        assert!((range_value(&lamp, Range::Reach, (1.0, 7.0)) - 6.0).abs() < 1e-4);
        assert_eq!(
            range_value(&lamp, Range::Reach, (1.0, 1.0)),
            0.25,
            "no baja de 0,25 m"
        );
        assert_eq!(
            range_value(&lamp, Range::Beam, (1.0, 5.0)),
            5.0,
            "no baja de 5°"
        );
        // Una práctica no tiene apertura; un micrófono solo alcance; una cámara, el largo del cono.
        lamp.light = Some(LightSpec::new(crate::model::LightFixture::Practical));
        assert_eq!(range_handles(&lamp).len(), 1);
        let mic = BlockItem {
            kind: BlockKind::Mic,
            mic: Some(MicSpec::default()),
            ..BlockItem::default()
        };
        assert_eq!(
            range_handles(&mic).iter().map(|h| h.0).collect::<Vec<_>>(),
            [Range::Reach]
        );
        assert_eq!(reach_of(&camera(1, "A", 0.0, 0.0, 0.0)), Some(5.0));
        assert!(range_handles(&actor(1, 0.0, 0.0)).is_empty());
    }
}
