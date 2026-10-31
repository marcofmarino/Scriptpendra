//! Formulario para crear (o convertir en) un personaje, locación, objeto, mundo o elemento sin salir del texto.
//! Lo usan el guion y el tratamiento: el formulario crea la ficha y quien lo abrió escribe el nombre en su texto.

use crate::ui::tokens;
use eframe::egui::{self, FontId, Id, RichText};

use crate::{
    editor::format_mention,
    model::{DocKind, EntityRef, Link, Mention, Project, adjust_links, validate_links},
    theme::{self, Palette},
    widgets,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Character,
    Location,
    Object,
    World,
    Element,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Character,
        Kind::Location,
        Kind::Object,
        Kind::World,
        Kind::Element,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Character => "Personaje",
            Kind::Location => "Locación",
            Kind::Object => "Objeto",
            Kind::World => "Mundo",
            Kind::Element => "Elemento",
        }
    }
}

/// Formulario para crear algo del proyecto sin salir del tratamiento.
#[derive(Default)]
pub struct NewEntity {
    pub kind: Kind,
    pub name: String,
    pub age: String,
    pub description: String,
    pub category: Option<u64>,
    /// Recién abierto: el cursor arranca en el nombre (una sola vez).
    pub fresh: bool,
    /// Texto seleccionado que se convierte en lo creado.
    pub replace: Option<(usize, usize)>,
    /// Párrafo del guion al que pertenece `replace`.
    pub para: Option<u64>,
}

pub fn entity_name(project: &Project, target: EntityRef) -> Option<String> {
    match target {
        EntityRef::Character(id) => project
            .characters
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone()),
        EntityRef::Location(id) => project
            .location_profiles
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.name.clone()),
        EntityRef::Object(id) => project
            .objects
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.name.clone()),
        EntityRef::Element(id) => project
            .elements
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.name.clone()),
        EntityRef::World(id) => project
            .documents
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone()),
    }
}

/// Crea lo que pide el formulario y devuelve a qué apunta.
pub fn create_entity(project: &mut Project, form: &NewEntity) -> Option<EntityRef> {
    let name = form.name.trim();
    if name.is_empty() {
        return None;
    }
    let about = form.description.trim().to_string();
    Some(match form.kind {
        Kind::Character => {
            let id = project.new_character(name);
            if let Some(c) = project.characters.iter_mut().find(|c| c.id == id) {
                c.age = form.age.trim().to_string();
                c.one_liner = about;
            }
            EntityRef::Character(id)
        }
        Kind::Location => {
            let id = project.add_location(name);
            if let Some(l) = project.location_profiles.iter_mut().find(|l| l.id == id) {
                l.one_liner = about;
            }
            EntityRef::Location(id)
        }
        Kind::Object => {
            let id = project.add_object(name);
            if let Some(o) = project.objects.iter_mut().find(|o| o.id == id) {
                o.one_liner = about;
            }
            EntityRef::Object(id)
        }
        Kind::World => {
            let id = project.add_document(DocKind::World, None);
            if let Some(d) = project.documents.iter_mut().find(|d| d.id == id) {
                d.name = name.to_string();
                d.description = about;
            }
            EntityRef::World(id)
        }
        Kind::Element => {
            let category = form
                .category
                .or_else(|| project.element_categories.first().map(|c| c.id))?;
            let id = project.add_element(category, name)?;
            if let Some(e) = project.elements.iter_mut().find(|e| e.id == id) {
                e.one_liner = about;
            }
            EntityRef::Element(id)
        }
    })
}

