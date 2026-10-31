//! Cómo se ve cada cosa de las plantas (docs/plantas.md §3): las formas de cada figura, el haz de las luces, el patrón
//! polar de los micrófonos y el nivel de detalle según el zoom. Todo es geometría pura, en metros: no pinta ni toca la
//! interfaz, así que se prueba sin ventana. El dibujo vive en `blocking.rs`.

use crate::model::{
    BlockItem, BlockKind, LightFixture, MicKind, MicPattern, ModifierKind, ObjectShape, OpeningKind,
};

/// Un punto en metros.
pub type Pt = (f32, f32);

/// Con qué se pinta una parte de la figura.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    /// El cuerpo: el color de la figura.
    Body,
    /// Un detalle del cuerpo (cabeza, visera, lente…): una variante del color del cuerpo.
    Detail,
    /// Un hueco (puerta, ventana): el fondo del lienzo, con borde.
    Void,
    /// Algo de otra persona que acompaña a la figura (el operador de un boom): gris.
    Muted,
    /// Una línea continua.
    Line,
    /// Una línea de trazos.
    Dashed,
}

/// Un polígono (o una línea, si no se cierra) de una figura.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub paint: Paint,
    pub points: Vec<Pt>,
    pub closed: bool,
}

fn poly(paint: Paint, points: Vec<Pt>) -> Part {
    Part {
        paint,
        points,
        closed: true,
    }
}

fn line(paint: Paint, points: Vec<Pt>) -> Part {
    Part {
        paint,
        points,
        closed: false,
    }
}

pub fn rotate(v: Pt, degrees: f32) -> Pt {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (v.0 * cos - v.1 * sin, v.0 * sin + v.1 * cos)
}

