//! El movimiento en las plantas (docs/plantas.md §5): las posiciones guardadas de cada cosa, qué fantasmas se ven, las
//! flechas y arcos del recorrido, y la edición de los instantes en la línea de tiempo. Todo es puro y en metros y
//! segundos; el dibujo y el mouse viven en `blocking.rs`.

use crate::model::{BlockItem, KEY_EPSILON};

pub type Pt = (f32, f32);

/// Cuántos fantasmas se ven (§5.1). Por defecto, solo los de lo seleccionado.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GhostMode {
    All,
    #[default]
    Selection,
    Nothing,
}

impl GhostMode {
    pub const ALL: [GhostMode; 3] = [GhostMode::All, GhostMode::Selection, GhostMode::Nothing];

    pub fn label(self) -> &'static str {
        match self {
            GhostMode::All => "Todas",
            GhostMode::Selection => "Solo la selección",
            GhostMode::Nothing => "Ninguna",
        }
    }
}

/// Una posición guardada de algo que se mueve.
#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    /// Cómo se numera: 1 es el inicio, después 2, 3…
    pub number: usize,
    /// Su lugar en `keys`; el inicio (`x`, `y`, `rot` de la cosa) no tiene.
    pub index: Option<usize>,
    pub t: f32,
    pub x: f32,
    pub y: f32,
    pub rot: f32,
    pub label: String,
    pub hold: f32,
}

/// Las posiciones de `item`, en orden de tiempo: el inicio y cada una de `keys`. Sin posiciones guardadas, ninguna.
pub fn positions(item: &BlockItem) -> Vec<Position> {
    if item.keys.is_empty() {
        return Vec::new();
    }
    let start = Position {
        number: 1,
        index: None,
        t: 0.0,
        x: item.x,
        y: item.y,
        rot: item.rot,
        label: String::new(),
        hold: 0.0,
    };
    std::iter::once(start)
        .chain(item.keys.iter().enumerate().map(|(i, k)| Position {
            number: i + 2,
            index: Some(i),
            t: k.t,
            x: k.x,
            y: k.y,
            rot: k.rot,
            label: k.label.clone(),
            hold: k.hold,
        }))
        .collect()
}

/// Las posiciones de `item` que se dibujan como fantasma según el modo: todas las de cada cosa con «Todas», las de lo
/// seleccionado con «Solo la selección» y ninguna con «Ninguna». La figura del instante actual se dibuja aparte.
pub fn ghosts_for(item: &BlockItem, mode: GhostMode, selected: Option<u64>) -> Vec<Position> {
    let shown = match mode {
        GhostMode::All => true,
        GhostMode::Selection => selected == Some(item.id),
        GhostMode::Nothing => false,
    };
    if shown { positions(item) } else { Vec::new() }
}

/// Un tiempo como «4 s» o «2,5 s».
pub fn seconds_text(t: f32) -> String {
    if (t - t.round()).abs() < 0.05 {
        format!("{:.0} s", t.round())
    } else {
        format!("{t:.1} s").replace('.', ",")
    }
}

/// El texto de la insignia de una posición: su número, la misma letra o iniciales que la figura y el tiempo.
pub fn badge_text(tag: &str, position: &Position) -> String {
    if tag.is_empty() {
        format!("{} · {}", position.number, seconds_text(position.t))
    } else {
        format!(
            "{} · {} · {}",
            position.number,
            tag,
            seconds_text(position.t)
        )
    }
}

/// «pausa 2 s», si la posición tiene espera.
pub fn hold_text(position: &Position) -> Option<String> {
    (position.hold > 0.0).then(|| format!("pausa {}", seconds_text(position.hold)))
}

