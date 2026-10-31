//! Páginas del proyecto distintas del editor de guion.

use crate::ui::tokens;
use std::collections::{BTreeMap, HashMap};

use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, Ui, vec2};

use crate::{
    export::{self, Selection},
    icon,
    model::{DocKind, MindNode, Project, StoryRole, TemplateSection},
    profile,
    storage::{RecentCard, Settings},
    theme::{self, Palette, Scheme, bold_family},
    widgets::{Row, accent_button, card, column, field, stepper, tree_row},
};

/// Página con margen y desplazamiento: nada queda pegado a los bordes ni se corta.
fn page(ui: &mut Ui, max_width: f32, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            column(ui, max_width, add);
            ui.add_space(tokens::SP_XL);
        });
}

fn title(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(19.0, bold_family()))
            .color(p.text),
    );
    ui.add_space(tokens::SP_M);
}

pub fn info(ui: &mut Ui, project: &mut Project, p: &Palette) -> bool {
    use crate::ui::{kit, tokens};
    let mut changed = false;
    let scenes = project.scenes();
    let seconds: f32 = scenes.iter().map(|s| s.seconds).sum();
    let eighths: u32 = scenes
        .iter()
        .map(|s| u32::from(project.scene_eighths(s.id)))
        .sum();
    let indicators = [
        ("Páginas", format!("{}", eighths.div_ceil(8))),
        ("Escenas", scenes.len().to_string()),
        ("Duración estimada", fmt_duration(seconds)),
        ("Personajes", project.characters.len().to_string()),
    ];
    egui::ScrollArea::vertical()
        .id_salt("info_page")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            kit::centered_column(ui, tokens::FICHA_MAX, |ui| {
                let gap = tokens::SP_L;
                ui.spacing_mut().item_spacing.y = gap;
                let total = ui.available_width();
                let side = total >= tokens::FICHA_SIDE_BREAK;
                let base = project.base_dir.clone();
                // Una tarjeta ocupa todo el ancho de su columna.
                let full = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui) -> bool| -> bool {
                    let mut c = false;
                    kit::card(ui, p, |ui| {
                        ui.set_width(ui.available_width());
                        c = add(ui);
                    });
                    c
                };
                let mut project_card = |ui: &mut Ui| -> bool {
                    title(ui, p, "Proyecto");
                    let mut c = false;
                    c |= kit::field(ui, p, "Título", &mut project.title, 1);
                    c |= kit::field(ui, p, "Autor", &mut project.author, 1);
                    c |= kit::field(ui, p, "Contacto", &mut project.contact, 1);
                    c |= kit::field(ui, p, "Versión", &mut project.version, 1);
                    c |= kit::field(
                        ui,
                        p,
                        "Descripción corta (logline)",
                        &mut project.logline,
                        3,
                    );
                    c
                };
                if side {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        ui.vertical(|ui| {
                            ui.set_width(total - tokens::COVER_W - gap);
                            changed |= full(ui, &mut project_card);
                        });
                        ui.vertical(|ui| {
                            ui.set_width(tokens::COVER_W);
                            changed |= full(ui, &mut |ui| {
                                title(ui, p, "Portada");
                                let w = ui.available_width();
                                image_picker(ui, p, &mut project.cover, w, POSTER, base.as_deref())
                            });
                        });
                    });
                } else {
                    changed |= full(ui, &mut project_card);
                    changed |= full(ui, &mut |ui| {
                        title(ui, p, "Portada");
                        let w = ui.available_width().min(tokens::COVER_W);
                        image_picker(ui, p, &mut project.cover, w, POSTER, base.as_deref())
                    });
                }
                // Indicadores de solo lectura: todos del mismo ancho, repartiéndose la fila (a dos por fila si es angosta).
                let per_row = if side { 4 } else { 2 };
                for row in indicators.chunks(per_row) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        let w = (total - gap * (per_row - 1) as f32) / per_row as f32;
                        for (label, value) in row {
                            ui.vertical(|ui| {
                                ui.set_width(w);
                                full(ui, &mut |ui| {
                                    ui.label(
                                        RichText::new(value)
                                            .font(FontId::new(tokens::FONT_TITLE, bold_family()))
                                            .color(p.accent),
                                    );
                                    ui.label(
                                        RichText::new(*label).size(tokens::FONT_SMALL).color(p.dim),
                                    );
                                    false
                                });
                            });
                        }
                    });
                }
                ui.add_space(tokens::SP_XL);
            });
        });
    changed
}

/// Proporción de un recuadro de imagen (ancho : alto).
#[derive(Clone, Copy)]
pub struct Ratio(pub u32, pub u32);

impl Ratio {
    pub fn label(self) -> String {
        format!("{}:{}", self.0, self.1)
    }

    /// Alto que le corresponde a un ancho dado.
    pub fn height_for(self, width: f32) -> f32 {
        width * self.1 as f32 / self.0 as f32
    }
}

/// Foto o plano apaisado.
pub const LANDSCAPE: Ratio = Ratio(4, 3);
/// Retrato de personaje.
pub const PORTRAIT: Ratio = Ratio(4, 5);
/// Portada de historia (póster).
pub const POSTER: Ratio = Ratio(2, 3);

/// Dibuja una imagen llenando `rect` y recortando lo que sobre en los bordes, centrada (como «cover» en CSS).
pub fn paint_cover(
    ui: &Ui,
    path: &std::path::Path,
    rect: egui::Rect,
    radius: impl Into<egui::CornerRadius>,
) {
    let image = egui::Image::new(format!("file://{}", path.display()));
    let original = image
        .load_for_size(ui.ctx(), rect.size())
        .ok()
        .and_then(|t| t.size());
    let uv = match original {
        Some(s) if s.x > 0.0 && s.y > 0.0 => {
            let (img, frame) = (s.x / s.y, rect.width() / rect.height());
            if img > frame {
                let w = frame / img;
                egui::Rect::from_min_max(
                    egui::pos2((1.0 - w) / 2.0, 0.0),
                    egui::pos2((1.0 + w) / 2.0, 1.0),
                )
            } else {
                let h = img / frame;
                egui::Rect::from_min_max(
                    egui::pos2(0.0, (1.0 - h) / 2.0),
                    egui::pos2(1.0, (1.0 + h) / 2.0),
                )
            }
        }
        _ => egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
    };
    image
        .uv(uv)
        .fit_to_exact_size(rect.size())
        .corner_radius(radius)
        .paint_at(ui, rect);
}

/// Recuadro de imagen con proporción fija que se puede pulsar para cargarla, con un botón «Cargar imagen» del
/// mismo ancho debajo (y «Quitar imagen» cuando ya hay una). Devuelve si cambió.
pub(crate) fn image_picker(
    ui: &mut Ui,
    p: &Palette,
    path: &mut Option<std::path::PathBuf>,
    width: f32,
    ratio: Ratio,
    base: Option<&std::path::Path>,
) -> bool {
    let size = vec2(width, ratio.height_for(width));
    let mut pick = false;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect(
        rect,
        4,
        p.field,
        Stroke::new(1.0, if resp.hovered() { p.accent } else { p.border }),
        egui::StrokeKind::Inside,
    );
    // Las imágenes del proyecto se guardan con ruta relativa a su carpeta.
    let shown = path.as_deref().map(|img| match base {
        Some(b) if img.is_relative() => b.join(img),
        _ => img.to_path_buf(),
    });
    match shown.as_deref() {
        Some(img) => paint_cover(ui, img, rect, 4),
        None => {
            ui.painter().text(
                egui::pos2(rect.center().x, rect.center().y - 12.0),
                Align2::CENTER_CENTER,
                icon::IMAGE,
                FontId::proportional(36.0),
                p.dim,
            );
            ui.painter().text(
                egui::pos2(rect.center().x, rect.center().y + 24.0),
                Align2::CENTER_CENTER,
                format!("Proporción {}", ratio.label()),
                FontId::proportional(13.0),
                p.dim,
            );
        }
    }
    // Con imagen, una etiqueta pequeña en la esquina recuerda la proporción.
    if path.is_some() {
        let tag =
            egui::Rect::from_min_size(rect.left_bottom() + vec2(6.0, -24.0), vec2(38.0, 18.0));
        ui.painter().rect_filled(tag, 4, p.shadow);
        ui.painter().text(
            tag.center(),
            Align2::CENTER_CENTER,
            ratio.label(),
            FontId::proportional(12.0),
            p.text,
        );
    }
    pick |= resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "Proporción {}. La imagen se recorta al centro para llenar el recuadro.",
            ratio.label()
        ))
        .clicked();
    ui.add_space(tokens::SP_S);
    pick |= crate::widgets::wide_button(ui, p, icon::IMAGE, "Cargar imagen", size.x).clicked();
    let mut changed = false;
    if path.is_some() {
        ui.add_space(tokens::SP_XS);
        if crate::widgets::wide_button(ui, p, icon::TRASH, "Quitar imagen", size.x).clicked() {
            *path = None;
            changed = true;
        }
    }
    if pick {
        if let Some(file) = rfd::FileDialog::new()
            .add_filter("Imágenes", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        {
            *path = Some(file);
            changed = true;
        }
    }
    changed
}

/// Estado de la vista previa de la página de título.
#[derive(Default)]
pub struct TitleState {
    /// Cambio pedido al zoom de la vista, en pasos; lo aplica la app.
    pub zoom_delta: i32,
}

/// Píxeles por pulgada de la hoja al 100 % de zoom (igual que el tratamiento y la sinopsis).
const INCH: f32 = 120.0;

/// La portada como se va a imprimir: una hoja del color del tema (como Sinopsis, Tratamiento y Guion) con las proporciones del papel elegido, centrada y pegada arriba, sin
/// edición directa (interfaz.md §3.2).
pub fn title_page(
    ui: &mut Ui,
    project: &Project,
    p: &Palette,
    paper: crate::storage::Paper,
    zoom: u32,
) {
    let sheet = paper.sheet();
    egui::ScrollArea::vertical()
        .id_salt("title_page_preview")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let size = vec2(sheet.width * INCH, sheet.height * INCH);
            crate::ui::kit::sheet(
                ui,
                p,
                crate::ui::kit::SheetTone::Theme,
                size,
                zoom as f32 / 100.0,
                |ui| {
                    let rect = ui.max_rect();
                    let (width, height) = (rect.width(), rect.height());
                    let paint = ui.painter().clone();
                    // Todo se dibuja en proporción al ancho de la hoja: con 12 pt a 8,5 in de ancho, el texto escala con la vista previa.
                    let scale = width / (sheet.width * 72.0);
                    let ink = p.text;
                    let mono = |size: f32| FontId::new(size * scale, egui::FontFamily::Monospace);
                    let center_x = rect.center().x;
                    let mut y = rect.top() + height * 0.33;
                    let title = if project.title.trim().is_empty() {
                        "SIN TÍTULO".to_string()
                    } else {
                        project.title.to_uppercase()
                    };
                    let galley = paint.layout(title, mono(14.0), ink, width * 0.8);
                    let h = galley.size().y;
                    paint.galley(egui::pos2(center_x - galley.size().x / 2.0, y), galley, ink);
                    y += h + 24.0 * scale;
                    if !project.author.trim().is_empty() {
                        paint.text(
                            egui::pos2(center_x, y),
                            egui::Align2::CENTER_TOP,
                            "Guion de",
                            mono(12.0),
                            ink,
                        );
                        y += 18.0 * scale;
                        paint.text(
                            egui::pos2(center_x, y),
                            egui::Align2::CENTER_TOP,
                            project.author.trim(),
                            mono(12.0),
                            ink,
                        );
                    }
                    if !project.version.trim().is_empty() {
                        paint.text(
                            egui::pos2(rect.left() + width * 0.1, rect.bottom() - height * 0.12),
                            egui::Align2::LEFT_BOTTOM,
                            project.version.trim(),
                            mono(10.0),
                            ink,
                        );
                    }
                    if !project.contact.trim().is_empty() {
                        let galley = paint.layout(
                            project.contact.trim().to_string(),
                            mono(10.0),
                            ink,
                            width * 0.4,
                        );
                        let pos = egui::pos2(
                            rect.right() - width * 0.1 - galley.size().x,
                            rect.bottom() - height * 0.08 - galley.size().y,
                        );
                        paint.galley(pos, galley, ink);
                    }
                },
            );
            ui.add_space(tokens::SP_XL);
        });
}

