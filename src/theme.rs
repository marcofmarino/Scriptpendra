//! Tipografía y temas de color. Es el mismo estilo que Hypnotica: Rubik, iconos de Phosphor
//! y paletas con nombre (Chubut por defecto).

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke,
    TextStyle,
};
use serde::{Deserialize, Serialize};

use crate::ui::tokens::CONTROL_H;

/// Fuente de los textos con peso (títulos, selección).
pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Una sans con las medidas de Arial (Liberation Sans): el tratamiento ocupa en la hoja lo mismo que en Arial.
pub fn sans_family() -> FontFamily {
    FontFamily::Name("sans".into())
}

pub fn sans_bold_family() -> FontFamily {
    FontFamily::Name("sans_bold".into())
}

/// Courier en negrita, para los nombres resaltados del guion.
pub fn script_bold_family() -> FontFamily {
    FontFamily::Name("script_bold".into())
}

pub fn icon_text(code: &str, size: f32) -> RichText {
    RichText::new(code).size(size)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let data: [(&str, &'static [u8]); 7] = [
        (
            "Rubik-Regular",
            include_bytes!("../assets/fonts/Rubik-Regular.ttf"),
        ),
        (
            "Rubik-SemiBold",
            include_bytes!("../assets/fonts/Rubik-SemiBold.ttf"),
        ),
        ("Phosphor", include_bytes!("../assets/fonts/Phosphor.ttf")),
        (
            "CourierPrime",
            include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"),
        ),
        (
            "CourierPrimeBold",
            include_bytes!("../assets/fonts/CourierPrime-Bold.ttf"),
        ),
        (
            "LiberationSans",
            include_bytes!("../assets/fonts/LiberationSans-Regular.ttf"),
        ),
        (
            "LiberationSansBold",
            include_bytes!("../assets/fonts/LiberationSans-Bold.ttf"),
        ),
    ];
    for (name, bytes) in data {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    // Phosphor queda de respaldo de la familia común: donde falta un carácter (un icono) se usa él.
    let common = fonts.families.entry(FontFamily::Proportional).or_default();
    common.insert(0, "Rubik-Regular".into());
    common.push("Phosphor".into());
    let fallback: Vec<String> = common.iter().skip(1).cloned().collect();

    let mut bold = vec!["Rubik-SemiBold".to_string()];
    bold.extend(fallback);
    fonts.families.insert(bold_family(), bold);

    fonts.families.insert(
        sans_family(),
        vec!["LiberationSans".into(), "Phosphor".into()],
    );
    fonts.families.insert(
        sans_bold_family(),
        vec!["LiberationSansBold".into(), "Phosphor".into()],
    );
    fonts.families.insert(
        script_bold_family(),
        vec!["CourierPrimeBold".into(), "Phosphor".into()],
    );

    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.insert(0, "CourierPrime".into());
    mono.push("Phosphor".into());
    ctx.set_fonts(fonts);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scheme {
    #[default]
    // Antes se llamaba Chubut (y antes de eso Tierra): lo guardado con cualquiera de los dos nombres sigue siendo este tema.
    #[serde(alias = "Chubut")]
    Tierra,
    Gruvbox,
    Catppuccin,
    Nord,
    KanagawaLotus,
    CatppuccinLatte,
    GruvboxLight,
    /// La provincia de Chubut: la meseta árida y el rojo de sus formaciones rocosas, de día (claro) y de noche (oscuro).
    ChubutDia,
    ChubutNoche,
    /// San Lorenzo de Almagro: el azul y el grana del Ciclón.
    SanLorenzo,
    /// Scolopendra: violeta sobre fondos casi negros (oscuro) o lavanda muy suave (claro).
    ScolopendraClaro,
    ScolopendraOscuro,
}

impl Scheme {
    pub const ALL: [Scheme; 12] = [
        Scheme::Tierra,
        Scheme::Gruvbox,
        Scheme::Catppuccin,
        Scheme::Nord,
        Scheme::KanagawaLotus,
        Scheme::CatppuccinLatte,
        Scheme::GruvboxLight,
        Scheme::ChubutDia,
        Scheme::ChubutNoche,
        Scheme::SanLorenzo,
        Scheme::ScolopendraClaro,
        Scheme::ScolopendraOscuro,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scheme::Tierra => "Tierra",
            Scheme::Gruvbox => "Gruvbox",
            Scheme::Catppuccin => "Catppuccin Mocha",
            Scheme::Nord => "Nord",
            Scheme::KanagawaLotus => "Kanagawa Lotus (claro)",
            Scheme::CatppuccinLatte => "Catppuccin Latte (claro)",
            Scheme::GruvboxLight => "Gruvbox Light (claro)",
            Scheme::ChubutDia => "Chubut día (claro)",
            Scheme::ChubutNoche => "Chubut noche",
            Scheme::SanLorenzo => "San Lorenzo",
            Scheme::ScolopendraClaro => "Scolopendra claro",
            Scheme::ScolopendraOscuro => "Scolopendra oscuro",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Scheme::Tierra => Palette::new(
                [0x120d08, 0x19120b, 0x271e15, 0x352a1f, 0x0e0a06, 0x52463a],
                [0xf5e2d2, 0xb9a899],
                [0xedbe8b, 0x6e4b22],
            ),
            Scheme::Gruvbox => Palette::new(
                [0x1d2021, 0x282828, 0x3c3836, 0x504945, 0x1d2021, 0x665c54],
                [0xfbf1c7, 0xa89984],
                [0xfabd2f, 0x6b5a1f],
            ),
            Scheme::Catppuccin => Palette::new(
                [0x11111b, 0x1e1e2e, 0x313244, 0x45475a, 0x181825, 0x585b70],
                [0xcdd6f4, 0xa6adc8],
                [0xcba6f7, 0x4a3f73],
            ),
            Scheme::Nord => Palette::new(
                [0x242933, 0x2e3440, 0x3b4252, 0x4c566a, 0x242933, 0x4c566a],
                [0xeceff4, 0xd8dee9],
                [0x88c0d0, 0x3d5a6a],
            ),
            Scheme::KanagawaLotus => Palette::with_roles(
                [0xf2ecbc, 0xdcd5ac, 0xe5ddb0, 0xd5cea3, 0xf9f5d2, 0xc9c19a],
                [0x2b2b3a, 0x716e61],
                [0x4d699b, 0xc7d7e0],
                Roles {
                    dark: Some(false),
                    ..Roles::default()
                },
            ),
            Scheme::CatppuccinLatte => Palette::with_roles(
                [0xeff1f5, 0xe6e9ef, 0xdce0e8, 0xccd0da, 0xf7f8fb, 0xbcc0cc],
                [0x4c4f69, 0x6c6f85],
                [0x8839ef, 0xd9c2f7],
                Roles {
                    dark: Some(false),
                    ..Roles::default()
                },
            ),
            Scheme::ChubutDia => Palette::with_roles(
                // La meseta: arena y arcilla, con el rojo de las formaciones rocosas.
                [0xf6eee2, 0xe8d9c3, 0xf0e4d0, 0xdcc7a8, 0xfbf6ec, 0xb89a76],
                [0x3a2a22, 0x6b5648],
                [0xb5532f, 0xf0cbb5],
                Roles {
                    dark: Some(false),
                    ..Roles::default()
                },
            ),
            Scheme::ChubutNoche => Palette::new(
                // Una noche de meseta: tierra oscura y el rojo de la roca.
                [0x14100e, 0x1d1613, 0x2b201b, 0x3b2c24, 0x120d0b, 0x5e473a],
                [0xefe2d3, 0xb5a290],
                [0xd9683f, 0x4a2519],
            ),
            Scheme::ScolopendraClaro => Palette::with_roles(
                [0xf5f1fa, 0xe6dff0, 0xeee8f6, 0xd7cbe6, 0xfbf9fe, 0xa898c2],
                [0x251c33, 0x5f5272],
                [0x7b3fc4, 0xdccdf2],
                Roles {
                    dark: Some(false),
                    ..Roles::default()
                },
            ),
            Scheme::ScolopendraOscuro => Palette::new(
                [0x0f0b16, 0x16101f, 0x231a31, 0x30244a, 0x0c0912, 0x54437a],
                [0xece6f5, 0xb0a4c6],
                [0x9b5de5, 0x3b2763],
            ),
            Scheme::SanLorenzo => Palette::new(
                [0x0a1020, 0x0f1830, 0x1a2748, 0x253662, 0x080d1a, 0x3a4b7a],
                [0xf3f0f0, 0xaab3cc],
                [0xd7263d, 0x5a1c2a],
            ),
            Scheme::GruvboxLight => Palette::with_roles(
                [0xfbf1c7, 0xebdbb2, 0xf2e5bc, 0xd5c4a1, 0xf9f5d7, 0xbdae93],
                [0x3c3836, 0x7c6f64],
                [0xb57614, 0xf2d58a],
                Roles {
                    dark: Some(false),
                    ..Roles::default()
                },
            ),
        }
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Capas de superficie, texto y colores principales de un tema (hoy todos los temas son oscuros).
#[derive(Clone, Copy)]
// Los roles nuevos los van tomando las pantallas a medida que se migran.
#[allow(dead_code)]
pub struct Palette {
    pub bg: Color32,
    /// Paneles y barras laterales.
    pub panel: Color32,
    /// Botones y tarjetas.
    pub container: Color32,
    pub container_hi: Color32,
    /// Campos de texto y fondos hundidos.
    pub field: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub primary: Color32,
    /// Fondo de lo seleccionado.
    pub selection: Color32,
    /// Color vivo que marca lo elegido.
    pub accent: Color32,
    /// Estados: aprobado, aviso (consumo excedido, actor no disponible) y peligro (eliminar).
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    /// Texto sobre un fondo `accent`.
    pub on_accent: Color32,
    /// Fondo y grilla de los lienzos.
    pub canvas: Color32,
    pub grid: Color32,
    /// Sombra del cajón y de los diálogos, y oscurecimiento del fondo.
    pub shadow: Color32,
    /// Un color por planta (cámara, luces, sonido), con contraste suficiente contra `canvas` y separados entre sí.
    pub plan_camera: Color32,
    pub plan_light: Color32,
    pub plan_sound: Color32,
    /// Hoja de las vistas previas de impresión (siempre clara, en todos los temas) y su tinta.
    pub paper: Color32,
    pub paper_ink: Color32,
    /// Si el tema es oscuro.
    pub dark: bool,
}

/// Roles que un tema puede definir por su cuenta; los que quedan en `None` se derivan de los demás (reglas §4.2).
#[derive(Clone, Copy, Default)]
pub struct Roles {
    pub success: Option<Color32>,
    pub warning: Option<Color32>,
    pub danger: Option<Color32>,
    pub on_accent: Option<Color32>,
    pub canvas: Option<Color32>,
    pub grid: Option<Color32>,
    pub shadow: Option<Color32>,
    pub paper: Option<Color32>,
    pub paper_ink: Option<Color32>,
    pub dark: Option<bool>,
    pub plan_camera: Option<Color32>,
    pub plan_light: Option<Color32>,
    pub plan_sound: Option<Color32>,
}

impl Palette {
    fn new(surfaces: [u32; 6], text: [u32; 2], primary: [u32; 2]) -> Self {
        Self::with_roles(surfaces, text, primary, Roles::default())
    }

    /// Como `new`, con los roles que el tema define por su cuenta.
    fn with_roles(surfaces: [u32; 6], text: [u32; 2], primary: [u32; 2], roles: Roles) -> Self {
        let [bg, panel, container, container_hi, field, border] = surfaces.map(rgb);
        // El borde tiene que verse contra el panel (contraste 3 como mínimo, reglas §4.3): si el tema lo trae más
        // tenue, se aclara hacia el texto lo justo.
        let border = (0..=20)
            .map(|n| mix(border, rgb(text[0]), n as f32 / 20.0))
            .find(|c| contrast(*c, panel) >= 3.0)
            .unwrap_or(rgb(text[0]));
        // Igual con el texto tenue: tiene que leerse (4,5) sobre las tres superficies.
        let dim_c = (0..=20)
            .map(|n| mix(rgb(text[1]), rgb(text[0]), n as f32 / 20.0))
            .find(|c| {
                [bg, panel, container]
                    .iter()
                    .all(|s| contrast(*c, *s) >= 4.5)
            })
            .unwrap_or(rgb(text[0]));
        let dark = roles.dark.unwrap_or(true);
        let text_c = rgb(text[0]);
        let mut accent = accent_of(rgb(primary[0]), dark);
        if !dark {
            // En un tema claro el acento se oscurece lo justo para verse (3) sobre el panel.
            let (h, s, mut l) = to_hsl(accent);
            while contrast(accent, panel) < 3.0 && l > 0.05 {
                l -= 0.02;
                accent = from_hsl(h, s, l);
            }
        }
        // Los roles que el tema no define se derivan de los que sí (ver reglas-interfaz.md §4.2).
        let canvas_c = roles.canvas.unwrap_or(field);
        Self {
            bg,
            panel,
            container,
            container_hi,
            field,
            border,
            text: text_c,
            dim: dim_c,
            primary: rgb(primary[0]),
            selection: rgb(primary[1]),
            accent,
            // Estados: el verde, el ámbar y el rojo se acercan al texto del tema lo justo para leerse sobre `panel`.
            success: roles
                .success
                .unwrap_or_else(|| state_color(rgb(0x7fc77f), text_c, panel)),
            warning: roles
                .warning
                .unwrap_or_else(|| state_color(rgb(0xf0b44c), text_c, panel)),
            danger: roles
                .danger
                .unwrap_or_else(|| state_color(rgb(0xf07178), text_c, panel)),
            on_accent: roles.on_accent.unwrap_or_else(|| {
                let best = ink_color(accent, text_c, bg);
                if contrast(best, accent) >= 4.5 {
                    best
                } else {
                    ink_color(accent, Color32::WHITE, Color32::BLACK)
                }
            }),
            canvas: canvas_c,
            grid: roles.grid.unwrap_or_else(|| mix(field, border, 0.45)),
            shadow: roles.shadow.unwrap_or(Color32::from_black_alpha(140)),
            plan_camera: roles
                .plan_camera
                .unwrap_or_else(|| plan_color(accent, 0.0, canvas_c)),
            plan_light: roles
                .plan_light
                .unwrap_or_else(|| plan_color(accent, 60.0, canvas_c)),
            plan_sound: roles
                .plan_sound
                .unwrap_or_else(|| plan_color(accent, 180.0, canvas_c)),
            paper: roles.paper.unwrap_or(Color32::WHITE),
            paper_ink: roles.paper_ink.unwrap_or(rgb(0x1a1a1a)),
            dark: roles.dark.unwrap_or(true),
        }
    }
}

/// Un color de estado: el base, o el más cercano al texto que alcance contraste 4,5 sobre la superficie.
fn state_color(base: Color32, text: Color32, surface: Color32) -> Color32 {
    (0..=20)
        .map(|n| mix(base, text, n as f32 / 20.0))
        .find(|c| contrast(*c, surface) >= 4.5)
        .unwrap_or(text)
}

/// Luminancia relativa según WCAG.
fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// Relación de contraste WCAG entre dos colores (de 1 a 21).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn ink_color(fondo: Color32, claro: Color32, oscuro: Color32) -> Color32 {
    if contrast(fondo, claro) >= contrast(fondo, oscuro) {
        claro
    } else {
        oscuro
    }
}

/// El texto con más contraste sobre un fondo que viene de los datos (categorías, personajes, tiras).
pub fn ink_on(fondo: Color32, p: &Palette) -> Color32 {
    ink_color(fondo, p.text, p.bg)
}

/// El color principal pero vivo: saturación de al menos 0,78 y luminosidad que se ve sobre fondo oscuro.
fn accent_of(primary: Color32, dark: bool) -> Color32 {
    let (h, s, _) = to_hsl(primary);
    // En un tema claro el acento va más oscuro para leerse sobre las superficies claras.
    from_hsl(h, s.clamp(0.6, 0.9), if dark { 0.60 } else { 0.38 })
}

fn to_hsl(c: Color32) -> (f32, f32, f32) {
    let (r, g, b) = (
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let ch = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(ch(r), ch(g), ch(b))
}

pub fn apply(ctx: &egui::Context, p: &Palette, zoom_percent: u32) {
    ctx.set_zoom_factor(zoom_percent as f32 / 100.0);
    // El zoom lo manejamos nosotros (Ajustes y Ctrl +/−) para que quede guardado.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    ctx.global_style_mut(|style| {
        let sizes = [
            (TextStyle::Heading, FontId::new(19.0, bold_family())),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ];
        for (name, id) in sizes {
            style.text_styles.insert(name, id);
        }

        let radius = CornerRadius::same(4);
        let hover = mix(p.container, p.primary, 0.18);
        let pressed = mix(p.container, p.bg, 0.5);
        let v = &mut style.visuals;
        v.dark_mode = p.dark;
        v.panel_fill = p.panel;
        v.window_fill = p.panel;
        v.extreme_bg_color = p.field;
        v.faint_bg_color = mix(p.panel, p.container, 0.5);
        v.selection.bg_fill = p.selection;
        v.selection.stroke = Stroke::new(1.0, p.primary);
        v.hyperlink_color = p.primary;
        v.override_text_color = None;

        v.widgets.noninteractive.bg_fill = p.panel;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, mix(p.panel, p.border, 0.6));
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
        v.widgets.inactive.bg_fill = p.container;
        v.widgets.inactive.weak_bg_fill = p.container;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
        v.widgets.hovered.bg_fill = hover;
        v.widgets.hovered.weak_bg_fill = hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.primary);
        v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
        v.widgets.active.bg_fill = pressed;
        v.widgets.active.weak_bg_fill = pressed;
        v.widgets.active.bg_stroke = Stroke::new(1.0, p.primary);
        v.widgets.active.fg_stroke = Stroke::new(1.5, p.text);
        v.widgets.open.bg_fill = hover;
        v.widgets.open.weak_bg_fill = hover;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = radius;
            // Sin expansión: si no, los widgets crecen 1 px al pasar el mouse y empujan a los vecinos.
            w.expansion = 0.0;
        }

        // Valores globales de reglas §3.1.
        style.spacing.item_spacing = egui::vec2(crate::ui::tokens::SP_S, crate::ui::tokens::SP_XS + 1.0);
        style.spacing.slider_width = 120.0;
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.indent = crate::ui::tokens::SP_L;
        style.spacing.interact_size.y = CONTROL_H;
        style.animation_time = crate::ui::kit::anim(crate::ui::tokens::ANIM);
    });
}