fn rect(cx: f32, cy: f32, w: f32, h: f32) -> Vec<Pt> {
    vec![
        (cx - w / 2.0, cy - h / 2.0),
        (cx + w / 2.0, cy - h / 2.0),
        (cx + w / 2.0, cy + h / 2.0),
        (cx - w / 2.0, cy + h / 2.0),
    ]
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, n: usize) -> Vec<Pt> {
    (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

/// Una cápsula (rectángulo de puntas redondas) de largo `l` sobre el eje x y grosor `t`, centrada en `(cx, cy)`.
fn capsule(cx: f32, cy: f32, l: f32, t: f32) -> Vec<Pt> {
    let r = t / 2.0;
    let straight = (l / 2.0 - r).max(0.0);
    let cap = |sign: f32, from: f32| -> Vec<Pt> {
        (0..=6)
            .map(|i| {
                let a = (from + 180.0 * i as f32 / 6.0).to_radians();
                (cx + sign * straight + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    let mut out = cap(1.0, -90.0);
    out.extend(cap(-1.0, 90.0));
    out
}

/// Una persona vista desde arriba, con el centro en `(cx, cy)` y mirando hacia +x: hombros, cabeza y la cuña de la nariz.
fn person(cx: f32, cy: f32, s: f32, shoulders: Paint) -> Vec<Part> {
    vec![
        poly(shoulders, ellipse(cx, cy, 0.15 * s, 0.25 * s, 20)),
        poly(
            Paint::Detail,
            ellipse(cx + 0.02 * s, cy, 0.11 * s, 0.11 * s, 16),
        ),
        poly(
            Paint::Detail,
            vec![
                (cx + 0.10 * s, cy + 0.05 * s),
                (cx + 0.19 * s, cy),
                (cx + 0.10 * s, cy - 0.05 * s),
            ],
        ),
    ]
}

/// Las viseras de un fresnel o un HMI: dos aletas a los costados del frente.
fn visors(r: f32) -> Vec<Part> {
    [-1.0f32, 1.0]
        .map(|s| {
            poly(
                Paint::Detail,
                vec![
                    (0.4 * r, s * 0.9 * r),
                    (1.3 * r, s * 1.7 * r),
                    (1.3 * r, s * 1.4 * r),
                    (0.6 * r, s * 0.8 * r),
                ],
            )
        })
        .to_vec()
}

/// La figura de un mueble visto desde arriba: el cuerpo y los detalles que lo distinguen (respaldo, almohadas, puertas…). El
/// frente mira hacia +x, así que el respaldo queda en -x.
fn furniture(shape: ObjectShape, w: f32, h: f32) -> Vec<Part> {
    let body = poly(Paint::Body, rect(0.0, 0.0, w, h));
    let back = |t: f32| poly(Paint::Detail, rect(-w / 2.0 + t / 2.0, 0.0, t, h));
    match shape {
        ObjectShape::Table => vec![body, poly(Paint::Detail, rect(0.0, 0.0, w * 0.8, h * 0.8))],
        ObjectShape::Chair => vec![body, back(w * 0.22)],
        ObjectShape::Armchair => {
            let arm = h * 0.16;
            vec![
                body,
                back(w * 0.25),
                poly(Paint::Detail, rect(0.0, -h / 2.0 + arm / 2.0, w, arm)),
                poly(Paint::Detail, rect(0.0, h / 2.0 - arm / 2.0, w, arm)),
            ]
        }
        ObjectShape::Sofa => {
            let arm = h * 0.07;
            vec![
                body,
                back(w * 0.28),
                poly(Paint::Detail, rect(0.0, -h / 2.0 + arm / 2.0, w, arm)),
                poly(Paint::Detail, rect(0.0, h / 2.0 - arm / 2.0, w, arm)),
                line(Paint::Line, vec![(-w * 0.22, -h * 0.5), (-w * 0.22, h * 0.5)]),
            ]
        }
        ObjectShape::Bed => {
            let pillow = (w * 0.12, h * 0.38);
            vec![
                body,
                poly(Paint::Detail, rect(-w / 2.0 + pillow.0 / 2.0 + 0.05, -h * 0.24, pillow.0, pillow.1)),
                poly(Paint::Detail, rect(-w / 2.0 + pillow.0 / 2.0 + 0.05, h * 0.24, pillow.0, pillow.1)),
                line(Paint::Line, vec![(-w * 0.1, -h / 2.0), (-w * 0.1, h / 2.0)]),
            ]
        }
        ObjectShape::Desk => vec![
            body,
            poly(Paint::Detail, rect(-w * 0.12, h * 0.3, w * 0.7, h * 0.36)),
        ],
        ObjectShape::Shelf => (1..4)
            .map(|i| line(Paint::Line, vec![(-w / 2.0, h * (i as f32 / 4.0 - 0.5)), (w / 2.0, h * (i as f32 / 4.0 - 0.5))]))
            .chain(std::iter::once(body))
            .collect(),
        ObjectShape::Wardrobe => vec![
            body,
            line(Paint::Line, vec![(w / 2.0, 0.0), (-w / 2.0, 0.0)]),
            line(Paint::Line, vec![(w / 2.0, -h / 2.0), (-w / 2.0, h / 2.0)]),
        ],
        ObjectShape::Tv => vec![body, poly(Paint::Detail, rect(w * 0.3, 0.0, w * 0.4, h * 0.9))],
        ObjectShape::Plant => {
            let r = w.min(h) / 2.0;
            let mut parts = vec![poly(Paint::Body, ellipse(0.0, 0.0, r, r, 20))];
            parts.extend((0..5).map(|i| {
                let a = std::f32::consts::TAU * i as f32 / 5.0;
                line(Paint::Line, vec![(0.0, 0.0), (r * 0.9 * a.cos(), r * 0.9 * a.sin())])
            }));
            parts.push(poly(Paint::Detail, ellipse(0.0, 0.0, r * 0.25, r * 0.25, 12)));
            parts
        }
        ObjectShape::Rug => vec![
            poly(Paint::Detail, rect(0.0, 0.0, w, h)),
            line(Paint::Dashed, {
                let (a, b) = (w * 0.42, h * 0.42);
                vec![(-a, -b), (a, -b), (a, b), (-a, b), (-a, -b)]
            }),
        ],
        ObjectShape::Car => vec![
            poly(Paint::Body, rect(0.0, 0.0, w, h)),
            poly(Paint::Detail, rect(w * 0.12, 0.0, w * 0.3, h * 0.84)),
            poly(Paint::Detail, rect(-w * 0.18, 0.0, w * 0.3, h * 0.84)),
            poly(Paint::Detail, rect(w * 0.36, 0.0, w * 0.1, h * 0.7)),
        ],
        ObjectShape::Rect | ObjectShape::Round => vec![body],
    }
}

/// La figura de `item` en su propio sistema de coordenadas: el centro en el origen y mirando hacia +x, en metros. Cada
/// tipo, aparato de luz, modificador y micrófono tiene una forma que no comparte con ninguno de los otros (§3.2).
pub fn local_figure(item: &BlockItem) -> Vec<Part> {
    let (w, h) = (item.w, item.h);
    match item.kind {
        BlockKind::Actor => person(0.0, 0.0, (w / 0.6).max(0.2), Paint::Body),
        BlockKind::Object => match item.shape {
            ObjectShape::Rect => vec![poly(Paint::Body, rect(0.0, 0.0, w, h))],
            ObjectShape::Round => vec![poly(Paint::Body, ellipse(0.0, 0.0, w / 2.0, h / 2.0, 28))],
            shape => furniture(shape, w, h),
        },
        BlockKind::Wall => vec![poly(Paint::Body, rect(0.0, 0.0, w, h.max(0.15)))],
        BlockKind::Opening => {
            let thick = h.max(0.15);
            let gap = poly(Paint::Void, rect(0.0, 0.0, w, thick));
            match item.opening {
                OpeningKind::Door => {
                    // La hoja abierta 90° desde la bisagra y el arco que barre al abrirse.
                    let hinge = -w / 2.0;
                    let arc = (0..=8)
                        .map(|i| {
                            let a = (90.0 * i as f32 / 8.0).to_radians();
                            (hinge + w * a.sin(), w * a.cos())
                        })
                        .collect();
                    vec![
                        gap,
                        line(Paint::Line, vec![(hinge, 0.0), (hinge, w)]),
                        line(Paint::Dashed, arc),
                    ]
                }
                OpeningKind::Window => vec![
                    gap,
                    line(
                        Paint::Line,
                        vec![(-w / 2.0, thick * 0.2), (w / 2.0, thick * 0.2)],
                    ),
                    line(
                        Paint::Line,
                        vec![(-w / 2.0, -thick * 0.2), (w / 2.0, -thick * 0.2)],
                    ),
                ],
            }
        }
        BlockKind::Camera => {
            // El cuerpo y, al frente, la lente: un trapecio que se abre hacia adelante.
            let lens = 0.28 * (w / 0.5).max(0.2);
            vec![
                poly(Paint::Body, rect(0.0, 0.0, w, h)),
                poly(
                    Paint::Detail,
                    vec![
                        (w / 2.0, -h * 0.3),
                        (w / 2.0 + lens, -h * 0.46),
                        (w / 2.0 + lens, h * 0.46),
                        (w / 2.0, h * 0.3),
                    ],
                ),
            ]
        }
        BlockKind::Light => {
            let u = w.max(0.1);
            match item
                .light
                .as_ref()
                .map_or(LightFixture::Fresnel, |l| l.fixture)
            {
                LightFixture::Fresnel => {
                    let mut out = vec![poly(Paint::Body, ellipse(0.0, 0.0, u / 2.0, u / 2.0, 20))];
                    out.extend(visors(u / 2.0));
                    out
                }
                LightFixture::Panel => vec![
                    poly(Paint::Body, rect(0.0, 0.0, 0.25 * u, 2.2 * u)),
                    line(Paint::Line, vec![(0.2 * u, -0.95 * u), (0.2 * u, 0.95 * u)]),
                ],
                LightFixture::Tube => vec![
                    poly(Paint::Body, rect(0.0, 0.0, 0.18 * u, 4.0 * u)),
                    poly(Paint::Detail, rect(0.0, -1.9 * u, 0.3 * u, 0.2 * u)),
                    poly(Paint::Detail, rect(0.0, 1.9 * u, 0.3 * u, 0.2 * u)),
                ],
                LightFixture::Softbox => vec![
                    poly(
                        Paint::Body,
                        vec![
                            (-0.5 * u, -0.45 * u),
                            (0.5 * u, -0.9 * u),
                            (0.5 * u, 0.9 * u),
                            (-0.5 * u, 0.45 * u),
                        ],
                    ),
                    poly(
                        Paint::Detail,
                        vec![
                            (0.28 * u, -0.7 * u),
                            (0.46 * u, -0.8 * u),
                            (0.46 * u, 0.8 * u),
                            (0.28 * u, 0.7 * u),
                        ],
                    ),
                    line(Paint::Line, vec![(0.0, -0.68 * u), (0.0, 0.68 * u)]),
                ],
                LightFixture::Hmi => {
                    let r = 0.7 * u;
                    let mut out = vec![poly(Paint::Body, ellipse(0.0, 0.0, r, r, 28))];
                    out.extend(visors(r));
                    out.push(poly(
                        Paint::Detail,
                        ellipse(0.35 * r, 0.0, 0.45 * r, 0.45 * r, 12),
                    ));
                    out
                }
                LightFixture::Practical => vec![
                    poly(Paint::Body, ellipse(0.0, 0.0, 0.3 * u, 0.3 * u, 16)),
                    poly(Paint::Detail, ellipse(0.0, 0.0, 0.12 * u, 0.12 * u, 10)),
                ],
                LightFixture::Other => vec![poly(Paint::Body, rect(0.0, 0.0, u, u))],
            }
        }
        BlockKind::Modifier => {
            // Un segmento como una pared (largo sobre x); el lado reflectante o de trabajo mira hacia +y.
            let spec = item.modifier.clone().unwrap_or_default();
            let len = if w > 0.0 { w } else { spec.w };
            match spec.kind {
                ModifierKind::Bounce => vec![
                    poly(Paint::Body, rect(0.0, 0.0, len, 0.08)),
                    poly(Paint::Detail, rect(0.0, 0.055, len, 0.03)),
                ],
                ModifierKind::Flag => vec![
                    poly(Paint::Body, rect(0.0, 0.0, len, 0.06)),
                    poly(Paint::Detail, rect(-len / 2.0, 0.0, 0.05, 0.22)),
                    poly(Paint::Detail, rect(len / 2.0, 0.0, 0.05, 0.22)),
                    line(Paint::Line, vec![(-len / 2.0, 0.0), (len / 2.0, 0.0)]),
                ],
                ModifierKind::Diffuser => vec![
                    poly(Paint::Void, rect(0.0, 0.0, len, 0.04)),
                    line(Paint::Dashed, vec![(-len / 2.0, 0.0), (len / 2.0, 0.0)]),
                ],
            }
        }
        BlockKind::Mic => {
            let spec = item.mic.clone().unwrap_or_default();
            match spec.kind {
                MicKind::Boom => {
                    // La cápsula en la punta, la caña hasta el operador (como una persona chica en gris).
                    let (dx, dy) = spec.operator.map_or((-1.2, 0.0), |o| {
                        rotate((o.0 - item.x, o.1 - item.y), -item.rot)
                    });
                    let mut out = vec![
                        poly(Paint::Body, capsule(0.0, 0.0, 0.3, 0.1)),
                        line(Paint::Line, vec![(-0.15, 0.0), (dx, dy)]),
                    ];
                    out.extend(person(dx, dy, 0.7, Paint::Muted));
                    out
                }
                MicKind::Lavalier => vec![
                    poly(Paint::Body, ellipse(0.0, 0.0, 0.05, 0.05, 12)),
                    line(Paint::Line, vec![(-0.05, 0.0), (-0.13, 0.0)]),
                ],
                MicKind::Plant => vec![
                    poly(Paint::Body, capsule(0.0, 0.0, 0.22, 0.08)),
                    Part {
                        paint: Paint::Dashed,
                        points: rect(0.0, 0.0, 0.34, 0.34),
                        closed: true,
                    },
                ],
                MicKind::Stand => {
                    // Un trípode visto desde arriba, con la cápsula en el centro.
                    let mut out = vec![poly(Paint::Body, capsule(0.0, 0.0, 0.22, 0.08))];
                    for a in [90.0f32, 210.0, 330.0] {
                        out.push(line(Paint::Line, vec![(0.0, 0.0), rotate((0.24, 0.0), a)]));
                    }
                    out
                }
            }
        }
    }
}

/// La figura de `item` en el plano: en su lugar (`x`, `y`), girada `rot` grados y agrandada `grow` veces alrededor de su
/// centro (para que no baje de 14 px en pantalla, ver `body_scale`).
pub fn figure(item: &BlockItem, grow: f32) -> Vec<Part> {
    let mut parts = local_figure(item);
    for part in &mut parts {
        for p in &mut part.points {
            let (x, y) = rotate((p.0 * grow, p.1 * grow), item.rot);
            *p = (item.x + x, item.y + y);
        }
    }
    parts
}

/// El lado más largo del cuerpo (sin lo que lo acompaña: caña, operador, hoja de una puerta), en metros.
pub fn body_extent(parts: &[Part]) -> f32 {
    let mut lo = (f32::MAX, f32::MAX);
    let mut hi = (f32::MIN, f32::MIN);
    for part in parts
        .iter()
        .filter(|p| matches!(p.paint, Paint::Body | Paint::Void))
    {
        for p in &part.points {
            lo = (lo.0.min(p.0), lo.1.min(p.1));
            hi = (hi.0.max(p.0), hi.1.max(p.1));
        }
    }
    if lo.0 > hi.0 {
        0.0
    } else {
        (hi.0 - lo.0).max(hi.1 - lo.1)
    }
}

/// Lo menos que mide una figura en pantalla, en píxeles, para seguir siendo clicable al alejarse.
pub const MIN_BODY_PX: f32 = 14.0;
/// Desde qué tamaño en pantalla el cuerpo lleva un icono adentro.
pub const ICON_MIN_PX: f32 = 18.0;

/// Cuánto hay que agrandar una figura de `extent_m` metros, con `px_per_m` píxeles por metro, para que mida al menos
/// `MIN_BODY_PX`. Si ya es más grande, 1.
pub fn body_scale(extent_m: f32, px_per_m: f32) -> f32 {
    let px = extent_m * px_per_m;
    if px <= 0.0 {
        1.0
    } else {
        (MIN_BODY_PX / px).max(1.0)
    }
}

/// Qué se dibuja según el zoom. Al alejarse se ocultan primero las etiquetas, después los iconos y al final los patrones
/// dejan de llevar relleno y quedan como contorno (§3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detail {
    pub labels: bool,
    pub icons: bool,
    pub fills: bool,
}

pub fn detail_for(px_per_m: f32) -> Detail {
    Detail {
        labels: px_per_m >= 36.0,
        icons: px_per_m >= 28.0,
        fills: px_per_m >= 22.0,
    }
}

// ---- Haz de luz ----

/// Una banda del haz: un abanico desde el origen (el primer punto) y su opacidad.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub points: Vec<Pt>,
    pub alpha: f32,
}

/// Cuántas bandas concéntricas tiene el haz de un aparato: el softbox, que tiene el borde difuso, tiene más.
pub fn band_count(fixture: LightFixture) -> usize {
    if fixture == LightFixture::Softbox {
        8
    } else {
        4
    }
}

/// El haz de una luz: `bands` sectores concéntricos de `beam_deg` grados y hasta `reach_m` metros, del más grande al más
/// chico. Cada uno suma su opacidad sobre los de afuera, así que baja hacia afuera; `intensity` (0 a 100 %) la escala.
pub fn beam_bands(
    origin: Pt,
    rot_deg: f32,
    beam_deg: f32,
    reach_m: f32,
    bands: usize,
    intensity: f32,
) -> Vec<Band> {
    let beam = beam_deg.clamp(1.0, 360.0);
    let alpha = intensity.clamp(0.0, 100.0) / 100.0 * 0.45 / bands.max(1) as f32;
    let steps = ((beam / 6.0).ceil() as usize).max(2);
    (1..=bands)
        .rev()
        .map(|b| {
            let r = reach_m * b as f32 / bands as f32;
            let mut points = vec![origin];
            for i in 0..=steps {
                let a = rot_deg - beam / 2.0 + beam * i as f32 / steps as f32;
                let v = rotate((r, 0.0), a);
                points.push((origin.0 + v.0, origin.1 + v.1));
            }
            Band { points, alpha }
        })
        .collect()
}

// ---- Patrón polar de un micrófono ----

/// El coeficiente `a` de r(θ) = alcance × |a + (1 − a) · cos θ|. El lobular (shotgun) no entra en esa familia.
fn polar_a(pattern: MicPattern) -> Option<f32> {
    match pattern {
        MicPattern::Omni => Some(1.0),
        MicPattern::Cardioid => Some(0.5),
        MicPattern::Supercardioid => Some(0.37),
        MicPattern::Hypercardioid => Some(0.25),
        MicPattern::Figure8 => Some(0.0),
        MicPattern::Shotgun => None,
    }
}

/// Cuánto capta un micrófono hacia la dirección `theta` (radianes, 0 = hacia donde apunta): el radio del patrón. Es
/// `alcance × |a + (1 − a) · cos θ|`, y para el lobular `alcance × ((1 + cos θ) / 2)⁴`.
pub fn polar_radius(pattern: MicPattern, reach_m: f32, theta: f32) -> f32 {
    match polar_a(pattern) {
        Some(a) => reach_m * (a + (1.0 - a) * theta.cos()).abs(),
        None => reach_m * ((1.0 + theta.cos()) / 2.0).powi(4),
    }
}

/// Si la dirección `theta` cae en un lóbulo de atrás (en contrafase): el de atrás de super e hipercardioide y la mitad de
/// atrás de la figura 8.
pub fn polar_is_rear(pattern: MicPattern, theta: f32) -> bool {
    polar_a(pattern).is_some_and(|a| a + (1.0 - a) * theta.cos() < -1e-6)
}

/// Un lóbulo del patrón: un abanico desde el origen y si es de atrás (se dibuja más tenue).
#[derive(Clone, Debug, PartialEq)]
pub struct Lobe {
    pub points: Vec<Pt>,
    pub rear: bool,
}

/// Los lóbulos del patrón de un micrófono en `origin` que apunta hacia `rot_deg`, con la curva real de `polar_radius`.
pub fn polar_lobes(origin: Pt, rot_deg: f32, pattern: MicPattern, reach_m: f32) -> Vec<Lobe> {
    const STEP: i32 = 4;
    let mut lobes: Vec<Lobe> = Vec::new();
    let mut current: Option<Lobe> = None;
    for deg in (-180..=180).step_by(STEP as usize) {
        let theta = (deg as f32).to_radians();
        let rear = polar_is_rear(pattern, theta);
        let v = rotate(
            (polar_radius(pattern, reach_m, theta), 0.0),
            rot_deg + deg as f32,
        );
        let point = (origin.0 + v.0, origin.1 + v.1);
        match current.as_mut() {
            Some(lobe) if lobe.rear == rear => lobe.points.push(point),
            _ => {
                lobes.extend(current.take());
                current = Some(Lobe {
                    points: vec![origin, point],
                    rear,
                });
            }
        }
    }
    lobes.extend(current);
    lobes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LightSpec, MicSpec, ModifierSpec};

    const EPS: f32 = 1e-4;

    #[test]
    fn the_cardioid_is_deaf_at_the_back_and_full_range_at_the_front() {
        let reach = 2.0;
        assert!((polar_radius(MicPattern::Cardioid, reach, 0.0) - reach).abs() < EPS);
        assert!(polar_radius(MicPattern::Cardioid, reach, std::f32::consts::PI).abs() < EPS);
        assert!(
            (polar_radius(MicPattern::Cardioid, reach, std::f32::consts::FRAC_PI_2) - reach * 0.5)
                .abs()
                < EPS
        );
    }

    #[test]
    fn the_omni_reaches_the_same_in_every_direction() {
        for deg in 0..360 {
            let r = polar_radius(MicPattern::Omni, 1.5, (deg as f32).to_radians());
            assert!((r - 1.5).abs() < EPS, "{deg}°: {r}");
        }
    }

    #[test]
    fn the_figure_eight_is_deaf_at_the_sides_and_its_back_lobe_is_flagged() {
        assert!(polar_radius(MicPattern::Figure8, 1.0, std::f32::consts::FRAC_PI_2).abs() < EPS);
        assert!((polar_radius(MicPattern::Figure8, 1.0, 0.0) - 1.0).abs() < EPS);
        assert!((polar_radius(MicPattern::Figure8, 1.0, std::f32::consts::PI) - 1.0).abs() < EPS);
        assert!(
            polar_is_rear(MicPattern::Figure8, std::f32::consts::PI)
                && !polar_is_rear(MicPattern::Figure8, 0.0)
        );
        // El cardioide y el omni no tienen lóbulo de atrás; el super y el hiper sí.
        assert!(
            !polar_is_rear(MicPattern::Cardioid, std::f32::consts::PI)
                && !polar_is_rear(MicPattern::Omni, std::f32::consts::PI)
        );
        assert!(
            polar_is_rear(MicPattern::Supercardioid, std::f32::consts::PI)
                && polar_is_rear(MicPattern::Hypercardioid, std::f32::consts::PI)
        );
    }

    #[test]
    fn the_shotgun_has_a_narrow_forward_lobe() {
        let front = polar_radius(MicPattern::Shotgun, 1.0, 0.0);
        let side = polar_radius(MicPattern::Shotgun, 1.0, std::f32::consts::FRAC_PI_2);
        assert!((front - 1.0).abs() < EPS && side < 0.1 * front);
        assert!(polar_radius(MicPattern::Shotgun, 1.0, std::f32::consts::PI) < EPS);
        // Es más angosto que el cardioide.
        assert!(
            polar_radius(MicPattern::Shotgun, 1.0, 1.0)
                < polar_radius(MicPattern::Cardioid, 1.0, 1.0)
        );
    }

    #[test]
    fn polar_lobes_follow_the_curve_and_the_orientation() {
        let lobes = polar_lobes((10.0, 5.0), 90.0, MicPattern::Cardioid, 2.0);
        assert!(lobes.iter().all(|l| !l.rear));
        // Mirando hacia abajo (90°), el punto más lejano del frente está 2 m más abajo del micrófono.
        let far = lobes
            .iter()
            .flat_map(|l| l.points.iter())
            .fold(
                (0.0f32, 0.0f32),
                |best, p| if p.1 > best.1 { *p } else { best },
            );
        assert!(
            (far.0 - 10.0).abs() < 0.05 && (far.1 - 7.0).abs() < 0.01,
            "{far:?}"
        );
        // La figura 8 tiene un lóbulo de adelante y dos mitades del de atrás.
        let eight = polar_lobes((0.0, 0.0), 0.0, MicPattern::Figure8, 1.0);
        assert_eq!(eight.iter().filter(|l| !l.rear).count(), 1);
        assert!(eight.iter().filter(|l| l.rear).count() >= 1);
    }

    fn item(kind: BlockKind) -> BlockItem {
        let (w, h) = match kind {
            BlockKind::Actor => (0.6, 0.6),
            BlockKind::Camera => (0.5, 0.35),
            BlockKind::Light => (0.4, 0.4),
            BlockKind::Modifier => (1.0, 0.1),
            BlockKind::Mic => (0.3, 0.3),
            BlockKind::Opening => (0.9, 0.15),
            _ => (1.2, 0.7),
        };
        BlockItem {
            id: 1,
            kind,
            w,
            h,
            ..BlockItem::default()
        }
    }

    /// Todas las figuras que tienen que verse distintas entre sí (§3.2), con su nombre.
    fn every_figure() -> Vec<(String, BlockItem)> {
        let mut out = vec![
            ("persona".to_string(), item(BlockKind::Actor)),
            ("cámara".to_string(), item(BlockKind::Camera)),
        ];
        for fixture in LightFixture::ALL {
            out.push((
                format!("luz {fixture:?}"),
                BlockItem {
                    light: Some(LightSpec::new(fixture)),
                    ..item(BlockKind::Light)
                },
            ));
        }
        for kind in ModifierKind::ALL {
            out.push((
                format!("modificador {kind:?}"),
                BlockItem {
                    modifier: Some(ModifierSpec {
                        kind,
                        ..ModifierSpec::default()
                    }),
                    ..item(BlockKind::Modifier)
                },
            ));
        }
        for opening in [OpeningKind::Door, OpeningKind::Window] {
            out.push((
                format!("abertura {opening:?}"),
                BlockItem {
                    opening,
                    ..item(BlockKind::Opening)
                },
            ));
        }
        for kind in MicKind::ALL {
            out.push((
                format!("micrófono {kind:?}"),
                BlockItem {
                    mic: Some(MicSpec::new(kind)),
                    ..item(BlockKind::Mic)
                },
            ));
        }
        out
    }

    /// La forma sin el color: de qué está hecha y cuánto mide cada parte.
    fn signature(parts: &[Part]) -> Vec<(Paint, bool, usize, i32, i32)> {
        parts
            .iter()
            .map(|p| {
                let xs = p.points.iter().map(|q| q.0);
                let ys = p.points.iter().map(|q| q.1);
                let span = |it: &mut dyn Iterator<Item = f32>| {
                    let v: Vec<f32> = it.collect();
                    v.iter().cloned().fold(f32::MIN, f32::max)
                        - v.iter().cloned().fold(f32::MAX, f32::min)
                };
                (
                    p.paint,
                    p.closed,
                    p.points.len(),
                    (span(&mut xs.clone()) * 100.0).round() as i32,
                    (span(&mut ys.clone()) * 100.0).round() as i32,
                )
            })
            .collect()
    }

    #[test]
    fn no_two_figures_share_a_shape() {
        let figures = every_figure();
        assert!(figures.len() >= 17);
        for (i, (a, ia)) in figures.iter().enumerate() {
            assert!(!local_figure(ia).is_empty(), "{a} no tiene forma");
            for (b, ib) in &figures[i + 1..] {
                assert_ne!(
                    signature(&local_figure(ia)),
                    signature(&local_figure(ib)),
                    "{a} y {b} tienen la misma forma"
                );
            }
        }
    }

    #[test]
    fn a_figure_is_placed_turned_and_grown_around_its_center() {
        let mut cam = item(BlockKind::Camera);
        let local = local_figure(&cam);
        (cam.x, cam.y, cam.rot) = (10.0, 4.0, 90.0);
        let placed = figure(&cam, 1.0);
        // Girada 90°: lo que miraba hacia +x ahora mira hacia +y.
        let (l, w) = (local[0].points[1], placed[0].points[1]);
        assert!((w.0 - (10.0 - l.1)).abs() < EPS && (w.1 - (4.0 + l.0)).abs() < EPS);
        // Agrandada alrededor del centro.
        let big = figure(&cam, 2.0);
        assert!((big[0].points[1].0 - (10.0 - 2.0 * l.1)).abs() < EPS);
    }

    #[test]
    fn nothing_is_drawn_under_fourteen_pixels() {
        let cam = item(BlockKind::Camera);
        let extent = body_extent(&local_figure(&cam));
        assert!((extent - 0.5).abs() < EPS);
        for px_per_m in [18.0, 30.0, 60.0, 220.0] {
            let k = body_scale(extent, px_per_m);
            assert!(
                extent * px_per_m * k >= MIN_BODY_PX - 0.01,
                "{px_per_m} px/m"
            );
        }
        assert_eq!(
            body_scale(extent, 220.0),
            1.0,
            "lo que ya es grande no cambia"
        );
        // La caña y el operador de un boom no cuentan para el tamaño del cuerpo.
        let boom = BlockItem {
            mic: Some(MicSpec::new(MicKind::Boom)),
            ..item(BlockKind::Mic)
        };
        assert!(body_extent(&local_figure(&boom)) < 0.4);
    }

    #[test]
    fn detail_drops_labels_first_then_icons_then_fills() {
        let levels: Vec<Detail> = [18.0, 24.0, 30.0, 60.0, 220.0].map(detail_for).to_vec();
        assert_eq!(
            levels[0],
            Detail {
                labels: false,
                icons: false,
                fills: false
            }
        );
        assert_eq!(
            levels[1],
            Detail {
                labels: false,
                icons: false,
                fills: true
            }
        );
        assert_eq!(
            levels[2],
            Detail {
                labels: false,
                icons: true,
                fills: true
            }
        );
        assert!(levels[3].labels && levels[4].labels);
    }

    #[test]
    fn a_beam_has_concentric_bands_that_scale_with_intensity() {
        let full = beam_bands((0.0, 0.0), 0.0, 40.0, 3.0, 4, 100.0);
        assert_eq!(full.len(), 4);
        let reach = |b: &Band| {
            b.points
                .iter()
                .skip(1)
                .map(|p| p.0.hypot(p.1))
                .fold(0.0, f32::max)
        };
        assert!((reach(&full[0]) - 3.0).abs() < EPS && (reach(&full[3]) - 0.75).abs() < EPS);
        assert!(
            full.windows(2).all(|w| reach(&w[0]) > reach(&w[1])),
            "de la más grande a la más chica"
        );
        let half = beam_bands((0.0, 0.0), 0.0, 40.0, 3.0, 4, 50.0);
        assert!((half[0].alpha * 2.0 - full[0].alpha).abs() < EPS);
        assert!(beam_bands((0.0, 0.0), 0.0, 40.0, 3.0, 4, 0.0)[0].alpha == 0.0);
        // El softbox, de borde difuso, tiene más bandas.
        assert!(band_count(LightFixture::Softbox) > band_count(LightFixture::Fresnel));
        // El haz de una práctica es un halo: da toda la vuelta.
        let halo = beam_bands((0.0, 0.0), 0.0, 360.0, 1.5, 4, 100.0);
        assert!(
            halo[0].points.iter().skip(1).any(|p| p.0 < -1.0)
                && halo[0].points.iter().skip(1).any(|p| p.0 > 1.0)
        );
    }
}