/// El inspector de la página de título: pestaña DATOS con lo que se imprime en la portada.
pub fn title_inspector(ui: &mut Ui, project: &mut Project, p: &Palette) -> bool {
    let mut changed = false;
    (|ui: &mut Ui| {
        changed |= field(ui, p, "Título", &mut project.title, 1);
        changed |= field(ui, p, "Autor", &mut project.author, 1);
        changed |= field(ui, p, "Contacto", &mut project.contact, 3);
        changed |= field(ui, p, "Versión", &mut project.version, 1);
    })(ui);
    changed
}

/// Secciones plegables de una ficha. Devuelve si cambió algún campo.
fn profile_sections(
    ui: &mut Ui,
    p: &Palette,
    sections: &[TemplateSection],
    fields: &mut BTreeMap<String, String>,
    salt: &str,
) -> bool {
    let mut changed = false;
    for (n, section) in sections.iter().enumerate() {
        let done = section
            .fields
            .iter()
            .filter(|f| fields.get(&f.key).is_some_and(|v| !v.trim().is_empty()))
            .count();
        let header = RichText::new(format!(
            "{}  ({}/{})",
            section.title,
            done,
            section.fields.len()
        ))
        .font(FontId::new(15.0, bold_family()))
        .color(if done > 0 { p.text } else { p.dim });
        egui::CollapsingHeader::new(header)
            .id_salt((salt, n))
            .default_open(n == 0)
            .show(ui, |ui| {
                ui.add_space(tokens::SP_S);
                for f in &section.fields {
                    let mut value = fields.get(&f.key).cloned().unwrap_or_default();
                    if field(ui, p, &f.label, &mut value, f.rows) {
                        if value.is_empty() {
                            fields.remove(&f.key);
                        } else {
                            fields.insert(f.key.clone(), value);
                        }
                        changed = true;
                    }
                }
            });
        ui.add_space(tokens::SP_XS);
    }
    changed
}

/// Resultado de la ficha de un personaje.
#[derive(Default)]
pub struct CharacterOutcome {
    pub changed: bool,
    pub reset: bool,
    pub delete: bool,
}

/// Ficha del personaje `id`. El nombre se edita directamente (siempre en mayúsculas) y el guion lo sigue.
pub fn character(ui: &mut Ui, project: &mut Project, id: u64, p: &Palette) -> CharacterOutcome {
    let mut out = CharacterOutcome::default();
    let activity = project
        .character_activity()
        .into_iter()
        .find(|c| c.id == Some(id));
    let base = project.base_dir.clone();
    let people: Vec<(u64, String)> = project
        .contacts
        .iter()
        .map(|c| (c.id, c.name.clone()))
        .collect();
    let Some(c) = project.characters.iter_mut().find(|c| c.id == id) else {
        page(ui, 640.0, |ui| {
            ui.label(RichText::new("Este personaje ya no existe.").color(p.dim));
        });
        return out;
    };
    // Las notas de las fichas viejas pasan al campo «Notas» de la ficha completa.
    if !c.notes.is_empty() && !c.fields.contains_key("notes") {
        c.fields
            .insert("notes".into(), std::mem::take(&mut c.notes));
        out.changed = true;
    }
    let color_key = id.to_string();
    page(ui, 760.0, |ui| {
        // Las acciones destructivas van arriba: abajo se desplazarían cada vez que se agrega algo.
        crate::ui::kit::sides(
            ui,
            |_ui| {},
            |ui| {
                if accent_button(ui, p, icon::TRASH, "Eliminar personaje").clicked() {
                    out.delete = true;
                }
                ui.add_space(tokens::SP_L);
                if accent_button(ui, p, icon::ARROW_COUNTER_CLOCKWISE, "Resetear ficha").clicked() {
                    out.reset = true;
                }
            },
        );
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            // Los campos al lado de la foto empiezan más a la derecha que los de abajo: son otro grupo.
            crate::ui::checks::push_group();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    out.changed |=
                        image_picker(ui, p, &mut c.photo, 168.0, PORTRAIT, base.as_deref());
                });
                ui.add_space(tokens::SP_L);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(icon::USER)
                                .size(22.0)
                                .color(theme::character_color(&color_key)),
                        );
                        ui.label(RichText::new(&c.name).font(FontId::new(19.0, bold_family())));
                    });
                    if let Some(a) = &activity {
                        ui.label(
                            RichText::new(format!(
                                "{} intervenciones · {} palabras · {} escenas",
                                a.dialogues, a.words, a.scenes
                            ))
                            .color(p.dim),
                        );
                    }
                    ui.add_space(tokens::SP_S);
                    if field(
                        ui,
                        p,
                        "Nombre (se cambia también en todo el guion)",
                        &mut c.name,
                        1,
                    ) {
                        c.name = c.name.to_uppercase();
                        out.changed = true;
                    }
                    ui.label(RichText::new("Rol en la historia").size(12.0).color(p.dim));
                    egui::ComboBox::from_id_salt("role")
                        .selected_text(c.role.label())
                        .show_ui(ui, |ui| {
                            for r in StoryRole::ALL {
                                out.changed |=
                                    ui.selectable_value(&mut c.role, r, r.label()).changed();
                            }
                        });
                });
            });
            crate::ui::checks::pop_group();
            ui.add_space(tokens::SP_M);
            crate::ui::kit::field_group(ui, 2, |ui, n| {
                out.changed |= if n == 0 {
                    field(ui, p, "Edad", &mut c.age, 1)
                } else {
                    field(ui, p, "Género", &mut c.gender, 1)
                };
            });
            out.changed |= field(ui, p, "Descripción en una frase", &mut c.one_liner, 2);
            ui.label(RichText::new("ELENCO").size(11.0).color(p.dim));
            ui.add_space(tokens::SP_XS);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Nº").size(12.0).color(p.dim));
                let mut number = c.cast_number.unwrap_or(0);
                if ui
                    .add(egui::DragValue::new(&mut number).range(0..=999))
                    .changed()
                {
                    c.cast_number = (number > 0).then_some(number);
                    out.changed = true;
                }
                ui.add_space(tokens::SP_M);
                ui.label(RichText::new("Lo interpreta").size(12.0).color(p.dim));
                let shown = c
                    .actor
                    .and_then(|a| people.iter().find(|(i, _)| *i == a))
                    .map_or("Sin asignar".to_string(), |(_, n)| n.clone());
                egui::ComboBox::from_id_salt("character_actor")
                    .width(crate::ui::kit::fill_width(ui, 80.0, 220.0))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(c.actor.is_none(), "Sin asignar")
                            .clicked()
                        {
                            c.actor = None;
                            out.changed = true;
                        }
                        for (pid, name) in &people {
                            if ui.selectable_label(c.actor == Some(*pid), name).clicked() {
                                c.actor = Some(*pid);
                                out.changed = true;
                            }
                        }
                    });
            });
            if people.is_empty() {
                ui.label(
                    RichText::new(
                        "Cargá a la persona en Producción → Contactos para elegirla acá.",
                    )
                    .size(12.0)
                    .color(p.dim),
                );
            }
        });
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            title(ui, p, "Ficha completa");
            out.changed |= profile_sections(
                ui,
                p,
                &profile::owned(profile::CHARACTER),
                &mut c.fields,
                "character",
            );
        });
    });
    out
}

/// Panel derecho de la ficha: botón de exportar y qué partes incluir. Devuelve si se pidió exportar.
pub fn character_export_panel(
    ui: &mut Ui,
    project: &Project,
    id: u64,
    sel: &mut Selection,
    p: &Palette,
) -> bool {
    let mut export = false;
    let Some(c) = project.characters.iter().find(|c| c.id == id) else {
        return false;
    };
    let key_owner = id.to_string();
    let name = key_owner.as_str();
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("EXPORTAR").size(11.0).color(p.dim));
            ui.add_space(tokens::SP_S);
            export = crate::widgets::wide_button(
                ui,
                p,
                icon::EXPORT,
                "Exportar ficha (SVG)",
                ui.available_width(),
            )
            .clicked();
            ui.add_space(tokens::SP_S);
        });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    crate::ui::kit::sides(
                        ui,
                        |ui| {
                            ui.label(RichText::new("QUÉ INCLUIR").size(11.0).color(p.dim));
                        },
                        |ui| {
                            if ui.small_button("Nada").clicked() {
                                set_all(sel, name, false);
                            }
                            if ui.small_button("Todo").clicked() {
                                set_all(sel, name, true);
                            }
                        },
                    );
                    ui.add_space(tokens::SP_S);
                    let mut on = sel.includes(name, export::KEY_PHOTO);
                    if ui.checkbox(&mut on, "Foto").changed() {
                        sel.set(name, export::KEY_PHOTO, on);
                    }
                    let mut on = sel.includes(name, export::KEY_BASIC);
                    if ui
                        .checkbox(&mut on, "Rol, edad, género y descripción")
                        .changed()
                    {
                        sel.set(name, export::KEY_BASIC, on);
                    }
                    ui.add_space(tokens::SP_S);
                    for (n, section) in profile::CHARACTER.iter().enumerate() {
                        let key = export::section_key(n);
                        let section_on = sel.includes(name, &key);
                        let filled = |f: &profile::Field| {
                            c.fields.get(f.key).is_some_and(|v| !v.trim().is_empty())
                        };
                        let done = section.fields.iter().filter(|f| filled(f)).count();
                        let id = ui.make_persistent_id(("export_section", n));
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            id,
                            false,
                        )
                        .show_header(ui, |ui| {
                            let mut on = section_on;
                            if ui
                                .checkbox(
                                    &mut on,
                                    format!(
                                        "{} ({}/{})",
                                        section.title,
                                        done,
                                        section.fields.len()
                                    ),
                                )
                                .changed()
                            {
                                sel.set(name, &key, on);
                            }
                        })
                        .body(|ui| {
                            ui.add_enabled_ui(section_on, |ui| {
                                for f in section.fields {
                                    let fk = export::field_key(f.key);
                                    let mut on = sel.includes(name, &fk) && filled(f);
                                    let resp = ui.add_enabled(
                                        filled(f),
                                        egui::Checkbox::new(&mut on, f.label),
                                    );
                                    if resp.changed() {
                                        sel.set(name, &fk, on);
                                    }
                                }
                            });
                        });
                    }
                });
        });
    export
}

fn set_all(sel: &mut Selection, name: &str, include: bool) {
    sel.set(name, export::KEY_PHOTO, include);
    sel.set(name, export::KEY_BASIC, include);
    for (n, section) in profile::CHARACTER.iter().enumerate() {
        sel.set(name, &export::section_key(n), include);
        for f in section.fields {
            sel.set(name, &export::field_key(f.key), include);
        }
    }
}

/// Selector de icono para una categoría de elementos. Devuelve si cambió.
pub fn icon_picker(ui: &mut Ui, p: &Palette, current: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for glyph in icon::PICKABLE {
            let selected = current == glyph;
            let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::click());
            let fill = if selected {
                p.selection
            } else if resp.hovered() {
                p.container_hi
            } else {
                p.field
            };
            ui.painter().rect(
                rect,
                6,
                fill,
                Stroke::new(1.0, if selected { p.accent } else { p.border }),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                *glyph,
                FontId::proportional(21.0),
                if selected { p.accent } else { p.text },
            );
            if resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *current = glyph.to_string();
                changed = true;
            }
        }
    });
    changed
}

/// Resultado de la página de una categoría de elementos.
#[derive(Default)]
pub struct ElementCategoryOutcome {
    pub changed: bool,
    pub delete: bool,
    pub add: bool,
    pub open: Option<u64>,
    /// Quitar parte de la ficha que ya tiene contenido: (sección, campo, elementos afectados, nombre).
    pub confirm_part: Option<(usize, Option<usize>, usize, String)>,
}