/// De temperatura de color (en kelvin) a RGB, con la aproximación de Tanner Helland (válida de 1000 a 40000 K).
pub fn temperature_rgb(kelvin: f32) -> Color32 {
    let t = kelvin.clamp(1000.0, 40000.0) / 100.0;
    let red = if t <= 66.0 {
        255.0
    } else {
        329.698_73 * (t - 60.0).powf(-0.133_204_76)
    };
    let green = if t <= 66.0 {
        99.470_8 * t.ln() - 161.119_57
    } else {
        288.122_17 * (t - 60.0).powf(-0.075_514_85)
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(c(red), c(green), c(blue))
}

/// El color con que se dibuja el haz de una luz: el de su gelatina, o el de su temperatura, mezclado un poco con el fondo
/// del lienzo para que no encandile.
pub fn beam_color(p: &Palette, kelvin: u32, gel: Option<[u8; 3]>) -> Color32 {
    let base = gel.map_or_else(
        || temperature_rgb(kelvin as f32),
        |[r, g, b]| Color32::from_rgb(r, g, b),
    );
    mix(base, p.canvas, 0.2)
}

/// El color de un haz sin mezclar con ningún tema (para imprimir): el de su gelatina o el de su temperatura.
pub fn beam_color_plain(kelvin: u32, gel: Option<[u8; 3]>) -> Color32 {
    gel.map_or_else(
        || temperature_rgb(kelvin as f32),
        |[r, g, b]| Color32::from_rgb(r, g, b),
    )
}

/// El color de una planta: el acento del tema con el matiz girado `turn` grados, y la luminosidad que haga falta para que
/// se lea (contraste 3) contra el fondo del lienzo.
fn plan_color(accent: Color32, turn: f32, canvas: Color32) -> Color32 {
    let (h, s, l) = to_hsl(accent);
    let dark = luminance(canvas) < 0.5;
    let mut l = l;
    for _ in 0..30 {
        let c = from_hsl(h + turn, s.clamp(0.55, 0.85), l);
        if contrast(c, canvas) >= 3.0 {
            return c;
        }
        l = if dark {
            (l + 0.03).min(0.95)
        } else {
            (l - 0.03).max(0.05)
        };
    }
    from_hsl(h + turn, s.clamp(0.55, 0.85), l)
}

/// Color estable por personaje para las barras laterales del guion.
pub fn character_color(name: &str) -> Color32 {
    const COLORS: [Color32; 8] = [
        rgb(0xb39ddb),
        rgb(0x81c784),
        rgb(0xffb74d),
        rgb(0x4fc3f7),
        rgb(0xf06292),
        rgb(0xfff176),
        rgb(0x4db6ac),
        rgb(0xe57373),
    ];
    let h = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
    COLORS[h as usize % COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_meets_the_contrast_minimums() {
        for scheme in Scheme::ALL {
            let p = scheme.palette();
            let name = scheme.name();
            for (surface, sname) in [(p.bg, "bg"), (p.panel, "panel"), (p.container, "container")] {
                for (fg, fname) in [(p.text, "text"), (p.dim, "dim")] {
                    let c = contrast(fg, surface);
                    assert!(c >= 4.5, "{name}: {fname} sobre {sname} da {c:.2}");
                }
            }
            let c = contrast(p.on_accent, p.accent);
            assert!(c >= 4.5, "{name}: on_accent sobre accent da {c:.2}");
            let c = contrast(p.text, p.selection);
            assert!(c >= 4.5, "{name}: text sobre selection da {c:.2}");
            let c = contrast(p.paper_ink, p.paper);
            assert!(c >= 7.0, "{name}: paper_ink sobre paper da {c:.2}");
            for (fg, fname) in [(p.accent, "accent"), (p.border, "border")] {
                let c = contrast(fg, p.panel);
                assert!(c >= 3.0, "{name}: {fname} sobre panel da {c:.2}");
            }
        }
    }

    #[test]
    fn a_warm_light_is_redder_than_blue_and_a_cool_one_is_not() {
        let warm = temperature_rgb(2700.0);
        let cool = temperature_rgb(6500.0);
        assert!(warm.r() > warm.b(), "{warm:?}");
        assert!(
            (warm.r() as i32 - warm.b() as i32) > (cool.r() as i32 - cool.b() as i32) + 100,
            "{warm:?} {cool:?}"
        );
        assert!(
            cool.r().abs_diff(cool.b()) < 40,
            "6500 K es casi blanco: {cool:?}"
        );
        // Más frío, más azul.
        assert!(temperature_rgb(9000.0).b() >= temperature_rgb(9000.0).r());
    }

    #[test]
    fn the_plan_roles_are_readable_on_the_canvas_and_apart_from_each_other_in_every_theme() {
        for scheme in Scheme::ALL {
            let p = scheme.palette();
            let colors = [p.plan_camera, p.plan_light, p.plan_sound];
            for c in colors {
                assert!(
                    contrast(c, p.canvas) >= 3.0,
                    "{}: {c:?} sobre canvas da {:.2}",
                    scheme.name(),
                    contrast(c, p.canvas)
                );
            }
            for (i, a) in colors.iter().enumerate() {
                for b in &colors[i + 1..] {
                    let (ha, hb) = (to_hsl(*a).0, to_hsl(*b).0);
                    let apart = (ha - hb).abs().min(360.0 - (ha - hb).abs());
                    assert!(
                        apart >= 45.0,
                        "{}: {a:?} y {b:?} están a {apart}°",
                        scheme.name()
                    );
                }
            }
        }
    }

    #[test]
    fn a_beam_takes_its_gel_or_its_temperature() {
        let p = Scheme::Tierra.palette();
        assert_ne!(beam_color(&p, 3200, None), beam_color(&p, 6500, None));
        let gel = beam_color(&p, 3200, Some([0, 0, 255]));
        assert!(gel.b() > gel.r());
    }

    #[test]
    fn ink_on_picks_the_more_readable_of_text_and_bg() {
        let p = Scheme::Tierra.palette();
        assert_eq!(ink_on(Color32::from_rgb(250, 240, 200), &p), p.bg);
        assert_eq!(ink_on(Color32::from_rgb(20, 20, 30), &p), p.text);
    }
}
