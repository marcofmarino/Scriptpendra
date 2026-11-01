//! Exportar el guion a PDF con las mismas páginas que la vista: se reparte con la misma paginación que el editor
//! (`paginate`, el corte de diálogos con `(MÁS)` y `(CONT.)`), en Courier Prime de 12 puntos, 10 caracteres por
//! pulgada y 6 renglones por pulgada, así que todo cae en una grilla de renglones y columnas.
//!
//! Primero se arma el diseño de cada hoja (`layout`, sin tocar ningún archivo) y después se dibuja (`export`).

use std::sync::Arc;

use krilla::{
    Document,
    color::rgb,
    geom::{PathBuilder, Point, Rect},
    num::NormalizedF32,
    page::PageSettings,
    paint::{Fill, Stroke},
    text::{Font, TextDirection},
};

use crate::{
    editor::{PageItem, Placement, ends_sentence, paginate, split_plan},
    model::{ParagraphKind, Project, clean_character_name},
    storage::Paper,
};

/// Courier 12: 7,2 puntos por carácter y 12 por renglón.
const CHAR_W: f32 = 7.2;
const ROW_H: f32 = 12.0;
const PT: f32 = 72.0;

/// Un renglón ya ubicado: su esquina superior izquierda en puntos desde la esquina de la hoja.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub bold: bool,
    pub underline: bool,
    /// Tamaño de la letra en puntos (12 en el guion).
    pub size: f32,
    /// Un rectángulo de color que arranca en (x, y): ancho, alto y color. Va detrás del texto de la hoja.
    pub swatch: Option<(f32, f32, [u8; 3])>,
}

/// Todo el documento: la portada y las hojas del guion.
#[derive(Debug, Default)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub cover: Vec<Line>,
    pub pages: Vec<Vec<Line>>,
    /// Qué hojas tienen cambios de la revisión abierta (con asterisco en el margen).
    pub changed: Vec<bool>,
    /// La revisión abierta: su color y cómo se rotula arriba de las hojas cambiadas.
    pub revision: Option<(crate::model::RevisionColor, String)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
    Center,
}

/// Un elemento del guion listo para repartir: sus renglones y cómo se ubican.
struct Element {
    kind: ParagraphKind,
    /// Cada renglón con su sangría extra en columnas.
    rows: Vec<(f32, String)>,
    /// Qué caracteres de cada renglón van en negrita (vacío: ninguno; solo las acciones con personajes nombrados).
    marks: Vec<Vec<bool>>,
    /// Sangría del bloque y ancho de su caja, en pulgadas desde el margen izquierdo.
    left: f32,
    width: f32,
    align: Align,
    bold: bool,
    underline: bool,
    gap: f32,
    scene_number: Option<String>,
    /// Cambió desde que se abrió la revisión.
    changed: bool,
}

/// Parte un texto en renglones de a lo sumo `cols` caracteres, cortando entre palabras (una palabra más larga que el
/// renglón se corta donde llega).
pub fn wrap(text: &str, cols: usize) -> Vec<String> {
    wrap_marked(text, &[], cols).into_iter().map(|(row, _)| row).collect()
}

/// Lo mismo que `wrap`, y por cada renglón qué caracteres van en negrita según `marks` (un valor por carácter del texto; si
/// falta alguno, no va en negrita). Un espacio entre dos palabras en negrita también lo va.
pub fn wrap_marked(text: &str, marks: &[bool], cols: usize) -> Vec<(String, Vec<bool>)> {
    let cols = cols.max(1);
    let mut paragraphs: Vec<Vec<(char, bool)>> = vec![Vec::new()];
    for (i, c) in text.chars().enumerate() {
        if c == '\n' {
            paragraphs.push(Vec::new());
        } else {
            paragraphs.last_mut().expect("siempre hay un párrafo").push((c, marks.get(i).copied().unwrap_or(false)));
        }
    }
    let mut out: Vec<Vec<(char, bool)>> = Vec::new();
    for paragraph in paragraphs {
        let mut line: Vec<(char, bool)> = Vec::new();
        for word in paragraph.split(|(c, _)| c.is_whitespace()).filter(|w| !w.is_empty()) {
            let mut word: Vec<(char, bool)> = word.to_vec();
            while word.len() > cols {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                out.push(word.drain(..cols).collect());
            }
            if !line.is_empty() && line.len() + 1 + word.len() > cols {
                out.push(std::mem::take(&mut line));
            }
            if let Some(&(_, before)) = line.last() {
                line.push((' ', before && word.first().is_some_and(|w| w.1)));
            }
            line.extend(word);
        }
        out.push(line);
    }
    out.into_iter().map(|row| (row.iter().map(|(c, _)| *c).collect(), row.iter().map(|(_, b)| *b).collect())).collect()
}

/// Cuántos caracteres de espacio hay al comienzo del texto.
fn text_lead(text: &str) -> usize {
    text.chars().count() - text.trim_start().chars().count()
}

fn columns(inches: f32) -> usize {
    (inches * 10.0 + 0.001).floor() as usize
}

