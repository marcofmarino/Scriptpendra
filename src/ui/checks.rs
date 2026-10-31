//! Los detectores de las reglas de interfaz (reglas-interfaz.md §7): desborde en ancho y alto, alineación de controles dentro
//! de una sección o tarjeta, celdas de tabla más altas que `ROW_H` o de más de una línea, y el hueco arriba del contenido.
//!
//! El kit registra el rectángulo de cada control (`field`, `prop_row`) y mira las celdas de sus tablas; el marco avisa de
//! cada zona y de cada pantalla. En las pruebas todo queda anotado en un registro; fuera de ellas, estas funciones no hacen
//! nada. Mientras se migran las pantallas, `KNOWN_VIOLATIONS` lista las infracciones que todavía hay: los detectores las
//! ignoran, pero la prueba falla si alguna deja de ocurrir (hay que sacarla de la lista) o si aparece una nueva.

// Fuera de las pruebas el registro no existe y parte de esto no se usa.
#![allow(dead_code)]

use eframe::egui::{self, Rect};

/// Qué regla se rompió.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    /// Una zona usó más ancho del que tiene (§2.1, §2.2).
    Overflow,
    /// Una zona usó más alto del que tiene (§2.5, §2.9).
    OverflowHeight,
    /// Controles del mismo patrón en una sección o tarjeta que no empiezan o no terminan en el mismo lugar (§2.10).
    Alignment,
    /// Algo en una celda de tabla es más alto que `ROW_H` (§2.6).
    CellHeight,
    /// Una celda de tabla tiene texto en más de una línea (§2.6).
    CellLines,
    /// El primer elemento del contenido empieza a más de `SP_XL` del borde de arriba de la tarjeta (§2.9).
    TopGap,
}

impl Rule {
    pub fn name(self) -> &'static str {
        match self {
            Rule::Overflow => "desborde de ancho",
            Rule::OverflowHeight => "desborde de alto",
            Rule::Alignment => "alineación",
            Rule::CellHeight => "celda más alta que ROW_H",
            Rule::CellLines => "celda de más de una línea",
            Rule::TopGap => "hueco arriba",
        }
    }
}

/// Una infracción: en qué pantalla, de qué regla y el detalle de lo que se vio.
#[derive(Clone, Debug, PartialEq)]
pub struct Violation {
    pub screen: String,
    pub rule: Rule,
    pub detail: String,
}

/// Las infracciones que las pantallas actuales todavía tienen: (pantalla, regla). Cada paso de la migración la vacía un poco;
/// la migración termina cuando está vacía.
pub const KNOWN_VIOLATIONS: &[(&str, Rule)] = &[];

/// Cómo se dibuja un control, para saber con cuáles se compara (reglas §2.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pattern {
    /// Campo apilado: la etiqueta arriba y el control a todo el ancho.
    Stacked,
    /// Fila de propiedad: la etiqueta a la izquierda y el control en el resto.
    Prop,
}

/// Cuánto margen se admite al comparar posiciones, en puntos.
pub const TOLERANCE: f32 = 0.5;
/// Un control que termina a menos de esto del borde derecho de su zona se considera que está llenando la fila.
const FILL_WITHIN: f32 = 24.0;

/// ¿Está esta infracción en la lista de las conocidas?
pub fn is_known(screen: &str, rule: Rule) -> bool {
    KNOWN_VIOLATIONS
        .iter()
        .any(|(s, r)| *s == screen && *r == rule)
}

/// Compara las infracciones encontradas con `KNOWN_VIOLATIONS`: devuelve un texto con las que son nuevas y con las de la lista
/// que ya no ocurren, o `Ok` si coinciden exactamente.
pub fn verify(found: &[Violation]) -> Result<(), String> {
    verify_against(found, KNOWN_VIOLATIONS)
}

