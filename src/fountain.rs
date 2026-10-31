//! Fountain: el guion como texto plano. Se exporta con los elementos forzados cuando hace falta (los prefijos en
//! español no se reconocen solos) y se importa reconociendo encabezados, transiciones, personajes y diálogos.

use crate::model::{self, ParagraphKind, Project};

/// Lo que trae un archivo Fountain.
#[derive(Debug, Default, PartialEq)]
pub struct Imported {
    pub title: String,
    pub author: String,
    pub contact: String,
    pub version: String,
    pub blocks: Vec<(ParagraphKind, String)>,
    /// Número de escena de cada encabezado (`#12#`), en el mismo orden que los encabezados de `blocks`.
    pub scene_numbers: Vec<Option<String>>,
}

// ------------------------------------------------------------------------------------------------
// Exportar
// ------------------------------------------------------------------------------------------------

/// ¿Lo reconocería Fountain como encabezado por sí solo?
fn is_native_heading(text: &str) -> bool {
    let upper = text.to_uppercase();
    ["INT.", "EXT.", "INT/EXT", "INT./EXT.", "EST.", "I/E"]
        .iter()
        .any(|p| upper.starts_with(p))
}

fn needs_force_as_action(line: &str) -> bool {
    let first = line.chars().next().unwrap_or(' ');
    model::is_all_caps(line)
        || "!.@~#=>[".contains(first)
        || model::scene_heading_of(line).is_some()
        || model::looks_like_transition(line)
}