/// Los elementos del guion, ya con su texto, sus renglones y su geometría.
fn elements(project: &Project, paper: Paper, bold_names: bool) -> Vec<Element> {
    let sheet = paper.sheet();
    let text_w = sheet.text_w();
    let shown = project.shown_paragraphs_marked();
    let conts = project.cont_marks();
    let revised = project.revised_paragraphs();
    // El número de cada escena va siempre a la izquierda del encabezado, como en la hoja de la pantalla.
    let labels: std::collections::HashMap<u64, String> = project.scene_labels().into_iter().collect();
    let mut out: Vec<Element> = Vec::new();
    let mut previous: Option<ParagraphKind> = None;
    for (i, (kind, text, flags)) in shown.iter().enumerate() {
        use ParagraphKind::*;
        if *kind == Note || text.trim().is_empty() {
            continue;
        }
        let mut text = if kind.is_uppercase() {
            text.trim().to_uppercase()
        } else {
            text.trim().to_string()
        };
        // La negrita de los personajes nombrados sigue al texto recortado.
        let lead = text_lead(&shown[i].1);
        let flags: Vec<bool> = if bold_names {
            flags.iter().skip(lead).take(text.chars().count()).copied().collect()
        } else {
            Vec::new()
        };
        if conts.get(i).copied().unwrap_or(false) {
            text.push_str(" (CONT.)");
        }
        let plain = |rows: Vec<String>| rows.into_iter().map(|r| (0.0, r)).collect::<Vec<_>>();
        let mut el = Element {
            kind: *kind,
            rows: Vec::new(),
            marks: Vec::new(),
            left: 0.0,
            width: text_w,
            align: Align::Left,
            bold: false,
            underline: false,
            gap: 1.0,
            scene_number: None,
            changed: revised.contains(&project.paragraphs[i].id),
        };
        match kind {
            SceneHeading => {
                el.rows = plain(wrap(&text, columns(text_w)));
                el.gap = 2.0;
                el.scene_number = labels.get(&project.paragraphs[i].id).cloned();
            }
            Action => {
                let wrapped = wrap_marked(&text, &flags, columns(text_w));
                el.marks = wrapped.iter().map(|(_, m)| m.clone()).collect();
                el.rows = wrapped.into_iter().map(|(r, _)| (0.0, r)).collect();
            }
            Character => {
                el.left = 2.2;
                el.width = (3.8f32).min(text_w - 2.2);
                el.rows = plain(wrap(&text, columns(el.width)));
                el.bold = bold_names;
            }
            Parenthetical => {
                // El texto va entre paréntesis y se ajusta a lo que mide, hasta 22 columnas.
                el.left = 1.6;
                el.gap = 0.0;
                let inner = text.trim_matches(['(', ')']).to_string();
                let longest = inner.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                let cols = longest.clamp(1, 22);
                let mut rows = wrap(&inner, cols);
                let last = rows.len() - 1;
                for (n, row) in rows.iter_mut().enumerate() {
                    if n == 0 {
                        *row = format!("({row}");
                    }
                    if n == last {
                        row.push(')');
                    }
                }
                el.rows = rows
                    .into_iter()
                    .enumerate()
                    .map(|(n, r)| (if n == 0 { 0.0 } else { 1.0 }, r))
                    .collect();
            }
            Dialogue => {
                el.left = 1.0;
                el.width = 3.5;
                el.gap = 0.0;
                el.rows = plain(wrap(&text, columns(3.5)));
            }
            Lyrics => {
                el.left = 1.0;
                el.width = 3.5;
                el.gap = if previous == Some(Lyrics) { 0.0 } else { 1.0 };
                el.rows = plain(wrap(&text, columns(3.5)));
            }
            Transition => {
                el.width = 5.6f32.min(text_w);
                el.align = Align::Right;
                el.rows = plain(wrap(&text, columns(el.width)));
            }
            Act => {
                el.align = Align::Center;
                el.bold = true;
                el.underline = true;
                el.gap = 2.0;
                el.rows = plain(wrap(&text, columns(text_w)));
            }
            Sequence => {
                el.align = Align::Center;
                el.gap = 2.0;
                el.rows = plain(wrap(&text, columns(text_w)));
            }
            Note => unreachable!("las notas no se imprimen"),
        }
        previous = Some(*kind);
        out.push(el);
    }
    out
}