/// Página de una categoría de elementos: nombre, icono, ficha editable y la lista de sus elementos.
pub fn element_category(
    ui: &mut Ui,
    project: &mut Project,
    id: u64,
    editing: &mut Option<u64>,
    p: &Palette,
) -> ElementCategoryOutcome {
    let mut out = ElementCategoryOutcome::default();
    let members: Vec<(u64, String)> = project
        .elements
        .iter()
        .filter(|e| e.category == id)
        .map(|e| (e.id, e.name.clone()))
        .collect();
    if !project.element_categories.iter().any(|c| c.id == id) {
        page(ui, 640.0, |ui| {
            ui.label(RichText::new("Esta categoría ya no existe.").color(p.dim));
        });
        return out;
    }
    page(ui, 760.0, |ui| {
        crate::ui::kit::sides(
            ui,
            |_ui| {},
            |ui| {
                if accent_button(ui, p, icon::TRASH, "Eliminar categoría").clicked() {
                    out.delete = true;
                }
            },
        );
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            if let Some(cat) = project.element_categories.iter_mut().find(|c| c.id == id) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&cat.icon).size(24.0).color(p.accent));
                    ui.label(RichText::new(&cat.name).font(FontId::new(19.0, bold_family())));
                });
                ui.add_space(tokens::SP_M);
                out.changed |= field(ui, p, "Nombre de la categoría", &mut cat.name, 1);
                ui.label(RichText::new("Icono").size(12.0).color(p.dim));
                ui.add_space(tokens::SP_XS);
                out.changed |= icon_picker(ui, p, &mut cat.icon);
            }
        });
        ui.add_space(tokens::SP_M);
        template_card(ui, project, id, editing, p, &mut out);
        ui.add_space(tokens::SP_M);
        let glyph = project
            .element_categories
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.icon.clone())
            .unwrap_or_default();
        card(ui, p, |ui| {
            crate::ui::kit::sides(
                ui,
                |ui| {
                    ui.label(RichText::new("ELEMENTOS").size(11.0).color(p.dim));
                },
                |ui| {
                    if accent_button(ui, p, icon::PLUS, "Añadir elemento").clicked() {
                        out.add = true;
                    }
                },
            );
            ui.add_space(tokens::SP_S);
            if members.is_empty() {
                ui.label(RichText::new("Todavía no hay elementos en esta categoría.").color(p.dim));
            }
            for (eid, name) in &members {
                let row = Row {
                    depth: 0,
                    caret: None,
                    icon: Some(glyph.as_str()),
                    label: name,
                    selected: false,
                };
                if tree_row(ui, p, row).clicked() {
                    out.open = Some(*eid);
                }
            }
        });
    });
    out
}

/// Un cambio pedido sobre la plantilla mientras se dibuja; se aplica al terminar.
enum TemplateAction {
    MoveSection(usize, isize),
    MoveField(usize, usize, isize),
    DeleteSection(usize),
    DeleteField(usize, usize),
    AddField(usize),
    AddSection,
}

fn move_item<T>(items: &mut [T], index: usize, delta: isize) {
    let target = index as isize + delta;
    if target >= 0 && (target as usize) < items.len() {
        items.swap(index, target as usize);
    }
}

/// La ficha que verán los elementos de la categoría, con el modo para editarla.
fn template_card(
    ui: &mut Ui,
    project: &mut Project,
    id: u64,
    editing: &mut Option<u64>,
    p: &Palette,
    out: &mut ElementCategoryOutcome,
) {
    let is_editing = *editing == Some(id);
    let customised = project
        .element_categories
        .iter()
        .find(|c| c.id == id)
        .is_some_and(|c| c.template.is_some());
    let mut action: Option<TemplateAction> = None;
    let mut reset = false;
    card(ui, p, |ui| {
        crate::ui::kit::sides(
            ui,
            |ui| {
                ui.label(
                    RichText::new("FICHA DE LOS ELEMENTOS")
                        .size(11.0)
                        .color(p.dim),
                );
            },
            |ui| {
                if is_editing {
                    if accent_button(ui, p, icon::CHECK, "Listo").clicked() {
                        *editing = None;
                    }
                    if customised {
                        ui.add_space(tokens::SP_M);
                        if accent_button(
                            ui,
                            p,
                            icon::ARROW_COUNTER_CLOCKWISE,
                            "Restaurar por defecto",
                        )
                        .clicked()
                        {
                            reset = true;
                        }
                    }
                } else if accent_button(ui, p, icon::NOTE_PENCIL, "Editar ficha").clicked() {
                    *editing = Some(id);
                }
            },
        );
        ui.add_space(tokens::SP_S);

        if !is_editing {
            ui.add_space(tokens::SP_S);
            let sections = project
                .element_categories
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.sections())
                .unwrap_or_default();
            for section in &sections {
                ui.label(RichText::new(&section.title).font(FontId::new(14.0, bold_family())));
                let labels: Vec<&str> = section.fields.iter().map(|f| f.label.as_str()).collect();
                ui.label(
                    RichText::new(if labels.is_empty() {
                        "(sin campos)".to_string()
                    } else {
                        labels.join("  ·  ")
                    })
                    .size(12.0)
                    .color(p.dim),
                );
                ui.add_space(tokens::SP_S);
            }
            return;
        }

        let Some(sections) = project.template_mut(id) else {
            return;
        };
        let last = sections.len().saturating_sub(1);
        for (si, section) in sections.iter_mut().enumerate() {
            egui::Frame::new()
                .fill(p.field)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(6)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if crate::widgets::icon_button(
                            ui,
                            p,
                            icon::UP_ARROW,
                            "Subir sección",
                            si > 0,
                        )
                        .clicked()
                        {
                            action = Some(TemplateAction::MoveSection(si, -1));
                        }
                        if crate::widgets::icon_button(
                            ui,
                            p,
                            icon::DOWN_ARROW,
                            "Bajar sección",
                            si < last,
                        )
                        .clicked()
                        {
                            action = Some(TemplateAction::MoveSection(si, 1));
                        }
                        out.changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut section.title)
                                    .font(FontId::new(15.0, bold_family()))
                                    .desired_width(crate::ui::kit::beside(ui, 1, 0.0, 60.0)),
                            )
                            .changed();
                        if crate::widgets::icon_button(
                            ui,
                            p,
                            icon::TRASH,
                            "Quitar la sección",
                            true,
                        )
                        .clicked()
                        {
                            action = Some(TemplateAction::DeleteSection(si));
                        }
                    });
                    ui.add_space(tokens::SP_XS);
                    let last_field = section.fields.len().saturating_sub(1);
                    for (fi, f) in section.fields.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.add_space(tokens::SP_S);
                            if crate::widgets::icon_button(
                                ui,
                                p,
                                icon::UP_ARROW,
                                "Subir campo",
                                fi > 0,
                            )
                            .clicked()
                            {
                                action = Some(TemplateAction::MoveField(si, fi, -1));
                            }
                            if crate::widgets::icon_button(
                                ui,
                                p,
                                icon::DOWN_ARROW,
                                "Bajar campo",
                                fi < last_field,
                            )
                            .clicked()
                            {
                                action = Some(TemplateAction::MoveField(si, fi, 1));
                            }
                            out.changed |=
                                ui.add(egui::TextEdit::singleline(&mut f.label).desired_width(
                                    crate::ui::kit::beside(ui, 1, 90.0 + 8.0, 80.0),
                                ))
                                .changed();
                            egui::ComboBox::from_id_salt(("field_rows", id, si, fi))
                                .width(crate::ui::kit::fill_width(ui, 80.0, 90.0))
                                .selected_text(rows_label(f.rows))
                                .show_ui(ui, |ui| {
                                    for rows in [1, 2, 4, 8] {
                                        out.changed |= ui
                                            .selectable_value(&mut f.rows, rows, rows_label(rows))
                                            .changed();
                                    }
                                });
                            if crate::widgets::icon_button(
                                ui,
                                p,
                                icon::TRASH,
                                "Quitar el campo",
                                true,
                            )
                            .clicked()
                            {
                                action = Some(TemplateAction::DeleteField(si, fi));
                            }
                        });
                    }
                    ui.add_space(tokens::SP_XS);
                    if accent_button(ui, p, icon::PLUS, "Campo").clicked() {
                        action = Some(TemplateAction::AddField(si));
                    }
                });
            ui.add_space(tokens::SP_S);
        }
        if accent_button(ui, p, icon::PLUS, "Sección").clicked() {
            action = Some(TemplateAction::AddSection);
        }
    });

    if reset {
        project.reset_template(id);
        out.changed = true;
    }
    let Some(action) = action else { return };
    out.changed = true;
    match action {
        TemplateAction::MoveSection(si, d) => {
            if let Some(t) = project.template_mut(id) {
                move_item(t, si, d);
            }
        }
        TemplateAction::MoveField(si, fi, d) => {
            if let Some(sec) = project.template_mut(id).and_then(|t| t.get_mut(si)) {
                move_item(&mut sec.fields, fi, d);
            }
        }
        TemplateAction::AddSection => project.add_template_section(id, "Nueva sección"),
        TemplateAction::AddField(si) => project.add_template_field(id, si, "Nuevo campo"),
        TemplateAction::DeleteSection(si) | TemplateAction::DeleteField(si, _) => {
            let field = match action {
                TemplateAction::DeleteField(_, fi) => Some(fi),
                _ => None,
            };
            let count = project.template_usage(id, si, field);
            if count == 0 {
                project.remove_template_part(id, si, field);
            } else {
                let sections = project
                    .element_categories
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.sections())
                    .unwrap_or_default();
                let label = match field {
                    Some(fi) => sections
                        .get(si)
                        .and_then(|s| s.fields.get(fi))
                        .map(|f| f.label.clone()),
                    None => sections.get(si).map(|s| s.title.clone()),
                }
                .unwrap_or_default();
                out.confirm_part = Some((si, field, count, label));
            }
        }
    }
}

fn rows_label(rows: usize) -> &'static str {
    match rows {
        0..=1 => "1 línea",
        2..=3 => "2 líneas",
        4..=7 => "4 líneas",
        _ => "8 líneas",
    }
}

/// Ficha de un elemento de una categoría propia. El nombre se edita directamente.
pub fn element(ui: &mut Ui, project: &mut Project, id: u64, p: &Palette) -> EntityOutcome {
    let mut out = EntityOutcome::default();
    let Some(idx) = project.elements.iter().position(|e| e.id == id) else {
        page(ui, 640.0, |ui| {
            ui.label(RichText::new("Este elemento ya no existe.").color(p.dim));
        });
        return out;
    };
    let base = project.base_dir.clone();
    let category = project
        .element_categories
        .iter()
        .find(|c| c.id == project.elements[idx].category)
        .cloned();
    let glyph = category
        .as_ref()
        .map_or(icon::CUBE.to_string(), |c| c.icon.clone());
    let category_name = category
        .as_ref()
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let sections = category.as_ref().map(|c| c.sections()).unwrap_or_default();
    let e = &mut project.elements[idx];
    page(ui, 760.0, |ui| {
        crate::ui::kit::sides(
            ui,
            |_ui| {},
            |ui| {
                if accent_button(ui, p, icon::TRASH, "Eliminar elemento").clicked() {
                    out.delete = true;
                }
            },
        );
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            // El nombre al lado de la foto empieza más a la derecha que los campos de abajo: son otro grupo.
            crate::ui::checks::push_group();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    out.changed |=
                        image_picker(ui, p, &mut e.photo, 168.0, PORTRAIT, base.as_deref());
                });
                ui.add_space(tokens::SP_L);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&glyph).size(22.0).color(p.accent));
                        ui.label(RichText::new(&category_name).color(p.dim));
                    });
                    ui.add_space(tokens::SP_S);
                    out.changed |= field(ui, p, "Nombre", &mut e.name, 1);
                });
            });
            crate::ui::checks::pop_group();
            ui.add_space(tokens::SP_M);
            out.changed |= field(ui, p, "Descripción en una frase", &mut e.one_liner, 2);
        });
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            title(ui, p, "Ficha completa");
            out.changed |= profile_sections(ui, p, &sections, &mut e.fields, "element");
        });
    });
    out
}

/// Resultado de la página de un objeto o de una locación.
#[derive(Default)]
pub struct EntityOutcome {
    pub changed: bool,
    pub delete: bool,
}

pub fn object(ui: &mut Ui, project: &mut Project, id: u64, p: &Palette) -> EntityOutcome {
    let mut out = EntityOutcome::default();
    let Some(o) = project.objects.iter_mut().find(|o| o.id == id) else {
        page(ui, 640.0, |ui| {
            ui.label(RichText::new("Este objeto clave ya no existe.").color(p.dim));
        });
        return out;
    };
    page(ui, 760.0, |ui| {
        crate::ui::kit::sides(
            ui,
            |_ui| {},
            |ui| {
                if accent_button(ui, p, icon::TRASH, "Eliminar objeto clave").clicked() {
                    out.delete = true;
                }
            },
        );
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::CUBE).size(22.0).color(p.accent));
                ui.label(RichText::new(&o.name).font(FontId::new(19.0, bold_family())));
            });
            ui.add_space(tokens::SP_M);
            out.changed |= field(
                ui,
                p,
                "Nombre (se cambia también en todo el guion)",
                &mut o.name,
                1,
            );
            ui.add_space(tokens::SP_M);
            out.changed |= field(ui, p, "Descripción en una frase", &mut o.one_liner, 2);
        });
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            title(ui, p, "Ficha completa");
            out.changed |= profile_sections(
                ui,
                p,
                &profile::owned(profile::OBJECT),
                &mut o.fields,
                "object",
            );
        });
    });
    out
}

