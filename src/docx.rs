//! Exportar el guion a Word (.docx): portada y guion con la misma página, los mismos márgenes y las mismas sangrías
//! que la vista. Un .docx es un zip de archivos XML; el zip se escribe a mano (sin comprimir), así no hace falta
//! ninguna dependencia.

use crate::{
    model::{ParagraphKind, Project},
    storage::{Paper, Sheet},
};

// ------------------------------------------------------------------------------------------------
// Zip
// ------------------------------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Arma un zip con los archivos tal cual (método «almacenado»).
fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    // 1 de enero de 2026 a las 00:00 en formato DOS.
    let (time, date) = (0u16, ((2026u16 - 1980) << 9) | (1 << 5) | 1);
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let header = |out: &mut Vec<u8>, central: bool| {
            out.extend_from_slice(if central {
                &[0x50, 0x4B, 0x01, 0x02]
            } else {
                &[0x50, 0x4B, 0x03, 0x04]
            });
            if central {
                out.extend_from_slice(&20u16.to_le_bytes()); // versión con la que se hizo
            }
            out.extend_from_slice(&20u16.to_le_bytes()); // versión necesaria
            out.extend_from_slice(&0x0800u16.to_le_bytes()); // nombres en UTF-8
            out.extend_from_slice(&0u16.to_le_bytes()); // sin comprimir
            out.extend_from_slice(&time.to_le_bytes());
            out.extend_from_slice(&date.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // campo extra
            if central {
                out.extend_from_slice(&[0; 6]); // comentario, disco, atributos internos
                out.extend_from_slice(&0u32.to_le_bytes()); // atributos externos
                out.extend_from_slice(&offset.to_le_bytes());
            }
            out.extend_from_slice(name.as_bytes());
        };
        header(&mut out, false);
        out.extend_from_slice(data);
        header(&mut central, true);
    }
    let central_offset = out.len() as u32;
    let central_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06, 0, 0, 0, 0]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

// ------------------------------------------------------------------------------------------------
// Documento
// ------------------------------------------------------------------------------------------------

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Pulgadas a veinteavos de punto (la unidad de Word).
fn twips(inches: f32) -> i32 {
    (inches * 1440.0).round() as i32
}

/// Cuánto sobresale el número de escena hacia el margen izquierdo, en pulgadas.
const NUMBER_W: f32 = 0.6;

/// Un párrafo: su texto (las líneas separadas por «\n» salen como saltos de línea) y su formato.
struct Para {
    lines: Vec<String>,
    /// Espacio antes, en renglones.
    before: f32,
    /// Sangría a la izquierda y a la derecha, en pulgadas, desde los márgenes de la página.
    left: f32,
    right: f32,
    align: &'static str,
    keep_next: bool,
    bold: bool,
    /// Qué caracteres del texto van en negrita (un valor por carácter; vacío: ninguno).
    marks: Vec<bool>,
    underline: bool,
    page_break_before: bool,
    /// El número de la escena, a la izquierda del encabezado (en el margen) y, si la numeración está bloqueada, también a la
    /// derecha: en ese caso, la posición (en twips desde el margen izquierdo) donde va.
    number: Option<String>,
    number_right: Option<i32>,
}

impl Para {
    fn new(text: &str) -> Self {
        Self {
            lines: text.lines().map(str::to_string).collect(),
            before: 1.0,
            left: 0.0,
            right: 0.0,
            align: "left",
            keep_next: false,
            bold: false,
            marks: Vec::new(),
            underline: false,
            page_break_before: false,
            number: None,
            number_right: None,
        }
    }

