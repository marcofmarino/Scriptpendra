//! El contrato de una pantalla (reglas-interfaz.md §1.2): declara su tipo y entrega lo que va en cada zona del
//! marco; el shell las dibuja. Las pantallas se pasan a este contrato de a una.

#![allow(dead_code)]

use eframe::egui::Ui;

use crate::{model::Project, storage::Settings, theme::Palette};

/// Los seis tipos de pantalla de interfaz.md §2 (el plan de rodaje es una variante de `Table`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScreenKind {
    /// A. Hojas de papel con su formato real.
    Sheet,
    /// B. Vista previa del documento impreso más un formulario.
    Preview,
    /// C. Tabla con encabezado fijo más inspector.
    Table,
    /// D. Lienzo con zoom y paneo más inspector.
    Canvas,
    /// E. Una columna centrada de tarjetas.
    Card,
    /// F. Grilla de tarjetas de ancho fijo.
    Grid,
}

/// Acciones destructivas que piden confirmación (las atiende la app con `kit::confirm`).
pub enum Confirm {
    ResetCharacter(u64, String),
    DeleteShot(u64),
    DeleteBlocking(u64, String, usize, usize),
    DeleteShotColumn(u64, String, usize),
    DeleteShootLocation(u64, String, usize),
    DeleteCharacter(u64, String, usize),
    /// Reemplazar el guion por el texto Fountain ya leído (cuántos párrafos tiene el guion actual).
    ImportFountain(String, usize),
    DeleteElementCategory(u64, String, usize),
    /// Quitar un campo o una sección de la ficha de una categoría que ya tiene contenido.
    DeleteTemplatePart {
        category: u64,
        section: usize,
        field: Option<usize>,
        label: String,
        count: usize,
    },
}

/// Cosas que una pantalla le pide a la app y que tocan más que el proyecto.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Se renombró algo que el guion nombra: hay que reescribir los vínculos.
    AfterRename,
    /// Eliminar un personaje (y sus intervenciones) ya confirmado o sin intervenciones.
    DeleteCharacter(u64),
    /// Exportar la ficha de un personaje a SVG.
    ExportCharacter(u64),
    /// Volver a la información del proyecto (se eliminó lo que se estaba viendo).
    GoInfo,
    /// Ir a esta pantalla (por clave) tras borrar algo.
    GoPage(String),
    /// Cerrar los Ajustes y volver a la pantalla anterior.
    CloseSettings,
    /// Traer al guion los encabezados de la escaleta del tratamiento.
    ImportOutline,
    /// Abrir el diálogo Exportar.
    OpenExport,
    /// Exportar el guion (con su página de título) a PDF.
    ExportPdf,
    /// Exportar solo el guion a PDF, sin la página de título.
    ExportScriptPdf,
}

/// A dónde pide ir una pantalla. Los vínculos llevan a su pantalla con el elemento seleccionado (interfaz.md §8).
#[derive(Clone, Debug, PartialEq)]
pub enum Nav {
    /// Una pantalla por su clave (`Page::key`).
    Page(String),
    /// El guion, en esa escena.
    Script(u64),
    /// Las plantas, en esa escena.
    Plan(u64),
    /// Las plantas, en una cámara de un bloqueo: (bloqueo, cámara).
    Camera(u64, u64),
    /// El desglose de esa escena.
    Breakdown(u64),
    /// El guion técnico, filtrado a esa escena.
    Shots(u64),
}

/// Lo que recibe una pantalla en cada zona (reglas §1.2). Los pedidos hacia afuera (navegar, mensajes, confirmar) se
/// dejan en sus campos y la app los atiende al terminar el cuadro.
pub struct ScreenCx<'a> {
    pub project: &'a mut Project,
    pub palette: &'a Palette,
    pub settings: &'a Settings,
    /// La escena actual, compartida entre pantallas: Guion, Guion técnico, Plantas, Escenas y Desglose la leen y la cambian.
    pub current_scene: &'a mut Option<u64>,
    /// Pantallas visitadas (claves de `Page::key`), de la más vieja a la actual.
    pub history: &'a [String],
    /// Pantalla a la que se pidió ir.
    pub navigate: Option<Nav>,
    /// Mensaje breve para la barra de estado.
    pub status: Option<String>,
    /// Marca de cambios sin guardar.
    pub dirty: bool,
    /// Acción destructiva que hay que confirmar.
    pub confirm: Option<Confirm>,
    /// Pedido de abrir el inspector (por ejemplo, «Comentar» abre la pestaña de comentarios).
    pub open_inspector: bool,
    /// Pedido de cambiar de pestaña del inspector.
    pub inspector_tab: Option<usize>,
    pub effects: Vec<Effect>,
    /// Ajustes de la aplicación modificados: la app los aplica (tema, zoom) y los guarda.
    pub new_settings: Option<Settings>,
}