pub fn location(ui: &mut Ui, project: &mut Project, id: u64, p: &Palette) -> EntityOutcome {
    let mut out = EntityOutcome::default();
    let scenes = project.scenes_at(id);
    let places: Vec<(u64, String)> = project
        .shoot_locations
        .iter()
        .map(|l| (l.id, l.name.clone()))
        .collect();
    let Some(profile) = project.location_mut(id) else {
        page(ui, 640.0, |ui| {
            ui.label(RichText::new("Esta locación ya no existe.").color(p.dim));
        });
        return out;
    };
    page(ui, 760.0, |ui| {
        crate::ui::kit::sides(
            ui,
            |_ui| {},
            |ui| {
                if accent_button(ui, p, icon::TRASH, "Eliminar locación").clicked() {
                    out.delete = true;
                }
            },
        );
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::HOUSE).size(22.0).color(p.accent));
                ui.label(RichText::new(&profile.name).font(FontId::new(19.0, bold_family())));
            });
            ui.add_space(tokens::SP_M);
            if field(
                ui,
                p,
                "Nombre (se cambia también en todos los encabezados)",
                &mut profile.name,
                1,
            ) {
                profile.name = profile.name.to_uppercase();
                out.changed = true;
            }
            out.changed |= field(ui, p, "Descripción en una frase", &mut profile.one_liner, 2);
            ui.label(RichText::new("DÓNDE SE FILMA").size(11.0).color(p.dim));
            ui.add_space(tokens::SP_XS);
            let shown = profile
                .shoot_location
                .and_then(|s| places.iter().find(|(i, _)| *i == s))
                .map_or("Sin lugar de rodaje".to_string(), |(_, n)| n.clone());
            egui::ComboBox::from_id_salt("location_shoot")
                .width(crate::ui::kit::fill_width(ui, 80.0, 300.0))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(profile.shoot_location.is_none(), "Sin lugar de rodaje")
                        .clicked()
                    {
                        profile.shoot_location = None;
                        out.changed = true;
                    }
                    for (sid, name) in &places {
                        if ui
                            .selectable_label(profile.shoot_location == Some(*sid), name)
                            .clicked()
                        {
                            profile.shoot_location = Some(*sid);
                            out.changed = true;
                        }
                    }
                });
            if places.is_empty() {
                ui.label(
                    RichText::new("Creá el lugar real en Producción → Locaciones de rodaje.")
                        .size(12.0)
                        .color(p.dim),
                );
            }
            ui.add_space(tokens::SP_M);
            ui.label(
                RichText::new("ESCENAS EN ESTA LOCACIÓN")
                    .size(11.0)
                    .color(p.dim),
            );
            ui.add_space(tokens::SP_XS);
            if scenes.is_empty() {
                ui.label(
                    RichText::new("Todavía no aparece en ningún encabezado de escena.")
                        .color(p.dim),
                );
            }
            for (n, heading) in scenes {
                ui.label(format!("{n}. {heading}"));
            }
        });
        ui.add_space(tokens::SP_M);
        card(ui, p, |ui| {
            title(ui, p, "Ficha completa");
            out.changed |= profile_sections(
                ui,
                p,
                &profile::owned(profile::LOCATION),
                &mut profile.fields,
                "location",
            );
        });
    });
    out
}

/// Resultado de la página de un documento.
#[derive(Default)]
pub struct DocOutcome {
    pub changed: bool,
    pub delete: bool,
    /// Documento que se pidió abrir (desde una carpeta).
    pub open: Option<u64>,
}

pub fn document_icon(kind: DocKind, open: bool) -> &'static str {
    match kind {
        DocKind::Folder if open => icon::FOLDER_OPEN,
        DocKind::Folder => icon::FOLDER,
        DocKind::Text => icon::ARTICLE,
        DocKind::World => icon::GLOBE,
        DocKind::MindMap => icon::SHARE_NETWORK,
        DocKind::Gallery => icon::IMAGES,
        DocKind::Cards => icon::CARDS,
        DocKind::Relations => icon::FLOW_ARROW,
    }
}

pub fn document(
    ui: &mut Ui,
    project: &mut Project,
    id: u64,
    mind_sel: &mut Option<u64>,
    story: &mut crate::story::StoryState,
    p: &Palette,
) -> DocOutcome {
    let mut out = DocOutcome::default();
    let Some(idx) = project.documents.iter().position(|d| d.id == id) else {
        page(ui, 640.0, |ui| {
            ui.label(RichText::new("Este documento ya no existe.").color(p.dim));
        });
        return out;
    };
    let kind = project.documents[idx].kind;

    if kind == DocKind::MindMap {
        mind_map(ui, project, idx, mind_sel, &mut out, p);
        return out;
    }
    if matches!(kind, DocKind::Cards | DocKind::Relations) {
        let done = if kind == DocKind::Cards {
            crate::story::cards(ui, project, idx, story, p)
        } else {
            crate::story::relations(ui, project, idx, story, p)
        };
        out.changed |= done.changed;
        out.delete |= done.delete;
        return out;
    }

    let children: Vec<(u64, DocKind, String)> = project
        .documents
        .iter()
        .filter(|d| d.parent == Some(id))
        .map(|d| (d.id, d.kind, d.name.clone()))
        .collect();
    let base = project.base_dir.clone();
    let doc = &mut project.documents[idx];
    page(
        ui,
        if kind == DocKind::World {
            760.0
        } else if kind == DocKind::Text {
            780.0
        } else {
            700.0
        },
        |ui| {
            card(ui, p, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(document_icon(kind, true))
                            .size(22.0)
                            .color(p.accent),
                    );
                    ui.label(
                        RichText::new(kind.label())
                            .font(FontId::new(15.0, bold_family()))
                            .color(p.dim),
                    );
                });
                ui.add_space(tokens::SP_M);
                out.changed |= field(ui, p, "Nombre", &mut doc.name, 1);
                match kind {
                    DocKind::Text => out.changed |= field(ui, p, "Texto", &mut doc.text, 24),
                    DocKind::World => {
                        out.changed |=
                            field(ui, p, "Descripción en una frase", &mut doc.description, 2);
                    }
                    DocKind::Folder => {
                        ui.label(RichText::new("CONTENIDO").size(11.0).color(p.dim));
                        ui.add_space(tokens::SP_XS);
                        if children.is_empty() {
                            ui.label(RichText::new("La carpeta está vacía. Usá «Añadir» mientras la tenés abierta.").color(p.dim));
                        }
                        for (cid, ckind, cname) in &children {
                            let row = Row {
                                depth: 0,
                                caret: None,
                                icon: Some(document_icon(*ckind, false)),
                                label: cname,
                                selected: false,
                            };
                            if tree_row(ui, p, row).clicked() {
                                out.open = Some(*cid);
                            }
                        }
                    }
                    DocKind::Gallery => {
                        if accent_button(ui, p, icon::PLUS, "Añadir imágenes").clicked() {
                            if let Some(files) = rfd::FileDialog::new()
                                .add_filter("Imágenes", &["png", "jpg", "jpeg", "webp"])
                                .pick_files()
                            {
                                doc.images.extend(files);
                                out.changed = true;
                            }
                        }
                        ui.add_space(tokens::SP_M);
                        let mut remove = None;
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                            for (n, path) in doc.images.iter().enumerate() {
                                ui.vertical(|ui| {
                                    let (rect, _) =
                                        ui.allocate_exact_size(vec2(180.0, 130.0), Sense::hover());
                                    ui.painter().rect_filled(rect, 4, p.field);
                                    let shown = match &base {
                                        Some(b) if path.is_relative() => b.join(path),
                                        _ => path.clone(),
                                    };
                                    egui::Image::new(format!("file://{}", shown.display()))
                                        .fit_to_exact_size(rect.size())
                                        .corner_radius(4)
                                        .paint_at(ui, rect);
                                    if crate::widgets::icon_button(
                                        ui,
                                        p,
                                        icon::TRASH,
                                        "Quitar de la galería",
                                        true,
                                    )
                                    .clicked()
                                    {
                                        remove = Some(n);
                                    }
                                });
                            }
                        });
                        if let Some(n) = remove {
                            doc.images.remove(n);
                            out.changed = true;
                        }
                    }
                    DocKind::MindMap | DocKind::Cards | DocKind::Relations => {}
                }
                if kind == DocKind::World {
                    ui.add_space(tokens::SP_S);
                }
            });
            if kind == DocKind::World {
                ui.add_space(tokens::SP_M);
                card(ui, p, |ui| {
                    title(ui, p, "Ficha completa");
                    out.changed |= profile_sections(
                        ui,
                        p,
                        &profile::owned(profile::WORLD),
                        &mut doc.fields,
                        "world",
                    );
                });
                ui.add_space(tokens::SP_M);
            } else {
                ui.add_space(tokens::SP_M);
            }
            let label = if kind == DocKind::Folder {
                "Eliminar carpeta y su contenido"
            } else {
                "Eliminar"
            };
            if accent_button(ui, p, icon::TRASH, label).clicked() {
                out.delete = true;
            }
        },
    );
    out
}

/// Posiciones de los nodos de un mapa mental: árbol de izquierda a derecha.
fn mind_layout(nodes: &[MindNode]) -> HashMap<u64, (egui::Pos2, usize)> {
    fn place(
        id: u64,
        depth: usize,
        nodes: &[MindNode],
        row: &mut f32,
        out: &mut HashMap<u64, (egui::Pos2, usize)>,
    ) -> f32 {
        if out.contains_key(&id) {
            return out[&id].0.y; // protege de ciclos en archivos dañados
        }
        out.insert(id, (egui::pos2(0.0, 0.0), depth));
        let kids: Vec<u64> = nodes
            .iter()
            .filter(|n| n.parent == Some(id))
            .map(|n| n.id)
            .collect();
        let y = if kids.is_empty() {
            let y = *row;
            *row += 54.0;
            y
        } else {
            let ys: Vec<f32> = kids
                .iter()
                .map(|k| place(*k, depth + 1, nodes, row, out))
                .collect();
            (ys[0] + ys[ys.len() - 1]) / 2.0
        };
        out.insert(id, (egui::pos2(depth as f32 * 250.0, y), depth));
        y
    }
    let mut out = HashMap::new();
    let mut row = 0.0;
    for n in nodes
        .iter()
        .filter(|n| n.parent.is_none_or(|p| !nodes.iter().any(|m| m.id == p)))
    {
        place(n.id, 0, nodes, &mut row, &mut out);
    }
    out
}