    fn xml(&self, section_end: Option<&str>) -> String {
        let mut props = String::new();
        if self.keep_next {
            props.push_str("<w:keepNext/>");
        }
        if self.page_break_before {
            props.push_str("<w:pageBreakBefore/>");
        }
        props.push_str("<w:widowControl/>");
        if self.number.is_some() {
            props.push_str("<w:tabs><w:tab w:val=\"left\" w:pos=\"0\"/>");
            if let Some(pos) = self.number_right {
                props.push_str(&format!("<w:tab w:val=\"left\" w:pos=\"{pos}\"/>"));
            }
            props.push_str("</w:tabs>");
        }
        props.push_str(&format!(
            "<w:spacing w:before=\"{}\" w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/>",
            (self.before * 240.0).round() as i32
        ));
        // El número sobresale 0,6" hacia el margen: la primera línea arranca ahí y el texto sigue en su lugar.
        let hanging = if self.number.is_some() { format!(" w:hanging=\"{}\"", twips(NUMBER_W)) } else { String::new() };
        props.push_str(&format!(
            "<w:ind w:left=\"{}\" w:right=\"{}\"{hanging}/>",
            twips(self.left),
            twips(self.right)
        ));
        props.push_str(&format!("<w:jc w:val=\"{}\"/>", self.align));
        if let Some(section) = section_end {
            props.push_str(section);
        }
        let mut run_props = String::new();
        if self.bold {
            run_props.push_str("<w:b/>");
        }
        if self.underline {
            run_props.push_str("<w:u w:val=\"single\"/>");
        }
        let mut runs = String::new();
        if let Some(number) = &self.number {
            runs.push_str(&format!("<w:r><w:t xml:space=\"preserve\">{}.</w:t></w:r><w:r><w:tab/></w:r>", escape(number)));
        }
        let mut offset = 0;
        for (n, line) in self.lines.iter().enumerate() {
            if n > 0 {
                runs.push_str("<w:r><w:br/></w:r>");
            }
            let chars: Vec<char> = line.chars().collect();
            let marked = |i: usize| self.marks.get(offset + i).copied().unwrap_or(false);
            let mut from = 0;
            while from < chars.len() || (from == 0 && chars.is_empty()) {
                let flag = marked(from);
                let mut to = from;
                while to < chars.len() && marked(to) == flag {
                    to += 1;
                }
                let props = if flag && !self.bold { format!("<w:b/>{run_props}") } else { run_props.clone() };
                runs.push_str(&format!(
                    "<w:r><w:rPr>{props}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    escape(&chars[from..to].iter().collect::<String>())
                ));
                if chars.is_empty() {
                    break;
                }
                from = to;
            }
            offset += chars.len() + 1;
        }
        if let (Some(number), Some(_)) = (&self.number, self.number_right) {
            runs.push_str(&format!("<w:r><w:tab/></w:r><w:r><w:t xml:space=\"preserve\">{}.</w:t></w:r>", escape(number)));
        }
        format!("<w:p><w:pPr>{props}</w:pPr>{runs}</w:p>")
    }
}

/// El formato de cada elemento, con las mismas sangrías que la vista (en pulgadas desde el margen de texto).
fn paragraph(
    kind: ParagraphKind,
    text: &str,
    previous: Option<ParagraphKind>,
    sheet: Sheet,
    bold_names: bool,
) -> Para {
    use ParagraphKind::*;
    let text_w = sheet.text_w();
    let mut para = Para::new(text);
    match kind {
        SceneHeading => {
            para.before = 2.0;
            para.keep_next = true;
        }
        Action | Note => {}
        Character => {
            para.left = 2.2;
            para.right = (text_w - 2.2 - 3.8).max(0.0);
            para.keep_next = true;
            para.bold = bold_names;
        }
        Parenthetical => {
            para.before = 0.0;
            para.left = 1.6;
            para.right = (text_w - 1.6 - 2.4).max(0.0);
            para.keep_next = true;
            para.lines = vec![format!("({})", text.trim_matches(['(', ')']))];
        }
        Dialogue | Lyrics => {
            para.before = if kind == Lyrics && previous != Some(Lyrics) {
                1.0
            } else {
                0.0
            };
            para.left = 1.0;
            para.right = (text_w - 4.5).max(0.0);
        }
        Transition => {
            para.align = "right";
            para.right = (text_w - 5.6).max(0.0);
        }
        Act => {
            para.before = 2.0;
            para.align = "center";
            para.bold = true;
            para.underline = true;
            para.page_break_before = previous.is_some();
            para.keep_next = true;
        }
        Sequence => {
            para.before = 2.0;
            para.align = "center";
            para.keep_next = true;
        }
    }
    para
}