pub fn verify_against(found: &[Violation], known: &[(&str, Rule)]) -> Result<(), String> {
    let mut report = String::new();
    let mut seen: Vec<(&str, Rule)> = Vec::new();
    for v in found {
        let key = (v.screen.as_str(), v.rule);
        if known.contains(&key) {
            if !seen.contains(&key) {
                seen.push(key);
            }
            continue;
        }
        report.push_str(&format!(
            "Infracción nueva: {} / {}: {}\n",
            v.screen,
            v.rule.name(),
            v.detail
        ));
    }
    for (screen, rule) in known {
        if !seen.contains(&(*screen, *rule)) {
            report.push_str(&format!(
                "Ya no ocurre (sacala de KNOWN_VIOLATIONS): {screen} / {}\n",
                rule.name()
            ));
        }
    }
    if report.is_empty() {
        return Ok(());
    }
    // La lista que haría falta para que la prueba pase, para copiarla (las infracciones nuevas, sin repetir).
    let mut wanted: Vec<(String, Rule)> =
        found.iter().map(|v| (v.screen.clone(), v.rule)).collect();
    wanted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    wanted.dedup();
    report.push_str("\nKNOWN_VIOLATIONS con lo que se encontró:\n");
    for (screen, rule) in wanted {
        report.push_str(&format!("    (\"{screen}\", Rule::{rule:?}),\n"));
    }
    Err(report)
}

// ---- Los detectores sueltos (puros: reciben los rectángulos) ----

/// Desborde de ancho y de alto de una zona: lo usado contra lo disponible. El alto solo se mira en las zonas sin
/// desplazamiento (las que tienen `ScrollArea` con `auto_shrink` en falso usan justo lo disponible).
pub fn overflow_of(used: Rect, avail: Rect) -> (Option<f32>, Option<f32>) {
    let wide = (used.width() > avail.width() + TOLERANCE).then(|| used.width() - avail.width());
    let tall = (used.height() > avail.height() + TOLERANCE).then(|| used.height() - avail.height());
    (wide, tall)
}

/// Un control registrado: de qué patrón, a qué grupo (sección o tarjeta) y columna pertenece, y dónde quedó.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Control {
    pub group: u64,
    pub column: Option<(usize, usize)>,
    pub pattern: Pattern,
    pub rect: Rect,
    /// El borde derecho de la zona donde se dibujó.
    pub zone_right: f32,
}

/// Con qué se compara un control: su grupo, su columna y su patrón.
type GroupKey = (u64, Option<(usize, usize)>, Pattern);