/// El guion como texto Fountain, con la portada como datos del encabezado.
pub fn export(project: &Project) -> String {
    let mut out = String::new();
    let mut field = |key: &str, value: &str| {
        let lines: Vec<&str> = value.lines().filter(|l| !l.trim().is_empty()).collect();
        match lines.as_slice() {
            [] => {}
            [one] => out.push_str(&format!("{key}: {}\n", one.trim())),
            many => {
                out.push_str(&format!("{key}:\n"));
                for l in many {
                    out.push_str(&format!("    {}\n", l.trim()));
                }
            }
        }
    };
    field("Title", &project.title);
    if !project.author.trim().is_empty() {
        field("Credit", "Guion de");
        field("Author", &project.author);
    }
    field("Draft date", &project.version);
    field("Contact", &project.contact);
    if !out.is_empty() {
        out.push_str("\n===\n\n");
    }

    let shown = project.shown_paragraphs();
    let numbers: std::collections::HashMap<u64, String> = if project.scene_numbers_locked {
        project.scene_labels().into_iter().collect()
    } else {
        Default::default()
    };
    let mut previous: Option<ParagraphKind> = None;
    for (i, (kind, text)) in shown.iter().enumerate() {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        use ParagraphKind::*;
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        let body = match kind {
            Act => format!("# {}", text.to_uppercase()),
            Sequence => format!("## {}", text.to_uppercase()),
            SceneHeading => {
                let upper = text.to_uppercase();
                let mut head = if is_native_heading(&upper) {
                    upper
                } else {
                    format!(".{upper}")
                };
                if let Some(n) = numbers.get(&project.paragraphs[i].id) {
                    head.push_str(&format!(" #{n}#"));
                }
                head
            }
            Action => lines
                .iter()
                .map(|l| {
                    if needs_force_as_action(l) {
                        format!("!{l}")
                    } else {
                        (*l).to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Character => {
                // Sin diálogo debajo, o con minúsculas, no lo reconocería como personaje.
                let followed = shown.get(i + 1).is_some_and(|(k, t)| {
                    matches!(k, Parenthetical | Dialogue) && !t.trim().is_empty()
                });
                if followed && model::is_all_caps(text) {
                    text.to_string()
                } else {
                    format!("@{text}")
                }
            }
            Parenthetical => format!("({})", text.trim_matches(['(', ')'])),
            Dialogue => lines
                .iter()
                .map(|l| if l.is_empty() { "  " } else { l })
                .collect::<Vec<_>>()
                .join("\n"),
            Transition => format!("> {}", text.to_uppercase()),
            Lyrics => lines
                .iter()
                .map(|l| format!("~{l}"))
                .collect::<Vec<_>>()
                .join("\n"),
            Note => format!("[[{text}]]"),
        };
        // Personaje, acotación y diálogo van pegados; las líneas de una letra, también.
        let attached = match (previous, kind) {
            (Some(Character | Parenthetical | Dialogue), Parenthetical | Dialogue) => true,
            (Some(Lyrics), Lyrics) => true,
            _ => false,
        };
        if !out.is_empty() && !attached {
            out.push('\n');
        }
        out.push_str(&body);
        out.push('\n');
        previous = Some(*kind);
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Importar
// ------------------------------------------------------------------------------------------------

/// Quita los comentarios `/* … */`.
fn strip_boneyard(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Separa las notas `[[…]]` de una línea: lo que queda de la línea y las notas.
fn take_notes(line: &str) -> (String, Vec<String>) {
    let mut clean = String::new();
    let mut notes = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("[[") {
        match rest[start + 2..].find("]]") {
            Some(end) => {
                clean.push_str(&rest[..start]);
                notes.push(rest[start + 2..start + 2 + end].trim().to_string());
                rest = &rest[start + 2 + end + 2..];
            }
            None => break,
        }
    }
    clean.push_str(rest);
    (
        clean.split_whitespace().collect::<Vec<_>>().join(" "),
        notes,
    )
}

/// Un encabezado sin su número de escena: («INT. CASA - DÍA #12#» → «INT. CASA - DÍA», «12»).
fn split_scene_number(line: &str) -> (String, Option<String>) {
    let line = line.trim();
    if let Some(stripped) = line.strip_suffix('#') {
        if let Some(open) = stripped.rfind('#') {
            let number = stripped[open + 1..].trim();
            if !number.is_empty() && !number.contains(' ') {
                return (
                    stripped[..open].trim().to_string(),
                    Some(number.to_string()),
                );
            }
        }
    }
    (line.to_string(), None)
}

/// Lee el encabezado de datos (título, autor…) si el texto empieza con uno. Devuelve lo que sobra del texto.
fn title_page(text: &str, out: &mut Imported) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let key_of = |l: &str| -> Option<String> {
        let (k, _) = l.split_once(':')?;
        let k = k.trim().to_lowercase();
        (!k.is_empty()
            && !l.starts_with([' ', '\t'])
            && k.chars().all(|c| c.is_alphabetic() || c == ' '))
        .then_some(k)
    };
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    let known = [
        "title",
        "credit",
        "author",
        "authors",
        "source",
        "draft date",
        "date",
        "contact",
        "copyright",
        "notes",
        "revision",
    ];
    if !lines
        .get(start)
        .and_then(|l| key_of(l))
        .is_some_and(|k| known.contains(&k.as_str()))
    {
        return text.to_string();
    }
    let mut i = start;
    let mut current: Option<String> = None;
    while i < lines.len() && !lines[i].trim().is_empty() {
        let line = lines[i];
        if let Some(key) = key_of(line) {
            let value = line
                .split_once(':')
                .map_or("", |(_, v)| v)
                .trim()
                .to_string();
            current = Some(key.clone());
            store(out, &key, &value);
        } else if let Some(key) = &current {
            store(out, key, line.trim());
        }
        i += 1;
    }
    lines[i..].join("\n")
}

fn store(out: &mut Imported, key: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let slot = match key {
        "title" => &mut out.title,
        "author" | "authors" => &mut out.author,
        "contact" => &mut out.contact,
        "draft date" | "date" | "revision" => &mut out.version,
        _ => return,
    };
    if !slot.is_empty() {
        slot.push('\n');
    }
    slot.push_str(value);
}

fn push(out: &mut Imported, kind: ParagraphKind, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let text = if kind.is_uppercase() {
        text.to_uppercase()
    } else {
        text.to_string()
    };
    out.blocks.push((kind, text));
}

/// Un bloque de líneas seguidas (sin líneas en blanco entre ellas).
fn block(lines: &[String], out: &mut Imported) {
    let Some(first) = lines.first() else { return };
    let rest = &lines[1..];
    use ParagraphKind::*;

    // Secciones y sinopsis: un título de nivel 1 es un acto, de nivel 2 una secuencia; lo demás se ignora.
    if first.starts_with('#') {
        let depth = first.chars().take_while(|c| *c == '#').count();
        let title = first[depth..].trim();
        match depth {
            1 => push(out, Act, title),
            2 => push(out, Sequence, title),
            _ => {}
        }
        return block(rest, out);
    }
    if first.starts_with('=') {
        return block(rest, out);
    }
    // Un salto de página forzado o una línea solo de notas.
    let (clean_first, first_notes) = take_notes(first);
    if clean_first.is_empty() {
        for note in first_notes {
            push(out, Note, &note);
        }
        return block(rest, out);
    }

    // Encabezado de escena (forzado con «.» o por su prefijo).
    let forced_heading = first
        .strip_prefix('.')
        .filter(|r| r.starts_with(|c: char| c.is_alphanumeric()));
    let native = model::scene_heading_of(&clean_first).is_some() && forced_heading.is_none();
    if forced_heading.is_some() || native {
        let (head, number) = split_scene_number(forced_heading.unwrap_or(&clean_first));
        push(out, SceneHeading, &head);
        out.scene_numbers.push(number);
        for note in first_notes {
            push(out, Note, &note);
        }
        return block(rest, out);
    }

    // Transición.
    if (first.starts_with('>') && !first.ends_with('<'))
        || (model::looks_like_transition(&clean_first) && rest.is_empty())
    {
        push(out, Transition, clean_first.trim_start_matches('>'));
        return block(rest, out);
    }
    // Texto centrado: se queda como una acción.
    if first.starts_with('>') && first.ends_with('<') {
        push(
            out,
            Action,
            clean_first.trim_start_matches('>').trim_end_matches('<'),
        );
        return block(rest, out);
    }
    // Letra de una canción.
    if first.starts_with('~') {
        for line in lines.iter().take_while(|l| l.starts_with('~')) {
            push(out, Lyrics, line.trim_start_matches('~'));
        }
        return block(
            &lines[lines.iter().take_while(|l| l.starts_with('~')).count()..],
            out,
        );
    }
    // Personaje con su diálogo: en mayúsculas (o forzado con «@») y con algo debajo.
    let forced_cue = first.strip_prefix('@');
    let cue = forced_cue.unwrap_or(&clean_first);
    let cue = cue.trim_end_matches('^').trim();
    if !rest.is_empty()
        && (forced_cue.is_some() || (model::is_all_caps(cue) && !first.starts_with('!')))
    {
        push(out, Character, cue);
        let mut spoken: Vec<String> = Vec::new();
        let flush = |spoken: &mut Vec<String>, out: &mut Imported| {
            push(out, Dialogue, &spoken.join(" "));
            spoken.clear();
        };
        for line in rest {
            let (text, notes) = take_notes(line);
            if text.starts_with('(') && text.ends_with(')') {
                flush(&mut spoken, out);
                push(out, Parenthetical, text[1..text.len() - 1].trim());
            } else if !text.is_empty() {
                spoken.push(text);
            }
            for note in notes {
                flush(&mut spoken, out);
                push(out, Note, &note);
            }
        }
        flush(&mut spoken, out);
        return;
    }

    // Todo lo demás es acción: las líneas seguidas forman un solo párrafo.
    let mut text = Vec::new();
    for line in lines {
        let (clean, notes) = take_notes(line);
        let clean = clean
            .strip_prefix('!')
            .map_or(clean.clone(), |c| c.trim().to_string());
        if !clean.is_empty() {
            text.push(clean);
        }
        for note in notes {
            push(out, Note, &note);
        }
    }
    push(out, Action, &text.join(" "));
}

/// Lee un guion en Fountain.
pub fn import(text: &str) -> Imported {
    let text = strip_boneyard(&text.replace("\r\n", "\n").replace('\r', "\n"));
    let mut out = Imported::default();
    let body = title_page(text.trim_start_matches('\u{feff}'), &mut out);
    let mut current: Vec<String> = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            block(&current, &mut out);
            current.clear();
        } else if line.trim() == "===" {
            block(&current, &mut out);
            current.clear();
        } else {
            current.push(line.trim().to_string());
        }
    }
    block(&current, &mut out);
    out
}

impl Project {
    /// Reemplaza el guion y los datos de la portada por los de un archivo Fountain.
    pub fn import_fountain(&mut self, text: &str) {
        let imported = import(text);
        if !imported.title.is_empty() {
            self.title = imported.title.replace('\n', " ");
        }
        if !imported.author.is_empty() {
            self.author = imported.author.replace('\n', ", ");
        }
        if !imported.contact.is_empty() {
            self.contact = imported.contact.clone();
        }
        if !imported.version.is_empty() {
            self.version = imported.version.replace('\n', " ");
        }
        self.replace_script(imported.blocks, imported.scene_numbers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ParagraphKind::*;

    fn project(blocks: &[(ParagraphKind, &str)]) -> Project {
        let mut p = Project::default();
        p.title = "El nido".into();
        p.author = "Marco".into();
        p.paragraphs = blocks
            .iter()
            .map(|(k, t)| p.make_paragraph(*k, t))
            .collect();
        p
    }

    #[test]
    fn a_script_is_exported_with_forced_elements_where_fountain_would_not_recognize_them() {
        let p = project(&[
            (Act, "ACTO I"),
            (SceneHeading, "INT. LIVING - NOCHE"),
            (Action, "Ana espera. SE ESCUCHA UN GOLPE."),
            (Action, "FIN DEL DÍA"),
            (Character, "ANA"),
            (Parenthetical, "bajito"),
            (Dialogue, "No vino nadie."),
            (Dialogue, "Todavía."),
            (Transition, "CORTE A:"),
            (SceneHeading, "INTERIOR COCINA - DÍA"),
            (Lyrics, "La la la"),
            (Lyrics, "Lo lo lo"),
            (Note, "revisar"),
        ]);
        let text = export(&p);
        assert!(
            text.starts_with("Title: El nido\nCredit: Guion de\nAuthor: Marco\n\n===\n\n"),
            "{text}"
        );
        assert!(
            text.contains("# ACTO I\n\nINT. LIVING - NOCHE\n\n"),
            "{text}"
        );
        assert!(
            text.contains("!FIN DEL DÍA\n"),
            "una acción en mayúsculas se fuerza"
        );
        assert!(
            text.contains("\nANA\n(bajito)\nNo vino nadie.\nTodavía.\n\n> CORTE A:\n\n"),
            "{text}"
        );
        assert!(
            text.contains(".INTERIOR COCINA - DÍA\n"),
            "un encabezado sin INT./EXT. se fuerza"
        );
        assert!(
            text.contains("~La la la\n~Lo lo lo\n\n[[revisar]]"),
            "{text}"
        );
    }

    #[test]
    fn exporting_and_importing_gives_back_the_same_script() {
        let mut p = project(&[
            (SceneHeading, "INT. LIVING - NOCHE"),
            (Action, "Ana espera junto a la ventana."),
            (Character, "ANA (V.O.)"),
            (Parenthetical, "bajito"),
            (Dialogue, "No vino nadie."),
            (Transition, "CORTE A:"),
            (SceneHeading, "EXT. CALLE - DÍA"),
            (Action, "FIN DEL DÍA"),
            (Character, "juan"),
            (Dialogue, "Hola."),
        ]);
        p.lock_scene_numbers();
        let text = export(&p);
        let back = import(&text);
        assert_eq!(back.title, "El nido");
        assert_eq!(back.author, "Marco");
        let expected: Vec<(ParagraphKind, String)> = p
            .paragraphs
            .iter()
            .map(|q| {
                (
                    q.kind,
                    if q.kind == Character {
                        q.text.to_uppercase()
                    } else {
                        q.text.clone()
                    },
                )
            })
            .collect();
        assert_eq!(back.blocks, expected, "{text}");
        assert_eq!(
            back.scene_numbers,
            vec![Some("1".to_string()), Some("2".to_string())]
        );
    }

    #[test]
    fn a_typical_fountain_file_is_imported_by_element() {
        let text = "Title:\n    _**EL NIDO**_\nAuthor: Marco\nDraft date: octubre\nContact:\n    marco@correo.com\n    11 5555\n\n\
                    FUNDE DESDE NEGRO:\n\n.EN EL PASILLO\n\nINT. LIVING - NOCHE #12A#\n\nAna espera.\nMira el reloj.\n\n\
                    @Ana\n(bajito)\nNo vino nadie.\nTodavía no. [[ojo con el tono]]\n\n> CORTE A:\n\n# ACTO DOS\n\n= sinopsis\n\n/* borrar */\n\n===\n\n\
                    ~Canta la canción\n\nTEXTO EN MAYÚSCULAS SOLO\n";
        let got = import(text);
        assert_eq!(
            (got.author.as_str(), got.version.as_str()),
            ("Marco", "octubre")
        );
        assert_eq!(got.contact, "marco@correo.com\n11 5555");
        assert!(got.title.contains("EL NIDO"));
        assert_eq!(
            got.blocks,
            vec![
                (Transition, "FUNDE DESDE NEGRO:".to_string()),
                (SceneHeading, "EN EL PASILLO".to_string()),
                (SceneHeading, "INT. LIVING - NOCHE".to_string()),
                (Action, "Ana espera. Mira el reloj.".to_string()),
                (Character, "ANA".to_string()),
                (Parenthetical, "bajito".to_string()),
                (Dialogue, "No vino nadie. Todavía no.".to_string()),
                (Note, "ojo con el tono".to_string()),
                (Transition, "CORTE A:".to_string()),
                (Act, "ACTO DOS".to_string()),
                (Lyrics, "Canta la canción".to_string()),
                (Action, "TEXTO EN MAYÚSCULAS SOLO".to_string()),
            ]
        );
        assert_eq!(got.scene_numbers, vec![None, Some("12A".to_string())]);
    }

    #[test]
    fn importing_replaces_the_script_creates_the_characters_and_keeps_the_numbers() {
        let mut p = Project::default();
        p.import_fountain("Title: Prueba\n\n.UNO #1#\n\nANA\nHola.\n\n.DOS #2#\n\nJUAN\nChau.\n");
        assert_eq!(p.title, "Prueba");
        assert_eq!(p.paragraphs.len(), 6);
        assert!(p.scene_numbers_locked);
        assert_eq!(
            p.scene_labels()
                .into_iter()
                .map(|(_, n)| n)
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        let names: Vec<_> = p.characters.iter().map(|c| c.name.clone()).collect();
        assert!(
            names.contains(&"ANA".to_string()) && names.contains(&"JUAN".to_string()),
            "{names:?}"
        );
        // Sin números en todos los encabezados, la numeración queda libre.
        p.import_fountain("INT. A - DÍA\n\nTexto.\n");
        assert!(!p.scene_numbers_locked);
    }
}
