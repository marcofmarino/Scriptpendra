//! El marco de la ventana (interfaz.md §1): las zonas las dibuja el shell, nunca la pantalla.
//! Por ahora están la barra de herramientas y la barra de estado; el resto se suma a medida que se migra.

use eframe::egui::{self, Align, Layout, RichText, Stroke, Ui};

use super::{
    checks, kit,
    screen::{Screen, ScreenCx, ScreenKind},
    tokens::*,
};
use crate::{
    icon,
    theme::{self, Palette},
};

fn bar_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.panel)
        .inner_margin(egui::Margin::symmetric(SP_M as i8, 0))
}

/// Qué se puede hacer en el historial de la pantalla: los botones Deshacer y Rehacer los dibuja el marco.
#[derive(Clone, Copy, Default)]
pub struct History {
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Lo que se pidió en los botones de historial.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HistoryAct {
    Undo,
    Redo,
}

/// Qué parte de la barra de herramientas se está dibujando.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// Grupos propios de la pantalla, que se pliegan a `⋯` si no entran.
    Center,
    /// Ver y sacar (zoom, Exportar), contra el borde derecho de la ventana.
    End,
}

/// Fila del título de la pantalla (`TITLE_H`): el título a 24 del borde izquierdo, alineado con el margen del contenido.
pub fn title_row(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) {
    egui::Panel::top("shell_title")
        .exact_size(TITLE_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(p.panel).inner_margin(egui::Margin {
            left: SP_XL as i8,
            right: SP_M as i8,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
            ui.with_layout(Layout::left_to_right(Align::Center), add);
        });
}

/// Barra de herramientas (`TOOLBAR_H`): a la izquierda Deshacer y Rehacer (los dibuja el marco), en el centro lo propio de la
/// pantalla (`draw(ui, Zone::Center)`) y contra el borde derecho de la ventana lo de `Zone::End`. Ocupa todo el ancho que
/// queda a la derecha de la barra lateral, sobre el contenido y el inspector. Nunca pasa a dos líneas.
pub fn toolbar(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    history: History,
    mut draw: impl FnMut(&mut Ui, Zone),
) -> Option<HistoryAct> {
    let mut act = None;
    egui::Panel::top("shell_toolbar")
        .exact_size(TOOLBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(p.panel)
                .inner_margin(egui::Margin::symmetric(BAR_PAD_X as i8, 0)),
        )
        .show(ui, |ui| {
            // Lo que no entra se recorta: la barra nunca pasa a dos líneas.
            ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = SP_XS;
                draw(ui, Zone::End);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = SP_XS;
                    if kit::icon_button(
                        ui,
                        p,
                        icon::ARROW_COUNTER_CLOCKWISE,
                        "Deshacer (Ctrl+Z)",
                        history.can_undo,
                    )
                    .clicked()
                    {
                        act = Some(HistoryAct::Undo);
                    }
                    if kit::icon_button(
                        ui,
                        p,
                        icon::ARROW_CLOCKWISE,
                        "Rehacer (Ctrl+Shift+Z)",
                        history.can_redo,
                    )
                    .clicked()
                    {
                        act = Some(HistoryAct::Redo);
                    }
                    ui.separator();
                    draw(ui, Zone::Center);
                });
            });
            check_overflow(title, "barra de herramientas", ui.min_rect(), ui.max_rect());
        });
    act
}

/// Datos de la barra de estado (interfaz.md §1.5).
pub struct Status<'a> {
    pub saved: &'a str,
    pub message: &'a str,
    pub info: &'a str,
    /// El botón que muestra u oculta la barra lateral, en el extremo izquierdo.
    pub sidebar: &'a mut bool,
    /// El botón que muestra u oculta el inspector, en el extremo derecho (solo en pantallas que tienen).
    pub inspector: Option<&'a mut bool>,
    /// La campanita: si se muestran los avisos de lo que cambian otras personas.
    pub notifications: &'a mut bool,
}