/// Reparte el guion en hojas.
pub fn layout(project: &Project, paper: Paper, bold_names: bool) -> Layout {
    let sheet = paper.sheet();
    let (page_w, page_h) = (sheet.width * PT, sheet.height * PT);
    let (left, top) = (sheet.left * PT, sheet.top * PT);
    let text_w = sheet.text_w() * PT;
    let rows_per_page = sheet.text_h() * 6.0;
    let els = elements(project, paper, bold_names);

    let items: Vec<PageItem> = els
        .iter()
        .map(|e| {
            let splittable = matches!(
                e.kind,
                ParagraphKind::Action | ParagraphKind::Dialogue | ParagraphKind::Lyrics
            );
            PageItem {
                height: e.rows.len() as f32,
                gap: e.gap,
                keep_with_next: matches!(
                    e.kind,
                    ParagraphKind::SceneHeading
                        | ParagraphKind::Character
                        | ParagraphKind::Parenthetical
                        | ParagraphKind::Act
                        | ParagraphKind::Sequence
                ),
                row_h: if splittable { 1.0 } else { 0.0 },
                dialogue: e.kind == ParagraphKind::Dialogue,
                sentence_ends: e.rows.iter().map(|(_, r)| ends_sentence(r)).collect(),
            }
        })
        .collect();
    let placements: Vec<Placement> = paginate(&items, rows_per_page);
    let per_page = (rows_per_page.floor() as usize).max(1);

    let mut pages: Vec<Vec<Line>> = Vec::new();
    let mut put = |page: usize, row: f32, x: f32, text: String, bold: bool, underline: bool| {
        while pages.len() <= page {
            pages.push(Vec::new());
        }
        pages[page].push(Line {
            x,
            y: top + row * ROW_H,
            text,
            bold,
            underline,
            size: 12.0,
            swatch: None,
        });
    };
    let len_pt = |s: &str| s.chars().count() as f32 * CHAR_W;
    for (i, (el, at)) in els.iter().zip(&placements).enumerate() {
        let x_of = |extra_cols: f32, text: &str| -> f32 {
            let base = left + el.left * PT;
            match el.align {
                Align::Left => base + extra_cols * CHAR_W,
                Align::Right => base + el.width * PT - len_pt(text),
                Align::Center => left + (text_w - len_pt(text)) / 2.0,
            }
        };
        let n = el.rows.len();
        let plan = match at.cut {
            Some(cut) => split_plan(n, cut, per_page, el.kind == ParagraphKind::Dialogue),
            None => vec![n],
        };
        let speaker = els[..i]
            .iter()
            .rev()
            .find(|e| e.kind == ParagraphKind::Character)
            .and_then(|e| e.rows.first())
            .map(|(_, r)| clean_character_name(r).to_uppercase())
            .unwrap_or_default();
        let mut next_row = 0;
        for (k, rows_here) in plan.iter().enumerate() {
            let cont = usize::from(k > 0 && el.kind == ParagraphKind::Dialogue);
            let start = if k == 0 { at.y } else { cont as f32 };
            if cont == 1 {
                put(
                    at.page + k,
                    0.0,
                    left + 2.2 * PT,
                    format!("{speaker} (CONT.)"),
                    false,
                    false,
                );
            }
            for r in 0..*rows_here {
                let (extra, text) = &el.rows[next_row + r];
                let marks = el.marks.get(next_row + r).filter(|m| m.iter().any(|b| *b));
                match marks {
                    // Con personajes nombrados en negrita el renglón se parte en tramos; la letra es de ancho fijo.
                    Some(marks) => {
                        let chars: Vec<char> = text.chars().collect();
                        let mut from = 0;
                        while from < chars.len() {
                            let flag = marks.get(from).copied().unwrap_or(false);
                            let mut to = from;
                            while to < chars.len() && marks.get(to).copied().unwrap_or(false) == flag {
                                to += 1;
                            }
                            put(
                                at.page + k,
                                start + r as f32,
                                x_of(*extra, text) + from as f32 * CHAR_W,
                                chars[from..to].iter().collect(),
                                flag,
                                el.underline,
                            );
                            from = to;
                        }
                    }
                    None => put(
                        at.page + k,
                        start + r as f32,
                        x_of(*extra, text),
                        text.clone(),
                        el.bold,
                        el.underline,
                    ),
                }
                if el.changed {
                    put(
                        at.page + k,
                        start + r as f32,
                        left + text_w + CHAR_W,
                        "*".to_string(),
                        false,
                        false,
                    );
                }
                // El número de la escena, a los dos lados del encabezado.
                if let (Some(number), 0, 0) = (&el.scene_number, k, r) {
                    let tag = format!("{number}.");
                    put(
                        at.page,
                        start,
                        left - 2.0 * CHAR_W - len_pt(&tag),
                        tag.clone(),
                        false,
                        false,
                    );
                    // Con la numeración bloqueada (guion de rodaje) también va del lado derecho.
                    if project.scene_numbers_locked {
                        put(
                            at.page,
                            start,
                            left + text_w + 2.0 * CHAR_W,
                            tag,
                            false,
                            false,
                        );
                    }
                }
            }
            next_row += rows_here;
            if k + 1 < plan.len() && el.kind == ParagraphKind::Dialogue {
                put(
                    at.page + k,
                    start + *rows_here as f32,
                    left + 2.2 * PT,
                    "(MÁS)".to_string(),
                    false,
                    false,
                );
            }
        }
    }
    if pages.is_empty() {
        pages.push(Vec::new());
    }
    // Las hojas con cambios de la revisión abierta se rotulan arriba a la izquierda.
    let revision = project.active_revision().map(|r| {
        (
            r.color,
            format!(
                "Revisión {}: {} ({})",
                r.color.label().to_lowercase(),
                r.name,
                r.date
            ),
        )
    });
    let changed: Vec<bool> = pages
        .iter()
        .map(|pg| revision.is_some() && pg.iter().any(|l| l.text == "*"))
        .collect();
    if let Some((_, label)) = &revision {
        for (page, _) in pages.iter_mut().zip(&changed).filter(|(_, c)| **c) {
            page.push(Line {
                x: left,
                y: 0.5 * PT,
                text: label.clone(),
                bold: false,
                underline: false,
                size: 12.0,
                swatch: None,
            });
        }
    }
    // Número de página arriba a la derecha desde la segunda hoja, con punto.
    for (n, page) in pages.iter_mut().enumerate().skip(1) {
        let tag = format!("{}.", n + 1);
        page.push(Line {
            x: left + text_w - len_pt(&tag),
            y: 0.5 * PT,
            text: tag,
            bold: false,
            underline: false,
            size: 12.0,
            swatch: None,
        });
    }

    // Portada: título a un tercio de la altura, autoría debajo, contacto abajo a la izquierda y versión a la derecha.
    let mut cover = Vec::new();
    let centered = |text: &str, y: f32| Line {
        x: (page_w - len_pt(text)) / 2.0,
        y,
        text: text.to_string(),
        bold: false,
        underline: false,
        size: 12.0,
        swatch: None,
    };
    let cols = columns(sheet.text_w());
    let mut y = page_h / 3.0;
    for row in wrap(&project.title.to_uppercase(), cols) {
        cover.push(centered(&row, y));
        y += ROW_H;
    }
    if !project.author.trim().is_empty() {
        y += 3.0 * ROW_H;
        cover.push(centered("Guion de", y));
        for row in wrap(project.author.trim(), cols) {
            y += ROW_H;
            cover.push(centered(&row, y));
        }
    }
    let contact: Vec<String> = project
        .contact
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    for (n, row) in contact.iter().enumerate() {
        cover.push(Line {
            x: left,
            y: page_h - top - (contact.len() - n) as f32 * ROW_H,
            text: row.clone(),
            bold: false,
            underline: false,
            size: 12.0,
            swatch: None,
        });
    }
    if !project.version.trim().is_empty() {
        cover.push(Line {
            x: page_w - sheet.right * PT - len_pt(project.version.trim()),
            y: page_h - top - ROW_H,
            text: project.version.trim().to_string(),
            bold: false,
            underline: false,
            size: 12.0,
            swatch: None,
        });
    }
    Layout {
        width: page_w,
        height: page_h,
        cover,
        pages,
        changed,
        revision,
    }
}