fn mind_map(
    ui: &mut Ui,
    project: &mut Project,
    idx: usize,
    selected: &mut Option<u64>,
    out: &mut DocOutcome,
    p: &Palette,
) {
    let doc_id = project.documents[idx].id;
    if selected.is_some_and(|s| !project.documents[idx].nodes.iter().any(|n| n.id == s)) {
        *selected = None;
    }
    let mut add_child = None;
    let mut remove = None;

    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            let doc = &mut project.documents[idx];
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::SHARE_NETWORK)
                        .size(20.0)
                        .color(p.accent),
                );
                out.changed |= ui
                    .add_sized(vec2(260.0, 26.0), egui::TextEdit::singleline(&mut doc.name))
                    .changed();
                ui.add_space(tokens::SP_L);
                let sel = *selected;
                ui.add_enabled_ui(sel.is_some(), |ui| {
                    if accent_button(ui, p, icon::PLUS, "Idea hija").clicked() {
                        add_child = sel;
                    }
                });
                let root = sel
                    .and_then(|s| doc.nodes.iter().find(|n| n.id == s))
                    .is_some_and(|n| n.parent.is_none());
                ui.add_enabled_ui(sel.is_some() && !root, |ui| {
                    if accent_button(ui, p, icon::TRASH, "Eliminar idea").clicked() {
                        remove = sel;
                    }
                });
                ui.add_space(tokens::SP_L);
                if accent_button(ui, p, icon::TRASH, "Eliminar mapa").clicked() {
                    out.delete = true;
                }
            });
            if let Some(node) = selected.and_then(|s| doc.nodes.iter_mut().find(|n| n.id == s)) {
                ui.add_space(tokens::SP_S);
                out.changed |= field(ui, p, "Texto de la idea seleccionada", &mut node.text, 1);
            } else {
                ui.add_space(tokens::SP_S);
            }
        });

    let doc = &project.documents[idx];
    let layout = mind_layout(&doc.nodes);
    let node_size = |text: &str| {
        vec2(
            (text.chars().count() as f32 * 8.5 + 32.0).clamp(96.0, 260.0),
            34.0,
        )
    };
    let max_x = layout.values().map(|(pos, _)| pos.x).fold(0.0, f32::max) + 320.0;
    let max_y = layout.values().map(|(pos, _)| pos.y).fold(0.0, f32::max) + 90.0;
    let branch_colors = |id: u64| -> Color32 {
        // El color lo da la rama de primer nivel a la que pertenece.
        let mut cur = id;
        let mut guard = 0;
        while let Some(parent) = doc
            .nodes
            .iter()
            .find(|n| n.id == cur)
            .and_then(|n| n.parent)
        {
            let grand = doc
                .nodes
                .iter()
                .find(|n| n.id == parent)
                .and_then(|n| n.parent);
            if grand.is_none() {
                return theme::character_color(&cur.to_string());
            }
            cur = parent;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        p.accent
    };

    let mut clicked = None;
    let mut child_of: Option<u64> = None;
    let mut sibling_of: Option<u64> = None;
    egui::ScrollArea::both()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(
                vec2(
                    max_x.max(ui.available_width()),
                    max_y.max(ui.available_height()),
                ),
                Sense::hover(),
            );
            let origin = rect.min + vec2(40.0, 40.0);
            let painter = ui.painter_at(rect);
            for n in &doc.nodes {
                let Some((pos, _)) = layout.get(&n.id) else {
                    continue;
                };
                if let Some(parent) = n
                    .parent
                    .and_then(|pid| layout.get(&pid).map(|(pp, _)| (pid, *pp)))
                {
                    let a = origin
                        + parent.1.to_vec2()
                        + vec2(
                            node_size(
                                &doc.nodes
                                    .iter()
                                    .find(|m| m.id == parent.0)
                                    .map_or("", |m| &m.text),
                            )
                            .x,
                            17.0,
                        );
                    let b = origin + pos.to_vec2() + vec2(0.0, 17.0);
                    let mid = (a.x + b.x) / 2.0;
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        [a, egui::pos2(mid, a.y), egui::pos2(mid, b.y), b],
                        false,
                        Color32::TRANSPARENT,
                        Stroke::new(2.0, branch_colors(n.id).linear_multiply(0.8)),
                    ));
                }
            }
            for n in &doc.nodes {
                let Some((pos, _)) = layout.get(&n.id) else {
                    continue;
                };
                let size = node_size(&n.text);
                let r = egui::Rect::from_min_size(origin + pos.to_vec2(), size);
                let resp = ui.interact(r, egui::Id::unique(("mind_node", n.id)), Sense::click());
                let is_sel = *selected == Some(n.id);
                let accent = if n.parent.is_none() {
                    p.accent
                } else {
                    branch_colors(n.id)
                };
                let fill = if is_sel { p.selection } else { p.container };
                painter.rect(
                    r,
                    17,
                    fill,
                    Stroke::new(if is_sel { 2.0 } else { 1.5 }, accent),
                    egui::StrokeKind::Inside,
                );
                painter.with_clip_rect(r.shrink2(vec2(10.0, 0.0))).text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    &n.text,
                    FontId::proportional(14.0),
                    p.text,
                );
                if resp.clicked() {
                    clicked = Some(n.id);
                }
                // Tiradores: al pasar el mouse (o con el nodo elegido) aparecen un «+» a la derecha, para una idea hija, y
                // otro abajo, para una hermana; así no hace falta elegir el nodo y buscar el botón.
                let near = ui.rect_contains_pointer(r.expand2(vec2(22.0, 14.0)));
                if near || is_sel {
                    let handle = |center: egui::Pos2, id_salt: &str, tip: &str| -> bool {
                        let hr = egui::Rect::from_center_size(center, vec2(20.0, 20.0));
                        let hresp = ui.interact(hr, egui::Id::unique((id_salt, n.id)), Sense::click());
                        let hot = hresp.hovered();
                        painter.circle(center, 8.0, if hot { accent } else { p.container }, Stroke::new(1.5, accent));
                        painter.text(center, Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(11.0), if hot { p.on_accent } else { accent });
                        hresp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                    };
                    if handle(r.right_center() + vec2(12.0, 0.0), "mind_child", "Añadir una idea hija") {
                        child_of = Some(n.id);
                    }
                    if n.parent.is_some() && handle(r.center_bottom() + vec2(0.0, 12.0), "mind_sibling", "Añadir una idea hermana") {
                        sibling_of = n.parent;
                    }
                }
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        });
    if clicked.is_some() {
        *selected = clicked;
    }
    // Un tirador agrega y deja elegida la idea nueva, lista para escribirle el texto.
    if let Some(parent) = child_of.or(sibling_of) {
        add_child = Some(parent);
    }

    if let Some(parent) = add_child {
        if let Some(new) = project.add_mind_node(doc_id, parent) {
            *selected = Some(new);
            out.changed = true;
        }
    }
    if let Some(id) = remove {
        let d = &mut project.documents[idx];
        let mut doomed = vec![id];
        let mut i = 0;
        while i < doomed.len() {
            let cur = doomed[i];
            doomed.extend(
                d.nodes
                    .iter()
                    .filter(|n| n.parent == Some(cur))
                    .map(|n| n.id),
            );
            i += 1;
        }
        d.nodes.retain(|n| !doomed.contains(&n.id));
        *selected = None;
        out.changed = true;
    }
}

/// Vista del desglose.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum BreakdownView {
    #[default]
    ByCategory,
    /// La hoja de desglose de cada escena.
    ByScene,
    /// La lista de compras y alquileres.
    Shopping,
}

pub struct BreakdownState {
    pub view: BreakdownView,
    /// Ordenar por nombre en vez de por aparición en el guion.
    pub alphabetical: bool,
    drafts: HashMap<u64, String>,
    new_category: String,
    new_color: [u8; 3],
    /// Categorías desplegadas o plegadas por la persona; las demás parten abiertas si tienen elementos.
    open: HashMap<u64, bool>,
    /// Categoría propia que se está editando (nombre, color, borrar).
    pub(crate) editing: Option<u64>,
    /// El ítem elegido (categoría, nombre) para el inspector.
    pub selected: Option<(u64, String)>,
    /// La hoja por escena y la lista de compras.
    views: crate::breakdown_views::ViewState,
}

impl BreakdownState {
    /// Abre la hoja de desglose de una escena.
    pub fn show_scene(&mut self, scene: u64) {
        self.view = BreakdownView::ByScene;
        self.views.show_scene(scene);
    }
}

impl Default for BreakdownState {
    fn default() -> Self {
        Self {
            view: BreakdownView::default(),
            alphabetical: false,
            drafts: HashMap::new(),
            new_category: String::new(),
            new_color: [0x90, 0xA4, 0xAE],
            open: HashMap::new(),
            editing: None,
            selected: None,
            views: Default::default(),
        }
    }
}

/// A dónde ir desde una fila del desglose.
pub enum BreakdownTarget {
    Character(u64),
    Location(u64),
    Object(u64),
    Scene(u64),
}

#[derive(Default)]
pub struct BreakdownOutcome {
    pub changed: bool,
    /// El ítem que se eligió para ver su ficha en el inspector: (categoría, nombre).
    pub select: Option<(u64, String)>,
    pub open: Option<BreakdownTarget>,
    /// Un mensaje breve para el aviso de abajo (exportar).
    pub status: Option<String>,
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn scene_list(scenes: &[usize]) -> String {
    if scenes.is_empty() {
        return "—".into();
    }
    scenes
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Marco de una tarjeta del desglose: fondo, borde fino y esquinas redondeadas.
fn bd_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.container)
        .stroke(Stroke::new(1.0, theme::mix(p.container, p.border, 0.7)))
        .corner_radius(8)
}

/// Franja de color a la izquierda de una tarjeta.
fn paint_stripe(ui: &Ui, rect: egui::Rect, color: Color32) {
    let stripe = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 4.0, rect.max.y));
    ui.painter().rect_filled(
        stripe,
        egui::CornerRadius {
            nw: 8,
            sw: 8,
            ne: 0,
            se: 0,
        },
        color,
    );
}

/// Herramienta de desglose: lo que hace falta para rodar, por categoría o por escena. La cabecera y la barra
/// de herramientas quedan fijas arriba; solo el contenido se desplaza.
/// Herramientas de la barra del desglose: la vista (por ítem, por escena o compras).
pub fn breakdown_toolbar(ui: &mut Ui, state: &mut BreakdownState, p: &Palette) {
    let views = [
        BreakdownView::ByCategory,
        BreakdownView::ByScene,
        BreakdownView::Shopping,
    ];
    let active = views.iter().position(|v| *v == state.view).unwrap_or(0);
    if let Some(i) =
        crate::ui::kit::segmented(ui, p, &["Por ítem", "Por escena", "Compras"], active)
    {
        state.view = views[i];
    }
}

/// Barra de contexto del desglose: en la vista por ítem, ordenar y crear categorías.
pub fn breakdown_context_bar(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut BreakdownState,
    p: &Palette,
) -> bool {
    let mut changed = false;
    if state.view != BreakdownView::ByCategory {
        return changed;
    }
    crate::ui::shell::context_bar(ui, p, |ui| {
        ui.checkbox(&mut state.alphabetical, "A–Z");
        ui.separator();
        let width = crate::ui::kit::fill_width(ui, 100.0, 200.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.new_category)
                .hint_text("Nueva categoría…")
                .desired_width(width),
        );
        ui.color_edit_button_srgb(&mut state.new_color);
        if ui
            .add_enabled(
                !state.new_category.trim().is_empty(),
                egui::Button::new("Crear categoría"),
            )
            .clicked()
        {
            project.add_breakdown_category(&state.new_category, state.new_color);
            state.new_category.clear();
            changed = true;
        }
    });
    changed
}