/// Formulario modal para crear algo del proyecto. Devuelve el formulario cuando se aprieta «Crear e insertar»; quien
/// llama escribe el nombre en su texto (en lugar de la selección, si la había).
pub fn dialog(
    ctx: &egui::Context,
    entity: &mut Option<NewEntity>,
    project: &mut Project,
    p: &Palette,
) -> Option<NewEntity> {
    let form = entity.as_mut()?;
    let kinds: Vec<Kind> = Kind::ALL
        .iter()
        .copied()
        .filter(|k| *k != Kind::Element || !project.element_categories.is_empty())
        .collect();
    let (mut create, mut cancel) = (false, false);
    let modal = egui::Modal::new(Id::unique("new_entity_dialog")).show(ctx, |ui| {
        ui.set_max_width(tokens::DIALOG_MAX);
        ui.label(RichText::new("Crear e insertar").font(FontId::new(19.0, theme::bold_family())));
        ui.add_space(tokens::SP_M);
        let labels: Vec<&str> = kinds.iter().map(|k| k.label()).collect();
        let active = kinds.iter().position(|k| *k == form.kind).unwrap_or(0);
        if let Some(i) = widgets::tab_row(ui, p, &labels, active) {
            form.kind = kinds[i];
        }
        ui.add_space(tokens::SP_M);
        widgets::field_with(ui, p, "Nombre", &mut form.name, 1, form.fresh);
        form.fresh = false;
        if form.kind == Kind::Character {
            widgets::field(ui, p, "Edad", &mut form.age, 1);
        }
        if form.kind == Kind::Element {
            let current = form
                .category
                .or_else(|| project.element_categories.first().map(|c| c.id));
            let name = |id: Option<u64>| {
                id.and_then(|id| project.element_categories.iter().find(|c| c.id == id))
                    .map_or(String::new(), |c| c.name.clone())
            };
            ui.label(RichText::new("Categoría").size(12.0).color(p.dim));
            egui::ComboBox::from_id_salt("treatment_entity_category")
                .width(crate::ui::kit::fill_width(ui, 60.0, 1000.0))
                .selected_text(name(current))
                .show_ui(ui, |ui| {
                    for c in &project.element_categories {
                        if ui
                            .selectable_label(current == Some(c.id), &c.name)
                            .clicked()
                        {
                            form.category = Some(c.id);
                        }
                    }
                });
            ui.add_space(tokens::SP_M);
        }
        widgets::field(ui, p, "Descripción", &mut form.description, 3);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !form.name.trim().is_empty(),
                    egui::Button::new(RichText::new("Crear e insertar").color(p.accent)),
                )
                .clicked()
            {
                create = true;
            }
            if ui.button("Cancelar").clicked() {
                cancel = true;
            }
        });
    });
    if modal.should_close() {
        cancel = true;
    }
    if create {
        return entity.take();
    }
    if cancel {
        *entity = None;
    }
    None
}

/// Escribe el nombre de lo creado en un texto con vínculos: en lugar del rango `range` (lo seleccionado) o, si está
/// vacío, en esa posición. Devuelve dónde queda el cursor.
pub fn splice(
    text: &mut String,
    links: &mut Vec<Link>,
    project: &Project,
    target: EntityRef,
    range: (usize, usize),
    typed: &str,
) -> usize {
    let mention = Mention {
        name: entity_name(project, target).unwrap_or_default(),
        character: matches!(target, EntityRef::Character(_)),
        target: Some(target),
        detail: String::new(),
    };
    let name = format_mention(&mention, typed, false);
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = (
        range.0.min(chars.len()),
        range.1.min(chars.len()).max(range.0.min(chars.len())),
    );
    let lead = if a > 0 && chars[a - 1].is_alphanumeric() {
        " "
    } else {
        ""
    };
    let tail = if chars.get(b).is_some_and(|c| c.is_alphanumeric()) {
        " "
    } else {
        ""
    };
    let start = a + lead.chars().count();
    let end = start + name.chars().count();
    let old = std::mem::take(text);
    *text = chars[..a].iter().collect::<String>()
        + lead
        + &name
        + tail
        + &chars[b..].iter().collect::<String>();
    adjust_links(links, &old, text);
    links.retain(|l| !(l.start < end && l.end > start));
    links.push(Link { start, end, target });
    links.sort_by_key(|l| l.start);
    validate_links(links, text);
    end
}