/// Dibuja los renglones sobre una hoja.
fn draw_lines(surface: &mut krilla::surface::Surface, lines: &[Line], regular: &Font, bold: &Font) {
    // Los rectángulos de color primero, para que queden detrás del texto.
    for line in lines {
        let Some((w, h, [r, g, b])) = line.swatch else {
            continue;
        };
        if let Some(rect) = Rect::from_xywh(line.x, line.y, w, h) {
            let mut path = PathBuilder::new();
            path.push_rect(rect);
            if let Some(path) = path.finish() {
                let fill = |c: rgb::Color| Fill {
                    paint: c.into(),
                    opacity: NormalizedF32::ONE,
                    rule: Default::default(),
                };
                surface.set_fill(Some(fill(rgb::Color::new(r, g, b))));
                surface.draw_path(&path);
                surface.set_fill(Some(fill(rgb::Color::new(0, 0, 0))));
            }
        }
    }
    for line in lines {
        if line.text.is_empty() {
            continue;
        }
        let baseline = line.y + line.size * 0.78;
        let font = if line.bold {
            bold.clone()
        } else {
            regular.clone()
        };
        surface.draw_text(
            Point::from_xy(line.x, baseline),
            font,
            line.size,
            &line.text,
            false,
            TextDirection::Auto,
        );
        if line.underline {
            let mut path = PathBuilder::new();
            let y = baseline + 1.6;
            path.move_to(line.x, y);
            path.line_to(
                line.x + line.text.chars().count() as f32 * line.size * 0.6,
                y,
            );
            if let Some(path) = path.finish() {
                surface.set_stroke(Some(Stroke {
                    width: 0.7,
                    ..Default::default()
                }));
                surface.draw_path(&path);
                surface.set_stroke(None);
            }
        }
    }
}

/// Un PDF de renglones ya ubicados (informes del proyecto: desglose, compras…), en Courier Prime.
pub fn render(
    pages: &[Vec<Line>],
    width: f32,
    height: f32,
    title: &str,
    author: &str,
) -> Result<Vec<u8>, String> {
    let load = |bytes: &'static [u8]| {
        Font::new(Arc::new(bytes.to_vec()).into(), 0)
            .ok_or("no se pudo cargar la fuente".to_string())
    };
    let regular = load(include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"))?;
    let bold = load(include_bytes!("../assets/fonts/CourierPrime-Bold.ttf"))?;
    let mut document = Document::new();
    document.set_metadata(
        krilla::metadata::Metadata::new()
            .title(title.to_string())
            .authors(vec![author.to_string()])
            .creator("Scriptpendra".to_string()),
    );
    for lines in pages {
        let mut page = document.start_page_with(
            PageSettings::from_wh(width, height).ok_or("tamaño de hoja inválido".to_string())?,
        );
        let mut surface = page.surface();
        draw_lines(&mut surface, lines, &regular, &bold);
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|e| format!("{e:?}"))
}

/// Una hoja de un PDF de trazos: su tamaño y cómo se dibuja.
pub struct VectorPage<'a> {
    pub width: f32,
    pub height: f32,
    pub draw: Box<dyn FnOnce(&mut krilla::surface::Surface, &Font, &Font) + 'a>,
}

/// Un PDF con varias hojas dibujadas con trazos y texto (las plantas de varios bloqueos).
pub fn render_vector_pages(
    title: &str,
    author: &str,
    pages: Vec<VectorPage>,
) -> Result<Vec<u8>, String> {
    let load = |bytes: &'static [u8]| {
        Font::new(Arc::new(bytes.to_vec()).into(), 0)
            .ok_or("no se pudo cargar la fuente".to_string())
    };
    let regular = load(include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"))?;
    let bold = load(include_bytes!("../assets/fonts/CourierPrime-Bold.ttf"))?;
    let mut document = Document::new();
    document.set_metadata(
        krilla::metadata::Metadata::new()
            .title(title.to_string())
            .authors(vec![author.to_string()])
            .creator("Scriptpendra".to_string()),
    );
    for spec in pages {
        let mut page = document.start_page_with(
            PageSettings::from_wh(spec.width, spec.height)
                .ok_or("tamaño de hoja inválido".to_string())?,
        );
        let mut surface = page.surface();
        (spec.draw)(&mut surface, &regular, &bold);
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|e| format!("{e:?}"))
}

/// Trazos para dibujar plantas: un polígono relleno o con borde, una línea y un texto, con colores RGB.
pub struct Pen<'a, 'b> {
    pub surface: &'a mut krilla::surface::Surface<'b>,
    pub regular: &'a Font,
    pub bold: &'a Font,
}