/// El inspector del desglose: pestaña ÍTEM (la ficha del ítem elegido) y CATEGORÍAS (nombre y color de cada una).
/// Devuelve (cambió el proyecto, ir a la ficha de algo).
pub fn breakdown_inspector(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut BreakdownState,
    p: &Palette,
    tab: usize,
) -> bool {
    use crate::ui::kit;
    let mut changed = false;
    if tab == 0 {
        let Some((category, name)) = state.selected.clone() else {
            kit::empty_state(
                ui,
                p,
                icon::CLIPBOARD_TEXT,
                "Elegí un ítem (un clic en sus escenas) para ver y editar su ficha.",
                None,
            );
            return false;
        };
        let rows = project.breakdown_rows(state.alphabetical);
        let Some(row) = rows
            .into_iter()
            .find(|r| r.category == category && r.name == name)
        else {
            kit::empty_state(
                ui,
                p,
                icon::CLIPBOARD_TEXT,
                "Ese ítem ya no está en el desglose.",
                None,
            );
            state.selected = None;
            return false;
        };
        let cat_name = project
            .breakdown_categories
            .iter()
            .find(|c| c.id == category)
            .map_or(String::new(), |c| c.name.clone());
        ui.add(
            egui::Label::new(
                RichText::new(&row.name)
                    .font(FontId::new(tokens::FONT_BODY, bold_family()))
                    .color(p.text),
            )
            .truncate(),
        )
        .on_hover_text(&row.name);
        ui.add(
            egui::Label::new(
                RichText::new(&cat_name)
                    .size(tokens::FONT_SMALL)
                    .color(p.dim),
            )
            .truncate(),
        );
        let stored = row.item.map(|i| project.breakdown_items[i].clone());
        let mut status = stored
            .as_ref()
            .map_or(crate::model::Acquisition::Pending, |i| i.status);
        let mut quantity = stored.as_ref().map_or(0, |i| i.quantity);
        let mut notes = stored.as_ref().map(|i| i.notes.clone()).unwrap_or_default();
        let mut cost = stored.as_ref().and_then(|i| i.cost_cents);
        let mut unit_cost = stored.as_ref().is_some_and(|i| i.unit_cost);
        let mut responsible = stored.as_ref().and_then(|i| i.responsible);
        // Una locación con lugar de rodaje trae su estado, su costo y su contacto mientras la fila no tenga datos propios.
        if let Some(deal) = row.entity.and_then(|e| project.location_deal(e)) {
            if status == crate::model::Acquisition::Pending {
                status = deal.status.unwrap_or(status);
            }
            cost = cost.or(deal.cost_cents);
            responsible = responsible.or(deal.responsible);
        }
        let people: Vec<(u64, String)> = project
            .contacts
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect();
        let mut changed_scenes = false;
        kit::section(ui, p, "Conseguirlo", None);
        kit::combo_field(
            ui,
            p,
            "Estado",
            "bd_inspector_status",
            status.label(),
            |ui| {
                for s in crate::model::Acquisition::ALL {
                    changed |= ui.selectable_value(&mut status, s, s.label()).changed();
                }
            },
        );
        changed |= kit::drag_prop(
            ui,
            p,
            "Cantidad",
            egui::DragValue::new(&mut quantity)
                .range(0..=999)
                .speed(0.2),
        );
        let mut pesos = cost.unwrap_or(0) as f64 / 100.0;
        if kit::drag_prop(
            ui,
            p,
            "Costo",
            egui::DragValue::new(&mut pesos)
                .range(0.0..=1_000_000_000.0)
                .speed(50.0)
                .prefix("$ "),
        ) {
            cost = (pesos > 0.0).then(|| (pesos * 100.0).round() as i64);
            changed = true;
        }
        // El costo es de cada unidad (se multiplica por la cantidad) o de todas juntas.
        kit::prop_row(ui, p, "El costo es", |ui| {
            if let Some(n) =
                kit::segmented(ui, p, &["Por unidad", "Total"], usize::from(!unit_cost))
            {
                unit_cost = n == 0;
                changed = true;
            }
        });
        if let Some(total) = cost.map(|c| {
            if unit_cost {
                c * i64::from(quantity.max(1))
            } else {
                c
            }
        }) {
            kit::kv_row(ui, p, "Total", &crate::reports::money(total));
        }
        let shown = responsible
            .and_then(|r| people.iter().find(|(i, _)| *i == r))
            .map_or("—".to_string(), |(_, n)| n.clone());
        kit::combo_field(ui, p, "Responsable", "bd_inspector_who", &shown, |ui| {
            if ui
                .selectable_label(responsible.is_none(), "Sin responsable")
                .clicked()
            {
                responsible = None;
                changed = true;
            }
            for (pid, n) in &people {
                if ui.selectable_label(responsible == Some(*pid), n).clicked() {
                    responsible = Some(*pid);
                    changed = true;
                }
            }
        });
        changed |= kit::field(ui, p, "Notas", &mut notes, 3);
        kit::section(ui, p, "Escenas", None);
        ui.add(
            egui::Label::new(
                RichText::new(if row.scenes.is_empty() {
                    "—".to_string()
                } else {
                    scene_list(&row.scenes)
                })
                .color(p.dim),
            )
            .wrap(),
        );
        // Marcar a mano en qué escenas se necesita, aunque no esté marcado en el guion.
        ui.menu_button(
            format!(
                "{}  Marcar escenas  {}",
                icon::LIST_CHECKS,
                icon::CARET_DOWN
            ),
            |ui| {
                changed_scenes |= scene_picker(ui, p, project, &row);
            },
        );
        if changed {
            if let Some(i) = project.breakdown_item_for_row(&row) {
                let item = &mut project.breakdown_items[i];
                (
                    item.status,
                    item.quantity,
                    item.notes,
                    item.cost_cents,
                    item.responsible,
                ) = (status, quantity, notes, cost, responsible);
                item.unit_cost = unit_cost;
            }
        }
        changed |= changed_scenes;
    } else {
        let categories = project.breakdown_categories.clone();
        if categories.is_empty() {
            kit::empty_state(
                ui,
                p,
                icon::CLIPBOARD_TEXT,
                "Todavía no hay categorías.",
                None,
            );
        }
        let mut remove = None;
        for (ci, cat) in categories.iter().enumerate() {
            let custom = cat.source == crate::model::CategorySource::Custom;
            kit::section(ui, p, &cat.name, None);
            let origin = match cat.source {
                crate::model::CategorySource::Characters => "Personajes del proyecto",
                crate::model::CategorySource::Locations => "Locaciones del proyecto",
                crate::model::CategorySource::Objects => "Objetos clave",
                crate::model::CategorySource::Custom => "Manual",
            };
            ui.add(egui::Label::new(
                RichText::new(format!("Origen: {origin}"))
                    .size(tokens::FONT_SMALL)
                    .color(p.dim),
            ));
            if custom {
                let c = &mut project.breakdown_categories[ci];
                crate::ui::kit::sides(
                    ui,
                    |ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut c.name)
                                    .desired_width(kit::fill_width(ui, 40.0, 120.0)),
                            )
                            .changed();
                        changed |= ui.color_edit_button_srgb(&mut c.color).changed();
                    },
                    |ui| {
                        if kit::icon_button(ui, p, icon::TRASH, "Eliminar la categoría", true)
                            .clicked()
                        {
                            remove = Some(cat.id);
                        }
                    },
                );
            } else {
                let c = &mut project.breakdown_categories[ci];
                changed |= ui.color_edit_button_srgb(&mut c.color).changed();
            }
        }
        if let Some(id) = remove {
            changed |= project.remove_breakdown_category(id);
        }
    }
    changed
}

pub fn breakdown(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut BreakdownState,
    p: &Palette,
    paper: crate::storage::Paper,
) -> BreakdownOutcome {
    let mut out = BreakdownOutcome::default();
    ui.scope(|ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                match state.view {
                    BreakdownView::ByCategory => {
                        breakdown_by_category(ui, project, state, p, &mut out)
                    }
                    BreakdownView::ByScene => {
                        let views =
                            crate::breakdown_views::sheet(ui, project, &mut state.views, p, paper);
                        out.changed |= views.changed;
                        out.status = views.status.or(out.status.take());
                        if let Some(id) = views.open_scene {
                            out.open = Some(BreakdownTarget::Scene(id));
                        }
                    }
                    BreakdownView::Shopping => {
                        let views = crate::breakdown_views::shopping(
                            ui,
                            project,
                            &mut state.views,
                            p,
                            paper,
                        );
                        out.changed |= views.changed;
                        out.status = views.status.or(out.status.take());
                    }
                }
                ui.add_space(tokens::SP_XXL);
            });
    });
    if let Some(sel) = out.select.take() {
        state.selected = Some(sel);
    }
    out
}

fn breakdown_by_category(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut BreakdownState,
    p: &Palette,
    out: &mut BreakdownOutcome,
) {
    let rows = project.breakdown_rows(state.alphabetical);
    let categories = project.breakdown_categories.clone();
    let selected = state.selected.clone();
    let mut remove_category = None;
    for (ci, cat) in categories.iter().enumerate() {
        let mine: Vec<&crate::model::BreakdownRow> =
            rows.iter().filter(|r| r.category == cat.id).collect();
        let color = rgb(cat.color);
        let custom = cat.source == crate::model::CategorySource::Custom;
        let open = *state.open.entry(cat.id).or_insert(!mine.is_empty());

        let card = bd_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Cabecera: se pulsa para plegar o desplegar.
            egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 12, top: 10, bottom: 10 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let caret = if open { icon::CARET_DOWN } else { icon::CARET_RIGHT };
                    let r1 = ui.add(egui::Label::new(RichText::new(caret).size(13.0).color(p.dim)).sense(Sense::click()));
                    let r2 = ui.add(
                        egui::Label::new(RichText::new(&cat.name).font(FontId::new(16.0, bold_family())).color(p.text))
                            .sense(Sense::click()),
                    );
                    egui::Frame::new()
                        .fill(color.linear_multiply(0.22))
                        .corner_radius(9)
                        .inner_margin(egui::Margin::symmetric(8, 1))
                        .show(ui, |ui| {
                            ui.label(RichText::new(mine.len().to_string()).size(12.0).color(color));
                        });
                    let hint = match cat.source {
                        crate::model::CategorySource::Characters => Some("personajes del proyecto"),
                        crate::model::CategorySource::Locations => Some("locaciones del proyecto"),
                        crate::model::CategorySource::Objects => Some("objetos importantes para la historia"),
                        crate::model::CategorySource::Custom => None,
                    };
                    if let Some(hint) = hint {
                        ui.label(RichText::new(hint).size(12.0).color(p.dim));
                    }
                    if r1.clicked() || r2.clicked() {
                        state.open.insert(cat.id, !open);
                    }
                    if custom {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let editing = state.editing == Some(cat.id);
                            if crate::widgets::icon_button(ui, p, icon::GEAR, "Editar categoría", true).clicked() {
                                state.editing = if editing { None } else { Some(cat.id) };
                            }
                        });
                    }
                });
            });

            if open {
                ui.separator();
                if custom && state.editing == Some(cat.id) {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 8)).show(ui, |ui| {
                        let c = &mut project.breakdown_categories[ci];
                        crate::ui::kit::sides(
                            ui,
                            |ui| {
                                out.changed |= ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(crate::ui::kit::fill_width(ui, 40.0, 220.0))).changed();
                                out.changed |= ui.color_edit_button_srgb(&mut c.color).changed();
                            },
                            |ui| {
                                if accent_button(ui, p, icon::TRASH, "Eliminar categoría").clicked() {
                                    remove_category = Some(cat.id);
                                }
                            },
                        );
                    });
                    ui.separator();
                }
                egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if mine.is_empty() {
                        let msg = if custom {
                            "Todavía no hay elementos. Añadí uno acá abajo o marcalo desde el guion."
                        } else {
                            "Todavía no hay elementos. Esta categoría se completa sola con los del proyecto."
                        };
                        ui.add_space(tokens::SP_S);
                        ui.label(RichText::new(msg).color(p.dim));
                        ui.add_space(tokens::SP_S);
                    } else {
                        breakdown_table(ui, project, p, cat.id, &mine, selected.as_ref(), out);
                    }
                });
                if custom {
                    ui.separator();
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 8)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let draft = state.drafts.entry(cat.id).or_default();
                            let resp = ui.add(egui::TextEdit::singleline(draft).hint_text("Añadir elemento…").desired_width(crate::ui::kit::fill_width(ui, 40.0, 260.0)));
                            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (enter || ui.button("Añadir").clicked()) && !draft.trim().is_empty() {
                                if project.breakdown_item_for(cat.id, draft).is_some() {
                                    out.changed = true;
                                }
                                draft.clear();
                            }
                        });
                    });
                }
            }
        });
        paint_stripe(ui, card.response.rect, color);
        ui.add_space(tokens::SP_M);
    }
    if let Some(id) = remove_category {
        out.changed |= project.remove_breakdown_category(id);
        state.editing = None;
    }
}

/// Tabla alineada de los elementos de una categoría: solo texto en las celdas. Un clic elige el ítem (su ficha se edita en
/// el inspector), un clic en las escenas abre la lista para marcar dónde se necesita, y el menú de la fila quita el ítem.
fn breakdown_table(
    ui: &mut Ui,
    project: &mut Project,
    p: &Palette,
    category: u64,
    rows: &[&crate::model::BreakdownRow],
    selected: Option<&(u64, String)>,
    out: &mut BreakdownOutcome,
) {
    use crate::ui::kit::{self, ColAlign, ColumnDef, TableRowKind};
    // Las columnas, declaradas una sola vez (reglas §2.6).
    const COLS: [ColumnDef; 7] = [
        ColumnDef::new("name", "Elemento", 160.0).initial(230.0),
        ColumnDef::new("scenes", "Escenas", 90.0).initial(110.0),
        ColumnDef::new("status", "Estado", 100.0),
        ColumnDef::new("quantity", "Cant.", 64.0).align(ColAlign::Right),
        ColumnDef::new("cost", "Costo", 100.0).align(ColAlign::Right),
        ColumnDef::new("who", "Responsable", 120.0).initial(150.0),
        ColumnDef::new("notes", "Notas", 150.0).rest(),
    ];
    let kinds = vec![TableRowKind::Item; rows.len()];
    let picked = selected
        .filter(|(c, _)| *c == category)
        .and_then(|(_, name)| rows.iter().position(|r| &r.name == name));
    let mut remove: Option<usize> = None;
    let mut menu_select: Option<(u64, String)> = None;
    let table = kit::data_table_with(
        ui,
        p,
        &format!("breakdown_table_{category}"),
        &COLS,
        true,
        tokens::ROW_H,
        false,
        &kinds,
        picked,
        |ui, n, col| {
            let row = rows[n];
            let stored = row.item.map(|i| project.breakdown_items[i].clone());
            let mut status = stored
                .as_ref()
                .map_or(crate::model::Acquisition::Pending, |i| i.status);
            let quantity = stored.as_ref().map_or(0, |i| i.quantity);
            let mut cost = stored.as_ref().and_then(|i| i.total_cents());
            let mut responsible = stored.as_ref().and_then(|i| i.responsible);
            // Una locación con lugar de rodaje trae su estado, su costo y su contacto mientras la fila no tenga datos propios.
            if let Some(deal) = row.entity.and_then(|e| project.location_deal(e)) {
                if status == crate::model::Acquisition::Pending {
                    status = deal.status.unwrap_or(status);
                }
                cost = cost.or(deal.cost_cents);
                responsible = responsible.or(deal.responsible);
            }
            match col.id {
                "name" => {
                    kit::cell_text(ui, p, &row.name);
                }
                "scenes" => {
                    let resp = ui
                        .add(
                            egui::Label::new(RichText::new(scene_list(&row.scenes)).color(p.dim))
                                .truncate()
                                .sense(Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Clic: marcar en qué escenas se necesita");
                    if resp.clicked() {
                        out.select = Some((row.category, row.name.clone()));
                    }
                    egui::Popup::menu(&resp)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
                            if scene_picker(ui, p, project, row) {
                                out.changed = true;
                            }
                        });
                }
                "status" => {
                    kit::cell_text(ui, p, status.label());
                }
                "quantity" => {
                    kit::cell_text(
                        ui,
                        p,
                        &if quantity > 0 {
                            quantity.to_string()
                        } else {
                            String::new()
                        },
                    );
                }
                "cost" => {
                    let text = cost.map_or(String::new(), crate::reports::money);
                    let tip = match (&stored, cost) {
                        (Some(i), Some(total)) if i.unit_cost => format!(
                            "{} por unidad × {} = {}",
                            crate::reports::money(i.cost_cents.unwrap_or(0)),
                            i.quantity.max(1),
                            crate::reports::money(total)
                        ),
                        _ => text.clone(),
                    };
                    kit::cell_text(ui, p, &text).on_hover_text(tip);
                }
                "who" => {
                    let name = responsible
                        .and_then(|r| project.contacts.iter().find(|c| c.id == r))
                        .map_or(String::new(), |c| c.name.clone());
                    kit::cell_text(ui, p, &name);
                }
                _ => {
                    kit::cell_text(ui, p, stored.as_ref().map_or("", |i| i.notes.as_str()));
                }
            }
        },
        |ui, n| {
            let row = rows[n];
            if ui.button("Elegir (editar en el inspector)").clicked() {
                menu_select = Some((row.category, row.name.clone()));
                ui.close();
            }
            if !row.auto && row.item.is_some() && ui.button("Quitar del desglose").clicked() {
                remove = row.item;
                ui.close();
            }
        },
    );
    if let Some(n) = table.clicked.or(table.double_clicked) {
        out.select = Some((rows[n].category, rows[n].name.clone()));
    }
    out.select = out.select.take().or(menu_select);
    if let Some(n) = table.double_clicked {
        let row = rows[n];
        out.open = match row.entity {
            Some(crate::model::EntityRef::Character(id)) => Some(BreakdownTarget::Character(id)),
            Some(crate::model::EntityRef::Location(id)) => Some(BreakdownTarget::Location(id)),
            Some(crate::model::EntityRef::Object(id)) => Some(BreakdownTarget::Object(id)),
            _ => None,
        };
    }
    if let Some(i) = remove {
        project.remove_breakdown_item(i);
        out.changed = true;
    }
}

