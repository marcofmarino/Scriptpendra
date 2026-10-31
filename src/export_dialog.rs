//! El diálogo Exportar (interfaz.md §7, tipo B): a la izquierda los documentos para marcar, a la derecha el nombre de
//! archivo que se va a usar para cada uno, y abajo el botón. Cada documento sale a un PDF en la carpeta elegida, con el
//! nombre `Proyecto – Documento – versión – AAAA-MM-DD.pdf`, pensado para subir a Google Drive.

use eframe::egui::{self, RichText, Ui};

use crate::{
    model::Project,
    reports,
    storage::Paper,
    theme::Palette,
    ui::{kit, tokens},
};

/// Los documentos que se pueden exportar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Doc {
    Script,
    ShotList,
    Storyboard,
    DayPlan,
    Strips,
    Shopping,
    CallSheets,
}

impl Doc {
    pub const ALL: [Doc; 7] = [
        Doc::Script,
        Doc::ShotList,
        Doc::Storyboard,
        Doc::DayPlan,
        Doc::Strips,
        Doc::Shopping,
        Doc::CallSheets,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Doc::Script => "Guion y portada",
            Doc::ShotList => "Lista de planos",
            Doc::Storyboard => "Storyboard",
            Doc::DayPlan => "Plan de rodaje (día por día)",
            Doc::Strips => "Tiras del plan",
            Doc::Shopping => "Compras y alquileres",
            Doc::CallSheets => "Hojas de llamado",
        }
    }

    /// El nombre del documento dentro del nombre de archivo.
    fn name(self) -> &'static str {
        match self {
            Doc::Script => "Guion",
            Doc::ShotList => "Lista de planos",
            Doc::Storyboard => "Storyboard",
            Doc::DayPlan => "Plan de rodaje",
            Doc::Strips => "Tiras",
            Doc::Shopping => "Compras",
            Doc::CallSheets => "Hojas de llamado",
        }
    }
}

pub struct ExportDialog {
    pub open: bool,
    selected: [bool; 7],
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            open: false,
            selected: [true, false, false, false, false, false, false],
        }
    }
}