/// Un botón de la barra de estado: del alto de la barra, con el fundido de pasar el mouse.
fn status_button(ui: &mut Ui, p: &Palette, code: &str, tip: &str, on: bool) -> egui::Response {
    let side = STATUS_H - 4.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side + 4.0, side), egui::Sense::click());
    let hover = kit::fade(ui, resp.id.with("hover"), resp.hovered(), ANIM_FAST);
    ui.painter().rect_filled(
        rect,
        R_CONTROL,
        theme::mix(p.panel, p.container_hi, hover).gamma_multiply(hover),
    );
    let color = if on { p.accent } else { p.dim };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        code,
        egui::FontId::proportional(ICON_S),
        theme::mix(color, p.text, hover),
    );
    resp.on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Barra de estado: a la izquierda el botón de la barra lateral y el estado del guardado, al centro el último mensaje y a la
/// derecha los datos de la pantalla y, en el extremo, el botón del inspector.
pub fn status_bar(ui: &mut Ui, p: &Palette, status: Status) {
    egui::Panel::bottom("shell_status")
        .exact_size(STATUS_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(bar_frame(p))
        .show(ui, |ui| {
            let small = |text: &str| RichText::new(text).size(FONT_SMALL).color(p.dim);
            let Status {
                saved,
                message,
                info,
                sidebar,
                inspector,
                notifications,
            } = status;
            ui.columns(3, |cols| {
                cols[0].with_layout(Layout::left_to_right(Align::Center), |ui| {
                    if status_button(
                        ui,
                        p,
                        icon::SIDEBAR_SIMPLE,
                        "Mostrar u ocultar la barra lateral (Ctrl+\\)",
                        *sidebar,
                    )
                    .clicked()
                    {
                        *sidebar = !*sidebar;
                    }
                    ui.add(egui::Label::new(small(saved)).truncate());
                });
                cols[1].with_layout(
                    Layout::top_down(Align::Center).with_cross_justify(false),
                    |ui| {
                        ui.add(egui::Label::new(small(message)).truncate());
                    },
                );
                cols[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(inspector) = inspector
                        && status_button(
                            ui,
                            p,
                            icon::SIDEBAR,
                            "Mostrar u ocultar el inspector (Ctrl+Mayús+\\)",
                            *inspector,
                        )
                        .clicked()
                    {
                        *inspector = !*inspector;
                    }
                    if status_button(
                        ui,
                        p,
                        if *notifications { icon::BELL } else { icon::BELL_SLASH },
                        if *notifications { "Silenciar los avisos de cambios" } else { "Activar los avisos de cambios" },
                        *notifications,
                    )
                    .clicked()
                    {
                        *notifications = !*notifications;
                    }
                    ui.add(egui::Label::new(small(info)).truncate());
                });
            });
        });
}

/// Panel derecho redimensionable que crea el shell (interfaz.md §1.4). Se abre y se cierra con un cambio de ancho
/// animado. Devuelve su ancho medido solo cuando está quieto y abierto, para recordarlo (reglas §6.7).
pub fn inspector_panel(
    ui: &mut Ui,
    p: &Palette,
    id: &'static str,
    default_width: f32,
    open: &mut bool,
    add: impl FnOnce(&mut Ui),
) -> Option<f32> {
    let settled = ui.ctx().animate_bool_with_time(
        egui::Id::unique("inspector_open").with(id),
        *open,
        kit::anim(ANIM_SLOW),
    ) >= 1.0;
    let shown = egui::Panel::right(id)
        .default_size(default_width)
        .size_range(crate::storage::RIGHT_RANGE)
        .resizable(true)
        .frame(
            egui::Frame::new().fill(p.panel),
        )
        .show_collapsible(ui, open, add);
    shown
        .filter(|_| settled && *open)
        .map(|r| r.response.rect.width())
}

/// Qué parte del inspector se está dibujando.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Footer,
    Body,
}