/// Alineación: dentro de cada grupo, los controles del mismo patrón (y de la misma columna, si van en un `field_group`)
/// tienen el mismo borde izquierdo; y los que llenan la fila, el mismo borde derecho. Devuelve un texto por cada desacuerdo.
pub fn misaligned(controls: &[Control]) -> Vec<String> {
    let mut out = Vec::new();
    let mut keys: Vec<GroupKey> = Vec::new();
    for c in controls {
        let key = (c.group, c.column, c.pattern);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    for key in keys {
        let same: Vec<&Control> = controls
            .iter()
            .filter(|c| (c.group, c.column, c.pattern) == key)
            .collect();
        let Some(first) = same.first() else { continue };
        for c in &same[1..] {
            if (c.rect.left() - first.rect.left()).abs() > TOLERANCE {
                out.push(format!(
                    "empiezan en x = {:.1} y x = {:.1} ({:?})",
                    first.rect.left(),
                    c.rect.left(),
                    key.2
                ));
            }
        }
        let fills: Vec<&&Control> = same
            .iter()
            .filter(|c| c.zone_right - c.rect.right() < FILL_WITHIN)
            .collect();
        if let Some(first) = fills.first() {
            for c in &fills[1..] {
                if (c.rect.right() - first.rect.right()).abs() > TOLERANCE {
                    out.push(format!(
                        "terminan en x = {:.1} y x = {:.1} ({:?})",
                        first.rect.right(),
                        c.rect.right(),
                        key.2
                    ));
                }
            }
        }
    }
    out
}

/// Hueco arriba: lo que sobra entre el borde de arriba de la tarjeta y el primer elemento dibujado, más allá de lo que se
/// permite. `allowed` es `SP_XL` (más el margen de la hoja si el contenido es una hoja).
pub fn top_gap(card_top: f32, first_top: f32, allowed: f32) -> Option<f32> {
    let gap = first_top - card_top;
    (gap > allowed + TOLERANCE).then_some(gap)
}

// ---- El registro (solo en las pruebas) ----

#[cfg(test)]
mod registry {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct State {
        pub screen: String,
        pub violations: Vec<Violation>,
        pub controls: Vec<Control>,
        pub group: u64,
        pub next_group: u64,
        pub stack: Vec<u64>,
        pub column: Option<(usize, usize)>,
        /// La pantalla dibujó un `empty_state`, la excepción de reglas §2.9 al «contenido arriba».
        pub empty_state: bool,
    }

    thread_local! {
        pub static STATE: RefCell<State> = RefCell::new(State::default());
    }
}

/// Empieza a anotar lo que dibuja una pantalla.
#[cfg(test)]
pub fn begin_screen(name: &str) {
    registry::STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.screen = name.to_string();
        s.controls.clear();
        s.next_group += 1;
        s.group = s.next_group;
        s.stack.clear();
        s.column = None;
        s.empty_state = false;
    });
}
#[cfg(not(test))]
#[inline]
pub fn begin_screen(_name: &str) {}

/// Termina la pantalla: compara la alineación de todo lo que anotó.
#[cfg(test)]
pub fn end_screen() {
    registry::STATE.with(|s| {
        let mut s = s.borrow_mut();
        let screen = std::mem::take(&mut s.screen);
        for detail in misaligned(&s.controls) {
            s.violations.push(Violation {
                screen: screen.clone(),
                rule: Rule::Alignment,
                detail,
            });
        }
        s.controls.clear();
    });
}
#[cfg(not(test))]
#[inline]
pub fn end_screen() {}

/// La pantalla dibujó un `kit::empty_state`: es la única excepción a «el contenido arranca arriba» (reglas §2.9), así que el
/// hueco de arriba no cuenta como infracción.
#[cfg(test)]
pub fn note_empty_state() {
    registry::STATE.with(|s| s.borrow_mut().empty_state = true);
}
#[cfg(not(test))]
#[inline]
pub fn note_empty_state() {}

/// Anota una infracción de la pantalla `screen`.
#[cfg(test)]
pub fn report(screen: &str, rule: Rule, detail: String) {
    eprintln!("{}: {screen}: {detail}", rule.name());
    registry::STATE.with(|s| {
        s.borrow_mut().violations.push(Violation {
            screen: screen.to_string(),
            rule,
            detail,
        })
    });
}
#[cfg(not(test))]
pub fn report(screen: &str, rule: Rule, detail: String) {
    #[cfg(debug_assertions)]
    eprintln!("{}: {screen}: {detail}", rule.name());
    #[cfg(not(debug_assertions))]
    let _ = (screen, rule, detail);
}

/// Compara lo usado con lo disponible en una zona y avisa del desborde de ancho y de alto.
pub fn overflow(screen: &str, zone: &str, used: Rect, avail: Rect) {
    let (wide, tall) = overflow_of(used, avail);
    if let Some(excess) = wide {
        report(
            screen,
            Rule::Overflow,
            format!("{zone}: {excess:.1} pt de más de ancho"),
        );
    }
    if let Some(excess) = tall {
        report(
            screen,
            Rule::OverflowHeight,
            format!("{zone}: {excess:.1} pt de más de alto"),
        );
    }
}