/// La lista de escenas para marcar a mano en cuáles se necesita un ítem del desglose. Las que ya salen del guion (por una
/// marca o por aparecer) quedan marcadas y no se pueden sacar desde acá. Devuelve si cambió algo.
fn scene_picker(
    ui: &mut Ui,
    p: &Palette,
    project: &mut Project,
    row: &crate::model::BreakdownRow,
) -> bool {
    let scenes = project.script_scenes();
    let labels = project.scene_labels();
    let extra: Vec<u64> = row
        .item
        .map(|i| project.breakdown_items[i].extra_scenes.clone())
        .unwrap_or_default();
    let mut toggled: Option<u64> = None;
    if scenes.is_empty() {
        ui.label(RichText::new("Todavía no hay escenas en el guion.").color(p.dim));
    }
    egui::ScrollArea::vertical()
        .id_salt("breakdown_scene_picker")
        .max_height(6.0 * tokens::ROW_H)
        .show(ui, |ui| {
            for (n, (id, heading)) in scenes.iter().enumerate() {
                let marked = row.scenes.contains(&(n + 1));
                let from_script = marked && !extra.contains(id);
                let mut on = marked;
                let label = format!("{}. {}", labels[n].1, heading.trim());
                let resp = ui
                    .add_enabled(!from_script, egui::Checkbox::new(&mut on, label))
                    .on_disabled_hover_text("Ya está marcado en el guion");
                if resp.changed() {
                    toggled = Some(*id);
                }
            }
        });
    let Some(scene) = toggled else {
        return false;
    };
    let Some(i) = project.breakdown_item_for_row(row) else {
        return false;
    };
    let item = &mut project.breakdown_items[i];
    if let Some(at) = item.extra_scenes.iter().position(|s| *s == scene) {
        item.extra_scenes.remove(at);
    } else {
        item.extra_scenes.push(scene);
    }
    true
}

pub fn fmt_duration(seconds: f32) -> String {
    let s = seconds.round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

fn table_header(ui: &mut Ui, p: &Palette, headers: &[&str]) {
    for h in headers {
        ui.label(RichText::new(*h).color(p.dim));
    }
    ui.end_row();
}

pub fn statistics(ui: &mut Ui, project: &Project, p: &Palette) {
    let stats = project.stats();
    let scenes = project.scenes();
    let activity = project.character_activity();
    page(ui, 900.0, |ui| {
        card(ui, p, |ui| {
            ui.set_width(ui.available_width());
            ui.columns(4, |cols| {
                let items = [
                    ("Duración", fmt_duration(stats.seconds)),
                    ("Páginas", format!("{:.0}", stats.pages.ceil())),
                    ("Palabras", stats.words.to_string()),
                    (
                        "Caracteres con/sin espacios",
                        format!("{}/{}", stats.chars_with_spaces, stats.chars_without_spaces),
                    ),
                ];
                for (col, (label, value)) in cols.iter_mut().zip(items) {
                    col.vertical_centered(|ui| {
                        ui.label(RichText::new(label).size(12.0).color(p.dim));
                        ui.label(
                            RichText::new(value)
                                .font(FontId::new(24.0, bold_family()))
                                .color(p.accent),
                        );
                    });
                }
            });
        });
        ui.add_space(tokens::SP_L);

        card(ui, p, |ui| {
            ui.set_width(ui.available_width());
            title(ui, p, "Estadísticas de texto");
            let total = stats.words.max(1) as f32;
            egui::Grid::new("kinds")
                .num_columns(4)
                .min_col_width(110.0)
                .spacing([24.0, 10.0])
                .show(ui, |ui| {
                    table_header(ui, p, &["Párrafo", "Palabras", "Apariciones", "Porcentaje"]);
                    for (kind, s) in &stats.per_kind {
                        ui.label(kind.label());
                        ui.label(s.words.to_string());
                        ui.label(s.occurrences.to_string());
                        ui.label(format!("{:.1}%", s.words as f32 / total * 100.0));
                        ui.end_row();
                    }
                });
        });
        ui.add_space(tokens::SP_L);

        card(ui, p, |ui| {
            ui.set_width(ui.available_width());
            title(ui, p, "Escenas");
            egui::Grid::new("scenes")
                .num_columns(4)
                .min_col_width(70.0)
                .spacing([24.0, 10.0])
                .show(ui, |ui| {
                    table_header(ui, p, &["#", "Encabezado", "Palabras", "Duración"]);
                    for s in &scenes {
                        ui.label(s.number.to_string());
                        ui.label(if s.heading.is_empty() {
                            "(sin encabezado)"
                        } else {
                            &s.heading
                        });
                        ui.label(s.words.to_string());
                        ui.label(fmt_duration(s.seconds));
                        ui.end_row();
                    }
                });
        });
        ui.add_space(tokens::SP_L);

        card(ui, p, |ui| {
            ui.set_width(ui.available_width());
            title(ui, p, "Actividad de personajes");
            egui::Grid::new("cast")
                .num_columns(4)
                .min_col_width(110.0)
                .spacing([24.0, 10.0])
                .show(ui, |ui| {
                    table_header(
                        ui,
                        p,
                        &["Personaje", "Intervenciones", "Palabras", "Escenas"],
                    );
                    for c in &activity {
                        // Dos personajes con el mismo nombre se distinguen por lo que los diferencia.
                        let label = c.id.map_or(c.name.clone(), |id| {
                            project.label_for(crate::model::EntityRef::Character(id))
                        });
                        let color_key = c.id.map_or(c.name.clone(), |id| id.to_string());
                        ui.label(RichText::new(label).color(theme::character_color(&color_key)));
                        ui.label(c.dialogues.to_string());
                        ui.label(c.words.to_string());
                        ui.label(c.scenes.to_string());
                        ui.end_row();
                    }
                });
        });
    });
}

/// Fila de selección de tema con una muestra de sus colores.
fn scheme_row(ui: &mut Ui, p: &Palette, scheme: Scheme, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    let sp = scheme.palette();
    let fill = if selected {
        p.selection.linear_multiply(0.55)
    } else if resp.hovered() {
        p.container_hi
    } else {
        p.field
    };
    ui.painter().rect(
        rect,
        6,
        fill,
        Stroke::new(1.0, if selected { p.accent } else { p.border }),
        egui::StrokeKind::Inside,
    );
    for (n, c) in [sp.bg, sp.panel, sp.container, sp.primary, sp.accent]
        .into_iter()
        .enumerate()
    {
        let sw = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 12.0 + n as f32 * 22.0, rect.center().y - 9.0),
            vec2(18.0, 18.0),
        );
        ui.painter().rect(
            sw,
            9,
            c,
            Stroke::new(1.0, sp.border),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        egui::pos2(rect.left() + 136.0, rect.center().y),
        Align2::LEFT_CENTER,
        scheme.name(),
        if selected {
            FontId::new(14.0, bold_family())
        } else {
            FontId::proportional(14.0)
        },
        if selected { p.accent } else { p.text },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Devuelve si cambió algún ajuste.
/// Las secciones de Ajustes (interfaz.md §7).
pub const SETTINGS_SECTIONS: [&str; 5] = ["Apariencia", "Guion", "Producción", "Cámaras", "Agenda"];

/// Ancho de la lista de secciones de Ajustes.
const SETTINGS_NAV_W: f32 = 180.0;

/// Página de Ajustes: las secciones a la izquierda y el contenido en una columna de ancho máximo `SETTINGS_MAX`.
/// `project` es el proyecto abierto (la producción y las cámaras son de cada proyecto). Devuelve si cambiaron los ajustes
/// de la aplicación; `section` es la sección elegida.
pub fn settings(
    ui: &mut Ui,
    section: &mut usize,
    settings: &mut Settings,
    project: &mut Project,
    p: &Palette,
) -> (bool, bool) {
    use crate::ui::kit;
    let (mut changed, mut project_changed) = (false, false);
    ui.horizontal_top(|ui| {
        ui.allocate_ui(vec2(SETTINGS_NAV_W, ui.available_height()), |ui| {
            ui.spacing_mut().item_spacing.y = tokens::LIST_GAP;
            for (n, name) in SETTINGS_SECTIONS.iter().enumerate() {
                if kit::tree_row(ui, p, kit::Row { depth: 0, caret: None, icon: None, label: name, selected: *section == n }).clicked() {
                    *section = n;
                }
            }
        });
        ui.add_space(tokens::SP_XL);
        egui::ScrollArea::vertical().id_salt(("settings_body", *section)).auto_shrink([false; 2]).show(ui, |ui| {
            column(ui, tokens::SETTINGS_MAX, |ui| match *section {
                0 => {
                    kit::section(ui, p, "Tema de color", None);
                    for scheme in Scheme::ALL {
                        if scheme_row(ui, p, scheme, settings.scheme == scheme).clicked() {
                            settings.scheme = scheme;
                            changed = true;
                        }
                    }
                    kit::section(ui, p, "Movimiento", None);
                    changed |= ui.checkbox(&mut settings.reduce_motion, "Reducir animaciones").changed();
                    kit::section(ui, p, "Tamaños", None);
                    kit::prop_row(ui, p, "Interfaz", |ui| {
                        let (min, max) = crate::storage::ZOOM_RANGE;
                        let d = stepper(ui, p, &format!("{} %", settings.ui_zoom), settings.ui_zoom > min, settings.ui_zoom < max);
                        if d != 0 {
                            settings.ui_zoom = (settings.ui_zoom as i32 + d * 5).clamp(min as i32, max as i32) as u32;
                            changed = true;
                        }
                    });
                    if ui.button("Restablecer el tamaño de la interfaz").clicked() && settings.ui_zoom != crate::storage::DEFAULT_ZOOM {
                        settings.ui_zoom = crate::storage::DEFAULT_ZOOM;
                        changed = true;
                    }
                    ui.add(egui::Label::new(RichText::new("También con Ctrl + y Ctrl −").size(tokens::FONT_SMALL).color(p.dim)).wrap());
                    kit::prop_row(ui, p, "Zoom de las hojas", |ui| {
                        let (zmin, zmax) = crate::storage::VIEW_ZOOM_RANGE;
                        let d = stepper(ui, p, &format!("{} %", settings.view_zoom), settings.view_zoom > zmin, settings.view_zoom < zmax);
                        if d != 0 {
                            let step = crate::storage::VIEW_ZOOM_STEP as i32;
                            settings.view_zoom = (settings.view_zoom as i32 + d * step).clamp(zmin as i32, zmax as i32) as u32;
                            changed = true;
                        }
                    });
                }
                1 => {
                    kit::section(ui, p, "Papel", None);
                    ui.add(egui::Label::new("Tamaño de las hojas del guion y del tratamiento").wrap());
                    for paper in crate::storage::Paper::ALL {
                        changed |= ui.radio_value(&mut settings.paper, paper, paper.label()).changed();
                    }
                    kit::section(ui, p, "Nombres", None);
                    changed |= ui.checkbox(&mut settings.bold_names, "Nombres de personajes en negrita").changed();
                    kit::section(ui, p, "Guardado", None);
                    changed |= ui.checkbox(&mut settings.autosave, "Guardar los cambios automáticamente").changed();
                    kit::section(ui, p, "Tu nombre", None);
                    ui.add(egui::Label::new("Para saber quién hizo cada versión").wrap());
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut settings.user_name).desired_width(240.0).hint_text("Ana"))
                        .changed();
                    kit::section(ui, p, "Atajos del editor", None);
                    for (k, d) in [
                        ("Enter", "Párrafo nuevo: Encabezado → Acción, Personaje → Diálogo, Acotación → Diálogo, Diálogo → Acción"),
                        ("Tab (vacío)", "Cambia el tipo: Acción → Personaje → Transición → Encabezado; Diálogo ⇄ Acotación"),
                        ("Tab (tras un nombre)", "Añade una acotación antes del diálogo"),
                        ("Ctrl+1…6", "Fuerza el tipo del párrafo"),
                        ("Backspace (vacío)", "Vuelve a Acción; si ya lo es, borra el párrafo"),
                        ("↑ ↓ ← →", "Pasan al párrafo vecino al llegar al borde"),
                        ("Ctrl+A", "Selecciona todo el guion"),
                        ("Mayús + flechas o clic", "Extiende la selección entre párrafos"),
                        ("Arrastrar el mouse", "Selecciona varios párrafos a la vez"),
                        ("Ctrl+C / X / V", "Copiar, cortar y pegar (también varios párrafos)"),
                        ("Ctrl+Z / Ctrl+Y", "Deshacer / rehacer"),
                        ("Ctrl+S", "Guardar"),
                    ] {
                        kit::prop_row(ui, p, k, |ui| {
                            // El valor llega hasta el borde de la zona, como los demás controles de la sección.
                            ui.set_min_width(ui.available_width());
                            ui.add(egui::Label::new(d).wrap());
                        });
                    }
                }
                2 => {
                    kit::section(ui, p, "Jornada de rodaje", None);
                    let mut hours = project.day_minutes as f32 / 60.0;
                    if kit::drag_prop(
                        ui,
                        p,
                        "Duración de la jornada",
                        egui::DragValue::new(&mut hours).range(1.0..=24.0).speed(0.1).suffix(" h").max_decimals(1),
                    ) {
                        project.day_minutes = (hours * 60.0).round() as u32;
                        project_changed = true;
                    }
                    project_changed |= kit::drag_prop(
                        ui,
                        p,
                        "Minutos por 1/8 de página",
                        egui::DragValue::new(&mut project.minutes_per_eighth).range(1..=240).suffix(" min"),
                    );
                    kit::section(ui, p, "Colores de las tiras", None);
                    for (n, label) in ["INT día", "EXT día", "INT noche", "EXT noche"].into_iter().enumerate() {
                        kit::prop_row(ui, p, label, |ui| project_changed |= ui.color_edit_button_srgb(&mut project.strip_colors[n]).changed());
                    }
                    if ui.button("Restablecer colores").clicked() {
                        project.strip_colors = crate::schedule::DEFAULT_STRIP_COLORS;
                        project_changed = true;
                    }
                }
                3 => {
                    kit::section(ui, p, "Cuerpos de cámara del proyecto", None);
                    if project.camera_bodies.is_empty() {
                        kit::empty_state(ui, p, icon::VIDEO_CAMERA, "Todavía no hay cuerpos de cámara. Se agregan desde la ficha de una cámara en las plantas.", None);
                    } else {
                        project_changed |= crate::blocking::bodies_editor(ui, p, project);
                    }
                }
                _ => {
                    kit::section(ui, p, "Agenda de contactos", None);
                    let mut agenda = crate::storage::load_agenda();
                    if agenda.is_empty() {
                        kit::empty_state(ui, p, icon::USERS_THREE, "La agenda está vacía. Guardá contactos desde la pantalla Contactos.", None);
                    }
                    let mut remove = None;
                    for c in &agenda {
                        crate::ui::kit::sides(
                            ui,
                            |ui| {
                                ui.add(egui::Label::new(RichText::new(format!("{} · {}", c.name, c.role)).color(p.text)).truncate()).on_hover_text(&c.name);
                            },
                            |ui| {
                                if kit::icon_button(ui, p, icon::TRASH, "Quitar de la agenda", true).clicked() {
                                    remove = Some(c.id);
                                }
                            },
                        );
                    }
                    if let Some(id) = remove {
                        agenda.retain(|c| c.id != id);
                        crate::storage::save_agenda(&agenda);
                    }
                }
            });
        });
    });
    (changed, project_changed)
}