/// La fecha de hoy como AAAA-MM-DD (a partir del reloj del sistema, en UTC).
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Fecha civil desde días desde 1970 (algoritmo de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// El nombre de archivo propuesto para un documento.
pub fn file_name(project: &Project, doc: Doc, date: &str) -> String {
    let clean = |s: &str| {
        s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-")
            .trim()
            .to_string()
    };
    let title = if project.title.trim().is_empty() {
        "Sin título".to_string()
    } else {
        clean(&project.title)
    };
    let version = clean(&project.version);
    let mut parts = vec![title, doc.name().to_string()];
    if !version.is_empty() {
        parts.push(version);
    }
    parts.push(date.to_string());
    format!("{}.pdf", parts.join(" – "))
}

/// Genera el PDF de un documento.
fn render(project: &Project, doc: Doc, paper: Paper, bold_names: bool) -> Result<Vec<u8>, String> {
    let sheet = paper.sheet();
    let (w, h) = (sheet.width * 72.0, sheet.height * 72.0);
    let title = format!("{} — {}", project.title, doc.name());
    let lines = |pages: Vec<Vec<crate::pdf::Line>>| {
        crate::pdf::render(&pages, w, h, &title, &project.author)
    };
    match doc {
        Doc::Script => crate::pdf::export(project, paper, bold_names, false),
        Doc::ShotList => lines(reports::shot_list_pages(
            project,
            project.shots.iter().any(|s| s.shoot_order.is_some()),
            paper,
        )),
        Doc::Storyboard => {
            let frames = reports::storyboard_frames(project, project.storyboard_only_images);
            crate::pdf::render_storyboard(
                &frames,
                6,
                project.aspect.ratio(),
                w,
                h,
                &title,
                &project.author,
            )
        }
        Doc::DayPlan => lines(reports::day_plan_pages(project, paper)),
        Doc::Strips => lines(reports::strip_pages(project, paper)),
        Doc::Shopping => {
            let rows = project.shopping_list(
                &[true, false, true, true, true]
                    .iter()
                    .enumerate()
                    .filter(|(_, on)| **on)
                    .map(|(n, _)| crate::model::Acquisition::ALL[n])
                    .collect::<Vec<_>>(),
            );
            lines(reports::shopping_pages(
                project,
                rows,
                crate::reports::Group::Category,
                paper,
            ))
        }
        Doc::CallSheets => {
            let mut pages = Vec::new();
            for day in project.days_view() {
                if let Some(sheet) = crate::callsheet::build(project, day.id) {
                    pages.extend(reports::call_sheet_pages(&sheet, paper));
                }
            }
            if pages.is_empty() {
                return Err("Todavía no hay días de rodaje.".to_string());
            }
            lines(pages)
        }
    }
}

/// Dibuja el diálogo. Devuelve un mensaje para la barra de estado cuando se exportó (o falló).
pub fn show(
    ctx: &egui::Context,
    p: &Palette,
    state: &mut ExportDialog,
    project: &Project,
    paper: Paper,
    bold_names: bool,
) -> Option<String> {
    if !state.open {
        return None;
    }
    let date = today();
    let mut status = None;
    let mut export = false;
    let mut cancel = false;
    let any = state.selected.iter().any(|s| *s);
    let close = kit::dialog(
        ctx,
        p,
        "export",
        "Exportar",
        kit::DialogWidth::Normal,
        |ui: &mut Ui| {
            ui.columns(2, |cols| {
                kit::section(&mut cols[0], p, "Documentos", None);
                for (n, doc) in Doc::ALL.iter().enumerate() {
                    cols[0].checkbox(&mut state.selected[n], doc.label());
                }
                kit::section(&mut cols[1], p, "Archivos", None);
                if !any {
                    cols[1].add(
                        egui::Label::new(
                            RichText::new("Marcá al menos un documento.").color(p.dim),
                        )
                        .wrap(),
                    );
                }
                for (n, doc) in Doc::ALL
                    .iter()
                    .enumerate()
                    .filter(|(n, _)| state.selected[*n])
                {
                    let _ = n;
                    let name = file_name(project, *doc, &date);
                    cols[1]
                        .add(
                            egui::Label::new(
                                RichText::new(&name).size(tokens::FONT_SMALL).color(p.dim),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&name);
                }
            });
        },
        |ui| {
            let why = (!any).then_some("Marcá al menos un documento.");
            if kit::primary_button(ui, p, &format!("{}  Exportar", crate::icon::EXPORT), why)
                .clicked()
            {
                export = true;
            }
            if ui.button("Cancelar").clicked() {
                cancel = true;
            }
        },
    );
    if export {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let mut done = 0;
            let mut failed = None;
            for (n, doc) in Doc::ALL
                .iter()
                .enumerate()
                .filter(|(n, _)| state.selected[*n])
            {
                let _ = n;
                let path = dir.join(file_name(project, *doc, &date));
                match render(project, *doc, paper, bold_names)
                    .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
                {
                    Ok(()) => done += 1,
                    Err(e) => failed = Some(format!("{}: {e}", doc.label())),
                }
            }
            status = Some(match failed {
                Some(e) => format!("Error al exportar {e}"),
                None => format!(
                    "Se exportaron {done} {}",
                    if done == 1 { "documento" } else { "documentos" }
                ),
            });
            state.open = false;
        }
    }
    if close || cancel {
        state.open = false;
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proposed_name_follows_project_document_version_date() {
        let mut p = Project::default();
        p.title = "Mi corto".into();
        p.version = "v2".into();
        assert_eq!(
            file_name(&p, Doc::Script, "2026-10-30"),
            "Mi corto – Guion – v2 – 2026-10-30.pdf"
        );
        p.version.clear();
        p.title = "A/B".into();
        assert_eq!(
            file_name(&p, Doc::Storyboard, "2026-10-30"),
            "A-B – Storyboard – 2026-10-30.pdf"
        );
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"), "{d}");
    }

    #[test]
    fn every_document_renders_to_a_pdf() {
        let mut p = Project::default();
        p.paragraphs =
            vec![p.make_paragraph(crate::model::ParagraphKind::SceneHeading, "INT. CASA - DÍA")];
        let scene = p.paragraphs[0].id;
        p.add_shot(scene, None);
        p.add_shoot_day();
        p.place_scenes(&[scene], None);
        for doc in Doc::ALL {
            let bytes =
                render(&p, doc, Paper::A4, false).unwrap_or_else(|e| panic!("{doc:?}: {e}"));
            assert!(bytes.starts_with(b"%PDF"), "{doc:?}");
        }
    }
}