fn sect_pr(sheet: Sheet, header: bool, restart_numbers: bool) -> String {
    format!(
        "<w:sectPr>{}<w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>{}{}</w:sectPr>",
        if header {
            "<w:headerReference w:type=\"default\" r:id=\"rId2\"/>"
        } else {
            ""
        },
        twips(sheet.width),
        twips(sheet.height),
        twips(sheet.top),
        twips(sheet.right),
        twips(sheet.bottom),
        twips(sheet.left),
        if restart_numbers {
            "<w:pgNumType w:start=\"1\"/>"
        } else {
            ""
        },
        // La primera página del guion no lleva número.
        if header { "<w:titlePg/>" } else { "" },
    )
}

/// El guion como un .docx en memoria.
pub fn export(project: &Project, paper: Paper, bold_names: bool) -> Vec<u8> {
    let sheet = paper.sheet();
    let mut body = String::new();

    // Portada: título a un tercio de la altura, autoría debajo, contacto y versión al pie.
    let cover = |text: &str, before: f32, align: &'static str| {
        let mut p = Para::new(text);
        p.before = before;
        p.align = align;
        p
    };
    let mut cover_paras = vec![cover(&project.title.to_uppercase(), 14.0, "center")];
    if !project.author.trim().is_empty() {
        cover_paras.push(cover("Guion de", 4.0, "center"));
        cover_paras.push(cover(project.author.trim(), 1.0, "center"));
    }
    if !project.contact.trim().is_empty() {
        cover_paras.push(cover(project.contact.trim(), 14.0, "left"));
    }
    if !project.version.trim().is_empty() {
        cover_paras.push(cover(project.version.trim(), 1.0, "right"));
    }
    let last = cover_paras.len() - 1;
    for (n, para) in cover_paras.iter().enumerate() {
        body.push_str(&para.xml((n == last).then(|| sect_pr(sheet, false, false)).as_deref()));
    }

    // El guion. Las notas no se imprimen.
    let shown = project.shown_paragraphs_marked();
    let conts = project.cont_marks();
    let labels: std::collections::HashMap<u64, String> = project.scene_labels().into_iter().collect();
    let mut previous: Option<ParagraphKind> = None;
    let mut any = false;
    for (i, (kind, text, flags)) in shown.iter().enumerate() {
        if *kind == ParagraphKind::Note || text.trim().is_empty() {
            continue;
        }
        let mut text = if kind.is_uppercase() {
            text.to_uppercase()
        } else {
            text.clone()
        };
        if conts.get(i).copied().unwrap_or(false) {
            text.push_str(" (CONT.)");
        }
        let mut para = paragraph(*kind, &text, previous, sheet, bold_names);
        if *kind == ParagraphKind::SceneHeading
            && let Some(number) = labels.get(&project.paragraphs[i].id)
        {
            para.number = Some(number.clone());
            if project.scene_numbers_locked {
                para.number_right = Some(twips(sheet.text_w() + 0.2));
            }
        }
        if bold_names {
            para.marks = flags.clone();
        }
        body.push_str(&para.xml(None));
        previous = Some(*kind);
        any = true;
    }
    if !any {
        body.push_str(&Para::new("").xml(None));
    }
    body.push_str(&sect_pr(sheet, true, true));

    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}</w:body></w:document>"
    );
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Courier New\" w:hAnsi=\"Courier New\" w:cs=\"Courier New\" w:eastAsia=\"Courier New\"/><w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/><w:lang w:val=\"es-AR\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style></w:styles>";
    // Número de página arriba a la derecha, con punto.
    let header = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:hdr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:pPr><w:jc w:val=\"right\"/></w:pPr><w:fldSimple w:instr=\" PAGE \"><w:r><w:t>2</w:t></w:r></w:fldSimple><w:r><w:t>.</w:t></w:r></w:p></w:hdr>";
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/header1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/></Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
    let doc_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/header\" Target=\"header1.xml\"/></Relationships>";

    zip(&[
        ("[Content_Types].xml", content_types.as_bytes().to_vec()),
        ("_rels/.rels", rels.as_bytes().to_vec()),
        ("word/document.xml", document.into_bytes()),
        ("word/styles.xml", styles.as_bytes().to_vec()),
        ("word/header1.xml", header.as_bytes().to_vec()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes().to_vec()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ParagraphKind::*;

    /// Los archivos de un zip sin comprimir: (nombre, contenido), comprobando el CRC de cada uno.
    fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
        let u32_at =
            |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let end = bytes.len() - 22;
        assert_eq!(&bytes[end..end + 4], &[0x50, 0x4B, 0x05, 0x06]);
        let count = u16_at(end + 10);
        let mut at = u32_at(end + 16) as usize;
        let mut out = Vec::new();
        for _ in 0..count {
            assert_eq!(&bytes[at..at + 4], &[0x50, 0x4B, 0x01, 0x02]);
            let (crc, size, name_len, extra, comment) = (
                u32_at(at + 16),
                u32_at(at + 24) as usize,
                u16_at(at + 28),
                u16_at(at + 30),
                u16_at(at + 32),
            );
            let local = u32_at(at + 42) as usize;
            let name = String::from_utf8(bytes[at + 46..at + 46 + name_len].to_vec()).unwrap();
            let data_at = local + 30 + u16_at(local + 26) + u16_at(local + 28);
            let data = bytes[data_at..data_at + size].to_vec();
            assert_eq!(crc32(&data), crc, "CRC de {name}");
            out.push((name, data));
            at += 46 + name_len + extra + comment;
        }
        out
    }

    fn sample() -> Project {
        let mut p = Project::default();
        p.title = "El nido & Cía".into();
        p.author = "Marco".into();
        p.paragraphs = [
            (SceneHeading, "INT. LIVING - NOCHE"),
            (Action, "Ana <espera>."),
            (Character, "ANA"),
            (Parenthetical, "bajito"),
            (Dialogue, "No vino nadie."),
            (Note, "no se imprime"),
            (Transition, "CORTE A:"),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p
    }

    #[test]
    fn bold_names_make_the_character_line_and_the_named_characters_bold() {
        let mut p = Project::default();
        p.paragraphs = [(Action, "Hoy Ana espera."), (Character, "ANA"), (Dialogue, "Hola")]
            .iter()
            .map(|(k, t)| p.make_paragraph(*k, t))
            .collect();
        p.auto_link_all();
        let xml = |bold| String::from_utf8(unzip(&export(&p, Paper::A4, bold))[2].1.clone()).unwrap();
        assert!(!xml(false).contains("<w:b/>"));
        let on = xml(true);
        assert!(on.contains("<w:rPr></w:rPr><w:t xml:space=\"preserve\"> espera.</w:t>"), "{on}");
        // Dos tramos en negrita: el nombre dentro de la acción y el del personaje que habla.
        assert_eq!(on.matches("<w:b/></w:rPr><w:t xml:space=\"preserve\">ANA</w:t>").count(), 2, "{on}");
        assert!(on.contains("<w:rPr></w:rPr><w:t xml:space=\"preserve\">Hoy </w:t>"));
    }

    #[test]
    fn crc_matches_the_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn the_docx_holds_every_part_with_the_page_and_the_indents_of_the_view() {
        let bytes = export(&sample(), Paper::A4, false);
        let files = unzip(&bytes);
        let names: Vec<_> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "[Content_Types].xml",
                "_rels/.rels",
                "word/document.xml",
                "word/styles.xml",
                "word/header1.xml",
                "word/_rels/document.xml.rels"
            ]
        );
        let doc = String::from_utf8(files[2].1.clone()).unwrap();
        // Texto escapado, notas fuera, personaje y transición con su formato.
        assert!(
            doc.contains("EL NIDO &amp; CÍA") && doc.contains("Ana &lt;espera&gt;."),
            "{doc}"
        );
        assert!(!doc.contains("no se imprime"));
        assert!(doc.contains("(bajito)") && doc.contains("w:val=\"right\""));
        // A4: 11906 × 16838 veinteavos de punto, con 60 columnas de texto (8,5 in de 1,5 + 6,0 + 0,77 → 6 in = 8640).
        assert!(
            doc.contains("w:w=\"11906\" w:h=\"16838\"")
                || doc.contains("w:w=\"11905\" w:h=\"16838\""),
            "{doc}"
        );
        let sheet = Paper::A4.sheet();
        assert!(doc.contains(&format!("w:left=\"{}\"", twips(sheet.left))));
        assert!(
            doc.contains("w:left=\"3168\""),
            "el personaje va a 2,2 in del margen"
        );
        // Dos secciones: la portada sin número y el guion con número desde la segunda página.
        assert_eq!(doc.matches("<w:sectPr>").count(), 2);
        assert!(doc.contains("<w:titlePg/>") && doc.contains("w:headerReference"));
        let letter =
            String::from_utf8(unzip(&export(&sample(), Paper::Letter, false))[2].1.clone()).unwrap();
        assert!(letter.contains("w:w=\"12240\" w:h=\"15840\""));
    }

    #[test]
    fn acts_start_a_new_page_but_not_when_they_open_the_script() {
        let mut p = Project::default();
        p.paragraphs = [(Act, "ACTO I"), (Action, "Uno."), (Act, "ACTO II")]
            .iter()
            .map(|(k, t)| p.make_paragraph(*k, t))
            .collect();
        let doc = String::from_utf8(unzip(&export(&p, Paper::A4, false))[2].1.clone()).unwrap();
        assert_eq!(doc.matches("<w:pageBreakBefore/>").count(), 1);
    }

    #[test]
    fn scene_numbers_go_beside_the_heading_and_on_both_sides_when_locked() {
        let mut p = Project::default();
        p.paragraphs = [(SceneHeading, "INT. LIVING - NOCHE"), (Action, "Ana espera."), (SceneHeading, "EXT. CALLE - DIA")]
            .iter()
            .map(|(k, t)| p.make_paragraph(*k, t))
            .collect();
        let xml = |p: &Project| String::from_utf8(unzip(&export(p, Paper::A4, false))[2].1.clone()).unwrap();
        let free = xml(&p);
        assert!(free.contains("<w:t xml:space=\"preserve\">1.</w:t></w:r><w:r><w:tab/></w:r>"), "{free}");
        assert!(free.contains("<w:t xml:space=\"preserve\">2.</w:t></w:r><w:r><w:tab/></w:r>"));
        // Sin bloquear, solo a la izquierda: la segunda pestaña (la del lado derecho) no existe.
        assert_eq!(free.matches("<w:tab w:val=\"left\"").count(), 2, "una pestaña por encabezado");
        p.lock_scene_numbers();
        let fixed = xml(&p);
        assert_eq!(fixed.matches("<w:tab w:val=\"left\"").count(), 4, "con la numeración bloqueada, también a la derecha");
        assert!(fixed.contains("<w:r><w:tab/></w:r><w:r><w:t xml:space=\"preserve\">1.</w:t></w:r>"));
    }
}
