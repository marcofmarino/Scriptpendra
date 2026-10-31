//! Modo de colocación de las plantas (docs/plantas.md §4): qué herramientas hay, cómo se arma lo que se coloca y la
//! geometría de los casos especiales (paredes encadenadas, puertas sobre paredes, corbatero sobre una persona). Todo es
//! puro y en metros; el mouse y el dibujo viven en `blocking.rs`.

use crate::model::{
    BlockItem, BlockKind, LightFixture, LightRole, MicKind, ModifierKind, ObjectShape, OpeningKind,
};

pub type Pt = (f32, f32);

/// Qué se coloca con una herramienta, ya con su aparato o tipo elegido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spec {
    Camera,
    Person,
    Object { round: bool },
    /// Un mueble u objeto con figura propia.
    Furniture(ObjectShape),
    Wall,
    Opening(OpeningKind),
    Light(LightFixture),
    Modifier(ModifierKind),
    Mic(MicKind),
}

impl Spec {
    pub fn kind(self) -> BlockKind {
        match self {
            Spec::Camera => BlockKind::Camera,
            Spec::Person => BlockKind::Actor,
            Spec::Object { .. } | Spec::Furniture(_) => BlockKind::Object,
            Spec::Wall => BlockKind::Wall,
            Spec::Opening(_) => BlockKind::Opening,
            Spec::Light(_) => BlockKind::Light,
            Spec::Modifier(_) => BlockKind::Modifier,
            Spec::Mic(_) => BlockKind::Mic,
        }
    }

    /// Cómo se llama la herramienta, para sus tooltips.
    pub fn name(self) -> &'static str {
        match self {
            Spec::Camera => "una cámara",
            Spec::Person => "una persona",
            Spec::Object { round: false } => "un mueble u objeto",
            Spec::Object { round: true } => "un objeto redondo",
            Spec::Furniture(shape) => shape.furniture_name().unwrap_or("un mueble"),
            Spec::Wall => "una pared",
            Spec::Opening(OpeningKind::Door) => "una puerta",
            Spec::Opening(OpeningKind::Window) => "una ventana",
            Spec::Light(_) => "una fuente de luz",
            Spec::Modifier(ModifierKind::Bounce) => "un rebote",
            Spec::Modifier(ModifierKind::Flag) => "una bandera",
            Spec::Modifier(ModifierKind::Diffuser) => "un difusor",
            Spec::Mic(MicKind::Boom) => "un boom",
            Spec::Mic(MicKind::Lavalier) => "un corbatero",
            Spec::Mic(MicKind::Plant) => "un micrófono oculto",
            Spec::Mic(MicKind::Stand) => "un micrófono en pie",
        }
    }

    /// Deja `item` (una cosa nueva de `self.kind()`) con los datos de esta herramienta.
    pub fn apply(self, item: &mut BlockItem) {
        match self {
            Spec::Object { round } => {
                item.shape = if round {
                    ObjectShape::Round
                } else {
                    ObjectShape::Rect
                };
                (item.w, item.h) = (1.0, 1.0);
            }
            Spec::Furniture(shape) => {
                item.shape = shape;
                if let Some((_, name, w, h)) = ObjectShape::FURNITURE.iter().find(|(s, ..)| *s == shape) {
                    (item.w, item.h) = (*w, *h);
                    item.label = name.to_string();
                }
            }
            Spec::Opening(kind) => {
                item.opening = kind;
                item.w = if kind == OpeningKind::Door { 0.9 } else { 1.2 };
            }
            Spec::Light(fixture) => {
                let mut spec = crate::model::LightSpec::new(fixture);
                if fixture == LightFixture::Practical {
                    spec.role = LightRole::Practical;
                }
                item.fov = spec.beam_deg;
                (item.watts, item.kelvin) = fixture.watts_kelvin();
                item.light = Some(spec);
            }
            Spec::Modifier(kind) => {
                item.modifier = Some(crate::model::ModifierSpec {
                    kind,
                    ..Default::default()
                });
            }
            Spec::Mic(kind) => item.mic = Some(crate::model::MicSpec::new(kind)),
            Spec::Camera | Spec::Person | Spec::Wall => {}
        }
    }

    /// La tecla de la herramienta (§5.4).
    pub fn key(self) -> char {
        match self {
            Spec::Camera => 'C',
            Spec::Person => 'P',
            Spec::Object { .. } | Spec::Furniture(_) => 'O',
            Spec::Wall => 'W',
            Spec::Opening(_) => 'D',
            Spec::Light(_) => 'L',
            Spec::Modifier(_) => 'B',
            Spec::Mic(_) => 'M',
        }
    }

    /// La ayuda de la barra de estado mientras se coloca.
    pub fn help(self, boom_operator_placed: bool, wall_started: bool) -> &'static str {
        match self {
            Spec::Wall if wall_started => {
                "Clic para cerrar la pared y seguir · Doble clic o Esc para terminar"
            }
            Spec::Wall => "Clic para marcar el inicio de la pared · Esc para cancelar",
            Spec::Opening(_) => {
                "Soltala sobre una pared · Arrastrá para definir el ancho · Esc para cancelar"
            }
            Spec::Object { .. } | Spec::Furniture(_) => {
                "Clic para colocar · Arrastrá para dibujar el objeto · Shift para seguir colocando · Esc para cancelar"
            }
            Spec::Mic(MicKind::Lavalier) => {
                "Soltalo sobre una persona · Shift para seguir colocando · Esc para cancelar"
            }
            Spec::Mic(MicKind::Boom) if boom_operator_placed => {
                "Clic para ubicar la cápsula · Esc para cancelar"
            }
            Spec::Mic(MicKind::Boom) => {
                "Clic para ubicar al operador y otro para la cápsula · Esc para cancelar"
            }
            _ => {
                "Clic para colocar · Arrastrá para orientar · Shift para seguir colocando · Esc para cancelar"
            }
        }
    }
}