fn dist(a: Pt, b: Pt) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// El camino corto entre dos ángulos, en grados con signo (−180 a 180).
pub fn turn_between(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

/// Si entre dos posiciones seguidas no cambia el lugar pero sí el giro (un paneo), el ángulo de partida y cuánto gira, con
/// signo: el arco que se dibuja alrededor de la figura.
pub fn pan_arc(a: &Position, b: &Position) -> Option<(f32, f32)> {
    let turn = turn_between(a.rot, b.rot);
    (dist((a.x, a.y), (b.x, b.y)) < 0.05 && turn.abs() > 1.0).then_some((a.rot, turn))
}

/// Los puntos del recorrido de `item`: sus posiciones y los puntos intermedios unidos con rectas y, donde un nodo es suave,
/// con curvas.
pub fn path_points(item: &BlockItem) -> Vec<Pt> {
    if item.keys.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Pt> = Vec::new();
    for segment in 0..item.keys.len() {
        let curve = item.segment_curve(segment);
        out.extend(curve.into_iter().skip(usize::from(!out.is_empty())));
    }
    out
}

/// Los nodos del recorrido que se pueden tocar: los puntos intermedios de cada tramo, con su lugar (la posición a la que llega
/// el tramo y cuál es dentro de él), dónde están y si son suaves.
pub fn waypoints(item: &BlockItem) -> Vec<(usize, usize, Pt, bool)> {
    item.keys
        .iter()
        .enumerate()
        .flat_map(|(k, key)| {
            key.via
                .iter()
                .enumerate()
                .map(move |(n, v)| (k, n, (v.x, v.y), v.smooth))
        })
        .collect()
}

/// Dónde de un tramo cae el punto más cercano a `point`: el tramo (la posición a la que llega), el lugar donde entraría un
/// punto nuevo entre sus intermedios y el punto sobre el trayecto, si queda a no más de `within` metros.
pub fn nearest_on_path(item: &BlockItem, point: Pt, within: f32) -> Option<(usize, usize, Pt)> {
    let mut best: Option<(f32, usize, usize, Pt)> = None;
    for segment in 0..item.keys.len() {
        let crate::model::PathNodes { nodes, at, .. } = item.path_nodes();
        let (from, to) = (at[segment], at[segment + 1]);
        let curve = item.segment_curve(segment);
        // A qué par de nodos pertenece cada punto de la curva: se recorre en el mismo orden en que se armó.
        let mut node = from;
        for pair in curve.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len2 = (b.0 - a.0).powi(2) + (b.1 - a.1).powi(2);
            let u = if len2 > 1e-9 {
                (((point.0 - a.0) * (b.0 - a.0) + (point.1 - a.1) * (b.1 - a.1)) / len2)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
            let d = dist(q, point);
            if d <= within && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, segment, node - from, q));
            }
            // Se pasa al nodo siguiente al llegar a su punto.
            if node < to && dist(b, nodes[node + 1].0) < 1e-4 {
                node += 1;
            }
        }
    }
    best.map(|(_, segment, index, q)| (segment, index, q))
}

/// Las flechas del recorrido: un punto y hacia dónde apunta, en grados, en el medio de cada tramo en que algo se mueve.
pub fn path_arrows(item: &BlockItem) -> Vec<(Pt, f32)> {
    let all = positions(item);
    all.windows(2)
        .filter(|w| dist((w[0].x, w[0].y), (w[1].x, w[1].y)) >= 0.05)
        .map(|w| {
            let start = (w[0].t + w[0].hold).min(w[1].t);
            let mid = (start + w[1].t) / 2.0;
            let (x0, y0, _) = item.pose_at(mid - 0.02);
            let (x1, y1, _) = item.pose_at(mid + 0.02);
            let (x, y, _) = item.pose_at(mid);
            ((x, y), (y1 - y0).atan2(x1 - x0).to_degrees())
        })
        .collect()
}

/// Los puntos del recorrido de `item` entre dos instantes: lo que se resalta para el tramo de un plano.
pub fn path_between(item: &BlockItem, from: f32, to: f32) -> Vec<Pt> {
    let (from, to) = (from.min(to), from.max(to));
    let steps = (((to - from) * 10.0).ceil() as usize).clamp(2, 240);
    (0..=steps)
        .map(|n| {
            let (x, y, _) = item.pose_at(from + (to - from) * n as f32 / steps as f32);
            (x, y)
        })
        .collect()
}

/// El tiempo al que se pega `t` si hay un imán a no más de `radius` segundos; si no, `t` redondeado a `step`.
pub fn snap_time(t: f32, magnets: &[f32], radius: f32, step: f32) -> f32 {
    magnets
        .iter()
        .copied()
        .filter(|m| (m - t).abs() <= radius)
        .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
        .unwrap_or_else(|| (t / step).round() * step)
}