/// El kit anota el rectángulo de un control.
#[cfg(test)]
pub fn control(pattern: Pattern, rect: Rect, zone_right: f32) {
    registry::STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.screen.is_empty() {
            return;
        }
        let c = Control {
            group: s.group,
            column: s.column,
            pattern,
            rect,
            zone_right,
        };
        s.controls.push(c);
    });
}
#[cfg(not(test))]
#[inline]
pub fn control(_pattern: Pattern, _rect: Rect, _zone_right: f32) {}

/// Una sección nueva: los controles de antes y de después ya no se comparan entre sí.
#[cfg(test)]
pub fn new_group() {
    registry::STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.next_group += 1;
        s.group = s.next_group;
    });
}
#[cfg(not(test))]
#[inline]
pub fn new_group() {}

/// Entra a una tarjeta: sus controles forman un grupo propio, y al salir se vuelve al grupo de afuera.
#[cfg(test)]
pub fn push_group() {
    registry::STATE.with(|s| {
        let mut s = s.borrow_mut();
        let outer = s.group;
        s.stack.push(outer);
        s.next_group += 1;
        s.group = s.next_group;
    });
}
#[cfg(not(test))]
#[inline]
pub fn push_group() {}

#[cfg(test)]
pub fn pop_group() {
    registry::STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Some(outer) = s.stack.pop() {
            s.group = outer;
        }
    });
}
#[cfg(not(test))]
#[inline]
pub fn pop_group() {}

/// Los campos de un `field_group` van uno al lado del otro: cada columna se compara solo con la suya.
#[cfg(test)]
pub fn set_column(column: Option<(usize, usize)>) {
    registry::STATE.with(|s| s.borrow_mut().column = column);
}
#[cfg(not(test))]
#[inline]
pub fn set_column(_column: Option<(usize, usize)>) {}

/// Todas las infracciones anotadas hasta ahora (las saca del registro).
#[cfg(test)]
pub fn take_violations() -> Vec<Violation> {
    registry::STATE.with(|s| std::mem::take(&mut s.borrow_mut().violations))
}

// ---- Lo que se mira en lo ya dibujado ----

/// Cuántas formas lleva dibujadas la capa de `ui`: sirve para mirar después solo las que se agregaron.
pub fn paint_mark(ui: &egui::Ui) -> usize {
    ui.ctx()
        .graphics(|g| g.get(ui.layer_id()).map_or(0, |l| l.all_entries().len()))
}

/// Lo dibujado en la capa de `ui` desde `mark` que se ve de verdad: su rectángulo (recortado a su zona) y, si es texto,
/// cuántas líneas tiene.
pub fn painted_since(ui: &egui::Ui, mark: usize) -> Vec<(Rect, Option<usize>)> {
    ui.ctx().graphics(|g| {
        let Some(list) = g.get(ui.layer_id()) else {
            return Vec::new();
        };
        list.all_entries()
            .skip(mark)
            .filter_map(|clipped| {
                let bounds = clipped.shape.visual_bounding_rect();
                let visible = bounds.intersect(clipped.clip_rect);
                if !bounds.is_finite() || visible.width() <= 0.0 || visible.height() <= 0.0 {
                    return None;
                }
                let rows = match &clipped.shape {
                    egui::Shape::Text(t) => Some(t.galley.rows.len()),
                    _ => None,
                };
                Some((bounds, rows))
            })
            .collect()
    })
}

/// Las celdas de una tabla ya dibujada (desde `mark`, dentro de `table`): nada más alto que `row_h` ni con más de una
/// línea de texto. Las formas de las barras de desplazamiento (angostas) no cuentan.
pub fn check_cells(ui: &egui::Ui, mark: usize, table: Rect, row_h: f32) {
    let screen = current_screen();
    if screen.is_empty() {
        return;
    }
    let mut tall = 0.0f32;
    let mut multi = false;
    for (bounds, rows) in painted_since(ui, mark) {
        if !table.expand(1.0).contains_rect(bounds) || bounds.width() <= 14.0 {
            continue;
        }
        tall = tall.max(bounds.height());
        multi |= rows.is_some_and(|r| r > 1);
    }
    if tall > row_h + TOLERANCE {
        report(
            &screen,
            Rule::CellHeight,
            format!("algo mide {tall:.1} pt y la fila {row_h:.1}"),
        );
    }
    if multi {
        report(
            &screen,
            Rule::CellLines,
            "una celda tiene texto en más de una línea".to_string(),
        );
    }
}