/// Cuánto se acerca a un extremo de otra pared un extremo que se está dibujando, en metros, para pegarse a él.
pub const WALL_SNAP_M: f32 = 0.3;
/// A cuánto del borde de una pared se puede soltar una puerta o una ventana, en metros.
pub const OPENING_TOL_M: f32 = 0.35;
/// Lo más angosta que queda una puerta o ventana dibujada a mano, en metros.
pub const OPENING_MIN_W: f32 = 0.4;

pub fn snap(p: Pt, step: f32, on: bool) -> Pt {
    if on {
        ((p.0 / step).round() * step, (p.1 / step).round() * step)
    } else {
        p
    }
}

fn dist(a: Pt, b: Pt) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Los dos extremos de una pared (la línea del medio), en metros.
pub fn wall_ends(wall: &BlockItem) -> [Pt; 2] {
    let (sin, cos) = wall.rot.to_radians().sin_cos();
    let half = (wall.w / 2.0 * cos, wall.w / 2.0 * sin);
    [
        (wall.x - half.0, wall.y - half.1),
        (wall.x + half.0, wall.y + half.1),
    ]
}

/// Si `p` está cerca de un extremo de alguna pared, ese extremo; si no, `p`.
pub fn snap_to_wall_ends(p: Pt, items: &[BlockItem], radius: f32) -> Pt {
    items
        .iter()
        .filter(|i| i.kind == BlockKind::Wall && !i.hidden)
        .flat_map(wall_ends)
        .filter(|e| dist(*e, p) <= radius)
        .min_by(|a, b| dist(*a, p).total_cmp(&dist(*b, p)))
        .unwrap_or(p)
}

/// El ángulo (0 a 360°) de `from` hacia `to`; con `shift`, de a 15°.
pub fn angle_between(from: Pt, to: Pt, shift: bool) -> f32 {
    let angle = (to.1 - from.1).atan2(to.0 - from.0).to_degrees();
    let angle = if shift {
        (angle / 15.0).round() * 15.0
    } else {
        angle
    };
    angle.rem_euclid(360.0)
}

/// Una pared sobre la que se puede soltar una abertura.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnWall {
    pub wall: u64,
    pub center: Pt,
    pub rot: f32,
    pub thick: f32,
    pub length: f32,
}

impl OnWall {
    /// Dónde cae `p` a lo largo de la pared, en metros desde su centro, sin salirse de ella.
    pub fn along(&self, p: Pt) -> f32 {
        let (sin, cos) = self.rot.to_radians().sin_cos();
        ((p.0 - self.center.0) * cos + (p.1 - self.center.1) * sin)
            .clamp(-self.length / 2.0, self.length / 2.0)
    }