impl<'a> ScreenCx<'a> {
    pub fn new(
        project: &'a mut Project,
        palette: &'a Palette,
        settings: &'a Settings,
        current_scene: &'a mut Option<u64>,
        history: &'a [String],
    ) -> Self {
        Self {
            project,
            palette,
            settings,
            current_scene,
            history,
            navigate: None,
            status: None,
            dirty: false,
            confirm: None,
            open_inspector: false,
            inspector_tab: None,
            effects: Vec::new(),
            new_settings: None,
        }
    }
}

/// Qué zonas del marco dibuja una pantalla (Inicio va sin el marco del proyecto).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Framing {
    Project,
    Bare,
}

pub trait Screen {
    fn kind(&self) -> ScreenKind;
    fn title(&self, cx: &ScreenCx) -> String;
    /// Solo si el título es un selector (Plantas): lo dibuja en la fila del título y devuelve `true`; si no, el marco dibuja
    /// el texto de `title`.
    fn title_ui(&mut self, _ui: &mut Ui, _cx: &mut ScreenCx) -> bool {
        false
    }
    /// Centro de la barra de herramientas. Los grupos que no entran se pliegan con `kit::toolbar_groups`.
    fn toolbar(&mut self, _ui: &mut Ui, _cx: &mut ScreenCx) {}
    /// Zona derecha de la barra de herramientas, contra el borde derecho de la ventana: ver y sacar (zoom, Exportar).
    fn toolbar_end(&mut self, _ui: &mut Ui, _cx: &mut ScreenCx) {}
    /// Deshacer y rehacer: los botones los dibuja el marco, la pantalla dice si se puede y lo hace.
    fn can_undo(&self, _cx: &ScreenCx) -> bool {
        false
    }
    fn can_redo(&self, _cx: &ScreenCx) -> bool {
        false
    }
    fn undo(&mut self, _cx: &mut ScreenCx) {}
    fn redo(&mut self, _cx: &mut ScreenCx) {}
    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        false
    }
    fn context_bar(&mut self, _ui: &mut Ui, _cx: &mut ScreenCx) {}
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx);
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &[]
    }
    fn inspector(&mut self, _ui: &mut Ui, _tab: usize, _cx: &mut ScreenCx) {}
    /// Acciones del pie fijo del inspector.
    fn inspector_footer(&mut self, _ui: &mut Ui, _tab: usize, _cx: &mut ScreenCx) {}
    /// ¿Tiene pie fijo en esta pestaña?
    fn has_inspector_footer(&self, _tab: usize, _cx: &ScreenCx) -> bool {
        false
    }
    /// Datos de la derecha de la barra de estado.
    fn status(&self, _cx: &ScreenCx) -> Vec<String> {
        Vec::new()
    }
    /// Se llama antes de dibujar: la pantalla toma lo que cambió afuera (por ejemplo, la escena actual).
    fn enter(&mut self, _cx: &mut ScreenCx) {}
}

/// La clave de pantalla (`Page::key`) de la ficha de algo del proyecto.
pub fn entity_page_key(target: crate::model::EntityRef) -> String {
    use crate::model::EntityRef::*;
    match target {
        Character(id) => format!("character:{id}"),
        Location(id) => format!("location:{id}"),
        Object(id) => format!("object:{id}"),
        Element(id) => format!("element:{id}"),
        World(id) => format!("document:{id}"),
    }
}

/// Lista de personajes, locaciones y elementos nombrados en un texto (sus vínculos): un clic lleva a la ficha. Es el
/// contenido de la pestaña VINCULADOS de Sinopsis y Tratamiento.
pub fn linked_entities(
    ui: &mut Ui,
    p: &Palette,
    project: &Project,
    links: &[crate::model::Link],
) -> (Option<Nav>, bool) {
    use crate::{icon, model::EntityRef, ui::kit};
    let mut seen: Vec<EntityRef> = Vec::new();
    for l in links {
        if !seen.contains(&l.target) {
            seen.push(l.target);
        }
    }
    if seen.is_empty() {
        // Icono, frase y acción, en ese orden (interfaz.md §3.3).
        let create = kit::empty_state(
            ui,
            p,
            icon::USERS_THREE,
            "Todavía no hay nombres vinculados. Los personajes, locaciones y elementos que nombres en el texto aparecen acá.",
            Some("Crear personaje, locación…"),
        );
        return (None, create);
    }
    let mut nav = None;
    for target in seen {
        let (code, label) = match target {
            EntityRef::Character(_) => (icon::USER, project.label_for(target)),
            EntityRef::Location(_) => (icon::HOUSE, project.label_for(target)),
            EntityRef::Object(_) => (icon::CUBE, project.label_for(target)),
            EntityRef::Element(_) => (icon::BOOKS, project.label_for(target)),
            EntityRef::World(_) => (icon::GLOBE, project.label_for(target)),
        };
        if kit::tree_row(
            ui,
            p,
            kit::Row {
                depth: 0,
                caret: None,
                icon: Some(code),
                label: &label,
                selected: false,
            },
        )
        .clicked()
        {
            nav = Some(Nav::Page(entity_page_key(target)));
        }
    }
    (nav, false)
}