pub enum HomeAction {
    Create,
    Open,
    OpenRecent(std::path::PathBuf),
    Settings,
}

pub fn ago(modified: Option<std::time::SystemTime>) -> String {
    let Some(t) = modified else {
        return String::new();
    };
    let secs = t.elapsed().map_or(0, |d| d.as_secs());
    match secs {
        0..=59 => "hace un momento".into(),
        60..=3599 => format!("hace {} min", secs / 60),
        3600..=86399 => format!("hace {} h", secs / 3600),
        _ => format!("hace {} días", secs / 86400),
    }
}

const CARD_W: f32 = 380.0;
const CARD_H: f32 = 190.0;
// Portada con proporción de póster (2:3) a la altura de la tarjeta.
const COVER_W: f32 = CARD_H * 2.0 / 3.0;

/// Texto que se acomoda al recuadro: se parte en líneas del ancho dado y se corta con «…» al llegar al alto.
fn fitted_text(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    height: f32,
) -> Option<std::sync::Arc<egui::Galley>> {
    let line = ui.fonts_mut(|f| f.row_height(&font));
    let rows = (height / line).floor() as usize;
    if rows == 0 || text.is_empty() {
        return None;
    }
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, width);
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    Some(ui.fonts_mut(|f| f.layout_job(job)))
}

/// Una tarjeta de historia: portada (o un icono), nombre y descripción que se ajustan al recuadro.
fn story_card(ui: &mut Ui, p: &Palette, card: &RecentCard) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click());
    let hover = resp.hovered();
    ui.painter().rect(
        rect,
        6,
        p.container,
        Stroke::new(
            1.0,
            if hover {
                p.primary
            } else {
                theme::mix(p.container, p.border, 0.6)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let cover = egui::Rect::from_min_size(rect.min, vec2(COVER_W, CARD_H));
    let radius = egui::CornerRadius {
        nw: 6,
        sw: 6,
        ne: 0,
        se: 0,
    };
    match &card.cover {
        Some(path) => {
            ui.painter().rect_filled(cover, radius, p.field);
            paint_cover(ui, path, cover, radius);
        }
        None => {
            ui.painter().rect_filled(
                cover,
                radius,
                theme::mix(p.container, theme::character_color(&card.title), 0.35),
            );
            ui.painter().text(
                cover.center(),
                Align2::CENTER_CENTER,
                icon::FILM_SLATE,
                FontId::proportional(44.0),
                p.text,
            );
        }
    }

    let pad = 16.0;
    let text_w = CARD_W - COVER_W - 2.0 * pad;
    let mut y = rect.top() + pad;
    let bottom = rect.bottom() - 34.0;
    if let Some(g) = fitted_text(
        ui,
        &card.title,
        FontId::new(18.0, bold_family()),
        p.text,
        text_w,
        46.0,
    ) {
        let h = g.size().y;
        ui.painter()
            .galley(egui::pos2(cover.right() + pad, y), g, p.text);
        y += h + 8.0;
    }
    if let Some(g) = fitted_text(
        ui,
        &card.logline,
        FontId::proportional(13.0),
        p.dim,
        text_w,
        (bottom - y).max(0.0),
    ) {
        ui.painter()
            .galley(egui::pos2(cover.right() + pad, y), g, p.dim);
    }
    ui.painter().text(
        egui::pos2(cover.right() + pad, rect.bottom() - 14.0),
        Align2::LEFT_CENTER,
        ago(card.modified),
        FontId::proportional(12.0),
        p.dim,
    );
    ui.painter().text(
        egui::pos2(rect.right() - 14.0, rect.bottom() - 14.0),
        Align2::RIGHT_CENTER,
        icon::LAPTOP,
        FontId::proportional(17.0),
        p.dim,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Tarjeta de acción («Crear historia», «Abrir historia») con el mismo tamaño que las de historia.
fn action_card(ui: &mut Ui, p: &Palette, code: &str, label: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click());
    let hover = resp.hovered();
    ui.painter().rect(
        rect,
        6,
        if hover {
            theme::mix(p.container, p.primary, 0.12)
        } else {
            p.container
        },
        Stroke::new(
            1.0,
            if hover {
                p.accent
            } else {
                theme::mix(p.container, p.border, 0.6)
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.center().y - 14.0),
        Align2::CENTER_CENTER,
        code,
        FontId::proportional(44.0),
        p.accent,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.center().y + 34.0),
        Align2::CENTER_CENTER,
        label.to_uppercase(),
        FontId::new(14.0, bold_family()),
        p.accent,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Pantalla de inicio: una grilla con «Crear historia», «Abrir historia» y las historias recientes.
pub fn home(ui: &mut Ui, cards: &[RecentCard], p: &Palette) -> Option<HomeAction> {
    let mut action = None;
    // Arriba, el nombre de la app y el engranaje de Ajustes contra el borde derecho.
    crate::ui::kit::sides(
        ui,
        |ui| {
            ui.label(
                RichText::new("Scriptpendra")
                    .font(FontId::new(tokens::FONT_TITLE, bold_family()))
                    .color(p.text),
            );
        },
        |ui| {
            if crate::ui::kit::icon_button(ui, p, icon::GEAR, "Ajustes", true).clicked() {
                action = Some(HomeAction::Settings);
            }
        },
    );
    egui::ScrollArea::vertical()
        .id_salt("home_body")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // Se centra la grilla: tantas columnas como entren en el ancho disponible.
            let gap = tokens::SP_XL;
            let avail = ui.available_width();
            let cols = (((avail + gap) / (CARD_W + gap)).floor() as usize).max(1);
            let grid_w = cols as f32 * CARD_W + (cols - 1) as f32 * gap;
            let side = ((avail - grid_w) / 2.0).max(0.0);
            ui.add_space(tokens::SP_M);
            ui.horizontal_top(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(gap, gap);
                    let total = cards.len() + 2;
                    for row in 0..total.div_ceil(cols) {
                        ui.horizontal(|ui| {
                            for col in 0..cols {
                                let n = row * cols + col;
                                if n >= total {
                                    break;
                                }
                                match n {
                                    0 => {
                                        if action_card(ui, p, icon::PLUS, "Nuevo proyecto")
                                            .clicked()
                                        {
                                            action = Some(HomeAction::Create);
                                        }
                                    }
                                    1 => {
                                        if action_card(ui, p, icon::FOLDER_OPEN, "Abrir…").clicked()
                                        {
                                            action = Some(HomeAction::Open);
                                        }
                                    }
                                    _ => {
                                        let c = &cards[n - 2];
                                        if story_card(ui, p, c).clicked() {
                                            action = Some(HomeAction::OpenRecent(c.path.clone()));
                                        }
                                    }
                                }
                            }
                        });
                    }
                });
            });
            ui.add_space(tokens::SP_XL);
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_give_exact_heights() {
        assert_eq!(PORTRAIT.height_for(168.0), 210.0);
        assert_eq!(POSTER.height_for(200.0), 300.0);
        assert_eq!(PORTRAIT.label(), "4:5");
        assert!((COVER_W / CARD_H - 2.0 / 3.0).abs() < 1e-4);
    }
}