    /// El punto de la línea del medio de la pared a `along` metros de su centro.
    pub fn point(&self, along: f32) -> Pt {
        let (sin, cos) = self.rot.to_radians().sin_cos();
        (self.center.0 + along * cos, self.center.1 + along * sin)
    }
}

/// La pared más cercana a `p` (a no más de `tol` de su borde), si hay alguna.
pub fn wall_near(items: &[BlockItem], p: Pt, tol: f32) -> Option<OnWall> {
    items
        .iter()
        .filter(|i| i.kind == BlockKind::Wall && !i.hidden)
        .filter_map(|w| {
            let on = OnWall {
                wall: w.id,
                center: (w.x, w.y),
                rot: w.rot,
                thick: w.h.max(0.15),
                length: w.w,
            };
            let (sin, cos) = w.rot.to_radians().sin_cos();
            let (dx, dy) = (p.0 - w.x, p.1 - w.y);
            let (along, across) = (dx * cos + dy * sin, -dx * sin + dy * cos);
            (along.abs() <= w.w / 2.0 + tol && across.abs() <= on.thick / 2.0 + tol)
                .then_some((across.abs(), on))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, on)| on)
}

/// Dónde queda una abertura sobre `wall` que se arrastró de `from` a `to` (puntos del plano, proyectados sobre la pared):
/// el centro y el ancho. Sin arrastre, queda centrada en `from` con el ancho `default_w`.
pub fn opening_on(wall: &OnWall, from: Pt, to: Pt, default_w: f32) -> (Pt, f32) {
    let (s0, s1) = (wall.along(from), wall.along(to));
    if (s1 - s0).abs() < 0.05 {
        let half = default_w / 2.0;
        let mid = s0.clamp(
            -wall.length / 2.0 + half.min(wall.length / 2.0),
            wall.length / 2.0 - half.min(wall.length / 2.0),
        );
        (wall.point(mid), default_w.min(wall.length))
    } else {
        let width = (s1 - s0).abs().max(OPENING_MIN_W).min(wall.length);
        (wall.point((s0 + s1) / 2.0), width)
    }
}