impl Pen<'_, '_> {
    /// Un polígono (`closed`) con relleno y/o borde. `alpha` va de 0 a 1.
    pub fn poly(
        &mut self,
        points: &[(f32, f32)],
        fill: Option<([u8; 3], f32)>,
        stroke: Option<([u8; 3], f32)>,
    ) {
        let mut path = PathBuilder::new();
        for (n, (x, y)) in points.iter().enumerate() {
            if n == 0 {
                path.move_to(*x, *y)
            } else {
                path.line_to(*x, *y)
            }
        }
        path.close();
        let Some(path) = path.finish() else { return };
        self.surface.set_fill(fill.map(|([r, g, b], a)| Fill {
            paint: rgb::Color::new(r, g, b).into(),
            opacity: NormalizedF32::new(a.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE),
            rule: Default::default(),
        }));
        self.surface.set_stroke(stroke.map(|([r, g, b], w)| Stroke {
            paint: rgb::Color::new(r, g, b).into(),
            width: w,
            ..Default::default()
        }));
        self.surface.draw_path(&path);
        self.surface.set_fill(None);
        self.surface.set_stroke(None);
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), color: [u8; 3], width: f32) {
        let mut path = PathBuilder::new();
        path.move_to(a.0, a.1);
        path.line_to(b.0, b.1);
        let Some(path) = path.finish() else { return };
        self.surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(color[0], color[1], color[2]).into(),
            width,
            ..Default::default()
        }));
        self.surface.draw_path(&path);
        self.surface.set_stroke(None);
    }

    pub fn text(&mut self, at: (f32, f32), text: &str, size: f32, bold: bool) {
        let font = if bold {
            self.bold.clone()
        } else {
            self.regular.clone()
        };
        self.surface.set_fill(Some(Fill {
            paint: rgb::Color::new(0, 0, 0).into(),
            opacity: NormalizedF32::ONE,
            rule: Default::default(),
        }));
        self.surface.draw_text(
            Point::from_xy(at.0, at.1 + size * 0.78),
            font,
            size,
            text,
            false,
            TextDirection::Auto,
        );
        self.surface.set_fill(None);
    }
}

/// Un cuadro del storyboard para imprimir.
pub struct BoardFrame {
    pub code: String,
    /// «PG · Travelling».
    pub kind: String,
    pub description: String,
    pub dialogue: String,
    pub image: Option<std::path::PathBuf>,
}

/// Dibuja una imagen llenando el rectángulo (recortando lo que sobre), si se puede leer; si no, deja un gris.
fn draw_cover(
    surface: &mut krilla::surface::Surface,
    path: &std::path::Path,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let bytes = std::fs::read(path).ok();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let data: Option<krilla::Data> = bytes.map(|b| Arc::new(b).into());
    let image = data.and_then(|d| match ext.as_str() {
        "png" => krilla::image::Image::from_png(d, true).ok(),
        "jpg" | "jpeg" => krilla::image::Image::from_jpeg(d, true).ok(),
        "webp" => krilla::image::Image::from_webp(d, true).ok(),
        _ => None,
    });
    let (Some(image), Ok((iw, ih))) = (image, image::image_dimensions(path)) else {
        return;
    };
    let scale = (w / iw as f32).max(h / ih as f32);
    let (dw, dh) = (iw as f32 * scale, ih as f32 * scale);
    let (Some(rect), Some(size)) = (
        Rect::from_xywh(x, y, w, h),
        krilla::geom::Size::from_wh(dw, dh),
    ) else {
        return;
    };
    let mut clip = PathBuilder::new();
    clip.push_rect(rect);
    let Some(clip) = clip.finish() else { return };
    surface.push_clip_path(&clip, &krilla::paint::FillRule::default());
    surface.push_transform(&krilla::geom::Transform::from_translate(
        x - (dw - w) / 2.0,
        y - (dh - h) / 2.0,
    ));
    surface.draw_image(image, size);
    surface.pop();
    surface.pop();
}

