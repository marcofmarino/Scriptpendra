//! Exportación de la ficha de un personaje a SVG. Figma, Illustrator, Inkscape y Affinity lo abren con
//! los textos, las formas y las imágenes como capas separadas y editables.

use std::{collections::HashSet, path::Path};

use eframe::egui::Color32;

use crate::{
    model::{CharacterProfile, clean_character_name},
    profile,
};

/// Qué partes de la ficha se exportan. Por defecto va todo; se guardan las que se desmarcan.
#[derive(Default)]
pub struct Selection {
    excluded: HashSet<String>,
}

impl Selection {
    pub fn includes(&self, character: &str, key: &str) -> bool {
        !self.excluded.contains(&format!("{character}|{key}"))
    }

    pub fn set(&mut self, character: &str, key: &str, include: bool) {
        let id = format!("{character}|{key}");
        if include {
            self.excluded.remove(&id);
        } else {
            self.excluded.insert(id);
        }
    }
}

pub const KEY_PHOTO: &str = "photo";
pub const KEY_BASIC: &str = "basic";

pub fn section_key(index: usize) -> String {
    format!("section:{index}")
}

pub fn field_key(key: &str) -> String {
    format!("field:{key}")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// Parte un texto en líneas de como máximo `max_chars` caracteres, respetando los saltos de línea.
fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let needed =
                line.chars().count() + word.chars().count() + usize::from(!line.is_empty());
            if !line.is_empty() && needed > max_chars {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn image_data_uri(path: &Path) -> Option<String> {
    let mime = match path.extension()?.to_str()?.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    };
    let data = std::fs::read(path).ok()?;
    Some(format!("data:{mime};base64,{}", base64(&data)))
}

const WIDTH: f32 = 1240.0;
const MARGIN: f32 = 72.0;
const INK: &str = "#1E1A16";
const DIM: &str = "#7A6F63";
const RULE: &str = "#E3DBD1";
const FONT: &str = "Rubik, Inter, Helvetica, Arial, sans-serif";

/// Genera el SVG de la ficha con lo que `selection` deja incluido. Solo se exportan los campos con contenido.
pub fn character_svg(profile: &CharacterProfile, selection: &Selection, accent: Color32) -> String {
    let name = clean_character_name(&profile.name);
    // La selección se guarda por id: dos personajes con el mismo nombre no comparten lo que se marcó.
    let key = profile.id.to_string();
    let accent = hex(accent);
    let mut body = String::new();
    let mut y = MARGIN;

    // --- Cabecera: foto y datos básicos.
    let show_photo = selection.includes(&key, KEY_PHOTO);
    let show_basic = selection.includes(&key, KEY_BASIC);
    let photo = profile
        .photo
        .as_deref()
        .filter(|_| show_photo)
        .and_then(image_data_uri);
    let photo_w = if show_photo { 300.0 } else { 0.0 };
    // Misma proporción que el recuadro de la ficha (retrato 4:5).
    let photo_h = 300.0 * 5.0 / 4.0;
    let text_x = MARGIN + if show_photo { photo_w + 48.0 } else { 0.0 };
    if show_photo {
        body.push_str(&format!(
            "  <g id=\"Foto\">\n    <clipPath id=\"foto-recorte\"><rect x=\"{MARGIN}\" y=\"{y}\" width=\"{photo_w}\" height=\"{photo_h}\" rx=\"12\"/></clipPath>\n    <rect x=\"{MARGIN}\" y=\"{y}\" width=\"{photo_w}\" height=\"{photo_h}\" rx=\"12\" fill=\"#EFE8DF\"/>\n"
        ));
        if let Some(uri) = photo {
            body.push_str(&format!(
                "    <image x=\"{MARGIN}\" y=\"{y}\" width=\"{photo_w}\" height=\"{photo_h}\" preserveAspectRatio=\"xMidYMid slice\" clip-path=\"url(#foto-recorte)\" href=\"{uri}\"/>\n"
            ));
        }
        body.push_str("  </g>\n");
    }

    let mut ty = y + 64.0;
    body.push_str(&format!(
        "  <g id=\"Datos básicos\">\n    <text x=\"{text_x}\" y=\"{ty}\" font-family=\"{FONT}\" font-weight=\"600\" font-size=\"60\" fill=\"{INK}\">{}</text>\n",
        escape(&name)
    ));
    ty += 20.0;
    if show_basic {
        let mut meta = vec![profile.role.label().to_string()];
        if !profile.age.trim().is_empty() {
            meta.push(format!("{} años", profile.age.trim()));
        }
        if !profile.gender.trim().is_empty() {
            meta.push(profile.gender.trim().to_string());
        }
        ty += 36.0;
        body.push_str(&format!(
            "    <text x=\"{text_x}\" y=\"{ty}\" font-family=\"{FONT}\" font-size=\"26\" fill=\"{accent}\">{}</text>\n",
            escape(&meta.join("  ·  "))
        ));
        if !profile.one_liner.trim().is_empty() {
            let max = ((WIDTH - MARGIN - text_x) / 13.5) as usize;
            ty += 20.0;
            for line in wrap(profile.one_liner.trim(), max) {
                ty += 36.0;
                body.push_str(&format!(
                    "    <text x=\"{text_x}\" y=\"{ty}\" font-family=\"{FONT}\" font-size=\"26\" fill=\"{INK}\">{}</text>\n",
                    escape(&line)
                ));
            }
        }
    }
    body.push_str("  </g>\n");
    y = if show_photo { y + photo_h } else { ty } + 56.0;

    // --- Secciones de la ficha completa.
    let max_chars = ((WIDTH - 2.0 * MARGIN) / 11.5) as usize;
    for (n, section) in profile::CHARACTER.iter().enumerate() {
        if !selection.includes(&key, &section_key(n)) {
            continue;
        }
        let entries: Vec<(&str, &str)> = section
            .fields
            .iter()
            .filter(|f| selection.includes(&key, &field_key(f.key)))
            .filter_map(|f| {
                let v = profile.fields.get(f.key)?.trim();
                (!v.is_empty()).then_some((f.label, v))
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        body.push_str(&format!("  <g id=\"{}\">\n", escape(section.title)));
        body.push_str(&format!(
            "    <text x=\"{MARGIN}\" y=\"{y}\" font-family=\"{FONT}\" font-weight=\"600\" font-size=\"34\" fill=\"{accent}\">{}</text>\n    <line x1=\"{MARGIN}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{RULE}\" stroke-width=\"2\"/>\n",
            escape(section.title),
            y + 14.0,
            WIDTH - MARGIN,
            y + 14.0
        ));
        y += 52.0;
        for (label, value) in entries {
            body.push_str(&format!("    <g id=\"{}\">\n", escape(label)));
            body.push_str(&format!(
                "      <text x=\"{MARGIN}\" y=\"{y}\" font-family=\"{FONT}\" font-size=\"18\" letter-spacing=\"1\" fill=\"{DIM}\">{}</text>\n",
                escape(&label.to_uppercase())
            ));
            y += 8.0;
            for line in wrap(value, max_chars) {
                y += 30.0;
                body.push_str(&format!(
                    "      <text x=\"{MARGIN}\" y=\"{y}\" font-family=\"{FONT}\" font-size=\"22\" fill=\"{INK}\">{}</text>\n",
                    escape(&line)
                ));
            }
            body.push_str("    </g>\n");
            y += 26.0;
        }
        body.push_str("  </g>\n");
        y += 26.0;
    }

    let height = y + MARGIN - 26.0;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{height}\" viewBox=\"0 0 {WIDTH} {height}\">\n  <title>Ficha de {}</title>\n  <rect id=\"Fondo\" width=\"{WIDTH}\" height=\"{height}\" fill=\"#FFFFFF\"/>\n{body}</svg>\n",
        escape(&name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> CharacterProfile {
        let mut p = CharacterProfile {
            id: 7,
            name: "ANA <&>".into(),
            age: "30".into(),
            ..Default::default()
        };
        p.fields.insert("eye_color".into(), "Verdes".into());
        p.fields.insert("hair_color".into(), "Castaño".into());
        p.fields
            .insert("childhood".into(), "Creció en el sur. ".repeat(30));
        p
    }

    #[test]
    fn svg_has_selected_text_and_escapes() {
        let svg = character_svg(
            &profile(),
            &Selection::default(),
            Color32::from_rgb(237, 150, 80),
        );
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("Verdes") && svg.contains("Castaño"));
        assert!(svg.contains("ANA &lt;&amp;&gt;"));
        assert!(svg.contains("30 años"));
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn deselected_parts_are_left_out() {
        let mut sel = Selection::default();
        let key = profile().id.to_string();
        sel.set(&key, &field_key("eye_color"), false);
        sel.set(&key, &section_key(3), false); // Biografía
        sel.set(&key, KEY_PHOTO, false);
        let svg = character_svg(&profile(), &sel, Color32::WHITE);
        assert!(!svg.contains("Verdes"));
        assert!(svg.contains("Castaño"));
        assert!(!svg.contains("Creció"));
        assert!(!svg.contains("id=\"Foto\""));
    }

    #[test]
    fn photo_frame_keeps_the_portrait_ratio() {
        let svg = character_svg(&profile(), &Selection::default(), Color32::WHITE);
        // 300 × 375 = 4:5, igual que el recuadro de la ficha.
        assert!(svg.contains("width=\"300\" height=\"375\""));
    }

    #[test]
    fn long_text_is_wrapped() {
        let lines = wrap(&"palabra ".repeat(40), 50);
        assert!(lines.len() > 4 && lines.iter().all(|l| l.chars().count() <= 50));
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