/// La persona (`Actor`) bajo `p`, con `pad` de margen para agarrarla.
pub fn person_under(items: &[BlockItem], p: Pt, pad: f32) -> Option<u64> {
    items
        .iter()
        .rev()
        .filter(|i| i.kind == BlockKind::Actor && !i.hidden)
        .find(|i| dist((i.x, i.y), p) <= i.w / 2.0 + pad)
        .map(|i| i.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(id: u64, x: f32, y: f32, w: f32, rot: f32) -> BlockItem {
        BlockItem {
            id,
            kind: BlockKind::Wall,
            x,
            y,
            w,
            h: 0.15,
            rot,
            ..BlockItem::default()
        }
    }

    #[test]
    fn the_grid_snaps_unless_it_is_off() {
        assert_eq!(snap((1.13, 2.4), 0.25, true), (1.25, 2.5));
        assert_eq!(snap((1.13, 2.4), 0.25, false), (1.13, 2.4));
    }

    #[test]
    fn a_wall_end_sticks_to_a_close_end_of_another_wall_and_only_a_close_one() {
        let walls = [wall(1, 2.0, 0.0, 4.0, 0.0)];
        assert_eq!(wall_ends(&walls[0]), [(0.0, 0.0), (4.0, 0.0)]);
        assert_eq!(
            snap_to_wall_ends((4.2, 0.1), &walls, WALL_SNAP_M),
            (4.0, 0.0)
        );
        assert_eq!(
            snap_to_wall_ends((4.5, 0.0), &walls, WALL_SNAP_M),
            (4.5, 0.0)
        );
        // También en una pared girada.
        let turned = [wall(2, 0.0, 1.0, 2.0, 90.0)];
        let ends = wall_ends(&turned[0]);
        assert!((ends[1].0 - 0.0).abs() < 1e-4 && (ends[1].1 - 2.0).abs() < 1e-4);
        assert_eq!(snap_to_wall_ends((0.1, 2.1), &turned, WALL_SNAP_M), ends[1]);
    }

    #[test]
    fn drag_angles_point_from_the_press_and_jump_by_fifteen_with_shift() {
        assert_eq!(angle_between((0.0, 0.0), (1.0, 0.0), false), 0.0);
        assert!((angle_between((0.0, 0.0), (0.0, 1.0), false) - 90.0).abs() < 1e-3);
        assert!((angle_between((0.0, 0.0), (-1.0, 0.0), false) - 180.0).abs() < 1e-3);
        let near_thirty = angle_between((0.0, 0.0), (1.0, 0.5), true);
        assert_eq!(near_thirty, 30.0, "26,6° salta a 30°");
        assert!((angle_between((0.0, 0.0), (1.0, -1.0), false) - 315.0).abs() < 1e-3);
    }

    #[test]
    fn an_opening_only_lands_on_a_wall_takes_its_angle_and_a_drag_sets_the_width() {
        let walls = [wall(7, 3.0, 1.0, 6.0, 0.0), wall(8, 10.0, 10.0, 2.0, 90.0)];
        assert!(
            wall_near(&walls, (3.0, 3.0), OPENING_TOL_M).is_none(),
            "lejos de toda pared"
        );
        let on = wall_near(&walls, (2.0, 1.1), OPENING_TOL_M).expect("sobre la pared 7");
        assert_eq!((on.wall, on.rot), (7, 0.0));
        // Un clic: centrada en el punto, con el ancho de siempre.
        let (center, width) = opening_on(&on, (2.0, 1.1), (2.0, 1.1), 0.9);
        assert!(
            (center.0 - 2.0).abs() < 1e-4
                && (center.1 - 1.0).abs() < 1e-4
                && (width - 0.9).abs() < 1e-4
        );
        // Un arrastre de 1,5 m: el ancho y el centro salen del arrastre, siempre sobre la línea de la pared.
        let (center, width) = opening_on(&on, (1.0, 1.0), (2.5, 3.0), 0.9);
        assert!(
            (width - 1.5).abs() < 1e-4
                && (center.0 - 1.75).abs() < 1e-4
                && (center.1 - 1.0).abs() < 1e-4
        );
        // No se sale de la pared.
        let (center, _) = opening_on(&on, (5.9, 1.0), (5.9, 1.0), 0.9);
        assert!(center.0 + 0.45 <= 6.0 + 1e-4);
        // La pared girada.
        let on = wall_near(&walls, (10.1, 10.5), OPENING_TOL_M).unwrap();
        assert_eq!((on.wall, on.rot), (8, 90.0));
    }

    #[test]
    fn a_lavalier_needs_a_person_under_the_mouse() {
        let people = [BlockItem {
            id: 3,
            kind: BlockKind::Actor,
            x: 2.0,
            y: 2.0,
            w: 0.6,
            h: 0.6,
            ..BlockItem::default()
        }];
        assert_eq!(person_under(&people, (2.1, 2.0), 0.1), Some(3));
        assert_eq!(person_under(&people, (3.0, 2.0), 0.1), None);
    }

    #[test]
    fn every_tool_builds_its_item_with_its_own_data() {
        let build = |spec: Spec| {
            let mut item = BlockItem::template(spec.kind(), 1, 0.0, 0.0, String::new(), 0);
            spec.apply(&mut item);
            item
        };
        assert_eq!(
            build(Spec::Object { round: true }).shape,
            ObjectShape::Round
        );
        assert_eq!(
            (
                build(Spec::Object { round: false }).w,
                build(Spec::Object { round: false }).h
            ),
            (1.0, 1.0)
        );
        assert_eq!(
            build(Spec::Opening(OpeningKind::Window)).opening,
            OpeningKind::Window
        );
        let panel = build(Spec::Light(LightFixture::Panel));
        assert_eq!(panel.light.as_ref().unwrap().fixture, LightFixture::Panel);
        assert_eq!((panel.fov, panel.watts), (100.0, 100));
        let practical = build(Spec::Light(LightFixture::Practical));
        assert_eq!(practical.light.as_ref().unwrap().role, LightRole::Practical);
        assert_eq!(
            build(Spec::Modifier(ModifierKind::Flag))
                .modifier
                .unwrap()
                .kind,
            ModifierKind::Flag
        );
        assert_eq!(
            build(Spec::Mic(MicKind::Plant)).mic.unwrap().kind,
            MicKind::Plant
        );
        assert!(Spec::Wall.help(false, true).contains("Doble clic"));
        assert!(
            Spec::Camera
                .help(false, false)
                .starts_with("Clic para colocar · Arrastrá para orientar · Shift")
        );
    }
}