/// Hueco arriba: el primer elemento que se dibujó (desde `mark`) contra el borde de arriba de la tarjeta. Se ignoran los
/// fondos, que ocupan casi toda la tarjeta y llegan hasta sus bordes.
pub fn check_top_gap(ui: &egui::Ui, mark: usize, card: Rect, allowed: f32, screen: &str) {
    #[cfg(test)]
    if registry::STATE.with(|s| s.borrow().empty_state) {
        return;
    }
    let area = card.width() * card.height();
    let first = painted_since(ui, mark)
        .into_iter()
        .map(|(b, _)| b.intersect(card))
        .filter(|b| b.width() > 0.0 && b.height() > 0.0)
        // Un fondo ocupa casi toda la tarjeta y llega hasta sus bordes; una tarjeta o una hoja grande, con margen, no.
        .filter(|b| b.width() * b.height() < 0.5 * area || b.width() < card.width() - 2.0)
        .map(|b| b.top())
        .fold(None, |a: Option<f32>, t| Some(a.map_or(t, |a| a.min(t))));
    if let Some(first_top) = first
        && let Some(gap) = top_gap(card.top(), first_top, allowed)
    {
        report(
            screen,
            Rule::TopGap,
            format!("el primer elemento está a {gap:.1} pt del borde de arriba"),
        );
    }
}