/// La estructura fija del inspector: pestañas arriba (tres como máximo), cuerpo con un único desplazamiento y pie
/// fijo, fuera del desplazamiento, con las acciones principales.
pub struct Inspector<'a> {
    pub id: &'a str,
    pub tabs: &'a [&'a str],
    pub active: usize,
}

impl<'a> Inspector<'a> {
    pub fn new(id: &'a str, tabs: &'a [&'a str], active: usize) -> Self {
        debug_assert!(
            tabs.len() <= 3,
            "el inspector tiene tres pestañas como máximo"
        );
        Self { id, tabs, active }
    }

    /// Dibuja el inspector con una sola función para el pie y el cuerpo: se llama primero con `Part::Footer` (si `has_footer`) y
    /// después con `Part::Body`. Así las dos partes pueden usar lo mismo sin pedirlo prestado a la vez.
    pub fn show_parts(
        self,
        ui: &mut Ui,
        p: &Palette,
        has_footer: bool,
        mut draw: impl FnMut(&mut Ui, Part),
    ) -> Option<usize> {
        // Pestañas con 16 a los lados; el subrayado de la activa queda pegado al divisor.
        let picked = egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(SP_L as i8, 0))
            .show(ui, |ui| {
                crate::ui::kit::tab_row(ui, p, self.tabs, self.active)
            })
            .inner;
        ui.separator();
        if has_footer {
            egui::Panel::bottom(ui.scope_id().with((self.id, "footer")))
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(SP_L as i8, SP_M as i8))
                        .stroke(Stroke::new(1.0, theme::mix(p.panel, p.border, 0.5))),
                )
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = SP_S;
                    draw(ui, Part::Footer);
                });
        }
        egui::ScrollArea::vertical()
            .id_salt((self.id, "body"))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: SP_L as i8,
                        right: SP_L as i8,
                        top: SP_L as i8,
                        bottom: SP_L as i8,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        draw(ui, Part::Body);
                    });
            });
        picked
    }
}

/// Ancho del cajón del inspector sobre un contenido de ancho `content`.
pub fn drawer_width(content: egui::Rect) -> f32 {
    (content.width() - SP_XL).clamp(*crate::storage::RIGHT_RANGE.start(), 360.0)
}

/// El inspector como cajón superpuesto sobre el contenido, para ventanas angostas (reglas §2.7). Entra deslizándose
/// desde la derecha, con sombra, y el contenido de atrás se oscurece con un fundido (reglas §6.7). Se cierra con Esc o
/// con un clic afuera: en ese caso deja `open` en falso. Mientras sale se sigue dibujando hasta que termina.
pub fn inspector_drawer(
    ctx: &egui::Context,
    p: &Palette,
    content: egui::Rect,
    width: f32,
    open: &mut bool,
    add: impl FnOnce(&mut Ui),
) {
    let id = egui::Id::unique("inspector_drawer");
    let t = ctx.animate_bool_with_time(id.with("t"), *open, kit::anim(ANIM_SLOW));
    if t <= 0.0 && !*open {
        return;
    }
    let eased = if *open {
        egui::emath::easing::cubic_out(t)
    } else {
        egui::emath::easing::cubic_in(t)
    };
    // El contenido de atrás se oscurece.
    ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, id.with("dim")))
        .rect_filled(content, 0, p.shadow.gamma_multiply(0.5 * eased));
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(content.right_top() + egui::vec2((1.0 - eased) * width, 0.0))
        .pivot(egui::Align2::RIGHT_TOP)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.border))
                .shadow(egui::Shadow {
                    offset: [-2, 0],
                    blur: 12,
                    spread: 0,
                    color: p.shadow,
                })
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.set_height(content.height());
                    add(ui);
                });
        });
    let outside = ctx.input(|i| {
        i.pointer.primary_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !area.response.rect.contains(pos))
    });
    if *open
        && (ctx.input(|i| i.key_pressed(egui::Key::Escape))
            || (outside && !egui::Popup::is_any_open(ctx)))
    {
        *open = false;
    }
}