/// Cambia el instante de la posición `index` de `item` y deja `keys` en orden. Devuelve su nuevo lugar, o `None` si caería
/// sobre otra posición de la misma cosa (no se mueve).
pub fn move_key_time(item: &mut BlockItem, index: usize, t: f32) -> Option<usize> {
    if index >= item.keys.len() || t < KEY_EPSILON {
        return None;
    }
    if item
        .keys
        .iter()
        .enumerate()
        .any(|(i, k)| i != index && (k.t - t).abs() < KEY_EPSILON)
    {
        return None;
    }
    item.keys[index].t = t;
    item.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
    item.keys.iter().position(|k| (k.t - t).abs() < 1e-6)
}

/// El instante de la posición anterior y de la siguiente a `t` (incluido el inicio, en 0).
pub fn neighbours(item: &BlockItem, t: f32) -> (Option<f32>, Option<f32>) {
    let times: Vec<f32> = std::iter::once(0.0)
        .chain(item.keys.iter().map(|k| k.t))
        .collect();
    let before = times
        .iter()
        .copied()
        .filter(|x| *x < t - KEY_EPSILON)
        .fold(None, |a: Option<f32>, x| Some(a.map_or(x, |a| a.max(x))));
    let after = times
        .iter()
        .copied()
        .filter(|x| *x > t + KEY_EPSILON)
        .fold(None, |a: Option<f32>, x| Some(a.map_or(x, |a| a.min(x))));
    (before, after)
}