#[cfg(test)]
fn current_screen() -> String {
    registry::STATE.with(|s| s.borrow().screen.clone())
}
#[cfg(not(test))]
fn current_screen() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, vec2};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    fn control(group: u64, pattern: Pattern, r: Rect) -> Control {
        Control {
            group,
            column: None,
            pattern,
            rect: r,
            zone_right: 300.0,
        }
    }

    #[test]
    fn overflow_looks_at_width_and_height_with_half_a_point_of_room() {
        let avail = rect(0.0, 0.0, 100.0, 50.0);
        assert_eq!(
            overflow_of(rect(0.0, 0.0, 100.4, 50.4), avail),
            (None, None)
        );
        let (wide, tall) = overflow_of(rect(0.0, 0.0, 130.0, 80.0), avail);
        assert_eq!((wide, tall), (Some(30.0), Some(30.0)));
        assert_eq!(
            overflow_of(rect(0.0, 0.0, 90.0, 80.0), avail),
            (None, Some(30.0))
        );
    }

    #[test]
    fn controls_of_a_pattern_in_a_group_must_share_their_edges() {
        let aligned = [
            control(1, Pattern::Stacked, rect(16.0, 0.0, 284.0, 32.0)),
            control(1, Pattern::Stacked, rect(16.0, 50.0, 284.0, 32.0)),
        ];
        assert!(misaligned(&aligned).is_empty());
        // Empieza en otro lugar.
        let moved = [
            aligned[0],
            control(1, Pattern::Stacked, rect(20.0, 50.0, 280.0, 32.0)),
        ];
        assert_eq!(misaligned(&moved).len(), 1, "{:?}", misaligned(&moved));
        // Termina en otro lugar, aunque casi llene la fila (una resta mágica).
        let short = [
            aligned[0],
            control(1, Pattern::Stacked, rect(16.0, 50.0, 264.0, 32.0)),
        ];
        assert_eq!(misaligned(&short).len(), 1);
        // Un control de ancho fijo (que no llena la fila) puede terminar antes.
        let fixed = [
            aligned[0],
            control(1, Pattern::Stacked, rect(16.0, 50.0, 128.0, 32.0)),
        ];
        assert!(misaligned(&fixed).is_empty());
        // Otro grupo u otro patrón no se compara.
        assert!(
            misaligned(&[
                aligned[0],
                control(2, Pattern::Stacked, rect(40.0, 50.0, 200.0, 32.0))
            ])
            .is_empty()
        );
        assert!(
            misaligned(&[
                aligned[0],
                control(1, Pattern::Prop, rect(40.0, 50.0, 200.0, 32.0))
            ])
            .is_empty()
        );
        // Los campos de un `field_group` se comparan por columna.
        let cols = [
            Control {
                column: Some((2, 0)),
                ..control(1, Pattern::Stacked, rect(16.0, 0.0, 130.0, 32.0))
            },
            Control {
                column: Some((2, 1)),
                ..control(1, Pattern::Stacked, rect(154.0, 0.0, 130.0, 32.0))
            },
            Control {
                column: Some((2, 0)),
                ..control(1, Pattern::Stacked, rect(16.0, 50.0, 130.0, 32.0))
            },
            Control {
                column: Some((2, 1)),
                ..control(1, Pattern::Stacked, rect(154.0, 50.0, 130.0, 32.0))
            },
        ];
        assert!(misaligned(&cols).is_empty());
    }

    #[test]
    fn the_top_gap_allows_sp_xl_and_flags_anything_more() {
        assert_eq!(top_gap(100.0, 124.0, 24.0), None);
        assert_eq!(top_gap(100.0, 124.4, 24.0), None);
        assert_eq!(top_gap(100.0, 190.0, 24.0), Some(90.0));
        assert_eq!(top_gap(100.0, 140.0, 48.0), None, "una hoja suma su margen");
    }

    #[test]
    fn the_known_list_must_match_what_is_found_exactly() {
        let v = |screen: &str, rule| Violation {
            screen: screen.into(),
            rule,
            detail: "x".into(),
        };
        let known = [("Escenas", Rule::Alignment), ("Plan", Rule::Overflow)];
        assert!(
            verify_against(
                &[v("Escenas", Rule::Alignment), v("Plan", Rule::Overflow)],
                &known
            )
            .is_ok()
        );
        // Algo de la lista dejó de fallar.
        let stale = verify_against(&[v("Escenas", Rule::Alignment)], &known).unwrap_err();
        assert!(
            stale.contains("Ya no ocurre") && stale.contains("Plan"),
            "{stale}"
        );
        // Apareció una infracción nueva.
        let fresh = verify_against(
            &[
                v("Escenas", Rule::Alignment),
                v("Plan", Rule::Overflow),
                v("Info", Rule::TopGap),
            ],
            &known,
        )
        .unwrap_err();
        assert!(
            fresh.contains("Infracción nueva") && fresh.contains("Info"),
            "{fresh}"
        );
        // Repetir una conocida no suma nada.
        assert!(
            verify_against(
                &[
                    v("Escenas", Rule::Alignment),
                    v("Escenas", Rule::Alignment),
                    v("Plan", Rule::Overflow)
                ],
                &known
            )
            .is_ok()
        );
    }

    #[test]
    fn a_table_with_two_line_text_and_tall_widgets_is_flagged_and_a_clean_one_is_not() {
        let ctx = egui::Context::default();
        for (text, expect_lines, expect_tall) in [
            ("una línea", false, false),
            (
                "un texto bastante largo que tiene que partirse en varias líneas porque no entra",
                true,
                true,
            ),
        ] {
            begin_screen("Prueba");
            let _ = take_violations();
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect(0.0, 0.0, 300.0, 300.0)),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        let mark = paint_mark(ui);
                        let table = ui
                            .scope(|ui| {
                                ui.set_max_width(120.0);
                                ui.label(text);
                            })
                            .response
                            .rect;
                        check_cells(ui, mark, table, 32.0);
                    });
                },
            );
            out.textures_delta.clear();
            let found = take_violations();
            assert_eq!(
                found.iter().any(|v| v.rule == Rule::CellLines),
                expect_lines,
                "{found:?}"
            );
            assert_eq!(
                found.iter().any(|v| v.rule == Rule::CellHeight),
                expect_tall,
                "{found:?}"
            );
            end_screen();
        }
    }

    /// Dibuja `add` dentro de una pantalla anotada y devuelve las infracciones de alineación.
    fn alignment_of(add: impl FnOnce(&mut egui::Ui, &crate::theme::Palette)) -> Vec<Violation> {
        let ctx = egui::Context::default();
        let p = crate::theme::Scheme::Tierra.palette();
        crate::theme::apply(&ctx, &p, 100);
        let _ = take_violations();
        begin_screen("Prueba");
        let mut add = Some(add);
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect(0.0, 0.0, 400.0, 600.0)),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if let Some(add) = add.take() {
                        add(ui, &p);
                    }
                });
            },
        );
        out.textures_delta.clear();
        end_screen();
        take_violations()
    }

    #[test]
    fn the_kit_registers_its_fields_and_flags_one_that_starts_somewhere_else() {
        use crate::ui::kit;
        let (mut a, mut b) = (String::new(), String::new());
        let clean = alignment_of(|ui, p| {
            kit::section(ui, p, "Plano", None);
            kit::field(ui, p, "Tipo", &mut a, 1);
            kit::field(ui, p, "Descripción", &mut b, 3);
        });
        assert!(clean.is_empty(), "{clean:?}");
        let crooked = alignment_of(|ui, p| {
            kit::section(ui, p, "Plano", None);
            kit::field(ui, p, "Tipo", &mut a, 1);
            ui.indent("x", |ui| {
                kit::field(ui, p, "Descripción", &mut b, 3);
            });
        });
        assert!(
            crooked.iter().any(|v| v.rule == Rule::Alignment),
            "{crooked:?}"
        );
        // En otra sección (o en otra tarjeta) ya no se comparan.
        let apart = alignment_of(|ui, p| {
            kit::section(ui, p, "Plano", None);
            kit::field(ui, p, "Tipo", &mut a, 1);
            kit::section(ui, p, "Contenido", None);
            ui.indent("x", |ui| {
                kit::field(ui, p, "Descripción", &mut b, 3);
            });
        });
        assert!(apart.is_empty(), "{apart:?}");
    }

    #[test]
    fn property_rows_and_field_groups_are_compared_by_pattern_and_column() {
        use crate::ui::kit;
        let (mut a, mut b, mut c) = (String::new(), String::new(), String::new());
        let found = alignment_of(|ui, p| {
            kit::section(ui, p, "Horarios", None);
            kit::prop_row(ui, p, "Fecha", |ui| {
                let w = kit::fill_width(ui, 40.0, 1000.0);
                ui.add_sized(egui::vec2(w, 32.0), egui::Button::new("x"));
            });
            kit::prop_row(ui, p, "Duración", |ui| {
                let w = kit::fill_width(ui, 40.0, 1000.0);
                ui.add_sized(egui::vec2(w, 32.0), egui::Button::new("y"));
            });
            kit::field_group(ui, 2, |ui, i| {
                kit::field(
                    ui,
                    p,
                    if i == 0 { "Citación" } else { "Fin" },
                    if i == 0 { &mut a } else { &mut b },
                    1,
                );
            });
            kit::field_group(ui, 2, |ui, i| {
                kit::field(
                    ui,
                    p,
                    if i == 0 { "Desde" } else { "Hasta" },
                    if i == 0 { &mut c } else { &mut a },
                    1,
                );
            });
        });
        assert!(found.is_empty(), "{found:?}");
    }
}