/// Dibuja una pantalla sin el marco del proyecto (Inicio): solo el contenido, con su margen.
pub fn show_bare(ui: &mut Ui, screen: &mut dyn Screen, cx: &mut ScreenCx) {
    let p = cx.palette;
    let title = screen.title(cx);
    checks::begin_screen(&title);
    let kind = screen.kind();
    let mark = checks::paint_mark(ui);
    let shown = egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(p.bg)
                .inner_margin(egui::Margin::same(CONTENT_PAD as i8)),
        )
        .show(ui, |ui| {
            screen.content(ui, cx);
            check_overflow(&title, "contenido", ui.min_rect(), ui.max_rect());
        });
    if let Some(allowed) = top_gap_allowed(kind) {
        checks::check_top_gap(ui, mark, shown.response.rect, allowed, &title);
    }
    checks::end_screen();
}

/// Lo que el marco necesita saber de la app para dibujar una pantalla.
pub struct ShellFlags<'a> {
    /// ¿Está pedido el inspector? Es el panel fijo, o el cajón si `compact`.
    pub inspector_on: &'a mut bool,
    /// El contenido no entra al lado del inspector: va como cajón (reglas §2.7).
    pub compact: bool,
    /// Pestaña del inspector que se ve (la recuerda la app por pantalla).
    pub inspector_tab: &'a mut usize,
    /// Id del panel derecho (para recordar su ancho) y el ancho con el que se crea.
    pub panel_id: &'static str,
    pub panel_default: f32,
    /// Clave de la pantalla: cuando cambia, el contenido aparece con un fundido.
    pub page_key: &'a str,
}

/// Lo que devuelve el marco.
#[derive(Default)]
pub struct ShellOut {
    /// Ancho medido del inspector fijo, para recordarlo.
    pub inspector_width: Option<f32>,
}

fn inspector_contents(
    ui: &mut Ui,
    screen: &mut dyn Screen,
    cx: &mut ScreenCx,
    tab: &mut usize,
    id: &str,
) {
    let p = cx.palette;
    let tabs = screen.inspector_tabs(cx);
    let active = (*tab).min(tabs.len().saturating_sub(1));
    let footer = screen.has_inspector_footer(active, cx);
    let picked =
        Inspector::new(id, tabs, active).show_parts(ui, p, footer, |ui, part| match part {
            Part::Footer => screen.inspector_footer(ui, active, cx),
            Part::Body => screen.inspector(ui, active, cx),
        });
    if let Some(i) = picked {
        *tab = i;
    }
}