/// Un tramo de un plano sobre la línea de tiempo: desde y hasta, en segundos. Sin tramo guardado, todo el movimiento.
pub fn shot_span(range: Option<(f32, f32)>, duration: f32) -> (f32, f32) {
    match range {
        Some((a, b)) => (a.min(b).clamp(0.0, duration), a.max(b).clamp(0.0, duration)),
        None => (0.0, duration),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockKey, BlockKind};

    fn mover() -> BlockItem {
        BlockItem {
            id: 1,
            kind: BlockKind::Actor,
            x: 0.0,
            y: 0.0,
            keys: vec![
                BlockKey::new(2.0, 4.0, 0.0, 0.0),
                BlockKey::new(4.0, 4.0, 0.0, 90.0),
                BlockKey::new(6.0, 4.0, 4.0, 90.0),
            ],
            ..BlockItem::default()
        }
    }

    #[test]
    fn something_with_n_positions_shows_n_ghosts_with_all_and_none_with_none() {
        let item = mover();
        let n = positions(&item).len();
        assert_eq!(
            n,
            item.keys.len() + 1,
            "el inicio cuenta como la posición 1"
        );
        assert_eq!(
            positions(&item)
                .iter()
                .map(|p| p.number)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(ghosts_for(&item, GhostMode::All, None).len(), n);
        assert_eq!(ghosts_for(&item, GhostMode::Nothing, Some(1)).len(), 0);
        // «Solo la selección»: solo los de lo elegido.
        assert_eq!(ghosts_for(&item, GhostMode::Selection, Some(1)).len(), n);
        assert_eq!(ghosts_for(&item, GhostMode::Selection, Some(2)).len(), 0);
        assert_eq!(ghosts_for(&item, GhostMode::Selection, None).len(), 0);
        assert_eq!(GhostMode::default(), GhostMode::Selection);
        // Lo que no se mueve no tiene fantasmas.
        assert!(ghosts_for(&BlockItem::default(), GhostMode::All, None).is_empty());
    }

    #[test]
    fn the_badge_carries_the_number_the_tag_the_time_and_the_hold() {
        let mut item = mover();
        item.keys[0].label = "Juana se sienta".into();
        item.keys[0].hold = 1.5;
        let all = positions(&item);
        assert_eq!(badge_text("JP", &all[1]), "2 · JP · 2 s");
        assert_eq!(badge_text("", &all[0]), "1 · 0 s");
        assert_eq!(badge_text("B", &all[3]), "4 · B · 6 s");
        assert_eq!(all[1].label, "Juana se sienta");
        assert_eq!(hold_text(&all[1]).as_deref(), Some("pausa 1,5 s"));
        assert_eq!(hold_text(&all[2]), None);
        assert_eq!(seconds_text(2.0), "2 s");
        assert_eq!(seconds_text(2.5), "2,5 s");
    }

    #[test]
    fn a_pan_is_a_turn_without_moving_and_takes_the_short_way() {
        let all = positions(&mover());
        assert!(pan_arc(&all[0], &all[1]).is_none(), "se mueve");
        assert_eq!(pan_arc(&all[1], &all[2]), Some((0.0, 90.0)));
        assert!(pan_arc(&all[2], &all[3]).is_none());
        let a = Position {
            number: 1,
            index: None,
            t: 0.0,
            x: 0.0,
            y: 0.0,
            rot: 350.0,
            label: String::new(),
            hold: 0.0,
        };
        let b = Position {
            rot: 10.0,
            ..a.clone()
        };
        assert_eq!(pan_arc(&a, &b), Some((350.0, 20.0)));
        assert!(pan_arc(&a, &a).is_none());
    }

    #[test]
    fn the_path_has_arrows_that_point_the_way_and_curves_when_asked() {
        let mut item = mover();
        let arrows = path_arrows(&item);
        assert_eq!(
            arrows.len(),
            2,
            "un tramo de 4 m y otro de 4 m; el paneo no tiene flecha"
        );
        assert!(
            arrows[0].1.abs() < 1.0,
            "va hacia la derecha: {}",
            arrows[0].1
        );
        assert!((arrows[1].1 - 90.0).abs() < 1.0, "baja: {}", arrows[1].1);
        assert_eq!(path_points(&item).len(), 4, "recta: un punto por posición");
        item.keys.iter_mut().for_each(|k| k.smooth = true);
        let curve = path_points(&item);
        assert!(curve.len() > 20, "curva: muchos puntos");
        assert!(
            curve
                .iter()
                .any(|p| (p.0 - 4.0).abs() < 0.05 && p.1.abs() < 0.05),
            "pasa por las posiciones"
        );
        // Lo que se queda quieto el tiempo de su espera mueve la flecha al resto del tramo.
        item.keys.iter_mut().for_each(|k| k.smooth = false);
        item.keys[0].hold = 1.0;
        assert_eq!(path_arrows(&item).len(), 2);
    }

    #[test]
    fn times_snap_to_a_close_magnet_and_otherwise_to_a_tenth() {
        assert_eq!(snap_time(3.04, &[3.0, 5.0], 0.15, 0.1), 3.0);
        assert!((snap_time(3.26, &[3.0, 5.0], 0.15, 0.1) - 3.3).abs() < 1e-5);
        assert_eq!(snap_time(4.9, &[3.0, 5.0], 0.15, 0.1), 5.0);
    }

    #[test]
    fn moving_a_position_in_time_keeps_the_order_and_refuses_to_land_on_another() {
        let mut item = mover();
        item.keys[0].label = "primera".into();
        assert_eq!(
            move_key_time(&mut item, 0, 5.0),
            Some(1),
            "pasa a ser la segunda"
        );
        assert_eq!(
            item.keys.iter().map(|k| k.t).collect::<Vec<_>>(),
            [4.0, 5.0, 6.0]
        );
        assert_eq!(item.keys[1].label, "primera", "se lleva su marca");
        assert_eq!(
            move_key_time(&mut item, 1, 4.02),
            None,
            "sobre otra posición no se mueve"
        );
        assert_eq!(move_key_time(&mut item, 1, 0.0), None, "no pisa el inicio");
        assert_eq!(item.keys[1].t, 5.0);
    }

    #[test]
    fn the_previous_and_next_position_include_the_start() {
        let item = mover();
        assert_eq!(neighbours(&item, 3.0), (Some(2.0), Some(4.0)));
        assert_eq!(neighbours(&item, 0.0), (None, Some(2.0)));
        assert_eq!(neighbours(&item, 1.0), (Some(0.0), Some(2.0)));
        assert_eq!(neighbours(&item, 6.0), (Some(4.0), None));
    }

    #[test]
    fn a_shot_span_defaults_to_the_whole_movement_and_stays_inside_it() {
        assert_eq!(shot_span(None, 10.0), (0.0, 10.0));
        assert_eq!(shot_span(Some((3.0, 6.0)), 10.0), (3.0, 6.0));
        assert_eq!(shot_span(Some((6.0, 3.0)), 10.0), (3.0, 6.0));
        assert_eq!(shot_span(Some((-1.0, 12.0)), 10.0), (0.0, 10.0));
        let item = mover();
        let along = path_between(&item, 0.0, 2.0);
        assert!((along.last().unwrap().0 - 4.0).abs() < 1e-3);
    }
}