/// Un PDF de storyboard con 2, 3 o 6 cuadros por hoja. Cada uno lleva su imagen, en la proporción de la película, con el
/// código del plano, el tipo, la descripción y el diálogo.
pub fn render_storyboard(
    frames: &[BoardFrame],
    per_page: usize,
    aspect: f32,
    width: f32,
    height: f32,
    title: &str,
    author: &str,
) -> Result<Vec<u8>, String> {
    let load = |bytes: &'static [u8]| {
        Font::new(Arc::new(bytes.to_vec()).into(), 0)
            .ok_or("no se pudo cargar la fuente".to_string())
    };
    let regular = load(include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"))?;
    let bold = load(include_bytes!("../assets/fonts/CourierPrime-Bold.ttf"))?;
    let mut document = Document::new();
    document.set_metadata(
        krilla::metadata::Metadata::new()
            .title(title.to_string())
            .authors(vec![author.to_string()])
            .creator("Scriptpendra".to_string()),
    );

    let per_page = per_page.clamp(1, 6);
    let (columns, rows, text_beside) = match per_page {
        1 | 2 => (1, per_page, false),
        3 => (1, 3, true),
        4 => (2, 2, false),
        _ => (2, 3, false),
    };
    let (margin, gap, header) = (36.0, 14.0, 26.0);
    let cell_w = (width - 2.0 * margin - gap * (columns - 1) as f32) / columns as f32;
    let cell_h = (height - 2.0 * margin - header - gap * (rows - 1) as f32) / rows as f32;
    let size = 9.0;
    let char_w = size * 0.6;
    let pages: Vec<&[BoardFrame]> = if frames.is_empty() {
        vec![&[]]
    } else {
        frames.chunks(per_page).collect()
    };
    for (n, chunk) in pages.iter().enumerate() {
        let mut page = document.start_page_with(
            PageSettings::from_wh(width, height).ok_or("tamaño de hoja inválido".to_string())?,
        );
        let mut surface = page.surface();
        let mut lines: Vec<Line> = Vec::new();
        let line = |x: f32, y: f32, text: &str, bold: bool| Line {
            x,
            y,
            text: text.to_string(),
            bold,
            underline: false,
            size,
            swatch: None,
        };
        lines.push(line(
            margin,
            margin - 6.0,
            &format!("{} · STORYBOARD", title.to_uppercase()),
            true,
        ));
        let tag = format!("{}", n + 1);
        lines.push(line(
            width - margin - tag.len() as f32 * char_w,
            margin - 6.0,
            &tag,
            false,
        ));
        for (k, frame) in chunk.iter().enumerate() {
            let (col, row) = (k % columns, k / columns);
            let (x, y) = (
                margin + col as f32 * (cell_w + gap),
                margin + header + row as f32 * (cell_h + gap),
            );
            // La imagen, en la proporción de la película, y el texto al costado o debajo.
            let text_h = if text_beside {
                0.0
            } else {
                (cell_h * 0.32).max(40.0)
            };
            let (mut iw, mut ih) = if text_beside {
                (cell_w * 0.58, cell_h)
            } else {
                (cell_w, cell_h - text_h)
            };
            if iw / ih > aspect {
                iw = ih * aspect;
            } else {
                ih = iw / aspect;
            }
            lines.push(Line {
                x,
                y,
                text: String::new(),
                bold: false,
                underline: false,
                size,
                swatch: Some((iw, ih, [232, 232, 232])),
            });
            let (tx, ty, tw) = if text_beside {
                (x + iw + 12.0, y, cell_w - iw - 12.0)
            } else {
                (x, y + ih + 5.0, cell_w)
            };
            let cols = (tw / char_w).floor().max(8.0) as usize;
            let mut at = ty;
            let limit = if text_beside {
                y + cell_h
            } else {
                y + cell_h - 1.0
            };
            let mut push = |text: &str, bold: bool, lines: &mut Vec<Line>| {
                if at + size * 1.3 <= limit {
                    lines.push(line(tx, at, text, bold));
                    at += size * 1.3;
                }
            };
            let head = if frame.kind.is_empty() {
                frame.code.clone()
            } else {
                format!("{} · {}", frame.code, frame.kind)
            };
            for row_text in wrap(&head, cols) {
                push(&row_text, true, &mut lines);
            }
            for row_text in wrap(&frame.description, cols) {
                push(&row_text, false, &mut lines);
            }
            if !frame.dialogue.trim().is_empty() {
                for row_text in wrap(&format!("«{}»", frame.dialogue.trim()), cols) {
                    push(&row_text, false, &mut lines);
                }
            }
        }
        // El fondo gris y las imágenes primero, después el texto.
        draw_lines(&mut surface, &lines, &regular, &bold);
        for (k, frame) in chunk.iter().enumerate() {
            let Some(path) = &frame.image else { continue };
            let (col, row) = (k % columns, k / columns);
            let (x, y) = (
                margin + col as f32 * (cell_w + gap),
                margin + header + row as f32 * (cell_h + gap),
            );
            let text_h = if text_beside {
                0.0
            } else {
                (cell_h * 0.32).max(40.0)
            };
            let (mut iw, mut ih) = if text_beside {
                (cell_w * 0.58, cell_h)
            } else {
                (cell_w, cell_h - text_h)
            };
            if iw / ih > aspect {
                iw = ih * aspect;
            } else {
                ih = iw / aspect;
            }
            draw_cover(&mut surface, path, x, y, iw, ih);
        }
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|e| format!("{e:?}"))
}

/// El guion como un PDF en memoria, con su página de título.
pub fn export(
    project: &Project,
    paper: Paper,
    bold_names: bool,
    only_changed: bool,
) -> Result<Vec<u8>, String> {
    export_with(project, paper, bold_names, only_changed, true)
}

