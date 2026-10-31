//! vCard (.vcf): la forma de pasarle un contacto a otra persona. Se escribe la versión 3.0 con lo que la app guarda
//! (nombre, rol, departamento, teléfono y mail) y se lee lo mismo de cualquier archivo.

use crate::model::Contact;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Los contactos como un archivo .vcf.
pub fn export(contacts: &[Contact]) -> String {
    let mut out = String::new();
    for c in contacts {
        out.push_str("BEGIN:VCARD\r\nVERSION:3.0\r\n");
        out.push_str(&format!(
            "FN:{}\r\nN:{};;;;\r\n",
            escape(&c.name),
            escape(&c.name)
        ));
        if !c.role.is_empty() {
            out.push_str(&format!("TITLE:{}\r\n", escape(&c.role)));
        }
        if !c.department.is_empty() {
            out.push_str(&format!("ORG:{}\r\n", escape(&c.department)));
        }
        if !c.phone.is_empty() {
            out.push_str(&format!("TEL;TYPE=CELL:{}\r\n", escape(&c.phone)));
        }
        if !c.email.is_empty() {
            out.push_str(&format!("EMAIL;TYPE=INTERNET:{}\r\n", escape(&c.email)));
        }
        out.push_str("END:VCARD\r\n");
    }
    out
}

/// Los contactos de un archivo .vcf. Los ids quedan en 0: los pone quien los agrega al proyecto.
pub fn parse(text: &str) -> Vec<Contact> {
    // Las líneas largas se doblan con un espacio al comienzo de la siguiente.
    let mut lines: Vec<String> = Vec::new();
    for raw in text.replace("\r\n", "\n").split('\n') {
        match (raw.starts_with([' ', '\t']), lines.last_mut()) {
            (true, Some(last)) => last.push_str(raw.trim_start()),
            _ => lines.push(raw.to_string()),
        }
    }
    let mut out = Vec::new();
    let mut current: Option<Contact> = None;
    for line in lines {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let name = key.split(';').next().unwrap_or("").to_uppercase();
        match (name.as_str(), current.as_mut()) {
            ("BEGIN", _) if value.eq_ignore_ascii_case("VCARD") => {
                current = Some(Contact::default())
            }
            ("END", _) if value.eq_ignore_ascii_case("VCARD") => {
                out.extend(current.take().filter(|c| !c.name.is_empty()))
            }
            ("FN", Some(c)) => c.name = unescape(value).trim().to_string(),
            ("N", Some(c)) if c.name.is_empty() => {
                let mut parts = value.split(';').map(unescape);
                let (family, given) = (
                    parts.next().unwrap_or_default(),
                    parts.next().unwrap_or_default(),
                );
                c.name = format!("{given} {family}").trim().to_string();
            }
            ("TITLE" | "ROLE", Some(c)) if c.role.is_empty() => {
                c.role = unescape(value).trim().to_string()
            }
            ("ORG", Some(c)) if c.department.is_empty() => {
                c.department = unescape(value.split(';').next().unwrap_or(""))
                    .trim()
                    .to_string()
            }
            ("TEL", Some(c)) if c.phone.is_empty() => c.phone = unescape(value).trim().to_string(),
            ("EMAIL", Some(c)) if c.email.is_empty() => {
                c.email = unescape(value).trim().to_string()
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contacts_survive_a_round_trip_through_vcard() {
        let ana = Contact {
            name: "Ana; Pérez, la jefa".into(),
            role: "Directora".into(),
            department: "Dirección".into(),
            phone: "+54 11 5555-0000".into(),
            email: "ana@correo.com".into(),
            ..Default::default()
        };
        let luis = Contact {
            name: "Luis".into(),
            ..Default::default()
        };
        let text = export(&[ana.clone(), luis.clone()]);
        assert!(
            text.starts_with("BEGIN:VCARD\r\nVERSION:3.0\r\n")
                && text.contains("TEL;TYPE=CELL:+54 11 5555-0000")
        );
        assert_eq!(parse(&text), vec![ana, luis]);
    }

    #[test]
    fn a_vcard_from_another_program_is_read_with_folded_lines_and_a_structured_name() {
        let text = "BEGIN:VCARD\nVERSION:3.0\nN:García;Marta;;;\nORG:Sonido;\nTEL;TYPE=WORK,VOICE:11 4444\nEMAIL:marta@\n correo.com\nEND:VCARD\nBEGIN:VCARD\nEND:VCARD\n";
        let got = parse(text);
        assert_eq!(got.len(), 1, "la tarjeta sin nombre se ignora");
        assert_eq!(
            (
                got[0].name.as_str(),
                got[0].department.as_str(),
                got[0].phone.as_str(),
                got[0].email.as_str()
            ),
            ("Marta García", "Sonido", "11 4444", "marta@correo.com")
        );
    }
}