/// Dibuja una pantalla dentro del marco: inspector (panel o cajón), barra de herramientas, barra de contexto y contenido.
/// La barra lateral y la de estado las dibuja la app antes, porque no dependen de la pantalla.
pub fn show_screen(
    ui: &mut Ui,
    screen: &mut dyn Screen,
    cx: &mut ScreenCx,
    flags: ShellFlags,
) -> ShellOut {
    let p = cx.palette;
    let mut out = ShellOut::default();
    screen.enter(cx);
    let title = screen.title(cx);
    checks::begin_screen(&title);
    let has_inspector = !screen.inspector_tabs(cx).is_empty();

    // Arriba, a todo el ancho que queda a la derecha de la barra lateral (sobre el contenido y el inspector): la fila del
    // título y la barra de herramientas.
    title_row(ui, p, |ui| {
        if !screen.title_ui(ui, cx) {
            kit::screen_title(ui, p, &title);
        }
    });
    let history = History {
        can_undo: screen.can_undo(cx),
        can_redo: screen.can_redo(cx),
    };
    let act = toolbar(ui, p, &title, history, |ui, zone| match zone {
        Zone::Center => screen.toolbar(ui, cx),
        Zone::End => screen.toolbar_end(ui, cx),
    });
    match act {
        Some(HistoryAct::Undo) => screen.undo(cx),
        Some(HistoryAct::Redo) => screen.redo(cx),
        None => {}
    }

    // Debajo, el inspector (a la altura de las pestañas de los espacios de trabajo) y la tarjeta de contenido.
    if has_inspector && !flags.compact {
        out.inspector_width = inspector_panel(
            ui,
            p,
            flags.panel_id,
            flags.panel_default,
            flags.inspector_on,
            |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                // Lo del inspector no se compara con lo del contenido (reglas §7.3).
                checks::new_group();
                inspector_contents(ui, screen, cx, flags.inspector_tab, flags.panel_id);
                check_overflow(&title, "inspector", ui.min_rect(), ui.max_rect());
            },
        );
    }
    let kind = screen.kind();
    let margin = match kind {
        ScreenKind::Sheet | ScreenKind::Canvas => 0.0,
        ScreenKind::Preview | ScreenKind::Table | ScreenKind::Card | ScreenKind::Grid => {
            CONTENT_PAD
        }
    };
    // Cambio de pantalla: el contenido aparece con un fundido; el marco queda quieto.
    let ctx = ui.ctx().clone();
    let key = egui::Id::unique("shell_page_key");
    let fade_id = egui::Id::unique("shell_page_fade");
    if ctx.data(|d| d.get_temp::<String>(key)).as_deref() != Some(flags.page_key) {
        ctx.data_mut(|d| d.insert_temp(key, flags.page_key.to_string()));
        ctx.animate_value_with_time(fade_id, 0.0, 0.0);
    }
    let opacity = ctx.animate_value_with_time(fade_id, 1.0, kit::anim(ANIM_FAST));
    let has_context = screen.has_context_bar(cx);
    let mut gap: Option<(usize, egui::Rect)> = None;
    // La tarjeta: fondo `bg`, esquinas de `R_FLOAT` y `SP_S` de separación con los demás paneles. Cada zona se recorta a su
    // rectángulo.
    let central = egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(p.bg)
                .corner_radius(R_FLOAT)
                .outer_margin(egui::Margin::same(SP_S as i8)),
        )
        .show(ui, |ui| {
            ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
            ui.set_opacity(opacity);
            ui.spacing_mut().item_spacing.y = 0.0;
            if has_context {
                context_bar(ui, p, |ui| screen.context_bar(ui, cx));
            }
            let body = egui::Rect::from_min_max(ui.cursor().min, ui.max_rect().max);
            let mark = checks::paint_mark(ui);
            egui::Frame::new()
                .inner_margin(egui::Margin::same(margin as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = ctx.global_style().spacing.item_spacing.y;
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    checks::new_group();
                    screen.content(ui, cx);
                    check_overflow(&title, "contenido", ui.min_rect(), ui.max_rect());
                });
            gap = Some((mark, body));
        });
    if let (Some(allowed), Some((mark, body))) = (top_gap_allowed(kind), gap) {
        checks::check_top_gap(ui, mark, body, allowed, &title);
    }
    // Con poco ancho el inspector no empuja el contenido: se abre como cajón encima.
    if has_inspector && flags.compact {
        let rect = central.response.rect;
        let width = drawer_width(rect);
        let mut open = *flags.inspector_on;
        inspector_drawer(&ctx, p, rect, width, &mut open, |ui| {
            checks::new_group();
            inspector_contents(ui, screen, cx, flags.inspector_tab, flags.panel_id);
        });
        *flags.inspector_on = open;
    }
    if cx.open_inspector {
        cx.open_inspector = false;
        *flags.inspector_on = true;
    }
    if let Some(tab) = cx.inspector_tab.take() {
        *flags.inspector_tab = tab;
    }
    checks::end_screen();
    out
}

/// Compara lo usado con lo disponible en una zona y avisa si se pasó, en ancho o en alto (reglas §7).
pub fn check_overflow(screen: &str, zone: &str, used: egui::Rect, avail: egui::Rect) {
    checks::overflow(screen, zone, used, avail);
}

/// Cuánto baja el texto dentro de una fila de `ROW_H` (la fila, el botón o la casilla): lo primero que se ve de un elemento
/// suele ser su texto, no su caja.
const TEXT_INSET: f32 = 11.0;