/// Como `export`; con `cover` en falso el PDF es solo el guion, sin la página de título.
pub fn export_with(
    project: &Project,
    paper: Paper,
    bold_names: bool,
    only_changed: bool,
    cover: bool,
) -> Result<Vec<u8>, String> {
    let layout = layout(project, paper, bold_names);
    if only_changed {
        if layout.revision.is_none() {
            return Err("no hay una revisión abierta".to_string());
        }
        if !layout.changed.iter().any(|c| *c) {
            return Err("todavía no hay páginas cambiadas".to_string());
        }
    }
    let load = |bytes: &'static [u8]| {
        Font::new(Arc::new(bytes.to_vec()).into(), 0)
            .ok_or("no se pudo cargar la fuente".to_string())
    };
    let regular = load(include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"))?;
    let bold = load(include_bytes!("../assets/fonts/CourierPrime-Bold.ttf"))?;

    let mut document = Document::new();
    document.set_metadata(
        krilla::metadata::Metadata::new()
            .title(project.title.clone())
            .authors(vec![project.author.clone()])
            .creator("Scriptpendra".to_string()),
    );
    let settings = || {
        PageSettings::from_wh(layout.width, layout.height)
            .ok_or("tamaño de hoja inválido".to_string())
    };
    let cover = (!only_changed && cover).then_some((&layout.cover, false));
    let sheets = cover.into_iter().chain(
        layout
            .pages
            .iter()
            .zip(&layout.changed)
            .filter(|(_, c)| !only_changed || **c)
            .map(|(p, c)| (p, *c)),
    );
    for (lines, tinted) in sheets {
        let mut page = document.start_page_with(settings()?);
        let mut surface = page.surface();
        // La hoja de una revisión va teñida de su color.
        if let (true, Some((color, _))) = (tinted, &layout.revision) {
            let [r, g, b] = color.tint();
            let fill = |c: rgb::Color| Fill {
                paint: c.into(),
                opacity: NormalizedF32::ONE,
                rule: Default::default(),
            };
            if let Some(rect) = Rect::from_xywh(0.0, 0.0, layout.width, layout.height) {
                let mut path = PathBuilder::new();
                path.push_rect(rect);
                if let Some(path) = path.finish() {
                    surface.set_fill(Some(fill(rgb::Color::new(r, g, b))));
                    surface.draw_path(&path);
                    surface.set_fill(Some(fill(rgb::Color::new(0, 0, 0))));
                }
            }
        }
        draw_lines(&mut surface, lines, &regular, &bold);
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ParagraphKind::*;

    fn project(blocks: &[(ParagraphKind, String)]) -> Project {
        let mut p = Project::default();
        p.title = "El nido".into();
        p.author = "Marco".into();
        p.paragraphs = blocks
            .iter()
            .map(|(k, t)| p.make_paragraph(*k, t))
            .collect();
        p
    }

    fn texts(page: &[Line]) -> Vec<String> {
        page.iter().map(|l| l.text.clone()).collect()
    }

    #[test]
    fn scene_numbers_are_printed_beside_every_heading_and_on_both_sides_when_locked() {
        let mut p = project(&[(SceneHeading, "INT. CASA - DIA".into()), (Action, "Ana.".into()), (SceneHeading, "EXT. CALLE - DIA".into())]);
        let tags = |p: &Project| -> Vec<String> {
            layout(p, Paper::A4, false).pages[0].iter().filter(|l| l.text.ends_with('.') && l.text[..l.text.len() - 1].parse::<u32>().is_ok()).map(|l| l.text.clone()).collect()
        };
        assert_eq!(tags(&p), ["1.", "2."], "sin bloquear: solo a la izquierda");
        p.lock_scene_numbers();
        assert_eq!(tags(&p), ["1.", "1.", "2.", "2."], "bloqueada: a los dos lados");
    }

    #[test]
    fn marked_wrapping_keeps_the_bold_marks_on_their_words() {
        let marks: Vec<bool> = "Ana y Luis hablan".chars().enumerate().map(|(i, _)| i < 3 || (6..10).contains(&i)).collect();
        let rows = wrap_marked("Ana y Luis hablan", &marks, 10);
        assert_eq!(rows.iter().map(|(r, _)| r.as_str()).collect::<Vec<_>>(), ["Ana y Luis", "hablan"]);
        assert_eq!(rows[0].1, [true, true, true, false, false, false, true, true, true, true]);
        assert!(rows[1].1.iter().all(|b| !b));
        // Dos palabras seguidas en negrita (un nombre compuesto) llevan el espacio también.
        let marks = vec![true; 11];
        assert!(wrap_marked("Juan Carlos", &marks, 20)[0].1.iter().all(|b| *b));
    }

    #[test]
    fn characters_named_in_an_action_are_bold_in_the_pdf_only_when_asked() {
        let mut p = project(&[
            (Action, "Hoy Ana espera en la puerta.".into()),
            (Character, "ANA".into()),
            (Dialogue, "Hola".into()),
        ]);
        p.auto_link_all();
        let bold_text = |bold_names: bool| -> Vec<String> {
            layout(&p, Paper::A4, bold_names).pages[0].iter().filter(|l| l.bold).map(|l| l.text.clone()).collect()
        };
        assert!(bold_text(false).is_empty());
        let on = bold_text(true);
        // El nombre dentro de la acción (según la opción de mayúsculas del proyecto) y el del personaje que habla.
        assert_eq!(on, ["ANA", "ANA"]);
        // Lo que no es el nombre queda en la misma línea, sin negrita y bien ubicado.
        let page = &layout(&p, Paper::A4, true).pages[0];
        let name = page.iter().find(|l| l.text == "ANA" && l.bold).unwrap();
        let before = page.iter().find(|l| l.text == "Hoy ").unwrap();
        assert!((name.x - before.x - 4.0 * CHAR_W).abs() < 0.01 && name.y == before.y);
    }

    #[test]
    fn text_wraps_between_words_into_columns() {
        assert_eq!(wrap("uno dos tres", 7), ["uno dos", "tres"]);
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("a\nb", 10), ["a", "b"]);
        assert_eq!(wrap("", 5), [""]);
        assert_eq!(
            wrap("exactamente sesenta".repeat(1).as_str(), 19),
            ["exactamente sesenta"]
        );
    }

    #[test]
    fn every_element_lands_in_its_column_on_the_grid_of_rows() {
        let p = project(&[
            (SceneHeading, "INT. LIVING - NOCHE".into()),
            (Action, "Ana espera.".into()),
            (Character, "ANA".into()),
            (Parenthetical, "bajito".into()),
            (Dialogue, "No vino nadie.".into()),
            (Transition, "CORTE A:".into()),
        ]);
        let sheet = Paper::A4.sheet();
        let left = sheet.left * PT;
        let l = layout(&p, Paper::A4, false);
        assert_eq!(l.pages.len(), 1);
        let page = &l.pages[0];
        let at = |t: &str| {
            page.iter()
                .find(|x| x.text == t)
                .unwrap_or_else(|| panic!("{t}: {:?}", texts(page)))
                .clone()
        };
        assert_eq!(at("INT. LIVING - NOCHE").x, left);
        assert_eq!(at("ANA").x, left + 2.2 * PT);
        assert_eq!(at("(bajito)").x, left + 1.6 * PT);
        assert_eq!(at("No vino nadie.").x, left + 1.0 * PT);
        // La transición termina a 5,6 in del margen; todo cae en múltiplos de renglón desde el margen de arriba.
        let t = at("CORTE A:");
        assert!((t.x + 8.0 * CHAR_W - (left + 5.6 * PT)).abs() < 0.01);
        assert!(
            page.iter()
                .all(|x| ((x.y - sheet.top * PT) / ROW_H).fract().abs() < 1e-3)
        );
        // Dos renglones antes del encabezado de arriba de página no hay (arranca en el primero), y uno entre bloques.
        let rows: Vec<f32> = [
            "INT. LIVING - NOCHE",
            "Ana espera.",
            "ANA",
            "(bajito)",
            "No vino nadie.",
            "CORTE A:",
        ]
        .iter()
        .map(|t| (at(t).y - sheet.top * PT) / ROW_H)
        .collect();
        assert!(
            rows.iter()
                .zip([0.0, 2.0, 4.0, 5.0, 6.0, 8.0])
                .all(|(a, b)| (a - b).abs() < 1e-3),
            "{rows:?}"
        );
    }

    #[test]
    fn a_long_script_numbers_its_sheets_and_a_split_dialogue_carries_more_and_cont() {
        let long = (0..400)
            .map(|n| format!("palabra{n}."))
            .collect::<Vec<_>>()
            .join(" ");
        let p = project(&[
            (Character, "ANA".into()),
            (Dialogue, long.clone()),
            (Action, "Fin.".into()),
        ]);
        let l = layout(&p, Paper::A4, false);
        assert!(l.pages.len() >= 3, "{}", l.pages.len());
        assert!(
            texts(&l.pages[1]).contains(&"2.".to_string())
                && texts(&l.pages[2]).contains(&"3.".to_string())
        );
        assert!(
            !texts(&l.pages[0])
                .iter()
                .any(|t| t.ends_with('.') && t.len() <= 3),
            "la primera hoja no lleva número"
        );
        assert!(texts(&l.pages[0]).contains(&"(MÁS)".to_string()));
        assert!(texts(&l.pages[1]).contains(&"ANA (CONT.)".to_string()));
        // Todas las palabras están, en orden, y ningún renglón pasa de las 35 columnas.
        let spoken: Vec<String> = l
            .pages
            .iter()
            .flatten()
            .filter(|x| x.text.starts_with("palabra") || x.text.contains(" palabra"))
            .map(|x| x.text.clone())
            .collect();
        assert_eq!(spoken.join(" "), long);
        assert!(spoken.iter().all(|t| t.chars().count() <= 35));
        // Misma cuenta de renglones que la paginación del editor.
        let rows_per_page = Paper::A4.sheet().text_h() * 6.0;
        assert!(l.pages.iter().all(|pg| pg.iter().all(|x| x.y
            <= (Paper::A4.sheet().top * PT) + rows_per_page * ROW_H + 1.0
            || x.y < 80.0)));
    }

    #[test]
    fn locked_scene_numbers_go_on_both_sides_of_the_heading_and_the_cover_has_title_and_credits() {
        let mut p = project(&[
            (SceneHeading, "INT. A - DÍA".into()),
            (Action, "Hola.".into()),
        ]);
        p.contact = "marco@correo.com".into();
        p.lock_scene_numbers();
        let l = layout(&p, Paper::Letter, false);
        let sheet = Paper::Letter.sheet();
        let tags: Vec<&Line> = l.pages[0].iter().filter(|x| x.text == "1.").collect();
        assert_eq!(tags.len(), 2);
        assert!(tags[0].x < sheet.left * PT && tags[1].x > (sheet.left + sheet.text_w()) * PT);
        assert!(
            texts(&l.cover).contains(&"EL NIDO".to_string())
                && texts(&l.cover).contains(&"Guion de".to_string())
                && texts(&l.cover).contains(&"marco@correo.com".to_string())
        );
        // Sin bloquear, el número va igual a la izquierda del encabezado, pero ya no del lado derecho.
        p.unlock_scene_numbers();
        let l = layout(&p, Paper::Letter, false);
        let tags: Vec<&Line> = l.pages[0].iter().filter(|x| x.text == "1.").collect();
        assert_eq!(tags.len(), 1);
        assert!(tags[0].x < sheet.left * PT);
    }

    #[test]
    fn the_pdf_is_a_valid_file_with_a_page_per_sheet_plus_the_cover() {
        let p = project(&[
            (SceneHeading, "INT. A - DÍA".into()),
            (Action, "Ñandú, ¿qué hacés? ¡Tomá!".into()),
        ]);
        let bytes = export(&p, Paper::A4, false, false).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
        assert!(bytes.len() > 2000);
    }

    #[test]
    fn changed_pages_carry_a_star_a_label_and_can_be_exported_alone() {
        let filler: Vec<(ParagraphKind, String)> = (0..120)
            .map(|n| (Action, format!("Acción número {n}.")))
            .collect();
        let mut p = project(&filler);
        assert!(
            export(&p, Paper::A4, false, true).is_err(),
            "sin revisión no hay páginas cambiadas"
        );
        p.new_revision("Primera entrega", "2026-10-30");
        assert!(
            export(&p, Paper::A4, false, true).is_err(),
            "todavía nada cambió"
        );
        // Se cambia un párrafo de la última hoja: solo esa hoja queda marcada.
        let last = p.paragraphs.len() - 1;
        p.paragraphs[last].text = "Acción cambiada.".into();
        let l = layout(&p, Paper::A4, false);
        assert!(l.pages.len() >= 3);
        let marked: Vec<usize> = l
            .changed
            .iter()
            .enumerate()
            .filter(|(_, c)| **c)
            .map(|(n, _)| n)
            .collect();
        assert_eq!(marked, vec![l.pages.len() - 1]);
        let page = &l.pages[l.pages.len() - 1];
        assert_eq!(page.iter().filter(|x| x.text == "*").count(), 1);
        assert!(
            page.iter()
                .any(|x| x.text == "Revisión azul: Primera entrega (2026-10-30)")
        );
        let sheet = Paper::A4.sheet();
        let star = page.iter().find(|x| x.text == "*").unwrap();
        assert!(
            star.x > (sheet.left + sheet.text_w()) * PT,
            "el asterisco va en el margen derecho"
        );
        assert!(l.pages[0].iter().all(|x| x.text != "*"));
        // El PDF de solo lo cambiado tiene una hoja; el completo, la portada y todas.
        let full = export(&p, Paper::A4, false, false).unwrap();
        let only = export(&p, Paper::A4, false, true).unwrap();
        assert!(only.len() < full.len());
        assert!(l.cover.iter().any(|x| x.text == "EL NIDO"));
    }
}