/// Cuánto se permite entre el borde de arriba de la tarjeta y el primer elemento del contenido: `SP_XL`, y otro tanto si el
/// contenido es una hoja, que lleva su propio margen (reglas §7.5). Se suma `TEXT_INSET` porque lo primero que se ve de una fila
/// o de un botón es su texto, que queda un poco adentro de su caja. Los lienzos no tienen hueco que medir.
fn top_gap_allowed(kind: ScreenKind) -> Option<f32> {
    match kind {
        ScreenKind::Canvas => None,
        ScreenKind::Table | ScreenKind::Card | ScreenKind::Grid => Some(SP_XL + TEXT_INSET),
        ScreenKind::Sheet | ScreenKind::Preview => Some(2.0 * SP_XL + TEXT_INSET),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zone_bigger_than_its_space_is_reported_in_width_and_in_height() {
        let _ = checks::take_violations();
        let avail = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 20.0));
        let used = |w: f32, h: f32| egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h));
        check_overflow("Prueba", "contenido", used(100.4, 20.4), avail);
        assert!(
            checks::take_violations().is_empty(),
            "0,4 pt de más es redondeo"
        );
        check_overflow("Prueba", "contenido", used(130.0, 20.0), avail);
        let found = checks::take_violations();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, checks::Rule::Overflow);
        check_overflow("Prueba", "contenido", used(100.0, 45.0), avail);
        assert_eq!(
            checks::take_violations()[0].rule,
            checks::Rule::OverflowHeight
        );
    }

    #[test]
    fn the_inspector_keeps_its_footer_outside_the_scroll() {
        let p = theme::Scheme::Tierra.palette();
        let ctx = egui::Context::default();
        let mut footer_top = f32::MAX;
        let mut body_bottom = 0.0f32;
        for n in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 500.0),
                )),
                time: Some(n as f64 * 0.1),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let mut open = true;
                inspector_panel(ui, &p, "test_side", 300.0, &mut open, |ui| {
                    let picked = Inspector::new("test", &["Uno", "Dos"], 0).show_parts(
                        ui,
                        &p,
                        true,
                        |ui, part| match part {
                            Part::Footer => {
                                footer_top = ui.min_rect().top();
                                let _ = ui.button("Exportar");
                            }
                            Part::Body => {
                                for i in 0..80 {
                                    ui.label(format!("fila {i}"));
                                }
                                body_bottom = ui.min_rect().bottom();
                            }
                        },
                    );
                    assert!(picked.is_none());
                });
            });
            out.textures_delta.clear();
        }
        assert!(
            footer_top < 500.0 && footer_top > 300.0,
            "el pie quedó en {footer_top}"
        );
        assert!(body_bottom > 500.0, "el cuerpo se desplaza: {body_bottom}");
    }
}

/// Barra de contexto (opcional): qué estoy mirando y las herramientas que actúan sobre el contenido. Es la primera fila de la
/// tarjeta de contenido (`CONTEXT_H`, con 12 a los lados) y lleva un divisor debajo. Los controles que no entran se recortan.
pub fn context_bar(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) {
    // Las pantallas que arman su propia barra con esta función dentro de `Screen::context_bar` no la duplican.
    let flag = egui::Id::unique("shell_in_context_bar");
    if ui.ctx().data(|d| d.get_temp::<bool>(flag)).unwrap_or(false) {
        add(ui);
        return;
    }
    ui.ctx().data_mut(|d| d.insert_temp(flag, true));
    let width = ui.available_width();
    let row = ui
        .allocate_ui_with_layout(
            egui::vec2(width, CONTEXT_H),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                ui.spacing_mut().item_spacing.x = SP_S;
                ui.add_space(BAR_PAD_X);
                add(ui);
            },
        )
        .response
        .rect;
    ui.ctx().data_mut(|d| d.insert_temp(flag, false));
    ui.painter().hline(
        row.x_range(),
        row.bottom(),
        Stroke::new(1.0, theme::mix(p.bg, p.border, 0.6)),
    );
}
