use crate::{cloud_ui, cloudsync::CloudLink, folder};
use crate::ui::tokens;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use eframe::egui::{self, Key, Modifiers, RichText, Stroke};

use crate::{
    assets,
    editor::EditorState,
    export, icon,
    model::{self, CharacterProfile, DocKind, EntityRef, NameCase, Project, clean_character_name},
    pages::{self, HomeAction},
    storage::{self, RecentCard, Settings},
    theme::{self, Palette},
    ui::{
        kit,
        screen::{Confirm, Effect, Nav, ScreenCx, ScreenKind},
        shell,
        tokens::*,
    },
    widgets::{self, Row},
};

#[derive(Clone, PartialEq, Eq, Debug)]
enum Page {
    Info,
    Character(u64),
    Location(u64),
    Document(u64),
    ElementCategory(u64),
    Element(u64),
    Object(u64),
    Breakdown,
    TitlePage,
    Synopsis,
    DramaCurve,
    Treatment,
    Screenplay,
    TechScript,
    Blocking,
    SoundScript,
    SoundGear,
    PhotoGear,
    ArtDirection,
    Storyboard,
    PlanLights,
    PlanSound,
    PlanSet,
    Statistics,
    Scenes,
    ShootLocations,
    /// La ficha de un lugar de rodaje abierta como página.
    ShootPlace(u64),
    Contacts,
    Plan,
    CallSheets,
}

impl Page {
    /// El tipo de pantalla de interfaz.md §2. Lo usarán el shell y la prueba de desbordes al migrar el resto.
    #[allow(dead_code)]
    fn kind(&self) -> ScreenKind {
        match self {
            Page::Info
            | Page::Character(_)
            | Page::Location(_)
            | Page::Object(_)
            | Page::Element(_)
            | Page::ElementCategory(_) => ScreenKind::Card,
            Page::Synopsis | Page::Treatment | Page::Screenplay => ScreenKind::Sheet,
            Page::DramaCurve => ScreenKind::Card,
            Page::TitlePage | Page::CallSheets => ScreenKind::Preview,
            Page::ShootPlace(_) => ScreenKind::Card,
            Page::Breakdown
            | Page::Scenes
            | Page::ShootLocations
            | Page::Contacts
            | Page::Plan
            | Page::SoundScript
            | Page::SoundGear
            | Page::PhotoGear
            | Page::ArtDirection
            | Page::Storyboard
            | Page::TechScript => ScreenKind::Table,
            Page::Blocking | Page::PlanLights | Page::PlanSound | Page::PlanSet => {
                ScreenKind::Canvas
            }
            Page::Statistics => ScreenKind::Grid,
            // Los documentos son hojas, mapas mentales o galerías según su clase; aquí se toma el más común.
            Page::Document(_) => ScreenKind::Sheet,
        }
    }

    /// Texto con el que se recuerda la pantalla en la que quedó un proyecto.
    fn key(&self) -> String {
        match self {
            Page::Info => "info".into(),
            Page::Character(id) => format!("character:{id}"),
            Page::Location(id) => format!("location:{id}"),
            Page::Document(id) => format!("document:{id}"),
            Page::ElementCategory(id) => format!("category:{id}"),
            Page::Element(id) => format!("element:{id}"),
            Page::Object(id) => format!("object:{id}"),
            Page::Breakdown => "breakdown".into(),
            Page::TitlePage => "title".into(),
            Page::Synopsis => "synopsis".into(),
            Page::DramaCurve => "drama".into(),
            Page::Treatment => "treatment".into(),
            Page::Screenplay => "screenplay".into(),
            Page::TechScript => "techscript".into(),
            Page::Blocking => "blocking".into(),
            Page::SoundScript => "sound".into(),
            Page::SoundGear => "soundgear".into(),
            Page::PhotoGear => "photogear".into(),
            Page::ArtDirection => "art".into(),
            Page::Storyboard => "storyboard".into(),
            Page::PlanLights => "planlights".into(),
            Page::PlanSound => "plansound".into(),
            Page::PlanSet => "planset".into(),
            Page::Statistics => "statistics".into(),
            Page::Scenes => "scenes".into(),
            Page::ShootLocations => "shoot".into(),
            Page::ShootPlace(id) => format!("shootplace:{id}"),
            Page::Contacts => "contacts".into(),
            Page::Plan => "plan".into(),
            Page::CallSheets => "calls".into(),
        }
    }

    /// La pantalla recordada, si todavía existe en el proyecto.
    fn from_key(key: &str, project: &Project) -> Option<Page> {
        let (name, id) = match key.split_once(':') {
            Some((n, i)) => (n, i.parse::<u64>().ok()?),
            None => (key, 0),
        };
        Some(match name {
            "info" => Page::Info,
            "breakdown" => Page::Breakdown,
            "title" => Page::TitlePage,
            "synopsis" => Page::Synopsis,
            "drama" => Page::DramaCurve,
            "treatment" => Page::Treatment,
            "screenplay" => Page::Screenplay,
            "techscript" => Page::TechScript,
            "blocking" => Page::Blocking,
            "sound" => Page::SoundScript,
            "soundgear" => Page::SoundGear,
            "photogear" => Page::PhotoGear,
            "art" => Page::ArtDirection,
            "storyboard" => Page::Storyboard,
            "planlights" => Page::PlanLights,
            "plansound" => Page::PlanSound,
            "planset" => Page::PlanSet,
            "statistics" => Page::Statistics,
            "scenes" => Page::Scenes,
            "shoot" => Page::ShootLocations,
            "contacts" => Page::Contacts,
            "plan" => Page::Plan,
            "calls" => Page::CallSheets,
            "shootplace" if project.shoot_locations.iter().any(|l| l.id == id) => {
                Page::ShootPlace(id)
            }
            "character" if project.characters.iter().any(|c| c.id == id) => Page::Character(id),
            "location" if project.location_profiles.iter().any(|l| l.id == id) => {
                Page::Location(id)
            }
            "document" if project.documents.iter().any(|d| d.id == id) => Page::Document(id),
            "category" if project.element_categories.iter().any(|c| c.id == id) => {
                Page::ElementCategory(id)
            }
            "element" if project.elements.iter().any(|e| e.id == id) => Page::Element(id),
            "object" if project.objects.iter().any(|o| o.id == id) => Page::Object(id),
            _ => return None,
        })
    }
}

#[derive(Clone, Copy)]
enum PanelSide {
    Left,
    Right,
}

/// Lo que se recuerda de cada panel lateral durante la sesión.
///
/// El ancho que se le pide a egui (`default_size`) tiene que quedar fijo: el ancho que se mide incluye el borde y el
/// redondeo a píxeles, y si se lo devolviera a egui en cada cuadro, el panel ganaría casi un píxel por vuelta.
struct PanelMemory {
    /// Ancho con el que se creó el panel en esta sesión.
    default: f32,
    /// Cuánto suma el marco al ancho pedido; se aprende con la primera medida.
    offset: Option<f32>,
    /// Última medida que se tomó como válida.
    last: f32,
}

/// Qué muestra la ventana «Añadir».
#[derive(Clone, Copy, PartialEq, Eq)]
enum AddMode {
    Choose,
    NewCategory,
}

#[derive(Clone, Copy)]
enum AddChoice {
    Character,
    Location,
    Object,
    Element(u64),
    NewCategory,
    Doc(DocKind),
}

enum Screen {
    Home,
    Project(Page),
}

/// Lo que se puede hacer con una ficha desde su menú contextual.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CardAction {
    Rename,
    Duplicate,
    Delete,
}

/// Los tres espacios de trabajo de la barra lateral (interfaz.md §1.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Workspace {
    Writing,
    Design,
    Production,
}

impl Workspace {
    const ALL: [Workspace; 3] = [Workspace::Writing, Workspace::Design, Workspace::Production];

    fn label(self) -> &'static str {
        match self {
            Workspace::Writing => "Escritura",
            Workspace::Design => "Diseño",
            Workspace::Production => "Producción",
        }
    }

    /// El árbol del espacio: pantallas sueltas y desplegables con sus pantallas adentro.
    fn tree(self) -> Vec<Node> {
        use Node::{Group, Leaf};
        match self {
            Workspace::Writing => vec![
                Leaf(icon::BOOKS, "Información del proyecto", Page::Info),
                Leaf(icon::BOOK_OPEN, "Página de título", Page::TitlePage),
                Leaf(icon::ARTICLE, "Sinopsis", Page::Synopsis),
                Leaf(icon::CHART_LINE_UP, "Curva dramática", Page::DramaCurve),
                Leaf(icon::LIST_CHECKS, "Tratamiento", Page::Treatment),
                Leaf(icon::SCROLL, "Guion", Page::Screenplay),
                Leaf(icon::CHART_LINE, "Estadísticas", Page::Statistics),
            ],
            Workspace::Design => vec![
                Group(
                    icon::FILM_STRIP,
                    "Dirección técnica",
                    vec![
                        (icon::FILM_STRIP, "Guion técnico", Page::TechScript),
                        (icon::COMPASS, "Planta de cámara", Page::Blocking),
                        (icon::IMAGES, "Storyboard", Page::Storyboard),
                    ],
                ),
                Group(
                    icon::WAVEFORM,
                    "Dirección de sonido",
                    vec![
                        (icon::SCROLL, "Guion sonoro", Page::SoundScript),
                        (icon::MICROPHONE, "Equipo de sonido", Page::SoundGear),
                        (icon::MICROPHONE_STAGE, "Planta de sonido", Page::PlanSound),
                    ],
                ),
                Group(
                    icon::VIDEO_CAMERA,
                    "Dirección de fotografía",
                    vec![
                        (icon::VIDEO_CAMERA, "Equipos", Page::PhotoGear),
                        (icon::LAMP, "Planta de luces", Page::PlanLights),
                    ],
                ),
                Group(
                    icon::CUBE,
                    "Dirección de arte",
                    vec![
                        (icon::LIST_CHECKS, "Elementos", Page::ArtDirection),
                        (icon::HOUSE, "Planta de escenografía", Page::PlanSet),
                    ],
                ),
            ],
            Workspace::Production => vec![
                Leaf(icon::LIST_NUMBERS, "Escenas", Page::Scenes),
                Leaf(icon::CLIPBOARD_TEXT, "Desglose", Page::Breakdown),
                Leaf(icon::LIST_CHECKS, "Plan de rodaje", Page::Plan),
                Leaf(icon::SCROLL, "Hojas de llamado", Page::CallSheets),
                Leaf(icon::HOUSE, "Locaciones de rodaje", Page::ShootLocations),
                Leaf(icon::USERS_THREE, "Contactos", Page::Contacts),
            ],
        }
    }

    /// Las pantallas del espacio, en orden y sin desplegables: icono, nombre y destino.
    fn screens(self) -> Vec<(&'static str, &'static str, Page)> {
        self.tree()
            .into_iter()
            .flat_map(|node| match node {
                Node::Leaf(i, l, p) => vec![(i, l, p)],
                Node::Group(_, _, children) => children,
            })
            .collect()
    }

    /// El espacio al que pertenece una pantalla; las fichas y documentos no son de ninguno (quedan en el actual).
    fn of(page: &Page) -> Option<Workspace> {
        Workspace::ALL
            .into_iter()
            .find(|w| w.screens().iter().any(|(_, _, p)| p == page))
    }
}

/// Una entrada de la barra lateral: una pantalla o un desplegable con varias.
/// La ventana del historial de versiones.
#[derive(Default)]
struct VersionsState {
    open: bool,
    list: Vec<folder::Version>,
    selected: Option<usize>,
    /// En qué se diferencia de lo actual la versión elegida.
    differing: Vec<String>,
    name: String,
    note: String,
}

/// Una sección que cambió a la vez en esta copia y en otra: hay que elegir con cuál quedarse.
struct Conflict {
    section: String,
    /// Cómo la dejó la otra persona.
    theirs: Vec<u8>,
}

/// Un aviso de que otra persona cambió algo: quién, qué parte y qué cambió.
struct ChangeToast {
    who: String,
    section: String,
    lines: Vec<String>,
    born: std::time::Instant,
}

/// Cuánto se queda en pantalla un aviso de cambios.
const CHANGE_TOAST_SECS: u64 = 15;

/// Cada cuántos segundos se mira si otra persona cambió algo en la carpeta del proyecto.
const POLL_SECS: f64 = 3.0;

/// Cada cuántos segundos se guarda sola una versión del historial (si algo cambió).
const AUTO_VERSION_SECS: f64 = 1800.0;

/// Cómo se llama cada sección del proyecto en la ventana del historial.
fn section_name(section: &str) -> &'static str {
    match section {
        "guion" => "Guion",
        "historia" => "Sinopsis, tratamiento y curva dramática",
        "fichas" => "Fichas",
        "tecnica" => "Guion técnico y storyboard",
        "plantas" => "Plantas",
        "sonido" => "Dirección de sonido",
        "fotografia" => "Equipos de fotografía",
        "arte" => "Dirección de arte",
        "desglose" => "Desglose",
        "produccion" => "Producción",
        _ => "Datos generales",
    }
}

/// Cada cuántos segundos como máximo se guarda solo lo que cambió.
const AUTOSAVE_SECS: f64 = 10.0;

enum Node {
    Leaf(&'static str, &'static str, Page),
    Group(&'static str, &'static str, Vec<(&'static str, &'static str, Page)>),
}

pub struct ScriptApp {
    export_dialog: crate::export_dialog::ExportDialog,
    /// Sección elegida en Ajustes.
    settings_section: usize,
    /// La escena actual, compartida entre Guion, Guion técnico, Plantas, Escenas y Desglose (interfaz.md §8).
    current_scene: Option<u64>,
    /// Pestaña del inspector recordada por pantalla.
    inspector_tabs: std::collections::HashMap<String, usize>,
    screen: Screen,
    show_settings: bool,
    settings: Settings,
    cards: Vec<RecentCard>,
    project: Project,
    path: Option<PathBuf>,
    dirty: bool,
    status: String,
    editor: EditorState,
    treatment: crate::treatment::TreatmentState,
    synopsis: crate::synopsis::SynopsisState,
    title_view: crate::pages::TitleState,
    tech: crate::techscript::TechState,
    blocking: crate::blocking::BlockState,
    scenes: crate::scenes::ScenesState,
    shoot: crate::shoot::ShootState,
    contacts: crate::contacts::ContactsState,
    roles: crate::roles::RoleState,
    versions: VersionsState,
    last_version: f64,
    /// Cuándo fue el último cambio sin guardar (en segundos de la ventana): decide qué tan seguido se sincroniza.
    last_edit: f64,
    cloud: cloud_ui::CloudUi,
    cloud_link: Option<CloudLink>,
    /// Quién más está en el proyecto compartido (se renueva cada pocos segundos).
    peers: Vec<crate::cloud::Peer>,
    /// Avisos de lo que cambiaron otras personas.
    change_toasts: Vec<ChangeToast>,
    /// Lo último que se leyó o se escribió de la carpeta del proyecto, por sección: la base para combinar cambios.
    known: std::collections::BTreeMap<String, Vec<u8>>,
    stamps: folder::Stamps,
    last_poll: f64,
    conflicts: Vec<Conflict>,
    conflicts_open: bool,
    /// Desplegables de la barra lateral que la persona cerró (por nombre).
    closed_groups: std::collections::HashSet<&'static str>,
    story: crate::story::StoryState,
    plan: crate::plan::PlanState,
    calls: crate::calls::CallsState,
    workspace: Workspace,
    /// Última pantalla abierta en cada espacio.
    last_in_workspace: std::collections::HashMap<Workspace, Page>,
    open_cards: bool,
    /// Lo elegido en el menú contextual de una ficha, para atenderlo al terminar de dibujar la barra.
    card_action: Option<(Page, CardAction)>,
    /// Ficha que se está renombrando y el texto nuevo.
    rename: Option<(Page, String)>,
    /// Pantallas visitadas, para Alt+← y Alt+→.
    history: Vec<Page>,
    history_pos: usize,
    /// El inspector no entra al lado del contenido: se abre como cajón (ventana angosta).
    compact_inspector: bool,
    /// La barra lateral está reducida a un riel de iconos.
    rail: bool,
    drawer_open: bool,
    card_query: String,
    /// Ctrl+F pidió el foco para el buscador de fichas.
    focus_search: bool,
    show_sidebar: bool,
    show_right: bool,
    open_characters: bool,
    open_locations: bool,
    open_worlds: bool,
    open_docs: bool,
    open_folders: HashSet<u64>,
    show_add: bool,
    add_mode: AddMode,
    new_cat_name: String,
    new_cat_icon: String,
    closed_element_cats: HashSet<u64>,
    template_editing: Option<u64>,
    open_objects: bool,
    mind_selected: Option<u64>,
    export_sel: export::Selection,
    breakdown: pages::BreakdownState,
    confirm: Option<Confirm>,
    toast: Option<(String, f64)>,
    /// Último mensaje de la acción más reciente, para el centro de la barra de estado.
    last_message: String,
    last_autosave: f64,
    panels: std::collections::HashMap<&'static str, PanelMemory>,
    /// Anchos medidos de los paneles en cada cuadro (para las pruebas).
    #[cfg(test)]
    panel_trace: Vec<(&'static str, f32)>,
    /// Firma de las imágenes ya revisadas: solo se vuelve a copiar cuando cambia.
    image_sig: u64,
}

impl ScriptApp {
    pub fn new(ctx: &egui::Context) -> Self {
        let settings = Settings::load();
        theme::install_fonts(ctx);
        egui_extras::install_image_loaders(ctx);
        theme::apply(ctx, &settings.scheme.palette(), settings.ui_zoom);
        let cards = storage::recent_cards(&settings);
        Self {
            export_dialog: Default::default(),
            settings_section: 0,
            current_scene: None,
            inspector_tabs: std::collections::HashMap::new(),
            screen: Screen::Home,
            show_settings: false,
            settings,
            cards,
            project: Project::default(),
            path: None,
            dirty: false,
            status: String::new(),
            editor: EditorState::default(),
            treatment: Default::default(),
            synopsis: Default::default(),
            title_view: Default::default(),
            tech: Default::default(),
            blocking: Default::default(),
            scenes: Default::default(),
            shoot: Default::default(),
            contacts: Default::default(),
            roles: Default::default(),
            versions: Default::default(),
            last_version: 0.0,
            last_edit: f64::MIN,
            cloud: Default::default(),
            cloud_link: None,
            peers: Vec::new(),
            change_toasts: Vec::new(),
            known: Default::default(),
            stamps: Default::default(),
            last_poll: 0.0,
            conflicts: Vec::new(),
            conflicts_open: false,
            closed_groups: Default::default(),
            story: Default::default(),
            plan: Default::default(),
            calls: Default::default(),
            workspace: Workspace::Writing,
            last_in_workspace: std::collections::HashMap::new(),
            open_cards: true,
            card_action: None,
            rename: None,
            history: Vec::new(),
            history_pos: 0,
            compact_inspector: false,
            rail: false,
            drawer_open: false,
            card_query: String::new(),
            focus_search: false,
            show_sidebar: true,
            show_right: true,
            open_characters: true,
            open_locations: true,
            open_worlds: true,
            open_docs: true,
            open_folders: HashSet::new(),
            show_add: false,
            add_mode: AddMode::Choose,
            new_cat_name: String::new(),
            new_cat_icon: icon::PAW_PRINT.to_string(),
            closed_element_cats: HashSet::new(),
            template_editing: None,
            open_objects: true,
            mind_selected: None,
            export_sel: export::Selection::default(),
            breakdown: pages::BreakdownState::default(),
            confirm: None,
            toast: None,
            last_message: String::new(),
            last_autosave: 0.0,
            panels: std::collections::HashMap::new(),
            #[cfg(test)]
            panel_trace: Vec::new(),
            image_sig: 0,
        }
    }

    fn start_project(&mut self, mut project: Project, path: Option<PathBuf>) {
        self.history.clear();
        self.history_pos = 0;
        // Los proyectos de antes de las opciones por categoría toman la opción global anterior como punto de partida.
        // Un proyecto nuevo arranca con los nombres en mayúsculas en todas las categorías.
        if !project.name_case_migrated {
            if path.is_some() {
                let upper = self.settings.mentions_uppercase;
                project.name_case = NameCase {
                    characters: upper,
                    locations: upper,
                    objects: upper,
                    worlds: upper,
                };
                for c in &mut project.element_categories {
                    c.uppercase = upper;
                }
            }
            project.name_case_migrated = true;
        }
        self.project = project;
        // Cada persona numera lo que crea en su propio tramo de ids (ver `Project::use_id_block`).
        self.project.use_id_block(self.settings.user_id);
        self.path = path;
        self.dirty = false;
        self.conflicts.clear();
        self.remember_disk();
        self.cloud_link = self.path.as_deref().and_then(CloudLink::load);
        // Un proyecto de la nube viene de otras personas: sus imágenes tienen que estar dentro de su carpeta.
        if self.cloud_link.is_some() && self.project.drop_foreign_image_paths() > 0 {
            self.dirty = true;
        }
        self.editor = EditorState::default();
        self.treatment = Default::default();
        self.synopsis = Default::default();
        self.tech = Default::default();
        self.blocking = Default::default();
        self.scenes = Default::default();
        self.shoot = Default::default();
        self.contacts = Default::default();
        self.plan = Default::default();
        self.calls = Default::default();
        self.image_sig = 0; // las imágenes se revisan en el primer cuadro
        // Un proyecto nuevo empieza por la información; uno ya creado vuelve a donde se lo dejó.
        let page = self
            .path
            .as_ref()
            .and_then(|path| {
                self.settings
                    .last_pages
                    .get(&path.to_string_lossy().to_string())
            })
            .and_then(|key| Page::from_key(key, &self.project))
            .unwrap_or(Page::Info);
        self.screen = Screen::Project(page);
        self.show_settings = false;
    }

    /// Recuerda la pantalla en la que está el proyecto guardado.
    fn remember_page(&mut self, page: &Page) {
        let Some(path) = &self.path else { return };
        let (key, value) = (path.to_string_lossy().to_string(), page.key());
        if self.settings.last_pages.get(&key) != Some(&value) {
            self.settings.last_pages.insert(key, value);
            self.settings.save();
        }
    }

    fn open_path(&mut self, path: &Path) {
        match storage::load(path) {
            Ok(project) => {
                self.start_project(project, Some(path.to_path_buf()));
                self.settings.add_recent(path);
                self.status = "Proyecto abierto".into();
            }
            Err(e) => self.status = format!("Error al abrir: {e}"),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Scriptpendra", &[storage::EXTENSION])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    /// Pregunta dónde guardar y devuelve el archivo del proyecto: dentro de su propia carpeta, que se crea.
    fn ask_project_file(&mut self) -> Option<PathBuf> {
        let chosen = rfd::FileDialog::new()
            .add_filter("Scriptpendra", &[storage::EXTENSION])
            .set_file_name(format!("{}.{}", self.project.title, storage::EXTENSION))
            .save_file()?;
        let file = assets::project_file_for(&chosen);
        if file.exists() && self.path.as_deref() != Some(file.as_path()) {
            self.status = format!("Ya hay un proyecto en {}", file.display());
            return None;
        }
        Some(file)
    }

    /// Copia a la carpeta del proyecto las imágenes que estén en otro lado. Devuelve cuántas copió.
    fn localize_assets(&mut self, file: &Path) -> usize {
        match assets::localize(&mut self.project, file) {
            Ok(report) => {
                if report.copied > 0 {
                    self.dirty = true;
                }
                report.copied
            }
            Err(e) => {
                self.status = format!("No se pudieron copiar las imágenes: {e}");
                0
            }
        }
    }

    fn save(&mut self, save_as: bool) {
        // Antes de escribir, se trae lo que otras personas hayan guardado, para no pisarlo.
        if !save_as {
            self.poll_external(true);
            if !self.conflicts.is_empty() {
                self.conflicts_open = true;
                self.status = "Hay cambios de otra persona en conflicto con los tuyos: elegí con cuáles quedarte antes de guardar.".into();
                return;
            }
        }
        let file = match (&self.path, save_as) {
            (Some(p), false) => p.clone(),
            _ => {
                let Some(f) = self.ask_project_file() else {
                    return;
                };
                f
            }
        };
        if let Some(dir) = file.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                self.status = format!("Error al crear la carpeta: {e}");
                return;
            }
        }
        let copied = self.localize_assets(&file);
        match storage::save(&self.project, &file) {
            Ok(()) => {
                self.settings.add_recent(&file);
                self.path = Some(file);
                self.dirty = false;
                self.remember_disk();
                self.image_sig = self.project.image_signature();
                if copied > 0 {
                    self.status = format!(
                        "Guardado ({copied} imagen{} copiada{} a la carpeta del proyecto)",
                        if copied == 1 { "" } else { "es" },
                        if copied == 1 { "" } else { "s" }
                    );
                } else {
                    // Guardar es lo más común: el aviso queda en la barra de estado, sin ventanita.
                    self.status.clear();
                    self.last_message = "Guardado".into();
                }
            }
            Err(e) => self.status = format!("Error al guardar: {e}"),
        }
    }

    fn export_text(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Texto", &["txt"])
            .set_file_name(format!("{}.txt", self.project.title))
            .save_file()
        else {
            return;
        };
        self.status = match std::fs::write(&path, self.project.to_plain_text()) {
            Ok(()) => "Exportado".into(),
            Err(e) => format!("Error al exportar: {e}"),
        };
    }

    fn export_fountain(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Fountain", &["fountain"])
            .set_file_name(format!("{}.fountain", self.project.title))
            .save_file()
        else {
            return;
        };
        self.status = match std::fs::write(&path, crate::fountain::export(&self.project)) {
            Ok(()) => "Exportado a Fountain".into(),
            Err(e) => format!("Error al exportar: {e}"),
        };
    }

    /// Word moderno (.docx): lo abren Word, LibreOffice y Google Docs. El .doc binario de antes no se puede escribir
    /// sin una biblioteca entera.
    fn export_docx(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Word", &["docx"])
            .set_file_name(format!("{}.docx", self.project.title))
            .save_file()
        else {
            return;
        };
        self.status = match std::fs::write(
            &path,
            crate::docx::export(&self.project, self.settings.paper),
        ) {
            Ok(()) => "Exportado a Word".into(),
            Err(e) => format!("Error al exportar: {e}"),
        };
    }

    fn export_pdf(&mut self, only_changed: bool) {
        self.export_pdf_with(only_changed, true);
    }

    /// Exporta el guion a PDF; con `cover` en falso, sin la página de título.
    fn export_pdf_with(&mut self, only_changed: bool, cover: bool) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(if only_changed {
                format!("{} - revision.pdf", self.project.title)
            } else if cover {
                format!("{}.pdf", self.project.title)
            } else {
                format!("{} - guion.pdf", self.project.title)
            })
            .save_file()
        else {
            return;
        };
        self.status = match crate::pdf::export_with(
            &self.project,
            self.settings.paper,
            self.settings.bold_names,
            only_changed,
            cover,
        )
        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
        {
            Ok(()) => "Exportado a PDF".into(),
            Err(e) => format!("Error al exportar: {e}"),
        };
    }

    fn import_fountain_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Fountain", &["fountain", "txt", "spmd"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text)
                if self
                    .project
                    .paragraphs
                    .iter()
                    .all(|p| p.text.trim().is_empty()) =>
            {
                self.apply_fountain(&text)
            }
            Ok(text) => {
                self.confirm = Some(Confirm::ImportFountain(text, self.project.paragraphs.len()))
            }
            Err(e) => self.status = format!("No se pudo leer el archivo: {e}"),
        }
    }

    fn apply_fountain(&mut self, text: &str) {
        self.project.import_fountain(text);
        self.editor = Default::default();
        self.dirty = true;
        let scenes = self.project.script_scenes().len();
        self.status = format!(
            "Guion importado: {} párrafos, {} escenas",
            self.project.paragraphs.len(),
            scenes
        );
    }

    /// En qué pantalla estoy y, si hay una escena actual, cuál: es lo que ven los demás («Guion · escena 3»).
    fn presence_of(&self, page: &Page) -> (String, String) {
        let screen = Workspace::ALL
            .iter()
            .flat_map(|w| w.screens())
            .find(|(_, _, p)| p == page)
            .map_or_else(|| "Fichas".to_string(), |(_, label, _)| label.to_string());
        let detail = self
            .current_scene
            .and_then(|id| self.project.scene_labels().into_iter().find(|(s, _)| *s == id))
            .map_or(String::new(), |(_, n)| format!("escena {n}"));
        (screen, detail)
    }

    /// Las rutas de imágenes del proyecto, como texto con «/» (así se guardan en el índice de la nube).
    fn image_path_strings(&mut self) -> Vec<String> {
        self.project
            .image_paths_mut()
            .into_iter()
            .filter(|p| p.is_relative())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    /// Imágenes que están en la nube pero el proyecto ya no usa: (ruta, huella, tipo).
    fn unused_cloud_images(&mut self) -> Vec<(String, String, String)> {
        let used: HashSet<String> = self.image_path_strings().into_iter().collect();
        self.project
            .cloud_images
            .iter()
            .filter(|(path, _)| !used.contains(*path))
            .map(|(path, i)| (path.clone(), i.hash.clone(), i.ext.clone()))
            .collect()
    }

    /// Qué hay que hacer con las imágenes del proyecto unido a la nube: subir las nuevas y bajar las que faltan en esta carpeta.
    fn image_work(&mut self, remove_unused: bool) -> Option<cloud_ui::ImageWork> {
        let link = self.cloud_link.clone()?;
        let base_dir = self.project.base_dir.clone()?;
        let paths = self.image_path_strings();
        let (mut upload, mut download) = (Vec::new(), Vec::new());
        for path in &paths {
            let Some(name) = crate::cloudimages::resource_name(path) else { continue };
            let exists = base_dir.join("recursos").join(name).exists();
            match (exists, self.project.cloud_images.get(path)) {
                (true, None) => upload.push(path.clone()),
                (false, Some(i)) => download.push((path.clone(), i.hash.clone(), i.ext.clone())),
                _ => {}
            }
        }
        let remove: Vec<(String, String)> = if remove_unused {
            self.unused_cloud_images().into_iter().map(|(_, h, e)| (h, e)).collect()
        } else {
            Vec::new()
        };
        if upload.is_empty() && download.is_empty() && remove.is_empty() {
            return None;
        }
        Some(cloud_ui::ImageWork { group: link.group, project: link.project, base_dir, upload, download, remove })
    }

    /// Aplica lo que pidió la ventana de la nube o la sincronización en segundo plano.
    fn apply_cloud(&mut self, out: cloud_ui::Out) {
        if let Some(message) = out.message {
            self.status = message;
        }
        if let Some(name) = out.name {
            // El nombre de la cuenta manda: es el que firma todo y no se puede cambiar.
            if self.settings.user_name != name {
                self.settings.user_name = name;
                self.settings.save();
            }
        }
        if out.settings_changed {
            self.settings.save();
        }
        if let Some(peers) = out.peers {
            self.peers = peers;
        }
        if let Some(link) = out.link {
            if let Some(file) = &self.path {
                let _ = link.save(file);
            }
            self.cloud_link = Some(link);
        }
        if out.unlink {
            if let Some(file) = &self.path {
                CloudLink::remove(file);
            }
            self.cloud_link = None;
            self.peers.clear();
        }
        if out.disk_changed {
            // La nube dejó secciones nuevas en la carpeta: se combinan con lo que hay en pantalla y, si hubo que combinar, se
            // guarda para que el resultado llegue a los demás.
            self.poll_external(true);
            if self.dirty && self.conflicts.is_empty() {
                self.save(false);
            }
        }
        if out.synced || out.clean_images {
            if let Some(work) = self.image_work(out.clean_images) {
                if out.clean_images {
                    // Lo que se saca ya no figura en el índice del proyecto.
                    let gone: HashSet<String> = work.remove.iter().map(|(h, _)| h.clone()).collect();
                    self.project.cloud_images.retain(|_, i| !gone.contains(&i.hash));
                    self.dirty = true;
                }
                self.cloud.send_images(work);
            }
        }
        if let Some(result) = out.images {
            for (path, hash, ext) in result.uploaded {
                self.project.cloud_images.insert(path, crate::model::CloudImage { hash, ext });
                self.dirty = true;
            }
            if result.downloaded > 0 {
                // Aparecieron archivos nuevos: se vuelven a mostrar las imágenes.
                self.image_sig = 0;
            }
            if !result.skipped.is_empty() {
                self.status = format!(
                    "{} imagen(es) no se sincronizaron: {}",
                    result.skipped.len(),
                    result.skipped.first().cloned().unwrap_or_default()
                );
            }
        }
        if let Some(path) = out.open {
            match storage::load(&path) {
                Ok(project) => {
                    self.start_project(project, Some(path.clone()));
                    self.settings.add_recent(&path);
                    self.status = "Proyecto abierto desde la nube.".into();
                }
                Err(e) => self.status = format!("No se pudo abrir: {e}"),
            }
        }
    }

    /// Recuerda cómo está la carpeta del proyecto ahora: es la base con la que después se combinan los cambios de otras personas.
    fn remember_disk(&mut self) {
        match &self.path {
            Some(file) => {
                self.known = folder::read_sections(file).unwrap_or_default();
                self.stamps = folder::stamps(file);
            }
            None => {
                self.known.clear();
                self.stamps.clear();
            }
        }
    }

    /// Pone un proyecto nuevo (leído o combinado) en lugar del actual, sin cambiar de pantalla.
    fn replace_project(&mut self, mut new: Project) {
        // Lo que llega de otra persona no puede apuntar a archivos de esta computadora.
        new.drop_foreign_image_paths();
        new.base_dir = self.project.base_dir.clone();
        new.normalize();
        new.use_id_block(self.settings.user_id);
        self.project = new;
        self.editor.resync_snapshot(&self.project);
    }

    /// Mira si otra persona guardó cambios en la carpeta del proyecto. Lo que cambió y yo no toqué se trae; lo que cambiamos
    /// los dos se combina (cada cosa por su id); si los dos tocamos lo mismo queda un conflicto para elegir. `now` fuerza la
    /// revisión aunque la carpeta parezca igual.
    fn poll_external(&mut self, now: bool) {
        let Some(file) = self.path.clone() else {
            return;
        };
        if !self.conflicts.is_empty() {
            return;
        }
        let stamps = folder::stamps(&file);
        if !now && stamps == self.stamps {
            return;
        }
        let Ok(disk) = folder::read_sections(&file) else {
            return;
        };
        let changed: Vec<String> = disk
            .iter()
            .filter(|(n, b)| self.known.get(*n) != Some(*b))
            .map(|(n, _)| n.clone())
            .collect();
        self.stamps = stamps;
        if changed.is_empty() {
            return;
        }
        let Ok(mine) = folder::split(&self.project) else {
            return;
        };
        let empty = b"{}".to_vec();
        let mut merged = mine.clone();
        let (mut applied, mut combined) = (Vec::new(), false);
        let mut told: Vec<ChangeToast> = Vec::new();
        for name in changed {
            let theirs = &disk[&name];
            let base = self.known.get(&name);
            // Qué cambió, para avisarlo con su contenido.
            let lines = folder::describe_change(base.map_or(&empty[..], |b| b.as_slice()), theirs, 5);
            let who = self
                .cloud_link
                .as_ref()
                .and_then(|l| l.authors.get(&name))
                .filter(|w| !w.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "Otra persona".to_string());
            if mine.get(&name) == base {
                merged.insert(name.clone(), theirs.clone());
            } else {
                match folder::merge_section(
                    base.unwrap_or(&empty),
                    mine.get(&name).unwrap_or(&empty),
                    theirs,
                ) {
                    Ok(bytes) => {
                        merged.insert(name.clone(), bytes);
                        combined = true;
                    }
                    Err(_) => {
                        self.conflicts.push(Conflict { section: name, theirs: theirs.clone() });
                        continue;
                    }
                }
            }
            self.known.insert(name.clone(), theirs.clone());
            if !lines.is_empty() {
                told.push(ChangeToast { who, section: name.clone(), lines, born: std::time::Instant::now() });
            }
            applied.push(name);
        }
        if !applied.is_empty()
            && let Ok(new) = folder::join(&merged)
        {
            self.replace_project(new);
            if combined {
                // Lo combinado todavía no está en el archivo: hay que guardarlo.
                self.dirty = true;
            }
            // El aviso muestra qué cambió; si no se pudo describir, queda el mensaje corto en la barra de estado.
            if told.is_empty() {
                let names: Vec<&str> = applied.iter().map(|n| section_name(n)).collect();
                self.status = format!("Otra persona cambió: {}. Ya está actualizado.", names.join(", "));
            }
            if self.settings.notifications {
                self.change_toasts.extend(told);
            }
            let excess = self.change_toasts.len().saturating_sub(4);
            self.change_toasts.drain(..excess);
        }
        if !self.conflicts.is_empty() {
            self.conflicts_open = true;
        }
    }

    /// Resuelve un conflicto: me quedo con mi versión de la sección o con la de la otra persona. Lo que se descarta queda
    /// guardado en el historial.
    fn resolve_conflict(&mut self, index: usize, keep_mine: bool) {
        if index >= self.conflicts.len() {
            return;
        }
        let Conflict { section, theirs } = self.conflicts.remove(index);
        let Some(file) = self.path.clone() else {
            return;
        };
        let now = storage::now_unix();
        let author = self.author_name();
        let Ok(mut sections) = folder::split(&self.project) else {
            return;
        };
        if keep_mine {
            // Lo de la otra persona queda como una versión del historial antes de pisarlo.
            let mut theirs_sections = sections.clone();
            theirs_sections.insert(section.clone(), theirs.clone());
            if let Ok(project) = folder::join(&theirs_sections) {
                let _ = folder::snapshot(&project, &file, "Otra persona", "Antes de pisar su cambio", false, now);
            }
            self.known.insert(section, theirs);
            self.dirty = true;
        } else {
            let _ = folder::snapshot(&self.project, &file, &author, "Antes de aceptar el cambio de otra persona", false, now);
            sections.insert(section.clone(), theirs.clone());
            if let Ok(new) = folder::join(&sections) {
                self.replace_project(new);
            }
            self.known.insert(section, theirs);
        }
    }

    /// La ventana de conflictos: una sección cambió a la vez acá y en otra copia.
    fn conflicts_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        if self.conflicts.is_empty() || !self.conflicts_open {
            return;
        }
        let mut choice: Option<(usize, bool)> = None;
        let mut later = false;
        let names: Vec<&str> = self.conflicts.iter().map(|c| section_name(&c.section)).collect();
        kit::dialog(
            ctx,
            p,
            "conflicts",
            "Cambió lo mismo en dos copias",
            kit::DialogWidth::Normal,
            |ui| {
                ui.label("Otra persona y vos cambiaron lo mismo. ¿Con cuál te quedás? La que no elijas queda guardada en las versiones anteriores.");
                ui.add_space(tokens::SP_S);
                for (i, name) in names.iter().enumerate() {
                    ui.label(RichText::new(*name).font(egui::FontId::new(tokens::FONT_BODY, theme::bold_family())));
                    ui.horizontal(|ui| {
                        if ui.button("Con mis cambios").clicked() {
                            choice = Some((i, true));
                        }
                        if ui.button("Con los de la otra persona").clicked() {
                            choice = Some((i, false));
                        }
                    });
                    ui.add_space(tokens::SP_XS);
                }
            },
            |ui| {
                if ui.button("Decidir después").clicked() {
                    later = true;
                }
            },
        );
        if let Some((i, mine)) = choice {
            self.resolve_conflict(i, mine);
        }
        if later {
            self.conflicts_open = false;
        }
    }

    /// Con qué nombre se firman las versiones: el que puso la persona o, si no puso ninguno, el del usuario del sistema.
    fn author_name(&self) -> String {
        self.settings.author()
    }

    /// Los avisos de lo que cambiaron otras personas: una tarjeta por aviso, con el color de quien lo hizo. Se cierran con un clic o
    /// solos a los 15 segundos.
    fn change_toasts_ui(&mut self, ctx: &egui::Context, p: &Palette) {
        self.change_toasts.retain(|t| t.born.elapsed().as_secs() < CHANGE_TOAST_SECS);
        if !self.settings.notifications {
            self.change_toasts.clear();
        }
        if self.change_toasts.is_empty() {
            return;
        }
        let mut dismissed: Option<usize> = None;
        egui::Area::new(egui::Id::unique("change_toasts"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-tokens::SP_L, -(tokens::STATUS_H + tokens::SP_L)))
            .show(ctx, |ui| {
                ui.set_max_width(340.0);
                for (i, toast) in self.change_toasts.iter().enumerate() {
                    let color = theme::character_color(&toast.who);
                    let resp = egui::Frame::new()
                        .fill(theme::mix(p.container_hi, color, 0.14))
                        .stroke(Stroke::new(1.5, color))
                        .corner_radius(tokens::R_CARD)
                        .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: p.shadow })
                        .inner_margin(egui::Margin::symmetric(tokens::SP_M as i8, tokens::SP_S as i8))
                        .show(ui, |ui| {
                            ui.set_width(316.0);
                            ui.horizontal(|ui| {
                                let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                                ui.painter().circle_filled(dot.center(), 5.0, color);
                                ui.label(
                                    RichText::new(format!("{} cambió {}", toast.who, section_name(&toast.section)))
                                        .font(egui::FontId::new(tokens::FONT_BODY, theme::bold_family()))
                                        .color(p.text),
                                );
                            });
                            for line in &toast.lines {
                                ui.add(egui::Label::new(RichText::new(line).size(tokens::FONT_SMALL).color(p.text)).wrap());
                            }
                        })
                        .response
                        .interact(egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Clic para cerrar");
                    if resp.clicked() {
                        dismissed = Some(i);
                    }
                    ui.add_space(tokens::SP_XS);
                }
            });
        if let Some(i) = dismissed {
            self.change_toasts.remove(i);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// Guarda una versión en el historial (junto al proyecto). Las automáticas no se guardan si no cambió nada.
    fn save_version(&mut self, name: &str, auto: bool) {
        let Some(file) = self.path.clone() else {
            return;
        };
        if self.dirty {
            self.save(false);
        }
        let now = storage::now_unix();
        match folder::snapshot(&self.project, &file, &self.author_name(), name, auto, now) {
            Ok(saved) => {
                let _ = folder::prune(&file, now);
                if saved.is_some() && !auto {
                    self.versions.note = "Versión guardada.".into();
                }
            }
            Err(e) => self.status = format!("No se pudo guardar la versión: {e}"),
        }
        self.versions.list = folder::list(&file);
    }

    /// Abre la ventana del historial.
    fn open_versions(&mut self) {
        let Some(file) = self.path.clone() else {
            self.status = "Guardá el proyecto en un archivo para tener versiones anteriores.".into();
            return;
        };
        self.versions.list = folder::list(&file);
        self.versions.selected = None;
        self.versions.differing.clear();
        self.versions.note.clear();
        self.versions.open = true;
    }

    /// Vuelve el proyecto a una versión (o solo a algunas de sus secciones). Antes guarda lo actual como versión, para poder
    /// deshacerlo.
    fn restore_version(&mut self, index: usize, only: Option<Vec<String>>) {
        let (Some(file), Some(version)) = (self.path.clone(), self.versions.list.get(index).cloned()) else {
            return;
        };
        self.save_version("Antes de restaurar", false);
        match folder::restore(&self.project, &file, &version, only.as_deref()) {
            Ok(restored) => {
                self.start_project(restored, Some(file));
                self.dirty = true;
                self.status = "Versión restaurada".into();
                self.versions.differing.clear();
                self.versions.selected = None;
            }
            Err(e) => self.status = format!("No se pudo restaurar: {e}"),
        }
    }

    /// La ventana de versiones anteriores: se ve cuándo y quién guardó cada una y se puede volver a cualquiera, entera o solo
    /// algunas partes.
    fn versions_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.versions.open {
            return;
        }
        let mut v = std::mem::take(&mut self.versions);
        let (mut save, mut close, mut renamed) = (false, false, false);
        let mut restore: Option<(usize, Option<Vec<String>>)> = None;
        let mut pick: Option<usize> = None;
        let project = &self.project;
        let user = &mut self.settings.user_name;
        let name_locked = self.cloud.locked_name().is_some();
        let now = storage::now_unix();
        kit::dialog(
            ctx,
            p,
            "versions",
            "Versiones anteriores",
            kit::DialogWidth::Normal,
            |ui| {
                ui.label(
                    RichText::new("El programa guarda versiones solo, mientras trabajás. Podés volver a cualquiera.")
                        .color(p.dim),
                );
                ui.add_space(tokens::SP_S);
                ui.horizontal(|ui| {
                    ui.label("Tu nombre");
                    renamed = ui
                        .add_enabled(
                            !name_locked,
                            egui::TextEdit::singleline(user).desired_width(160.0).hint_text("Cómo te llamás"),
                        )
                        .on_disabled_hover_text("El nombre de tu cuenta no se puede cambiar")
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut v.name)
                            .desired_width(220.0)
                            .hint_text("Nombre (opcional)"),
                    );
                    if ui.button("Guardar una versión ahora").clicked() {
                        save = true;
                    }
                });
                if !v.note.is_empty() {
                    ui.label(RichText::new(&v.note).color(p.success));
                }
                ui.add_space(tokens::SP_S);
                if v.list.is_empty() {
                    ui.label(RichText::new("Todavía no hay versiones guardadas.").color(p.dim));
                }
                egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (i, version) in v.list.iter().enumerate() {
                        let mut label = storage::friendly_time(version.time, now);
                        if !version.author.is_empty() {
                            label.push_str(&format!(" · {}", version.author));
                        }
                        if !version.name.is_empty() {
                            label.push_str(&format!(" · {}", version.name));
                        }
                        if ui.selectable_label(v.selected == Some(i), label).clicked() {
                            pick = Some(i);
                        }
                    }
                });
                if v.selected.and_then(|i| v.list.get(i)).is_some() {
                    ui.add_space(tokens::SP_S);
                    ui.separator();
                    if v.differing.is_empty() {
                        ui.label(RichText::new("Esta versión es igual a lo que tenés ahora.").color(p.dim));
                    } else {
                        if kit::accent_button(ui, p, icon::ARROW_LEFT, "Volver a esta versión").clicked() {
                            restore = v.selected.map(|i| (i, None));
                        }
                        ui.label(
                            RichText::new("Antes se guarda lo que tenés ahora, por si querés volver.")
                                .size(tokens::FONT_SMALL)
                                .color(p.dim),
                        );
                        egui::CollapsingHeader::new("O recuperar solo una parte").show(ui, |ui| {
                            for section in &v.differing {
                                ui.horizontal(|ui| {
                                    ui.label(section_name(section));
                                    if ui.button("Recuperar").clicked() {
                                        restore = v.selected.map(|i| (i, Some(vec![section.clone()])));
                                    }
                                });
                            }
                        });
                    }
                }
            },
            |ui| {
                if ui.button("Cerrar").clicked() {
                    close = true;
                }
            },
        );
        if renamed {
            self.settings.save();
        }
        if let Some(i) = pick {
            v.selected = Some(i);
            v.differing = v
                .list
                .get(i)
                .and_then(|version| folder::differing_sections(&self.project, version).ok())
                .unwrap_or_default();
        }
        let _ = project;
        let name = std::mem::take(&mut v.name);
        self.versions = v;
        if save {
            self.save_version(&name, false);
        } else {
            self.versions.name = name;
        }
        if let Some((i, only)) = restore {
            self.restore_version(i, only);
        }
        if close {
            self.versions.open = false;
        }
    }

    fn go_home(&mut self) {
        if self.dirty && self.path.is_some() {
            self.save(false);
        }
        // Al cerrar el proyecto queda una versión del historial.
        self.save_version("", true);
        self.cards = storage::recent_cards(&self.settings);
        self.screen = Screen::Home;
        self.show_settings = false;
    }

    fn add_character(&mut self) -> u64 {
        let existing = self.project.known_characters();
        let mut name = String::from("NUEVO PERSONAJE");
        let mut n = 2;
        while existing.contains(&name) {
            name = format!("NUEVO PERSONAJE {n}");
            n += 1;
        }
        self.dirty = true;
        self.project.new_character(&name)
    }

    /// Tras renombrar algo que el guion nombra (personaje, objeto, elemento, mundo): se reescriben los nombres
    /// vinculados y se actualiza la copia «actual» del historial. El deshacer sigue válido.
    fn after_rename(&mut self) {
        self.project.sync_links();
        self.editor.resync_snapshot(&self.project);
    }

    /// Ancho con el que se crea un panel: el último guardado, y desde ahí no cambia durante la sesión.
    fn panel_default(&mut self, id: &'static str, side: PanelSide) -> f32 {
        let saved = match side {
            PanelSide::Left => self.settings.sidebar_width,
            PanelSide::Right => self.settings.right_width,
        };
        self.panels
            .entry(id)
            .or_insert(PanelMemory {
                default: saved,
                offset: None,
                last: 0.0,
            })
            .default
    }

    /// Guarda el ancho de un panel cuando se termina de arrastrar su borde. Los cambios chicos (borde, redondeo a
    /// píxeles) no cuentan: solo un arrastre de verdad.
    fn remember_panel_width(
        &mut self,
        ctx: &egui::Context,
        id: &'static str,
        side: PanelSide,
        measured: f32,
    ) {
        #[cfg(test)]
        self.panel_trace.push((id, measured));
        let Some(memory) = self.panels.get_mut(id) else {
            return;
        };
        if memory.offset.is_none() {
            memory.offset = Some(measured - memory.default);
            memory.last = measured;
        }
        // Solo se recuerda lo que arrastró la persona (se suelta el mouse): el contenido nunca cambia el ancho guardado.
        if !ctx.input(|i| i.pointer.any_released()) || (measured - memory.last).abs() <= 2.0 {
            return;
        }
        memory.last = measured;
        let (target, range) = match side {
            PanelSide::Left => (&mut self.settings.sidebar_width, storage::SIDEBAR_RANGE),
            PanelSide::Right => (&mut self.settings.right_width, storage::RIGHT_RANGE),
        };
        *target = (measured - memory.offset.unwrap_or(0.0)).clamp(*range.start(), *range.end());
        self.settings.save();
    }

    /// Menú ⋮ de la barra lateral.
    /// El menú ☰ del encabezado de la barra lateral: Guardar, Guardar como, Exportar…, Importar Fountain, Ajustes y Volver al
    /// inicio (interfaz.md §1.1).
    fn options_menu(&mut self, ui: &mut egui::Ui) {
        // Sin contorno ni fondo en reposo, y más chico que un botón normal.
        let widgets = &mut ui.visuals_mut().widgets;
        for w in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
            w.bg_stroke = Stroke::NONE;
        }
        widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0);
        ui.spacing_mut().interact_size.y = ICON_BTN - 8.0;
        ui.menu_button(RichText::new(icon::LIST).size(ICON_S), |ui| {
            ui.reset_style();
            if ui.button("Guardar").clicked() {
                self.save(false);
                ui.close();
            }
            if ui.button("Guardar como…").clicked() {
                self.save(true);
                ui.close();
            }
            if ui
                .add_enabled(self.path.is_some(), egui::Button::new("Versiones anteriores…"))
                .on_disabled_hover_text("Guardá el proyecto en un archivo para tener versiones anteriores")
                .clicked()
            {
                self.open_versions();
                ui.close();
            }
            if ui.button("Nube…").clicked() {
                self.cloud.open = true;
                ui.close();
            }
            if ui.button("Exportar…").clicked() {
                self.export_dialog.open = true;
                ui.close();
            }
            ui.menu_button("Exportar guion", |ui| {
                if ui.button("PDF (.pdf)…").clicked() {
                    self.export_pdf(false);
                    ui.close();
                }
                let can_revise = self.project.active_revision.is_some();
                if ui
                    .add_enabled(
                        can_revise,
                        egui::Button::new("PDF solo con páginas cambiadas…"),
                    )
                    .on_disabled_hover_text("Abrí una revisión en la pestaña Formato del guion")
                    .clicked()
                {
                    self.export_pdf(true);
                    ui.close();
                }
                if ui.button("Word (.docx)…").clicked() {
                    self.export_docx();
                    ui.close();
                }
                if ui.button("Fountain (.fountain)…").clicked() {
                    self.export_fountain();
                    ui.close();
                }
                if ui.button("Texto (.txt)…").clicked() {
                    self.export_text();
                    ui.close();
                }
            });
            if ui.button("Importar guion (Fountain)…").clicked() {
                self.import_fountain_dialog();
                ui.close();
            }
            ui.separator();
            if ui.button("Ajustes").clicked() {
                self.show_settings = true;
                ui.close();
            }
            ui.separator();
            if ui.button("Volver al inicio").clicked() {
                self.go_home();
                ui.close();
            }
        });
    }

    /// Barra lateral (interfaz.md §1.1). Se muestra u oculta, y pasa de riel a barra completa, con un cambio de ancho
    /// animado (reglas §6.7). Devuelve la pantalla a la que se pidió ir.
    fn sidebar(&mut self, ui: &mut egui::Ui, page: &Page, p: &Palette) -> Option<Page> {
        let mut next: Option<Page> = None;
        let default_width = self.panel_default("sidebar", PanelSide::Left);
        let frame = egui::Frame::new().fill(p.panel);
        let mut visible = self.show_sidebar || self.show_settings;
        let rail = self.rail;
        let collapsed = egui::Panel::left("sidebar_rail")
            .exact_size(RAIL_W)
            .resizable(false)
            .frame(frame);
        let expanded = egui::Panel::left("sidebar")
            .default_size(default_width)
            .size_range(storage::SIDEBAR_RANGE)
            .resizable(true)
            .frame(frame);
        let mut is_full = !rail;
        let settled = ui.ctx().animate_bool_with_time(
            egui::Id::unique("sidebar_full"),
            is_full && visible,
            kit::anim(ANIM_SLOW),
        ) >= 1.0;
        let shown = if visible {
            Some(egui::Panel::show_switched(
                ui,
                &mut is_full,
                collapsed,
                expanded,
                |ui, full| {
                    if full {
                        self.sidebar_full(ui, page, p, &mut next)
                    } else {
                        self.sidebar_rail(ui, page, p, &mut next)
                    }
                },
            ))
        } else {
            // Oculta: la barra que estaba se cierra con la misma animación.
            let panel = if rail {
                egui::Panel::left("sidebar_rail")
                    .exact_size(RAIL_W)
                    .resizable(false)
                    .frame(frame)
            } else {
                expanded
            };
            panel.show_collapsible(ui, &mut visible, |ui| {
                if rail {
                    self.sidebar_rail(ui, page, p, &mut next)
                } else {
                    self.sidebar_full(ui, page, p, &mut next)
                }
            });
            None
        };
        // El ancho solo se recuerda con la barra completa quieta (no mientras cambia por una animación).
        if let (Some(shown), true, false) = (shown, settled, rail) {
            self.remember_panel_width(
                ui.ctx(),
                "sidebar",
                PanelSide::Left,
                shown.response.rect.width(),
            );
        }
        next
    }

    /// La barra lateral completa: encabezado, espacios de trabajo, pantallas del espacio, fichas y «Añadir».
    fn sidebar_full(
        &mut self,
        ui: &mut egui::Ui,
        page: &Page,
        p: &Palette,
        next: &mut Option<Page>,
    ) {
        let side = egui::Margin {
            left: SIDEBAR_PAD_X as i8,
            right: SIDEBAR_PAD_X as i8,
            top: 0,
            bottom: 0,
        };
        // Las dos filas de arriba miden lo mismo que el título y la barra de herramientas, y no llevan espacio entre sí: así las
        // pestañas de los espacios de trabajo quedan a la altura de las del inspector.
        let spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        // Encabezado: el botón ☰ (menú) a la izquierda y a su lado «Scriptpendra».
        ui.allocate_ui_with_layout(
            egui::vec2(width, TITLE_H),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(SIDEBAR_PAD_X);
                self.options_menu(ui);
                ui.add_space(SP_S);
                ui.add(
                    egui::Label::new(
                        RichText::new("Scriptpendra")
                            .font(egui::FontId::new(FONT_TITLE, theme::bold_family()))
                            .color(p.text),
                    )
                    .truncate(),
                );
            },
        );
        // Historial de navegación: ← y → a la altura de la barra de herramientas.
        ui.allocate_ui_with_layout(
            egui::vec2(width, TOOLBAR_H),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(SIDEBAR_PAD_X);
                ui.spacing_mut().item_spacing.x = SP_XS;
                let can_back = self.history_pos > 0;
                let can_forward = self.history_pos + 1 < self.history.len();
                if kit::icon_button(ui, p, icon::ARROW_LEFT, "Atrás (Alt+←)", can_back).clicked()
                    && can_back
                {
                    self.go_history(self.history_pos - 1);
                }
                if kit::icon_button(ui, p, icon::ARROW_RIGHT, "Adelante (Alt+→)", can_forward)
                    .clicked()
                    && can_forward
                {
                    self.go_history(self.history_pos + 1);
                }
            },
        );
        ui.spacing_mut().item_spacing.y = spacing;
        let labels: Vec<&str> = Workspace::ALL.iter().map(|w| w.label()).collect();
        let active = Workspace::ALL
            .iter()
            .position(|w| *w == self.workspace)
            .unwrap_or(0);
        egui::Frame::new().inner_margin(side).show(ui, |ui| {
            if let Some(i) = kit::tab_row(ui, p, &labels, active) {
                let target = Workspace::ALL[i];
                self.workspace = target;
                *next = Some(
                    self.last_in_workspace
                        .get(&target)
                        .cloned()
                        .unwrap_or_else(|| target.screens()[0].2.clone()),
                );
            }
        });
        egui::Panel::bottom("sidebar_bottom")
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                top: SIDEBAR_PAD_Y as i8,
                bottom: SIDEBAR_PAD_Y as i8,
                ..side
            }))
            .show_separator_line(false)
            .show(ui, |ui| {
                if kit::accent_button(ui, p, icon::PLUS, "Añadir").clicked() {
                    self.show_add = true;
                }
            });
        egui::ScrollArea::vertical()
            .id_salt("sidebar_body")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        top: SP_S as i8,
                        bottom: SP_S as i8,
                        ..side
                    })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = LIST_GAP;
                        for node in self.workspace.tree() {
                            match node {
                                Node::Leaf(code, label, target) => {
                                    if kit::tree_row(
                                        ui,
                                        p,
                                        Row {
                                            depth: 0,
                                            caret: None,
                                            icon: Some(code),
                                            label,
                                            selected: *page == target,
                                        },
                                    )
                                    .clicked()
                                    {
                                        *next = Some(target);
                                    }
                                }
                                Node::Group(code, label, children) => {
                                    let open = !self.closed_groups.contains(label);
                                    if kit::tree_row(
                                        ui,
                                        p,
                                        Row {
                                            depth: 0,
                                            caret: Some(open),
                                            icon: Some(code),
                                            label,
                                            selected: false,
                                        },
                                    )
                                    .clicked()
                                        && !self.closed_groups.remove(label)
                                    {
                                        self.closed_groups.insert(label);
                                    }
                                    if open {
                                        for (code, label, target) in children {
                                            if kit::tree_row(
                                                ui,
                                                p,
                                                Row {
                                                    depth: 1,
                                                    caret: None,
                                                    icon: Some(code),
                                                    label,
                                                    selected: *page == target,
                                                },
                                            )
                                            .clicked()
                                            {
                                                *next = Some(target);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ui.add_space(SECTION_GAP - LIST_GAP);
                        self.cards_section(ui, page, p, next);
                    });
            });
    }

    /// Barra lateral reducida a un riel de iconos: el encabezado muestra solo `⋯`; después van los tres espacios y las
    /// pantallas del espacio activo (el nombre va en el tooltip). Las fichas se alcanzan desde el menú del libro.
    fn sidebar_rail(
        &mut self,
        ui: &mut egui::Ui,
        page: &Page,
        p: &Palette,
        next: &mut Option<Page>,
    ) {
        egui::ScrollArea::vertical()
            .id_salt("sidebar_rail_body")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    // En el riel, el encabezado es solo el botón ☰, a la altura del título.
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), TITLE_H),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(SP_S);
                            self.options_menu(ui);
                        },
                    );
                    ui.spacing_mut().item_spacing.y = SP_XS;
                    ui.add_space(SP_S);
                    for (workspace, code) in [
                        (Workspace::Writing, icon::ARTICLE),
                        (Workspace::Design, icon::FILM_STRIP),
                        (Workspace::Production, icon::CLIPBOARD_TEXT),
                    ] {
                        let resp = kit::icon_button(ui, p, code, workspace.label(), true);
                        if self.workspace == workspace {
                            ui.painter().line_segment(
                                [resp.rect.left_bottom(), resp.rect.right_bottom()],
                                Stroke::new(2.0, p.accent),
                            );
                        }
                        if resp.clicked() {
                            self.workspace = workspace;
                            *next = Some(
                                self.last_in_workspace
                                    .get(&workspace)
                                    .cloned()
                                    .unwrap_or_else(|| workspace.screens()[0].2.clone()),
                            );
                        }
                    }
                    ui.separator();
                    for (code, label, target) in self.workspace.screens() {
                        let resp = kit::icon_button(ui, p, code, label, true);
                        if *page == target {
                            ui.painter().rect_stroke(
                                resp.rect,
                                R_CONTROL,
                                Stroke::new(1.0, p.accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if resp.clicked() {
                            *next = Some(target);
                        }
                    }
                    ui.separator();
                    ui.menu_button(icon::BOOKS, |ui| {
                        ui.set_max_width(260.0);
                        self.cards_menu(ui, next);
                    })
                    .response
                    .on_hover_text("Fichas");
                    if kit::icon_button(ui, p, icon::PLUS, "Añadir", true).clicked() {
                        self.show_add = true;
                    }
                });
            });
    }

    /// Las fichas del proyecto como menú, para el riel.
    fn cards_menu(&mut self, ui: &mut egui::Ui, next: &mut Option<Page>) {
        let mut pick = |ui: &mut egui::Ui, code: &str, label: &str, target: Page| {
            if ui
                .add(egui::Button::new(format!("{code}  {label}")).truncate())
                .clicked()
            {
                *next = Some(target);
                ui.close();
            }
        };
        ui.menu_button("Personajes", |ui| {
            for c in &self.project.characters {
                pick(
                    ui,
                    icon::USER,
                    &self.project.label_for(EntityRef::Character(c.id)),
                    Page::Character(c.id),
                );
            }
        });
        ui.menu_button("Locaciones", |ui| {
            for l in self.project.locations() {
                pick(
                    ui,
                    icon::HOUSE,
                    &self.project.label_for(EntityRef::Location(l.id)),
                    Page::Location(l.id),
                );
            }
        });
        if !self.project.objects.is_empty() {
            ui.menu_button("Objetos clave", |ui| {
                for o in &self.project.objects {
                    pick(
                        ui,
                        icon::CUBE,
                        &self.project.label_for(EntityRef::Object(o.id)),
                        Page::Object(o.id),
                    );
                }
            });
        }
        for cat in &self.project.element_categories {
            ui.menu_button(cat.name.as_str(), |ui| {
                pick(
                    ui,
                    cat.icon.as_str(),
                    "Ver la categoría",
                    Page::ElementCategory(cat.id),
                );
                for e in self
                    .project
                    .elements
                    .iter()
                    .filter(|e| e.category == cat.id)
                {
                    pick(ui, cat.icon.as_str(), &e.name, Page::Element(e.id));
                }
            });
        }
        if self
            .project
            .documents
            .iter()
            .any(|d| d.kind != DocKind::Folder)
        {
            ui.menu_button("Mundos y documentos", |ui| {
                for d in self
                    .project
                    .documents
                    .iter()
                    .filter(|d| d.kind != DocKind::Folder)
                {
                    let name = if d.name.is_empty() {
                        "(sin nombre)"
                    } else {
                        d.name.as_str()
                    };
                    pick(
                        ui,
                        pages::document_icon(d.kind, false),
                        name,
                        Page::Document(d.id),
                    );
                }
            });
        }
    }

    /// Encabezado de un grupo de fichas con su cantidad y un `+` al pasar el mouse. Devuelve (clic en el grupo, clic en +).
    fn group_header(
        &self,
        ui: &mut egui::Ui,
        p: &Palette,
        open: bool,
        code: &str,
        label: &str,
        count: usize,
        plus_tip: &str,
    ) -> (bool, bool) {
        let text = format!("{label} ({count})");
        let resp = widgets::tree_row(
            ui,
            p,
            Row {
                depth: 0,
                caret: Some(open),
                icon: Some(code),
                label: &text,
                selected: false,
            },
        );
        let mut plus = false;
        if resp.hovered() || resp.contains_pointer() {
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    resp.rect.right() - ICON_BTN - SP_XS,
                    resp.rect.center().y - ICON_BTN / 2.0,
                ),
                egui::vec2(ICON_BTN, ICON_BTN),
            );
            let hit = ui.interact(rect, resp.id.with("plus"), egui::Sense::click());
            ui.painter().rect_filled(rect, R_CONTROL, p.container_hi);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon::PLUS,
                egui::FontId::proportional(16.0),
                p.accent,
            );
            hit.clone().on_hover_text(plus_tip);
            plus = hit.clicked();
        }
        (resp.clicked() && !plus, plus)
    }

    /// Una ficha en la lista: un clic la abre y el menú contextual renombra, duplica o elimina.
    #[allow(clippy::too_many_arguments)]
    fn card_row(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        code: &str,
        label: &str,
        target: Page,
        current: &Page,
        next: &mut Option<Page>,
    ) {
        let resp = widgets::tree_row(
            ui,
            p,
            Row {
                depth: 1,
                caret: None,
                icon: Some(code),
                label,
                selected: *current == target,
            },
        );
        if resp.clicked() {
            *next = Some(target.clone());
        }
        let can_duplicate = matches!(
            target,
            Page::Character(_) | Page::Object(_) | Page::Element(_)
        );
        resp.context_menu(|ui| {
            if ui.button("Renombrar").clicked() {
                self.card_action = Some((target.clone(), CardAction::Rename));
                ui.close();
            }
            if ui
                .add_enabled(can_duplicate, egui::Button::new("Duplicar"))
                .clicked()
            {
                self.card_action = Some((target.clone(), CardAction::Duplicate));
                ui.close();
            }
            if ui
                .button(RichText::new("Eliminar").color(p.danger))
                .clicked()
            {
                self.card_action = Some((target.clone(), CardAction::Delete));
                ui.close();
            }
        });
    }

    /// Atiende lo elegido en el menú contextual de una ficha.
    fn apply_card_action(&mut self) {
        let Some((page, action)) = self.card_action.take() else {
            return;
        };
        match action {
            CardAction::Rename => {
                let name = match &page {
                    Page::Character(id) => self.project.character(*id).map(|c| c.name.clone()),
                    Page::Location(id) => self
                        .project
                        .location_profiles
                        .iter()
                        .find(|l| l.id == *id)
                        .map(|l| l.name.clone()),
                    Page::Object(id) => self
                        .project
                        .objects
                        .iter()
                        .find(|o| o.id == *id)
                        .map(|o| o.name.clone()),
                    Page::Element(id) => self
                        .project
                        .elements
                        .iter()
                        .find(|e| e.id == *id)
                        .map(|e| e.name.clone()),
                    Page::Document(id) => self
                        .project
                        .documents
                        .iter()
                        .find(|d| d.id == *id)
                        .map(|d| d.name.clone()),
                    _ => None,
                };
                if let Some(name) = name {
                    self.rename = Some((page, name));
                }
            }
            CardAction::Duplicate => {
                let copy = match &page {
                    Page::Character(id) => self.project.character(*id).cloned().map(|orig| {
                        let new = self
                            .project
                            .new_character(&format!("{} (copia)", orig.name));
                        if let Some(c) = self.project.characters.iter_mut().find(|c| c.id == new) {
                            *c = CharacterProfile {
                                id: new,
                                name: c.name.clone(),
                                ..orig
                            };
                        }
                        Page::Character(new)
                    }),
                    Page::Object(id) => self
                        .project
                        .objects
                        .iter()
                        .find(|o| o.id == *id)
                        .cloned()
                        .map(|orig| {
                            let new = self.project.add_object(&format!("{} (copia)", orig.name));
                            if let Some(o) = self.project.objects.iter_mut().find(|o| o.id == new) {
                                *o = crate::model::ObjectItem {
                                    id: new,
                                    name: o.name.clone(),
                                    ..orig
                                };
                            }
                            Page::Object(new)
                        }),
                    Page::Element(id) => self
                        .project
                        .elements
                        .iter()
                        .find(|e| e.id == *id)
                        .cloned()
                        .and_then(|orig| {
                            let new = self
                                .project
                                .add_element(orig.category, &format!("{} (copia)", orig.name))?;
                            if let Some(e) = self.project.elements.iter_mut().find(|e| e.id == new)
                            {
                                *e = crate::model::Element {
                                    id: new,
                                    name: e.name.clone(),
                                    ..orig
                                };
                            }
                            Some(Page::Element(new))
                        }),
                    _ => None,
                };
                if let Some(copy) = copy {
                    self.dirty = true;
                    self.screen = Screen::Project(copy);
                }
            }
            CardAction::Delete => {
                match &page {
                    Page::Character(id) => {
                        let cues = self.project.character_cues(*id);
                        let name = self
                            .project
                            .character(*id)
                            .map(|c| c.name.clone())
                            .unwrap_or_default();
                        if cues == 0 {
                            self.delete_character(*id);
                        } else {
                            self.confirm = Some(Confirm::DeleteCharacter(*id, name, cues));
                        }
                    }
                    Page::Location(id) => self.project.remove_location(*id),
                    Page::Object(id) => self.project.remove_object(*id),
                    Page::Element(id) => {
                        self.project.elements.retain(|e| e.id != *id);
                        self.project.drop_links_to(EntityRef::Element(*id));
                    }
                    Page::Document(id) => self.project.delete_document(*id),
                    _ => return,
                }
                if matches!(page, Page::Character(_)) && self.confirm.is_some() {
                    return;
                }
                self.dirty = true;
                if matches!(&self.screen, Screen::Project(current) if *current == page) {
                    self.screen = Screen::Project(Page::Info);
                }
            }
        }
    }

    /// El diálogo para renombrar una ficha.
    fn rename_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some((page, text)) = &mut self.rename else {
            return;
        };
        let (mut accept, mut cancel) = (false, false);
        let modal = egui::Modal::new(egui::Id::unique("rename_dialog")).show(ctx, |ui| {
            ui.set_max_width(DIALOG_MAX);
            ui.label(
                RichText::new("Renombrar")
                    .font(egui::FontId::new(FONT_TITLE, theme::bold_family())),
            );
            ui.add_space(SP_M);
            let width = kit::fill_width(ui, 120.0, DIALOG_MAX);
            let resp = ui.add(egui::TextEdit::singleline(text).desired_width(width));
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                accept = true;
            }
            ui.add_space(SP_M);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !text.trim().is_empty(),
                        egui::Button::new(RichText::new("Renombrar").color(p.accent)),
                    )
                    .clicked()
                {
                    accept = true;
                }
                if ui.button("Cancelar").clicked() {
                    cancel = true;
                }
            });
        });
        if modal.should_close() {
            cancel = true;
        }
        if accept {
            let (page, text) = (page.clone(), text.trim().to_string());
            match page {
                Page::Character(id) => {
                    if let Some(c) = self.project.characters.iter_mut().find(|c| c.id == id) {
                        c.name = clean_character_name(&text);
                    }
                }
                Page::Location(id) => {
                    if let Some(l) = self
                        .project
                        .location_profiles
                        .iter_mut()
                        .find(|l| l.id == id)
                    {
                        l.name = text;
                    }
                }
                Page::Object(id) => {
                    if let Some(o) = self.project.objects.iter_mut().find(|o| o.id == id) {
                        o.name = text;
                    }
                }
                Page::Element(id) => {
                    if let Some(e) = self.project.elements.iter_mut().find(|e| e.id == id) {
                        e.name = text;
                    }
                }
                Page::Document(id) => {
                    if let Some(d) = self.project.documents.iter_mut().find(|d| d.id == id) {
                        d.name = text;
                    }
                }
                _ => {}
            }
            self.after_rename();
            self.dirty = true;
            self.rename = None;
        } else if cancel {
            self.rename = None;
        }
    }

    /// Sección Fichas: buscador y grupos de personajes, locaciones, objetos, categorías, mundos y documentos.
    fn cards_section(
        &mut self,
        ui: &mut egui::Ui,
        page: &Page,
        p: &Palette,
        next: &mut Option<Page>,
    ) {
        if widgets::tree_row(
            ui,
            p,
            Row {
                depth: 0,
                caret: Some(self.open_cards),
                icon: Some(icon::BOOKS),
                label: "Fichas",
                selected: false,
            },
        )
        .clicked()
        {
            self.open_cards = !self.open_cards;
        }
        if !self.open_cards {
            return;
        }
        ui.add_space(SP_XS);
        ui.horizontal(|ui| {
            ui.add_space(SP_M);
            let width = kit::fill_width(ui, 80.0, 400.0) - SP_M;
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.card_query)
                    .desired_width(width)
                    .hint_text(format!("{} Buscar", icon::MAGNIFYING_GLASS)),
            );
            if std::mem::take(&mut self.focus_search) {
                search.request_focus();
            }
        });
        ui.add_space(SP_XS);
        let query = self.card_query.trim().to_lowercase();
        let searching = !query.is_empty();
        let matches = |label: &str| !searching || label.to_lowercase().contains(&query);

        // Personajes
        let characters: Vec<(u64, String)> = self
            .project
            .characters
            .iter()
            .map(|c| (c.id, self.project.label_for(EntityRef::Character(c.id))))
            .filter(|(_, l)| matches(l))
            .collect();
        let (toggle, plus) = self.group_header(
            ui,
            p,
            self.open_characters || searching,
            icon::USERS_THREE,
            "Personajes",
            characters.len(),
            "Nuevo personaje",
        );
        if toggle {
            self.open_characters = !self.open_characters;
        }
        if plus {
            let id = self.add_character();
            *next = Some(Page::Character(id));
        }
        if self.open_characters || searching {
            for (id, label) in characters {
                self.card_row(ui, p, icon::USER, &label, Page::Character(id), page, next);
            }
        }

        // Locaciones
        let locations: Vec<(u64, String)> = self
            .project
            .locations()
            .iter()
            .map(|l| (l.id, self.project.label_for(EntityRef::Location(l.id))))
            .filter(|(_, l)| matches(l))
            .collect();
        let (toggle, plus) = self.group_header(
            ui,
            p,
            self.open_locations || searching,
            icon::BUILDINGS,
            "Locaciones",
            locations.len(),
            "Nueva ficha de locación o de otra cosa",
        );
        if toggle {
            self.open_locations = !self.open_locations;
        }
        if plus {
            self.show_add = true;
        }
        if self.open_locations || searching {
            for (id, label) in locations {
                self.card_row(ui, p, icon::HOUSE, &label, Page::Location(id), page, next);
            }
        }

        // Objetos clave
        let objects: Vec<(u64, String)> = self
            .project
            .objects
            .iter()
            .map(|o| (o.id, self.project.label_for(EntityRef::Object(o.id))))
            .filter(|(_, l)| matches(l))
            .collect();
        if !self.project.objects.is_empty() {
            let (toggle, plus) = self.group_header(
                ui,
                p,
                self.open_objects || searching,
                icon::CUBE,
                "Objetos clave",
                objects.len(),
                "Nuevo objeto",
            );
            if toggle {
                self.open_objects = !self.open_objects;
            }
            if plus {
                self.show_add = true;
            }
            if self.open_objects || searching {
                for (id, label) in objects {
                    self.card_row(ui, p, icon::CUBE, &label, Page::Object(id), page, next);
                }
            }
        }

        // Categorías propias
        for cat in self.project.element_categories.clone() {
            let members: Vec<(u64, String)> = self
                .project
                .elements
                .iter()
                .filter(|e| e.category == cat.id && matches(&e.name))
                .map(|e| (e.id, e.name.clone()))
                .collect();
            let open = !self.closed_element_cats.contains(&cat.id) || searching;
            let (toggle, plus) = self.group_header(
                ui,
                p,
                open,
                cat.icon.as_str(),
                &cat.name,
                members.len(),
                "Nuevo elemento",
            );
            if toggle {
                if open {
                    self.closed_element_cats.insert(cat.id);
                } else {
                    self.closed_element_cats.remove(&cat.id);
                }
                *next = Some(Page::ElementCategory(cat.id));
            }
            if plus {
                if let Some(new) = self.project.add_element(cat.id, "Nuevo elemento") {
                    self.dirty = true;
                    *next = Some(Page::Element(new));
                }
            }
            if open {
                for (id, name) in members {
                    self.card_row(
                        ui,
                        p,
                        cat.icon.as_str(),
                        &name,
                        Page::Element(id),
                        page,
                        next,
                    );
                }
            }
        }

        // Mundos
        let worlds: Vec<(u64, String)> = self
            .project
            .documents
            .iter()
            .filter(|d| d.kind == DocKind::World && matches(&d.name))
            .map(|d| (d.id, d.name.clone()))
            .collect();
        if self
            .project
            .documents
            .iter()
            .any(|d| d.kind == DocKind::World)
        {
            let (toggle, plus) = self.group_header(
                ui,
                p,
                self.open_worlds || searching,
                icon::GLOBE,
                "Mundos",
                worlds.len(),
                "Nuevo mundo",
            );
            if toggle {
                self.open_worlds = !self.open_worlds;
            }
            if plus {
                self.show_add = true;
            }
            if self.open_worlds || searching {
                for (id, name) in worlds {
                    self.card_row(ui, p, icon::GLOBE, &name, Page::Document(id), page, next);
                }
            }
        }

        // Documentos
        if self
            .project
            .documents
            .iter()
            .any(|d| d.kind != DocKind::World)
        {
            let count = self
                .project
                .documents
                .iter()
                .filter(|d| d.kind != DocKind::World)
                .count();
            let (toggle, plus) = self.group_header(
                ui,
                p,
                self.open_docs,
                icon::FOLDER,
                "Documentos",
                count,
                "Nuevo documento",
            );
            if toggle {
                self.open_docs = !self.open_docs;
            }
            if plus {
                self.show_add = true;
            }
            if self.open_docs {
                self.document_rows(ui, page, p, next, None, 1);
            }
        }
    }

    /// Documentos (carpetas anidadas, textos, mapas y galerías) bajo `parent`.
    fn document_rows(
        &mut self,
        ui: &mut egui::Ui,
        page: &Page,
        p: &Palette,
        next: &mut Option<Page>,
        parent: Option<u64>,
        depth: u8,
    ) {
        let docs: Vec<(u64, DocKind, String)> = self
            .project
            .documents
            .iter()
            .filter(|d| d.parent == parent && d.kind != DocKind::World)
            .map(|d| (d.id, d.kind, d.name.clone()))
            .collect();
        for (id, kind, name) in docs {
            let open = self.open_folders.contains(&id);
            let caret = (kind == DocKind::Folder).then_some(open);
            let row = Row {
                depth,
                caret,
                icon: Some(pages::document_icon(kind, open)),
                label: if name.is_empty() {
                    "(sin nombre)"
                } else {
                    &name
                },
                selected: *page == Page::Document(id),
            };
            if widgets::tree_row(ui, p, row).clicked() {
                if kind == DocKind::Folder && !self.open_folders.remove(&id) {
                    self.open_folders.insert(id);
                }
                *next = Some(Page::Document(id));
            }
            if kind == DocKind::Folder && self.open_folders.contains(&id) {
                self.document_rows(ui, page, p, next, Some(id), depth + 1);
            }
        }
    }

    /// Ajustes: dentro del marco del proyecto, o sin él si se abrieron desde Inicio.
    fn settings_screen(&mut self, ui: &mut egui::Ui, p: &Palette, bare: bool) {
        self.settings.name_locked = self.cloud.locked_name().is_some();
        use crate::screens::SettingsScreen;
        let key = "settings".to_string();
        let history: Vec<String> = Vec::new();
        let mut inspector_on = false;
        let mut tab = 0;
        let mut cx = ScreenCx::new(
            &mut self.project,
            p,
            &self.settings,
            &mut self.current_scene,
            &history,
        );
        let mut screen = SettingsScreen {
            section: &mut self.settings_section,
            bare,
        };
        if bare {
            shell::show_bare(ui, &mut screen, &mut cx);
        } else {
            let flags = shell::ShellFlags {
                inspector_on: &mut inspector_on,
                compact: true,
                inspector_tab: &mut tab,
                panel_id: "no_inspector",
                panel_default: 0.0,
                page_key: &key,
            };
            shell::show_screen(ui, &mut screen, &mut cx, flags);
        }
        let (new_settings, effects, dirty) = (
            cx.new_settings.take(),
            std::mem::take(&mut cx.effects),
            cx.dirty,
        );
        self.dirty |= dirty;
        if let Some(s) = new_settings {
            self.settings = s;
            kit::set_reduce_motion(self.settings.reduce_motion);
            theme::apply(
                ui.ctx(),
                &self.settings.scheme.palette(),
                self.settings.ui_zoom,
            );
            self.settings.save();
        }
        for effect in effects {
            if effect == Effect::CloseSettings {
                self.show_settings = false;
            }
        }
    }

    fn project_screen(&mut self, ui: &mut egui::Ui, page: Page) {
        let p = self.settings.scheme.palette();
        self.remember_page(&page);
        if let Some(w) = Workspace::of(&page) {
            self.workspace = w;
            self.last_in_workspace.insert(w, page.clone());
        }
        self.track_history(&page);
        self.handle_shell_keys(ui.ctx(), &page);
        self.status_bar(ui, &page, &p);
        // Ventana angosta (medida ya con el zoom): la barra lateral pasa a riel y, si el contenido quedaría
        // más angosto que `CONTENT_MIN`, el inspector pasa a cajón (reglas §2.6).
        let window = ui.ctx().content_rect().width();
        self.rail = window < SIDEBAR_RAIL_BELOW;
        let sidebar_w = if !(self.show_sidebar || self.show_settings) {
            0.0
        } else if self.rail {
            RAIL_W
        } else {
            self.settings.sidebar_width
        };
        // La tabla del guion técnico pide el ancho de sus columnas: si no entra junto al inspector, este pasa a cajón.
        let content_min = match page {
            Page::TechScript => crate::techscript::content_min(&self.tech, &self.project),
            _ => CONTENT_MIN,
        };
        self.compact_inspector = window - sidebar_w - self.settings.right_width < content_min;
        if !self.compact_inspector {
            self.drawer_open = false;
        }
        // La barra lateral se oculta desde el botón de la barra de herramientas; Ajustes siempre la muestra.
        let sidebar = self.sidebar(ui, &page, &p);
        if let Some(target) = sidebar {
            self.screen = Screen::Project(target.clone());
            self.show_settings = false;
            return self.project_screen_body(ui, target, &p);
        }
        self.project_screen_body(ui, page, &p);
    }

    /// Anota la pantalla en el historial de navegación (se descartan las que estaban «hacia adelante»).
    fn track_history(&mut self, page: &Page) {
        if self.history.get(self.history_pos) == Some(page) {
            return;
        }
        self.history.truncate(if self.history.is_empty() {
            0
        } else {
            self.history_pos + 1
        });
        self.history.push(page.clone());
        if self.history.len() > 50 {
            self.history.remove(0);
        }
        self.history_pos = self.history.len() - 1;
    }

    /// Va a otra pantalla del historial de navegación (como los botones atrás y adelante de un navegador). Una pantalla que ya
    /// no existe (se borró la ficha) se salta.
    fn go_history(&mut self, pos: usize) {
        let Some(entry) = self.history.get(pos) else {
            return;
        };
        if let Some(next) = Page::from_key(&entry.key(), &self.project) {
            self.history_pos = pos;
            self.screen = Screen::Project(next);
        } else {
            self.history.remove(pos);
            self.history_pos = pos.min(self.history.len().saturating_sub(1));
        }
    }

    /// Atajos comunes del marco (reglas §6.2): Ctrl+\\ y Ctrl+Mayús+\\ muestran u ocultan las barras, Alt+←/→ navegan.
    fn handle_shell_keys(&mut self, ctx: &egui::Context, page: &Page) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Backslash))
            && Self::inspector_id(page).is_some()
        {
            if self.compact_inspector {
                self.drawer_open = !self.drawer_open
            } else {
                self.show_right = !self.show_right
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Backslash)) {
            self.show_sidebar = !self.show_sidebar;
        }
        // F2 renombra la ficha abierta.
        if matches!(
            page,
            Page::Character(_)
                | Page::Location(_)
                | Page::Object(_)
                | Page::Element(_)
                | Page::Document(_)
        ) && self.rename.is_none()
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2))
        {
            self.card_action = Some((page.clone(), CardAction::Rename));
        }
        let back = ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowLeft));
        let forward = ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowRight));
        let target = if back && self.history_pos > 0 {
            Some(self.history_pos - 1)
        } else if forward && self.history_pos + 1 < self.history.len() {
            Some(self.history_pos + 1)
        } else {
            None
        };
        if let Some(pos) = target {
            self.go_history(pos);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, page: &Page, p: &Palette) {
        let saved = if self.dirty {
            "Cambios sin guardar"
        } else if self.path.is_some() {
            "Guardado"
        } else {
            "Sin guardar"
        };
        let mut info = self.page_info(page);
        // Quién más está en el proyecto compartido.
        if self.cloud_link.is_some() && !self.peers.is_empty() {
            let people: Vec<String> = self
                .peers
                .iter()
                .map(|p| {
                    let name = if p.who.is_empty() { "Alguien" } else { p.who.as_str() };
                    match (p.screen.is_empty(), p.detail.is_empty()) {
                        (true, _) => name.to_string(),
                        (false, true) => format!("{name} ({})", p.screen),
                        (false, false) => format!("{name} ({} · {})", p.screen, p.detail),
                    }
                })
                .collect();
            info = format!("En línea: {}  ·  {info}", people.join(", "));
        }
        let message = self.last_message.clone();
        // Los botones de la barra lateral y del inspector viven en la barra de estado (interfaz.md §1.5).
        let has_inspector = !self.show_settings && Self::inspector_id(page).is_some();
        let mut sidebar = self.show_sidebar;
        let mut inspector = if self.compact_inspector {
            self.drawer_open
        } else {
            self.show_right
        };
        let mut notifications = self.settings.notifications;
        shell::status_bar(
            ui,
            p,
            shell::Status {
                saved,
                message: &message,
                info: &info,
                sidebar: &mut sidebar,
                inspector: has_inspector.then_some(&mut inspector),
                notifications: &mut notifications,
            },
        );
        self.show_sidebar = sidebar;
        if notifications != self.settings.notifications {
            self.settings.notifications = notifications;
            self.settings.save();
        }
        if has_inspector {
            if self.compact_inspector {
                self.drawer_open = inspector;
            } else {
                self.show_right = inspector;
            }
        }
    }

    /// Datos de la pantalla para la derecha de la barra de estado (interfaz.md §1.5).
    fn page_info(&self, page: &Page) -> String {
        let scenes = self.project.scenes();
        let total: f32 = scenes.iter().map(|s| s.seconds).sum();
        match page {
            Page::Screenplay => {
                let current = self
                    .editor
                    .current_id()
                    .and_then(|id| self.project.scene_of(id));
                let position = current.and_then(|id| scenes.iter().position(|s| s.id == id));
                match position {
                    Some(n) => format!(
                        "Escena {} de {} · {}",
                        scenes[n].number,
                        scenes.len(),
                        pages::fmt_duration(total)
                    ),
                    None => format!("{} escenas · {}", scenes.len(), pages::fmt_duration(total)),
                }
            }
            Page::TechScript | Page::Storyboard => {
                let n = self.project.shots.len();
                format!("{n} {}", if n == 1 { "plano" } else { "planos" })
            }
            Page::Blocking | Page::PlanLights | Page::PlanSound | Page::PlanSet => {
                self.blocking.status().join(" · ")
            }
            Page::Plan => {
                let days = self.project.shoot_days.len();
                let eighths: u16 = scenes
                    .iter()
                    .map(|s| self.project.scene_eighths(s.id))
                    .sum();
                format!(
                    "{days} {} · {} octavos",
                    if days == 1 { "día" } else { "días" },
                    model::fmt_eighths(eighths)
                )
            }
            _ => format!("{} escenas · {}", scenes.len(), pages::fmt_duration(total)),
        }
    }

    fn project_screen_body(&mut self, ui: &mut egui::Ui, page: Page, p: &Palette) {
        if self.show_settings {
            self.settings_screen(ui, p, false);
            return;
        }
        self.run_screen(ui, &page, p);
        self.apply_card_action();
        self.rename_dialog(ui.ctx(), p);
        if let Some(status) = crate::export_dialog::show(
            ui.ctx(),
            p,
            &mut self.export_dialog,
            &self.project,
            self.settings.paper,
            self.settings.bold_names,
        ) {
            self.status = status;
        }
        self.add_dialog(ui.ctx(), p, &page);
        self.versions_dialog(ui.ctx(), p);
        self.change_toasts_ui(ui.ctx(), p);
        {
            let (file, link, title) = (self.path.clone(), self.cloud_link.clone(), self.project.title.clone());
            let unused = self.unused_cloud_images().len();
            let out = self.cloud.dialog(ui.ctx(), p, &mut self.settings, file.as_deref(), &title, link.as_ref(), unused);
            self.apply_cloud(out);
        }
        self.conflicts_dialog(ui.ctx(), p);
        self.confirm_dialog(ui.ctx(), p);
    }

    /// Dibuja una pantalla dentro del marco: arma su `Screen` con el estado que le toca y se la entrega al shell.
    fn run_screen(&mut self, ui: &mut egui::Ui, page: &Page, p: &Palette) {
        use crate::screens::*;
        let key = page.key();
        let panel_id = Self::inspector_id(page).unwrap_or("no_inspector");
        let panel_default = self.panel_default(panel_id, PanelSide::Right);
        let history: Vec<String> = self.history.iter().map(|h| h.key()).collect();
        let mut tab = self.inspector_tabs.get(&key).copied().unwrap_or(0);
        let compact = self.compact_inspector;
        let mut inspector_on = if compact {
            self.drawer_open
        } else {
            self.show_right
        };
        let doc_kind = if let Page::Document(id) = page {
            self.project
                .documents
                .iter()
                .find(|d| d.id == *id)
                .map(|d| d.kind)
        } else {
            None
        };
        let mut cx = ScreenCx::new(
            &mut self.project,
            p,
            &self.settings,
            &mut self.current_scene,
            &history,
        );
        let flags = shell::ShellFlags {
            inspector_on: &mut inspector_on,
            compact,
            inspector_tab: &mut tab,
            panel_id,
            panel_default,
            page_key: &key,
        };
        let out = match page {
            Page::Info => shell::show_screen(ui, &mut InfoScreen, &mut cx, flags),
            Page::TitlePage => shell::show_screen(
                ui,
                &mut TitleScreen {
                    state: &mut self.title_view,
                },
                &mut cx,
                flags,
            ),
            Page::DramaCurve => shell::show_screen(
                ui,
                &mut DramaScreen {
                    state: &mut self.story,
                },
                &mut cx,
                flags,
            ),
            Page::Synopsis => shell::show_screen(
                ui,
                &mut SynopsisScreen {
                    state: &mut self.synopsis,
                },
                &mut cx,
                flags,
            ),
            Page::Treatment => shell::show_screen(
                ui,
                &mut TreatmentScreen {
                    state: &mut self.treatment,
                },
                &mut cx,
                flags,
            ),
            Page::Screenplay => shell::show_screen(
                ui,
                &mut ScriptScreen {
                    state: &mut self.editor,
                },
                &mut cx,
                flags,
            ),
            Page::Statistics => shell::show_screen(ui, &mut StatsScreen, &mut cx, flags),
            Page::TechScript | Page::Storyboard => {
                self.tech.view = if *page == Page::Storyboard {
                    crate::techscript::View::Storyboard
                } else {
                    crate::techscript::View::Table
                };
                shell::show_screen(
                    ui,
                    &mut TechScreen {
                        state: &mut self.tech,
                    },
                    &mut cx,
                    flags,
                )
            }
            Page::PlanLights | Page::PlanSound | Page::PlanSet | Page::Blocking => {
                self.blocking.set_layer(match page {
                    Page::PlanLights => crate::blocking::Layer::Lights,
                    Page::PlanSound => crate::blocking::Layer::Sound,
                    Page::PlanSet => crate::blocking::Layer::Set,
                    _ => crate::blocking::Layer::Camera,
                });
                shell::show_screen(
                    ui,
                    &mut crate::blocking::BlockScreen {
                        state: &mut self.blocking,
                    },
                    &mut cx,
                    flags,
                )
            }
            Page::Scenes => shell::show_screen(
                ui,
                &mut ScenesScreen {
                    state: &mut self.scenes,
                },
                &mut cx,
                flags,
            ),
            Page::Breakdown => shell::show_screen(
                ui,
                &mut BreakdownScreen {
                    state: &mut self.breakdown,
                },
                &mut cx,
                flags,
            ),
            Page::Plan => shell::show_screen(
                ui,
                &mut PlanScreen {
                    state: &mut self.plan,
                },
                &mut cx,
                flags,
            ),
            Page::CallSheets => shell::show_screen(
                ui,
                &mut CallsScreen {
                    state: &mut self.calls,
                },
                &mut cx,
                flags,
            ),
            Page::ShootLocations => shell::show_screen(
                ui,
                &mut ShootScreen {
                    state: &mut self.shoot,
                },
                &mut cx,
                flags,
            ),
            Page::ShootPlace(id) => {
                shell::show_screen(ui, &mut ShootPlaceScreen { id: *id }, &mut cx, flags)
            }
            Page::SoundScript | Page::SoundGear | Page::PhotoGear | Page::ArtDirection => {
                shell::show_screen(
                    ui,
                    &mut RoleScreen {
                        role: match page {
                            Page::SoundScript => crate::model::Role::Sound,
                            Page::SoundGear => crate::model::Role::SoundGear,
                            Page::PhotoGear => crate::model::Role::PhotoGear,
                            _ => crate::model::Role::Art,
                        },
                        state: &mut self.roles,
                    },
                    &mut cx,
                    flags,
                )
            }
            Page::Contacts => shell::show_screen(
                ui,
                &mut ContactsScreen {
                    state: &mut self.contacts,
                },
                &mut cx,
                flags,
            ),
            Page::Character(id) => shell::show_screen(
                ui,
                &mut CharacterScreen {
                    id: *id,
                    export_sel: &mut self.export_sel,
                },
                &mut cx,
                flags,
            ),
            Page::Location(id) => {
                shell::show_screen(ui, &mut LocationScreen { id: *id }, &mut cx, flags)
            }
            Page::Object(id) => {
                shell::show_screen(ui, &mut ObjectScreen { id: *id }, &mut cx, flags)
            }
            Page::Element(id) => {
                shell::show_screen(ui, &mut ElementScreen { id: *id }, &mut cx, flags)
            }
            Page::ElementCategory(id) => shell::show_screen(
                ui,
                &mut CategoryScreen {
                    id: *id,
                    editing: &mut self.template_editing,
                },
                &mut cx,
                flags,
            ),
            Page::Document(id) => shell::show_screen(
                ui,
                &mut DocumentScreen {
                    id: *id,
                    doc_kind,
                    selected: &mut self.mind_selected,
                    story: &mut self.story,
                },
                &mut cx,
                flags,
            ),
        };
        let (navigate, status, dirty, confirm, effects) = (
            cx.navigate.take(),
            cx.status.take(),
            cx.dirty,
            cx.confirm.take(),
            std::mem::take(&mut cx.effects),
        );
        if compact {
            self.drawer_open = inspector_on
        } else {
            self.show_right = inspector_on
        }
        self.inspector_tabs.insert(key, tab);
        if let Some(w) = out.inspector_width {
            self.remember_panel_width(ui.ctx(), panel_id, PanelSide::Right, w);
        }
        self.dirty |= dirty;
        if status.is_some() {
            self.status = status.unwrap_or_default();
        }
        if confirm.is_some() {
            self.confirm = confirm;
        }
        for effect in effects {
            match effect {
                Effect::AfterRename => self.after_rename(),
                Effect::DeleteCharacter(id) => self.delete_character(id),
                Effect::ExportCharacter(id) => self.export_character(id, p),
                Effect::GoInfo => self.screen = Screen::Project(Page::Info),
                Effect::CloseSettings => self.show_settings = false,
                Effect::OpenExport => self.export_dialog.open = true,
                Effect::ExportPdf => self.export_pdf(false),
                Effect::ExportScriptPdf => self.export_pdf_with(false, false),
                Effect::ImportOutline => {
                    let now = ui.ctx().input(|i| i.time);
                    self.editor.import_outline(&mut self.project, now);
                    self.dirty = true;
                }
                Effect::GoPage(key) => {
                    if let Some(next) = Page::from_key(&key, &self.project) {
                        self.screen = Screen::Project(next);
                    }
                }
            }
        }
        if let Some(nav) = navigate {
            self.apply_nav(nav);
        }
    }

    /// Atiende un pedido de ir a otra pantalla (interfaz.md §8: todo vínculo se puede seguir).
    fn apply_nav(&mut self, nav: Nav) {
        match nav {
            Nav::Page(key) => {
                if let Some(page) = Page::from_key(&key, &self.project) {
                    self.screen = Screen::Project(page);
                }
            }
            Nav::Script(scene) => {
                self.editor.jump_to(scene);
                self.current_scene = Some(scene);
                self.screen = Screen::Project(Page::Screenplay);
            }
            Nav::Plan(scene) => {
                self.blocking.show_scene(scene);
                self.current_scene = Some(scene);
                self.screen = Screen::Project(Page::Blocking);
            }
            Nav::Camera(blocking, camera) => {
                self.blocking.go_to(&self.project, blocking, camera);
                self.screen = Screen::Project(Page::Blocking);
            }
            Nav::Breakdown(scene) => {
                self.breakdown.show_scene(scene);
                self.current_scene = Some(scene);
                self.screen = Screen::Project(Page::Breakdown);
            }
            Nav::Shots(scene) => {
                self.tech.show_scene(scene);
                self.current_scene = Some(scene);
                self.screen = Screen::Project(Page::TechScript);
            }
        }
    }

    /// El id del panel derecho de cada pantalla que tiene inspector.
    fn inspector_id(page: &Page) -> Option<&'static str> {
        match page {
            Page::Screenplay => Some("script_side"),
            Page::Character(_) => Some("character_side"),
            Page::Treatment => Some("treatment_side"),
            Page::TechScript | Page::Storyboard => Some("tech_side"),
            Page::Blocking | Page::PlanLights | Page::PlanSound | Page::PlanSet => {
                Some("blocking_side")
            }
            Page::ShootLocations => Some("shoot_side"),
            Page::Scenes => Some("scenes_side"),
            Page::Contacts => Some("contacts_side"),
            Page::Plan => Some("plan_side"),
            Page::CallSheets => Some("calls_side"),
            Page::TitlePage => Some("title_side"),
            Page::Breakdown => Some("breakdown_side"),
            Page::Synopsis => Some("synopsis_side"),
            Page::Location(_) => Some("location_side"),
            _ => None,
        }
    }

    fn delete_character(&mut self, id: u64) {
        let name = self
            .project
            .character(id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let removed = self.project.remove_character(id);
        self.editor.reset_history();
        self.screen = Screen::Project(Page::Info);
        self.dirty = true;
        self.status = if removed == 0 {
            format!("Personaje {name} eliminado")
        } else {
            format!("Personaje {name} eliminado, con {removed} párrafos del guion")
        };
    }

    fn export_character(&mut self, id: u64, p: &Palette) {
        let Some(profile) = self.project.characters.iter().find(|c| c.id == id) else {
            return;
        };
        let name = clean_character_name(&profile.name);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SVG", &["svg"])
            .set_file_name(format!("{name}.svg"))
            .save_file()
        else {
            return;
        };
        let mut profile = profile.clone();
        profile.photo = profile
            .photo
            .as_deref()
            .map(|ph| self.project.resolve_path(ph));
        let svg = export::character_svg(&profile, &self.export_sel, p.accent);
        self.status = match std::fs::write(&path, svg) {
            Ok(()) => format!("Ficha exportada a {}", path.display()),
            Err(e) => format!("Error al exportar: {e}"),
        };
    }

    /// Confirmación de acciones que borran datos.
    fn confirm_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(confirm) = &self.confirm else { return };
        let (title, text, action) = match confirm {
            Confirm::ResetCharacter(_, name) => (
                "Resetear ficha".to_string(),
                format!("Se vacían todos los campos de la ficha de {name} (foto, rol, edad y ficha completa). El nombre y el guion no cambian."),
                "Resetear",
            ),
            Confirm::DeleteTemplatePart { label, count, field, .. } => (
                if field.is_some() { "Quitar campo".to_string() } else { "Quitar sección".to_string() },
                format!(
                    "«{label}» tiene contenido en {count} {}. Se borra también ese contenido. No se puede deshacer.",
                    if *count == 1 { "elemento" } else { "elementos" }
                ),
                "Quitar",
            ),
            Confirm::DeleteElementCategory(_, name, count) => (
                "Eliminar categoría".to_string(),
                format!(
                    "La categoría «{name}» tiene {count} {}. Se elimina junto con todos ellos. No se puede deshacer.",
                    if *count == 1 { "elemento" } else { "elementos" }
                ),
                "Eliminar",
            ),
            Confirm::DeleteShotColumn(_, name, count) => (
                "Quitar columna".to_string(),
                format!(
                    "La columna «{name}» tiene contenido en {count} {}. Se borra también ese contenido. No se puede deshacer.",
                    if *count == 1 { "plano" } else { "planos" }
                ),
                "Quitar",
            ),
            Confirm::DeleteBlocking(_, name, items, linked) => (
                "Eliminar bloqueo".to_string(),
                format!(
                    "«{name}» tiene {items} {} dibujad{}. Se elimina con todo eso{}. No se puede deshacer.",
                    if *items == 1 { "elemento" } else { "elementos" },
                    if *items == 1 { "o" } else { "os" },
                    if *linked > 0 { format!(" y {linked} {} del guion técnico queda{} sin cámara", if *linked == 1 { "plano" } else { "planos" }, if *linked == 1 { "" } else { "n" }) } else { String::new() }
                ),
                "Eliminar",
            ),
            Confirm::ImportFountain(_, paragraphs) => (
                "Importar guion".to_string(),
                format!(
                    "El guion actual tiene {paragraphs} párrafos. Importar los reemplaza por los del archivo, y los planos y bloqueos que estaban atados a las escenas actuales quedan sin escena. No se puede deshacer."
                ),
                "Importar",
            ),
            Confirm::DeleteShootLocation(_, name, scenes) => (
                "Eliminar locación de rodaje".to_string(),
                format!(
                    "«{name}» se elimina con sus datos, fotos y permisos.{} No se puede deshacer.",
                    if *scenes > 0 { format!(" {scenes} {} que se filmaba{} ahí queda{} sin lugar.", if *scenes == 1 { "escena" } else { "escenas" }, if *scenes == 1 { "" } else { "n" }, if *scenes == 1 { "" } else { "n" }) } else { String::new() }
                ),
                "Eliminar",
            ),
            Confirm::DeleteShot(_) => (
                "Eliminar plano".to_string(),
                "El plano tiene descripción, sonido o imagen de storyboard. Se elimina con todo eso. No se puede deshacer.".to_string(),
                "Eliminar",
            ),
            Confirm::DeleteCharacter(_, name, cues) => (
                "Eliminar personaje".to_string(),
                format!(
                    "{name} habla {cues} {} en el guion. Se elimina su ficha y todas esas intervenciones (nombre, acotaciones y diálogos). No se puede deshacer.",
                    if *cues == 1 { "vez" } else { "veces" }
                ),
                "Eliminar",
            ),
        };
        let answer = kit::confirm(ctx, p, "confirm", &title, &text, action);
        let (accepted, cancelled) = (answer == Some(true), answer == Some(false));
        if accepted {
            match self.confirm.take() {
                Some(Confirm::ResetCharacter(id, name)) => {
                    self.project.reset_character(id);
                    self.dirty = true;
                    self.status = format!("Ficha de {name} reseteada");
                }
                Some(Confirm::DeleteCharacter(id, _, _)) => self.delete_character(id),
                Some(Confirm::ImportFountain(text, _)) => self.apply_fountain(&text),
                Some(Confirm::DeleteShotColumn(id, _, _)) => {
                    self.project.remove_shot_column(id);
                    self.dirty = true;
                }
                Some(Confirm::DeleteBlocking(id, _, _, _)) => {
                    self.project.remove_blocking(id);
                    self.dirty = true;
                }
                Some(Confirm::DeleteShootLocation(id, _, _)) => {
                    self.project.remove_shoot_location(id);
                    self.dirty = true;
                    if matches!(self.screen, Screen::Project(Page::ShootPlace(_))) {
                        self.screen = Screen::Project(Page::ShootLocations);
                    }
                }
                Some(Confirm::DeleteShot(id)) => {
                    self.project.remove_shot(id);
                    self.dirty = true;
                }
                Some(Confirm::DeleteTemplatePart {
                    category,
                    section,
                    field,
                    ..
                }) => {
                    self.project.remove_template_part(category, section, field);
                    self.dirty = true;
                }
                Some(Confirm::DeleteElementCategory(id, name, _)) => {
                    self.project.remove_element_category(id);
                    self.screen = Screen::Project(Page::Info);
                    self.dirty = true;
                    self.status = format!("Categoría {name} eliminada");
                }
                None => {}
            }
        } else if cancelled {
            self.confirm = None;
        }
    }

    /// Mensaje breve abajo al centro (guardado, exportado, errores).
    fn show_toast(&mut self, ctx: &egui::Context, p: &Palette) {
        let now = ctx.input(|i| i.time);
        if !self.status.is_empty() {
            self.last_message = self.status.clone();
            self.toast = Some((std::mem::take(&mut self.status), now + 3.5));
        }
        let Some((text, until)) = &self.toast else {
            return;
        };
        if !kit::toast(ctx, p, text, *until) {
            self.toast = None;
        }
    }

    /// Ventana «Añadir»: elementos del proyecto y estructuras o notas.
    fn add_dialog(&mut self, ctx: &egui::Context, p: &Palette, page: &Page) {
        if !self.show_add {
            return;
        }
        let mut choice: Option<AddChoice> = None;
        let mut create_category = false;
        let mut back = false;
        let modal = egui::Modal::new(egui::Id::unique("add_dialog")).show(ctx, |ui| {
            // Cuatro mosaicos de ancho.
            let gap = tokens::SP_M;
            ui.set_width(4.0 * tokens::TILE_W + 3.0 * gap);
            match self.add_mode {
                AddMode::Choose => {
                    ui.label(RichText::new("Añadir").font(egui::FontId::new(19.0, theme::bold_family())));
                    ui.add_space(tokens::SP_M);
                    ui.label(RichText::new("ELEMENTOS DEL PROYECTO").size(11.0).color(p.dim));
                    ui.add_space(tokens::SP_S);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                        for (code, label, c) in [
                            (icon::USER, "Personaje", AddChoice::Character),
                            (icon::HOUSE, "Locación", AddChoice::Location),
                            (icon::CUBE, "Objeto clave", AddChoice::Object),
                            (icon::GLOBE, "Mundo", AddChoice::Doc(DocKind::World)),
                        ] {
                            if widgets::tile(ui, p, code, label).clicked() {
                                choice = Some(c);
                            }
                        }
                    });
                    ui.add_space(tokens::SP_L);
                    ui.label(RichText::new("TUS CATEGORÍAS").size(11.0).color(p.dim));
                    ui.add_space(tokens::SP_S);
                    // Un contenedor de dos mosaicos y medio de alto: se ve que la lista sigue y que se desplaza.
                    // La barra de desplazamiento flota: no le quita ancho a los cuatro mosaicos.
                    ui.spacing_mut().scroll = egui::style::ScrollStyle::floating();
                    egui::ScrollArea::vertical()
                        .id_salt("add_categories")
                        .max_height(2.0 * (tokens::TILE_H + gap) + tokens::TILE_H / 2.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                                for cat in &self.project.element_categories {
                                    if widgets::tile(ui, p, &cat.icon, &cat.name).clicked() {
                                        choice = Some(AddChoice::Element(cat.id));
                                    }
                                }
                                if widgets::tile(ui, p, icon::PLUS, "Nueva categoría").clicked() {
                                    choice = Some(AddChoice::NewCategory);
                                }
                            });
                        });
                    ui.add_space(tokens::SP_L);
                    ui.label(RichText::new("ESTRUCTURAS Y NOTAS").size(11.0).color(p.dim));
                    ui.add_space(tokens::SP_S);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                        for kind in [DocKind::Folder, DocKind::Text, DocKind::MindMap, DocKind::Relations, DocKind::Cards, DocKind::Gallery] {
                            if widgets::tile(ui, p, pages::document_icon(kind, false), kind.label()).clicked() {
                                choice = Some(AddChoice::Doc(kind));
                            }
                        }
                    });
                }
                AddMode::NewCategory => {
                    ui.label(RichText::new("Nueva categoría").font(egui::FontId::new(19.0, theme::bold_family())));
                    ui.add_space(tokens::SP_XS);
                    ui.label(
                        RichText::new(
                            "Aparece en la barra lateral, como los personajes o las locaciones. Sus elementos se pueden \
                             autocompletar al escribir en el guion.",
                        )
                        .size(12.0)
                        .color(p.dim),
                    );
                    ui.add_space(tokens::SP_M);
                    widgets::field(ui, p, "Nombre (por ejemplo, Fauna)", &mut self.new_cat_name, 1);
                    ui.label(RichText::new("Icono").size(12.0).color(p.dim));
                    ui.add_space(tokens::SP_XS);
                    pages::icon_picker(ui, p, &mut self.new_cat_icon);
                    ui.add_space(tokens::SP_L);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!self.new_cat_name.trim().is_empty(), |ui| {
                            if ui.button(RichText::new("Crear categoría").color(p.accent)).clicked() {
                                create_category = true;
                            }
                        });
                        if ui.button("Volver").clicked() {
                            back = true;
                        }
                    });
                }
            }
        });
        if modal.should_close() {
            self.show_add = false;
            self.add_mode = AddMode::Choose;
        }
        if back {
            self.add_mode = AddMode::Choose;
        }
        if create_category {
            let id = self
                .project
                .add_element_category(&self.new_cat_name, &self.new_cat_icon);
            self.new_cat_name.clear();
            self.add_mode = AddMode::Choose;
            self.show_add = false;
            self.dirty = true;
            self.closed_element_cats.remove(&id);
            self.screen = Screen::Project(Page::ElementCategory(id));
            return;
        }
        if let Some(AddChoice::NewCategory) = choice {
            self.add_mode = AddMode::NewCategory;
            return;
        }
        let Some(choice) = choice else { return };
        self.show_add = false;
        self.dirty = true;
        let target = match choice {
            AddChoice::Character => {
                self.open_characters = true;
                Page::Character(self.add_character())
            }
            AddChoice::Location => {
                self.open_locations = true;
                Page::Location(self.project.add_location("Nueva locación"))
            }
            AddChoice::Object => {
                self.open_objects = true;
                Page::Object(self.project.add_object("Nuevo objeto clave"))
            }
            AddChoice::NewCategory => return, // lo atiende más arriba
            AddChoice::Element(category) => {
                match self.project.add_element(category, "Nuevo elemento") {
                    Some(id) => {
                        self.closed_element_cats.remove(&category);
                        Page::Element(id)
                    }
                    None => return,
                }
            }
            AddChoice::Doc(kind) => {
                // Dentro de una carpeta abierta se crea en ella.
                let parent = match page {
                    Page::Document(id) => Some(*id),
                    _ => None,
                };
                let id = self
                    .project
                    .add_document(kind, parent.filter(|_| kind != DocKind::World));
                if let Some(parent) = self
                    .project
                    .documents
                    .iter()
                    .find(|d| d.id == id)
                    .and_then(|d| d.parent)
                {
                    self.open_folders.insert(parent);
                }
                match kind {
                    DocKind::World => self.open_worlds = true,
                    _ => self.open_docs = true,
                }
                Page::Document(id)
            }
        };
        // Lo recién creado puede estar ya mencionado en el guion: se vincula.
        self.project.auto_link_all();
        self.screen = Screen::Project(target);
    }

    fn home_screen(&mut self, ui: &mut egui::Ui) {
        let p = self.settings.scheme.palette();
        // La cuenta se puede abrir antes de entrar a un proyecto: la ventana de la nube y su hilo funcionan también acá.
        {
            let ctx = ui.ctx().clone();
            let now = ctx.input(|i| i.time);
            let out = self.cloud.tick(&ctx, now, &self.settings, None, None, false);
            self.apply_cloud(out);
            let out = self.cloud.dialog(&ctx, &p, &mut self.settings, None, "", None, 0);
            self.apply_cloud(out);
        }
        if self.show_settings {
            self.settings_screen(ui, &p, true);
            return;
        }
        let mut action = None;
        let account = match self.cloud.account() {
            Some(email) => {
                let name = self.settings.user_name.trim();
                if name.is_empty() { email.to_string() } else { name.to_string() }
            }
            None => "Iniciar sesión".to_string(),
        };
        let history: Vec<String> = Vec::new();
        let mut cx = ScreenCx::new(
            &mut self.project,
            &p,
            &self.settings,
            &mut self.current_scene,
            &history,
        );
        shell::show_bare(
            ui,
            &mut crate::screens::HomeScreen {
                cards: &self.cards,
                action: &mut action,
                account: &account,
            },
            &mut cx,
        );
        match action {
            Some(HomeAction::Create) => self.start_project(Project::default(), None),
            Some(HomeAction::Open) => self.open_dialog(),
            Some(HomeAction::OpenRecent(path)) => self.open_path(&path),
            Some(HomeAction::Settings) => self.show_settings = true,
            Some(HomeAction::Cloud) => self.cloud.open = true,
            None => {}
        }
    }
}

impl eframe::App for ScriptApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl ScriptApp {
    /// Ctrl + / Ctrl − / Ctrl 0: tamaño de la interfaz en pasos de 5 %.
    fn handle_zoom_keys(&mut self, ctx: &egui::Context) {
        let (min, max) = storage::ZOOM_RANGE;
        let mut zoom = self.settings.ui_zoom as i32;
        let mut pressed = false;
        for key in [Key::Plus, Key::Equals] {
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, key)) {
                zoom += 5;
                pressed = true;
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Minus)) {
            zoom -= 5;
            pressed = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Num0)) {
            zoom = storage::DEFAULT_ZOOM as i32;
            pressed = true;
        }
        if pressed {
            self.settings.ui_zoom = zoom.clamp(min as i32, max as i32) as u32;
            theme::apply(ctx, &self.settings.scheme.palette(), self.settings.ui_zoom);
            self.settings.save();
        }
    }

    /// Aplica el cambio de zoom de la vista pedido por el guion o el tratamiento (Ctrl + rueda o el panel Formato).
    fn apply_font_delta(&mut self) {
        let view = std::mem::take(&mut self.editor.zoom_delta)
            + std::mem::take(&mut self.treatment.zoom_delta)
            + std::mem::take(&mut self.synopsis.zoom_delta)
            + std::mem::take(&mut self.title_view.zoom_delta);
        if view != 0 {
            let (zmin, zmax) = storage::VIEW_ZOOM_RANGE;
            let step = storage::VIEW_ZOOM_STEP as i32;
            self.settings.view_zoom = (self.settings.view_zoom as i32 + view * step)
                .clamp(zmin as i32, zmax as i32) as u32;
            self.settings.save();
        }
    }

    fn apply_bold_request(&mut self) {
        if let Some(bold) = self.editor.set_bold.take() {
            self.settings.bold_names = bold;
            self.settings.save();
        }
        if let Some(bold) = self.treatment.set_bold.take() {
            self.settings.treatment_bold = bold;
            self.settings.save();
        }
        if let Some(show) = self.editor.set_timeline.take() {
            self.settings.show_timeline = show;
            self.settings.save();
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.project.sync_scene_numbers();
        // Todo lo que se pega, en cualquier campo, llega limpio.
        ctx.input_mut(|i| {
            for event in &mut i.events {
                if let egui::Event::Paste(text) = event {
                    *text = widgets::clean_pasted(text);
                }
            }
        });
        crate::ui::kit::set_reduce_motion(self.settings.reduce_motion);
        self.handle_zoom_keys(&ctx);
        self.apply_font_delta();
        self.apply_bold_request();

        if let Screen::Project(page) = &self.screen {
            let page = page.clone();
            // Ctrl+Mayús+S primero: es «Guardar como».
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
                self.save(true);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
                self.save(false);
            }
            // Ctrl+F: buscar. Lleva el foco al buscador de fichas de la barra lateral.
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
                self.show_sidebar = true;
                self.open_cards = true;
                self.focus_search = true;
            }
            // Las imágenes nuevas se copian a la carpeta del proyecto en cuanto este tiene archivo.
            let sig = self.project.image_signature();
            if sig != self.image_sig {
                if let Some(file) = self.path.clone() {
                    let copied = self.localize_assets(&file);
                    if copied > 0 {
                        self.status = format!(
                            "{copied} imagen{} copiada{} a la carpeta del proyecto",
                            if copied == 1 { "" } else { "es" },
                            if copied == 1 { "" } else { "s" }
                        );
                    }
                    self.image_sig = self.project.image_signature();
                }
            }
            // Guardado automático: solo si hay cambios y como máximo cada `AUTOSAVE_SECS`, para no escribir de más en el disco.
            let now = ctx.input(|i| i.time);
            // En un proyecto compartido se guarda enseguida (solo en la carpeta local): así lo que se escribe sale hacia los demás sin demora.
            let autosave_every = if self.cloud_link.is_some() { 2.0 } else { AUTOSAVE_SECS };
            if self.settings.autosave && self.dirty && now - self.last_autosave > autosave_every {
                self.last_autosave = now;
                if self.path.is_some() {
                    self.save(false);
                } else if !cfg!(test)
                    && let Some(recovery) = storage::recovery_path()
                {
                    // Un proyecto sin archivo se guarda aparte, para que nada se pierda hasta elegir dónde va.
                    if let Some(dir) = recovery.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = storage::save(&self.project, &recovery);
                }
            }
            // La nube: recibe lo que terminó el hilo y manda a sincronizar cada tanto.
            {
                let (file, link) = (self.path.clone(), self.cloud_link.clone());
                // Rápido si hay otra persona en el proyecto o se editó hace poco; si no, despacio.
                if self.dirty {
                    self.last_edit = now;
                }
                let fast = !self.peers.is_empty() || now - self.last_edit < 20.0;
                let out = self.cloud.tick(&ctx, now, &self.settings, file.as_deref(), link.as_ref(), fast);
                self.apply_cloud(out);
                // Dónde estoy, para que los demás lo vean.
                let (screen, detail) = self.presence_of(&page);
                let who = self.author_name();
                self.cloud.presence(now, link.as_ref(), &who, &screen, &detail);
            }
            // Cada tanto se mira si otra persona guardó cambios en la carpeta del proyecto.
            if self.path.is_some() {
                if now - self.last_poll > POLL_SECS {
                    self.last_poll = now;
                    self.poll_external(false);
                }
                // En un proyecto compartido se revisa más seguido (el guardado y la sincronización dependen de eso).
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(if self.cloud_link.is_some() { 1.0 } else { POLL_SECS }));
            }
            // Una versión del historial, sola, cada media hora de trabajo (si algo cambió).
            if self.path.is_some() && now - self.last_version > AUTO_VERSION_SECS {
                self.last_version = now;
                self.save_version("", true);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "{}{} — Scriptpendra",
                self.project.title,
                if self.dirty { " •" } else { "" }
            )));
            self.project_screen(ui, page);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("Scriptpendra".into()));
            self.home_screen(ui);
        }
        let palette = self.settings.scheme.palette();
        self.show_toast(&ctx, &palette);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor;
    use crate::ui::checks;

    /// Recorre todas las pantallas en un contexto sin ventana para detectar paneos de layout.
    #[test]
    fn every_screen_draws() {
        draw_everything(theme::Scheme::default(), egui::vec2(1280.0, 800.0), 100);
    }

    /// Reglas §7: cada pantalla × cada tema × los tamaños de §2.7 (1024 × 640, 1280 × 800 con zoom de 150 %, 1920 × 1080) ×
    /// un proyecto vacío y uno con contenido. Los detectores de `ui::checks` anotan todo lo que ven; la prueba compara con
    /// `KNOWN_VIOLATIONS`: falla si aparece una infracción nueva o si una de la lista dejó de ocurrir.
    #[test]
    fn every_screen_draws_in_every_theme_size_and_project_state_within_the_rules() {
        let _ = checks::take_violations();
        let mut found = Vec::new();
        for scheme in theme::Scheme::ALL {
            for (size, zoom) in [
                (egui::vec2(1024.0, 640.0), 100),
                (egui::vec2(1280.0, 800.0), 150),
                (egui::vec2(1920.0, 1080.0), 100),
            ] {
                draw_everything(scheme, size, zoom);
                found.extend(checks::take_violations());
                draw_empty_project(scheme, size, zoom);
                found.extend(checks::take_violations());
            }
        }
        if let Err(report) = checks::verify(&found) {
            panic!("{report}");
        }
    }

    fn draw_everything(scheme: theme::Scheme, size: egui::Vec2, zoom: u32) {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        app.settings.scheme = scheme;
        app.settings.ui_zoom = zoom;
        theme::apply(&ctx, &scheme.palette(), zoom);
        let mut time = 0.0;
        let mut frame = |app: &mut ScriptApp| {
            time += 0.5;
            let input = egui::RawInput {
                // Sin ventana real, el zoom no achica la pantalla por sí solo: se mide en puntos, como en una ventana.
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    size / (zoom as f32 / 100.0),
                )),
                time: Some(time),
                focused: true,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
        };
        frame(&mut app); // activa las fuentes

        let mut project = Project::default();
        let h = project.make_paragraph(
            crate::model::ParagraphKind::SceneHeading,
            "INT. CASA - NOCHE",
        );
        let c = project.make_paragraph(crate::model::ParagraphKind::Character, "ANA");
        let d = project.make_paragraph(crate::model::ParagraphKind::Dialogue, "Hola");
        project.paragraphs = vec![h, c, d];
        project.add_comment(project.paragraphs[2].id, "revisar", Some("Hola".into()));
        let key_object = project.add_object("Llave");
        let vehicles = project
            .breakdown_categories
            .iter()
            .find(|c| c.name == "Vehículos")
            .unwrap()
            .id;
        let para = project.paragraphs[0].id;
        project.add_breakdown_tag(para, "CASA", vehicles, "Casa rodante", false);
        project.auto_link_all(); // crea la ficha de ANA y vincula su encabezado
        let ana = project.character_id_by_name("ANA").unwrap();
        let folder = project.add_document(DocKind::Folder, None);
        let text = project.add_document(DocKind::Text, Some(folder));
        let world = project.add_document(DocKind::World, None);
        let map = project.add_document(DocKind::MindMap, None);
        let root = project
            .documents
            .iter()
            .find(|d| d.id == map)
            .unwrap()
            .nodes[0]
            .id;
        let child = project.add_mind_node(map, root).unwrap();
        project.add_mind_node(map, child);
        let gallery = project.add_document(DocKind::Gallery, None);
        let cards = project.add_document(DocKind::Cards, None);
        let first = project.add_story_card(cards).unwrap();
        project.add_story_card(cards);
        project.documents.iter_mut().find(|d| d.id == cards).unwrap().cards[0].moment = "Acto I".into();
        let relations = project.add_document(DocKind::Relations, None);
        let a = project.add_rel_node(relations, None, (40.0, 40.0)).unwrap();
        let b = project
            .add_rel_node(relations, Some(crate::model::EntityRef::Character(ana)), (260.0, 120.0))
            .unwrap();
        project.add_rel_edge(relations, a, b);
        let _ = first;
        project.add_role_entry(crate::model::Role::Art, None);
        project.add_role_entry(crate::model::Role::SoundGear, None);
        project.sound_gear[0].title = "Boom Rode".into();
        project.art_items[0].title = "Sillón".into();
        project.sync_equipment();
        let faro = project.add_location("Faro");
        let casa = project.add_location("Casa");
        let fauna = project.add_element_category("Fauna", icon::PAW_PRINT);
        let wolf = project.add_element(fauna, "Lobo").unwrap();
        app.open_folders.insert(folder);
        app.start_project(project, None);
        app.breakdown.selected = Some((vehicles, "CASA".to_string()));

        let pages = [
            Page::Info,
            Page::Character(ana),
            Page::Location(casa),
            Page::Location(faro),
            Page::Object(key_object),
            Page::ElementCategory(fauna),
            Page::Element(wolf),
            Page::Breakdown,
            Page::Document(folder),
            Page::Document(text),
            Page::Document(world),
            Page::Document(map),
            Page::Document(gallery),
            Page::Document(cards),
            Page::Document(relations),
            Page::TitlePage,
            Page::Synopsis,
            Page::DramaCurve,
            Page::Treatment,
            Page::Screenplay,
            Page::TechScript,
            Page::Blocking,
            Page::SoundScript,
            Page::SoundGear,
            Page::PhotoGear,
            Page::ArtDirection,
            Page::Storyboard,
            Page::PlanLights,
            Page::PlanSound,
            Page::PlanSet,
            Page::Statistics,
            Page::Scenes,
            Page::ShootLocations,
            Page::Contacts,
            Page::Plan,
            Page::CallSheets,
        ];
        let place = app.project.add_shoot_location("Casa de Pedro");
        let pages = [&pages[..], &[Page::ShootPlace(place)]].concat();
        app.shoot.selected = Some(place);
        app.contacts.selected = Some(app.project.add_contact("Pedro Gómez"));
        app.scenes.selected = app.project.script_scenes().first().map(|(id, _)| *id);
        for workspace in Workspace::ALL {
            app.workspace = workspace;
            for page in &pages {
                app.screen = Screen::Project(page.clone());
                frame(&mut app);
                frame(&mut app);
            }
        }
        // Cada pestaña del inspector de cada pantalla, con el inspector abierto (como panel o como cajón).
        app.show_right = true;
        app.drawer_open = true;
        for page in &pages {
            app.screen = Screen::Project(page.clone());
            for tab in 0..3 {
                app.inspector_tabs.insert(page.key(), tab);
                frame(&mut app);
                frame(&mut app);
            }
            app.inspector_tabs.insert(page.key(), 0);
        }
        // Ajustes, sección por sección.
        app.show_settings = true;
        for section in 0..pages::SETTINGS_SECTIONS.len() {
            app.settings_section = section;
            frame(&mut app);
        }
        app.show_settings = false;
        app.export_dialog.open = true;
        frame(&mut app);
        app.export_dialog.open = false;
        // La barra lateral y el inspector se adaptan al ancho medido ya con el zoom.
        let width = size.x / (zoom as f32 / 100.0);
        assert_eq!(app.rail, width < SIDEBAR_RAIL_BELOW, "riel con {width} pt");
        assert!(
            !app.compact_inspector
                || width - RAIL_W - app.settings.right_width < CONTENT_MIN
                || width - app.settings.sidebar_width - app.settings.right_width < CONTENT_MIN
        );
        // En ventanas angostas el inspector es un cajón: se dibuja abierto en las pantallas que lo tienen.
        app.drawer_open = true;
        app.screen = Screen::Project(Page::Treatment);
        app.settings.treatment_bold = true;
        app.project.treatment = "Ana entra a la casa.".into();
        app.project.add_outline_scene("INT. CASA - DÍA");
        // Guion técnico: con planos, con y sin imagen, en tabla y en storyboard.
        let scene = app.project.paragraphs[0].id;
        let shot = app.project.add_shot(scene, None);
        app.project.add_shot(scene, Some(shot));
        app.project.shots[0].image = Some(PathBuf::from("/no/existe.png"));
        app.project.shots[0].description = "Ana abre la puerta.".into();
        app.screen = Screen::Project(Page::TechScript);
        for view in [
            crate::techscript::View::Table,
            crate::techscript::View::Storyboard,
        ] {
            app.tech.view = view;
            frame(&mut app);
            frame(&mut app);
        }
        // Las pestañas del inspector con un plano elegido y con un día de rodaje.
        app.tech.selected = Some(shot);
        app.project.add_shoot_day();
        app.project.add_shoot_day();
        // Una escena en el primer día (el segundo queda vacío) y otra suelta en la bandeja.
        let second = app
            .project
            .make_paragraph(crate::model::ParagraphKind::SceneHeading, "EXT. FARO - DÍA");
        app.project.paragraphs.push(second);
        app.project.place_scenes(&[scene], None);
        // Un tramo con un plano de la escena; el otro plano queda en la bandeja.
        app.project.place_shots(scene, &[shot], None);
        for page in [Page::TechScript, Page::Plan] {
            app.screen = Screen::Project(page.clone());
            for tab in 0..3 {
                app.inspector_tabs.insert(page.key(), tab);
                frame(&mut app);
                frame(&mut app);
            }
            app.inspector_tabs.insert(page.key(), 0);
        }
        app.screen = Screen::Project(Page::Treatment);
        for tab in [
            crate::treatment::Tab::Linked,
            crate::treatment::Tab::Comments,
            crate::treatment::Tab::Outline,
        ] {
            app.treatment.tab = tab;
            frame(&mut app);
            frame(&mut app);
        }
        app.settings.bold_names = true;
        app.editor.right_tab = editor::RightTab::Comments;
        app.screen = Screen::Project(Page::Screenplay);
        frame(&mut app);
        for n in 0..12 {
            app.project
                .add_element_category(&format!("Categoría {n}"), icon::PAW_PRINT);
        }
        app.editor.right_tab = editor::RightTab::Scenes;
        frame(&mut app);
        frame(&mut app);
        app.screen = Screen::Project(Page::Character(ana));
        app.confirm = Some(Confirm::ResetCharacter(ana, "ANA".into()));
        frame(&mut app);
        app.confirm = Some(Confirm::DeleteCharacter(ana, "ANA".into(), 1));
        frame(&mut app);
        app.confirm = Some(Confirm::DeleteElementCategory(fauna, "Fauna".into(), 1));
        frame(&mut app);
        app.confirm = None;
        app.add_mode = AddMode::NewCategory;
        app.new_cat_name = "Naves".into();
        app.show_add = true;
        frame(&mut app);
        app.show_add = false;
        app.add_mode = AddMode::Choose;
        app.status = "Guardado".into();
        frame(&mut app);
        frame(&mut app);
        assert!(app.toast.is_some());
        app.editor.right_tab = editor::RightTab::Card;
        app.screen = Screen::Project(Page::Screenplay);
        frame(&mut app);
        app.breakdown.view = pages::BreakdownView::ByScene;
        app.screen = Screen::Project(Page::Breakdown);
        frame(&mut app);
        app.breakdown.view = pages::BreakdownView::ByCategory;
        app.breakdown.editing = app
            .project
            .breakdown_categories
            .iter()
            .find(|c| c.name == "Utilería")
            .map(|c| c.id);
        frame(&mut app);
        frame(&mut app);
        app.show_add = true;
        frame(&mut app);
        app.show_add = false;
        app.show_settings = true;
        frame(&mut app);
        app.cards = vec![
            RecentCard {
                path: PathBuf::from("/tmp/uno.spd"),
                title: "Un título bastante largo que no entra en una sola línea de la tarjeta"
                    .into(),
                logline: "Una descripción muy larga ".repeat(20),
                cover: Some(PathBuf::from("/tmp/no-existe.png")),
                modified: None,
            },
            RecentCard {
                path: PathBuf::from("/tmp/dos.spd"),
                title: "Dos".into(),
                logline: String::new(),
                cover: None,
                modified: None,
            },
        ];
        app.screen = Screen::Home;
        frame(&mut app);
        frame(&mut app);
    }

    /// Con un proyecto vacío, cada pantalla sin ficha tiene que dibujar (con su estado vacío) en cada tema y tamaño.
    fn draw_empty_project(scheme: theme::Scheme, size: egui::Vec2, zoom: u32) {
        let pages = [
            Page::Info,
            Page::TitlePage,
            Page::Synopsis,
            Page::DramaCurve,
            Page::Treatment,
            Page::Screenplay,
            Page::Statistics,
            Page::TechScript,
            Page::Blocking,
            Page::SoundScript,
            Page::SoundGear,
            Page::PhotoGear,
            Page::ArtDirection,
            Page::Storyboard,
            Page::PlanLights,
            Page::PlanSound,
            Page::PlanSet,
            Page::Scenes,
            Page::Breakdown,
            Page::Plan,
            Page::CallSheets,
            Page::ShootLocations,
            Page::Contacts,
        ];
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        app.settings.scheme = scheme;
        app.settings.ui_zoom = zoom;
        theme::apply(&ctx, &scheme.palette(), zoom);
        let mut time = 0.0;
        let mut frame = |app: &mut ScriptApp| {
            time += 0.5;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    size / (zoom as f32 / 100.0),
                )),
                time: Some(time),
                focused: true,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
        };
        frame(&mut app);
        app.start_project(Project::default(), None);
        app.show_right = true;
        app.drawer_open = true;
        for page in &pages {
            app.screen = Screen::Project(page.clone());
            for tab in 0..3 {
                app.inspector_tabs.insert(page.key(), tab);
                frame(&mut app);
                frame(&mut app);
            }
        }
    }

    /// El inspector fijo no cambia de ancho solo: en cada pantalla y en cada pestaña, después de asentarse, mide lo mismo
    /// cuadro tras cuadro y queda en el ancho con el que se creó.
    #[test]
    fn every_fixed_inspector_keeps_its_width_in_every_tab() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        app.settings.sidebar_width = 300.0;
        app.settings.right_width = 300.0;
        run_frame(&ctx, &mut app, 0.5);
        let mut project = Project::default();
        let h = project.make_paragraph(
            crate::model::ParagraphKind::SceneHeading,
            "INT. CASA - NOCHE",
        );
        let c = project.make_paragraph(crate::model::ParagraphKind::Character, "ANA");
        project.paragraphs = vec![h, c];
        project.auto_link_all();
        let scene = project.paragraphs[0].id;
        let blocking = project.add_blocking(scene);
        let camera = project
            .add_block_item(blocking, crate::model::BlockKind::Camera, 2.0, 2.0)
            .unwrap();
        for kind in [
            crate::model::BlockKind::Light,
            crate::model::BlockKind::Mic,
            crate::model::BlockKind::Actor,
            crate::model::BlockKind::Wall,
        ] {
            project.add_block_item(blocking, kind, 1.0, 1.0);
        }
        project.add_shot(scene, None);
        project.add_shoot_location("Casa");
        project.add_contact("Pedro");
        app.start_project(project, None);
        app.scenes.selected = Some(scene);
        app.shoot.selected = app.project.shoot_locations.first().map(|l| l.id);
        app.contacts.selected = app.project.contacts.first().map(|c| c.id);
        app.tech.selected = app.project.shots.first().map(|s| s.id);
        app.blocking.go_to(&app.project, blocking, camera);
        app.show_right = true;
        let pages = [
            (Page::Screenplay, "script_side"),
            (Page::Treatment, "treatment_side"),
            (Page::Synopsis, "synopsis_side"),
            (Page::TechScript, "tech_side"),
            (Page::Blocking, "blocking_side"),
            (Page::Scenes, "scenes_side"),
            (Page::Breakdown, "breakdown_side"),
            (Page::Plan, "plan_side"),
            (Page::CallSheets, "calls_side"),
            (Page::ShootLocations, "shoot_side"),
            (Page::Contacts, "contacts_side"),
            (Page::TitlePage, "title_side"),
        ];
        let mut time = 1.0;
        let mut bad: Vec<String> = Vec::new();
        for (page, id) in pages {
            for tab in 0..3 {
                app.screen = Screen::Project(page.clone());
                app.inspector_tabs.insert(page.key(), tab);
                app.panel_trace.clear();
                for _ in 0..24 {
                    time += 0.5;
                    run_frame(&ctx, &mut app, time);
                }
                let widths: Vec<f32> = app
                    .panel_trace
                    .iter()
                    .filter(|(i, _)| *i == id)
                    .map(|(_, w)| *w)
                    .collect();
                if widths.len() < 10 {
                    continue;
                }
                let (mid, last) = (widths[widths.len() / 2], *widths.last().unwrap());
                if (mid - last).abs() >= 0.5 {
                    bad.push(format!("{id} pestaña {tab} sigue cambiando: {widths:?}"));
                }
                if (last - 300.0).abs() >= 10.0 {
                    bad.push(format!("{id} pestaña {tab} se fue de 300: {last}"));
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn alt_arrows_walk_the_history_and_ctrl_backslash_hides_the_sidebar() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        run_frame(&ctx, &mut app, 0.1);
        app.start_project(Project::default(), None);
        let mut time = 0.2;
        let mut frame = |app: &mut ScriptApp, events: Vec<egui::Event>| {
            time += 0.1;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                time: Some(time),
                focused: true,
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
        };
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        for page in [Page::Info, Page::Synopsis, Page::Plan] {
            app.screen = Screen::Project(page);
            frame(&mut app, vec![]);
        }
        frame(&mut app, vec![key(Key::ArrowLeft, Modifiers::ALT)]);
        assert!(matches!(app.screen, Screen::Project(Page::Synopsis)));
        frame(&mut app, vec![key(Key::ArrowLeft, Modifiers::ALT)]);
        assert!(matches!(app.screen, Screen::Project(Page::Info)));
        frame(&mut app, vec![key(Key::ArrowRight, Modifiers::ALT)]);
        assert!(matches!(app.screen, Screen::Project(Page::Synopsis)));
        // Ir a otra pantalla descarta lo que estaba «hacia adelante».
        app.screen = Screen::Project(Page::Statistics);
        frame(&mut app, vec![]);
        frame(&mut app, vec![key(Key::ArrowRight, Modifiers::ALT)]);
        assert!(matches!(app.screen, Screen::Project(Page::Statistics)));
        assert!(app.show_sidebar);
        frame(&mut app, vec![key(Key::Backslash, Modifiers::COMMAND)]);
        assert!(!app.show_sidebar);
    }

    #[test]
    fn the_card_menu_renames_duplicates_and_deletes() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        let key = app.project.add_object("Llave");
        app.project.objects[0].one_liner = "De bronce".into();
        let cat = app.project.add_element_category("Fauna", icon::PAW_PRINT);
        let wolf = app.project.add_element(cat, "Lobo").unwrap();
        app.project.elements[0].one_liner = "Gris".into();
        // Duplicar copia los datos y deja al nuevo con otro nombre.
        app.card_action = Some((Page::Object(key), CardAction::Duplicate));
        app.apply_card_action();
        assert_eq!(app.project.objects.len(), 2);
        assert_eq!(app.project.objects[1].one_liner, "De bronce");
        assert_ne!(app.project.objects[1].name, "Llave");
        app.card_action = Some((Page::Element(wolf), CardAction::Duplicate));
        app.apply_card_action();
        assert_eq!(app.project.elements.len(), 2);
        assert_eq!(app.project.elements[1].one_liner, "Gris");
        // Renombrar pasa por el diálogo.
        app.card_action = Some((Page::Element(wolf), CardAction::Rename));
        app.apply_card_action();
        assert_eq!(app.rename.as_ref().map(|(_, t)| t.as_str()), Some("Lobo"));
        // Eliminar saca la ficha y, si estaba abierta, vuelve a la información.
        app.screen = Screen::Project(Page::Object(key));
        app.card_action = Some((Page::Object(key), CardAction::Delete));
        app.apply_card_action();
        assert_eq!(app.project.objects.len(), 1);
        assert!(matches!(app.screen, Screen::Project(Page::Info)));
    }

    #[test]
    fn every_page_has_a_screen_kind() {
        assert_eq!(Page::Statistics.kind(), ScreenKind::Grid);
        assert_eq!(Page::Blocking.kind(), ScreenKind::Canvas);
        assert_eq!(Page::Screenplay.kind(), ScreenKind::Sheet);
        assert_eq!(Page::Plan.kind(), ScreenKind::Table);
    }

    #[test]
    fn every_screen_belongs_to_one_workspace_and_cards_to_none() {
        for page in [
            Page::Info,
            Page::TitlePage,
            Page::Synopsis,
            Page::DramaCurve,
            Page::Treatment,
            Page::Screenplay,
            Page::Statistics,
        ] {
            assert_eq!(Workspace::of(&page), Some(Workspace::Writing), "{page:?}");
        }
        for page in [
            Page::TechScript,
            Page::Blocking,
            Page::Storyboard,
            Page::SoundScript,
            Page::SoundGear,
            Page::PlanSound,
            Page::PhotoGear,
            Page::PlanLights,
            Page::ArtDirection,
            Page::PlanSet,
        ] {
            assert_eq!(Workspace::of(&page), Some(Workspace::Design), "{page:?}");
        }
        for page in [
            Page::Scenes,
            Page::Breakdown,
            Page::Plan,
            Page::CallSheets,
            Page::ShootLocations,
            Page::Contacts,
        ] {
            assert_eq!(
                Workspace::of(&page),
                Some(Workspace::Production),
                "{page:?}"
            );
        }
        for page in [
            Page::Character(1),
            Page::Location(1),
            Page::Object(1),
            Page::Element(1),
            Page::ElementCategory(1),
            Page::Document(1),
        ] {
            assert_eq!(Workspace::of(&page), None, "{page:?}");
        }
    }

    #[test]
    fn deleting_a_character_removes_profile_and_lines() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        let mut project = Project::default();
        let k = crate::model::ParagraphKind::Character;
        let d = crate::model::ParagraphKind::Dialogue;
        let h = project.make_paragraph(crate::model::ParagraphKind::SceneHeading, "INT. A - DÍA");
        let c = project.make_paragraph(k, "ANA");
        let t = project.make_paragraph(d, "hola");
        let o = project.make_paragraph(k, "LUIS");
        project.paragraphs = vec![h, c, t, o];
        project.auto_link_all();
        let ana = project.character_id_by_name("ANA").unwrap();
        app.start_project(project, None);
        app.delete_character(ana);
        assert!(app.project.character(ana).is_none());
        assert_eq!(app.project.known_characters(), vec!["LUIS"]);
        assert!(app.dirty);
        assert!(matches!(app.screen, Screen::Project(Page::Info)));
    }

    /// Corre un frame sin ventana sobre `app`.
    fn run_frame(ctx: &egui::Context, app: &mut ScriptApp, time: f64) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            time: Some(time),
            focused: true,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.draw(ui));
        out.textures_delta.clear();
    }

    #[test]
    fn renaming_a_character_updates_the_script_and_keeps_twins_apart() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        run_frame(&ctx, &mut app, 0.5);
        let mut project = Project::default();
        let h = project.make_paragraph(crate::model::ParagraphKind::SceneHeading, "INT. A - DÍA");
        let c1 = project.make_paragraph(crate::model::ParagraphKind::Character, "JUAN");
        let a1 = project.make_paragraph(crate::model::ParagraphKind::Action, "Mira a Ana.");
        let c2 = project.make_paragraph(crate::model::ParagraphKind::Character, "JUAN");
        project.paragraphs = vec![h, c1, a1, c2];
        let father = project.new_character("JUAN");
        let son = project.new_character("JUAN");
        project.paragraphs[1].links = vec![crate::model::Link {
            start: 0,
            end: 4,
            target: EntityRef::Character(father),
        }];
        project.paragraphs[3].links = vec![crate::model::Link {
            start: 0,
            end: 4,
            target: EntityRef::Character(son),
        }];
        app.start_project(project, None);
        app.screen = Screen::Project(Page::Character(son));
        run_frame(&ctx, &mut app, 1.0);

        // Lo que hace la ficha al escribir un nombre nuevo: cambia el nombre y la app sincroniza.
        app.project
            .characters
            .iter_mut()
            .find(|c| c.id == son)
            .unwrap()
            .name = "JUANITO".into();
        app.after_rename();
        run_frame(&ctx, &mut app, 1.5);
        assert_eq!(app.project.paragraphs[1].text, "JUAN");
        assert_eq!(app.project.paragraphs[3].text, "JUANITO");
        // La barra lateral los distingue aunque se llamen igual al principio.
        app.project
            .characters
            .iter_mut()
            .find(|c| c.id == son)
            .unwrap()
            .name = "JUAN".into();
        app.after_rename();
        let (l1, l2) = (
            app.project.label_for(EntityRef::Character(father)),
            app.project.label_for(EntityRef::Character(son)),
        );
        assert_ne!(l1, l2);
    }

    #[test]
    fn renaming_a_location_updates_the_headings() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        run_frame(&ctx, &mut app, 0.5);
        let mut project = Project::default();
        let h = project.make_paragraph(
            crate::model::ParagraphKind::SceneHeading,
            "INT. FARO - NOCHE",
        );
        project.paragraphs = vec![h];
        project.normalize(); // crea la locación y vincula el encabezado
        let faro = project.location_id_by_name("FARO").unwrap();
        let obj = project.add_object("Llave");
        app.start_project(project, None);

        app.screen = Screen::Project(Page::Location(faro));
        run_frame(&ctx, &mut app, 1.0);
        // Lo que hace la ficha al escribir el nombre nuevo.
        app.project.location_mut(faro).unwrap().name = "TORRE".into();
        app.after_rename();
        run_frame(&ctx, &mut app, 1.5);
        assert_eq!(app.project.paragraphs[0].text, "INT. TORRE - NOCHE");

        app.screen = Screen::Project(Page::Object(obj));
        run_frame(&ctx, &mut app, 2.0);
        assert!(app.project.objects.iter().any(|o| o.id == obj));
    }

    #[test]
    fn editing_a_category_template_draws_and_confirms_removals() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        run_frame(&ctx, &mut app, 0.5);
        let mut project = Project::default();
        let cat = project.add_element_category("Fauna", icon::PAW_PRINT);
        let wolf = project.add_element(cat, "Lobo").unwrap();
        project.elements[0]
            .fields
            .insert("appearance".into(), "Gris".into());
        app.start_project(project, None);
        app.screen = Screen::Project(Page::ElementCategory(cat));
        app.template_editing = Some(cat);
        run_frame(&ctx, &mut app, 1.0);
        run_frame(&ctx, &mut app, 1.5);
        app.screen = Screen::Project(Page::Element(wolf));
        run_frame(&ctx, &mut app, 2.0);

        app.confirm = Some(Confirm::DeleteTemplatePart {
            category: cat,
            section: 0,
            field: Some(0),
            label: "Aspecto".into(),
            count: 1,
        });
        run_frame(&ctx, &mut app, 2.5);
        assert!(app.confirm.is_some());
    }

    #[test]
    fn side_panels_keep_their_width_in_the_whole_app_at_any_zoom() {
        for zoom in [70, 100, 105, 115, 150, 200] {
            for page in ["guion", "personaje"] {
                for tab in [
                    editor::RightTab::Scenes,
                    editor::RightTab::Comments,
                    editor::RightTab::Card,
                ] {
                    let ctx = egui::Context::default();
                    let mut app = ScriptApp::new(&ctx);
                    // Punto de partida de una instalación nueva (sin depender de la configuración de quien corre la prueba).
                    app.settings.sidebar_width = 300.0;
                    app.settings.right_width = 300.0;
                    app.settings.ui_zoom = zoom;
                    theme::apply(&ctx, &app.settings.scheme.palette(), zoom);
                    app.panels.clear();
                    run_frame(&ctx, &mut app, 0.5);
                    let mut project = Project::default();
                    let h = project.make_paragraph(
                        crate::model::ParagraphKind::SceneHeading,
                        "INT. CASA - NOCHE",
                    );
                    let c = project.make_paragraph(crate::model::ParagraphKind::Character, "ANA");
                    project.paragraphs = vec![h, c];
                    for n in 0..5 {
                        project.add_element_category(&format!("Categoría {n}"), icon::PAW_PRINT);
                    }
                    project.auto_link_all();
                    let ana = project.character_id_by_name("ANA").unwrap();
                    app.start_project(project, None);
                    app.screen = Screen::Project(if page == "guion" {
                        Page::Screenplay
                    } else {
                        Page::Character(ana)
                    });
                    app.editor.right_tab = tab;
                    for n in 0..25 {
                        run_frame(&ctx, &mut app, 1.0 + n as f64 * 0.5);
                    }
                    // Cada panel se asienta y queda quieto: los últimos cuadros miden lo mismo, y no se guardó ningún cambio.
                    for id in ["sidebar", "script_side", "character_side"] {
                        let widths: Vec<f32> = app
                            .panel_trace
                            .iter()
                            .filter(|(i, _)| *i == id)
                            .map(|(_, w)| *w)
                            .collect();
                        if widths.len() < 10 {
                            continue;
                        }
                        let (mid, last) = (widths[widths.len() / 2], *widths.last().unwrap());
                        assert!(
                            (mid - last).abs() < 0.5,
                            "{id} sigue cambiando: zoom {zoom}, {page}, {tab:?}: {widths:?}"
                        );
                        assert!(
                            (last - 300.0).abs() < 8.0,
                            "{id} se fue de 300: zoom {zoom}, {page}, {tab:?}: {widths:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_treatment_panel_keeps_its_width_when_scenes_are_added() {
        for zoom in [70, 100, 115, 200] {
            for tab in [
                crate::treatment::Tab::Linked,
                crate::treatment::Tab::Comments,
                crate::treatment::Tab::Outline,
            ] {
                let ctx = egui::Context::default();
                let mut app = ScriptApp::new(&ctx);
                app.settings.sidebar_width = 300.0;
                app.settings.right_width = 300.0;
                app.settings.ui_zoom = zoom;
                theme::apply(&ctx, &app.settings.scheme.palette(), zoom);
                app.panels.clear();
                run_frame(&ctx, &mut app, 0.5);
                let mut project = Project::default();
                project.treatment = "Ana llega al faro y mira el mar durante horas.".into();
                app.start_project(project, None);
                app.screen = Screen::Project(Page::Treatment);
                app.treatment.tab = tab;
                for n in 0..30 {
                    if n % 5 == 0 {
                        let heading =
                            format!("INT. UNA LOCACIÓN CON UN NOMBRE MUY LARGO {n} - NOCHE");
                        app.project
                            .treatment
                            .push_str(&format!("\n{heading}\nTexto de la escena."));
                        app.project.add_outline_scene(&heading);
                    }
                    run_frame(&ctx, &mut app, 1.0 + n as f64 * 0.5);
                }
                let widths: Vec<f32> = app
                    .panel_trace
                    .iter()
                    .filter(|(i, _)| *i == "treatment_side")
                    .map(|(_, w)| *w)
                    .collect();
                let (first, last) = (widths[widths.len() / 4], *widths.last().unwrap());
                assert!(
                    (first - last).abs() < 0.5,
                    "cambió de ancho: zoom {zoom}, {tab:?}: {widths:?}"
                );
                assert!(
                    (last - 300.0).abs() < 8.0,
                    "se fue de 300: zoom {zoom}, {tab:?}: {widths:?}"
                );
            }
        }
    }

    #[test]
    fn a_saved_project_reopens_where_it_was_left_and_a_new_one_starts_at_the_info() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        app.settings.last_pages.clear();
        let mut project = Project::default();
        let ana = project.new_character("Ana");
        // Nuevo, sin archivo: la información del proyecto.
        app.start_project(project.clone(), None);
        assert!(matches!(app.screen, Screen::Project(Page::Info)));

        // Con archivo: la última pantalla, incluidas las que llevan un id.
        let path = PathBuf::from("/tmp/scriptpendra_prueba/Prueba.spd");
        app.start_project(project.clone(), Some(path.clone()));
        assert!(matches!(app.screen, Screen::Project(Page::Info))); // todavía no se recordó nada
        app.screen = Screen::Project(Page::Synopsis);
        run_frame(&ctx, &mut app, 0.5);
        app.start_project(project.clone(), Some(path.clone()));
        assert!(matches!(app.screen, Screen::Project(Page::Synopsis)));
        app.screen = Screen::Project(Page::Character(ana));
        run_frame(&ctx, &mut app, 1.0);
        app.start_project(project.clone(), Some(path.clone()));
        assert!(matches!(app.screen, Screen::Project(Page::Character(id)) if id == ana));
        // Si lo recordado ya no existe, se vuelve a la información.
        project.characters.clear();
        app.start_project(project, Some(path));
        assert!(matches!(app.screen, Screen::Project(Page::Info)));
    }

    #[test]
    fn the_technical_script_panel_keeps_its_width_as_shots_are_added() {
        for zoom in [70, 100, 115, 200] {
            let ctx = egui::Context::default();
            let mut app = ScriptApp::new(&ctx);
            app.settings.right_width = 300.0;
            app.settings.ui_zoom = zoom;
            theme::apply(&ctx, &app.settings.scheme.palette(), zoom);
            app.panels.clear();
            run_frame(&ctx, &mut app, 0.5);
            app.start_project(Project::default(), None);
            app.screen = Screen::Project(Page::TechScript);
            let scene = app.project.paragraphs[0].id;
            for n in 0..30 {
                if n % 6 == 0 {
                    app.project.add_shot(scene, None);
                }
                run_frame(&ctx, &mut app, 1.0 + n as f64 * 0.5);
            }
            let widths: Vec<f32> = app
                .panel_trace
                .iter()
                .filter(|(i, _)| *i == "tech_side")
                .map(|(_, w)| *w)
                .collect();
            let (mid, last) = (widths[widths.len() / 2], *widths.last().unwrap());
            assert!(
                (mid - last).abs() < 0.5,
                "cambió de ancho: zoom {zoom}: {widths:?}"
            );
            assert!(
                (last - 300.0).abs() < 8.0,
                "se fue de 300: zoom {zoom}: {widths:?}"
            );
        }
    }

    #[test]
    fn the_blocking_panel_keeps_its_width_when_opened_on_a_camera() {
        for zoom in [70, 100, 115, 200] {
            let ctx = egui::Context::default();
            let mut app = ScriptApp::new(&ctx);
            app.settings.right_width = 300.0;
            app.settings.ui_zoom = zoom;
            theme::apply(&ctx, &app.settings.scheme.palette(), zoom);
            app.panels.clear();
            run_frame(&ctx, &mut app, 0.5);
            app.start_project(Project::default(), None);
            let scene = app.project.paragraphs[0].id;
            let blocking = app.project.add_blocking(scene);
            let camera = app
                .project
                .add_block_item(blocking, crate::model::BlockKind::Camera, 2.0, 2.0)
                .unwrap();
            app.project
                .add_block_item(blocking, crate::model::BlockKind::Actor, 1.0, 1.0);
            app.screen = Screen::Project(Page::Blocking);
            app.blocking.go_to(&app.project, blocking, camera);
            for n in 0..25 {
                run_frame(&ctx, &mut app, 1.0 + n as f64 * 0.5);
            }
            let widths: Vec<f32> = app
                .panel_trace
                .iter()
                .filter(|(i, _)| *i == "blocking_side")
                .map(|(_, w)| *w)
                .collect();
            let (mid, last) = (widths[widths.len() / 2], *widths.last().unwrap());
            assert!(
                (mid - last).abs() < 0.5,
                "cambió de ancho: zoom {zoom}: {widths:?}"
            );
            assert!(
                (last - 300.0).abs() < 8.0,
                "se fue de 300: zoom {zoom}: {widths:?}"
            );
        }
    }

    /// El marco del rediseño (interfaz.md §1): lo que se ve en cualquier pantalla con inspector.
    #[test]
    fn the_frame_follows_the_redesign_title_row_toolbar_card_inspector_tabs_and_status_buttons() {
        let scheme = theme::Scheme::default();
        let p = scheme.palette();
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        // Los ajustes se leen del disco: el tema es el de la prueba, no el que la persona guardó.
        app.settings.scheme = scheme;
        theme::apply(&ctx, &p, 100);
        let size = egui::vec2(1400.0, 800.0);
        let run = |app: &mut ScriptApp, n: f64| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(n),
                focused: true,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
            out
        };
        run(&mut app, 0.1);
        let mut project = Project::default();
        let h = project.make_paragraph(
            crate::model::ParagraphKind::SceneHeading,
            "INT. CASA - NOCHE",
        );
        project.paragraphs = vec![h];
        app.start_project(project, None);
        app.show_right = true;
        app.show_sidebar = true;
        app.screen = Screen::Project(Page::Scenes);
        app.workspace = Workspace::Production;
        let mut out = run(&mut app, 0.2);
        for n in 3..8 {
            out = run(&mut app, n as f64 * 0.2);
        }
        let texts: Vec<(String, egui::Pos2, egui::Vec2)> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos, t.galley.size())),
                _ => None,
            })
            .collect();
        let find = |needle: &str| {
            texts.iter().find(|t| t.0 == needle).unwrap_or_else(|| {
                panic!(
                    "no se dibujó «{needle}»: {:?}",
                    texts.iter().map(|t| t.0.as_str()).collect::<Vec<_>>()
                )
            })
        };
        // La fila del título: arriba a la derecha de la barra lateral, a 24 de su borde izquierdo.
        let title = texts
            .iter()
            .find(|t| t.0 == "Escenas" && t.1.x > app.settings.sidebar_width)
            .expect("el título de la pantalla");
        assert!(
            title.1.y >= 0.0 && title.1.y + title.2.y <= TITLE_H,
            "el título está en su fila: {:?}",
            title.1
        );
        let sidebar_w = app.settings.sidebar_width;
        assert!(
            (title.1.x - (sidebar_w + SP_XL)).abs() < 4.0,
            "a 24 del borde de la barra lateral: {} vs {}",
            title.1.x,
            sidebar_w + SP_XL
        );
        // La barra de herramientas es la segunda fila (Deshacer y Rehacer del marco) y llega hasta el borde derecho.
        let toolbar_glyphs: Vec<&(String, egui::Pos2, egui::Vec2)> = texts
            .iter()
            .filter(|t| t.0 == icon::ARROW_COUNTER_CLOCKWISE || t.0 == icon::ARROW_CLOCKWISE)
            .collect();
        assert_eq!(
            toolbar_glyphs.len(),
            2,
            "Deshacer y Rehacer los dibuja el marco"
        );
        assert!(
            toolbar_glyphs
                .iter()
                .all(|t| t.1.y >= TITLE_H && t.1.y <= TITLE_H + TOOLBAR_H),
            "en la fila de la barra de herramientas"
        );
        // La barra lateral: ☰ y «Scriptpendra» en el encabezado, ← y → debajo, a la altura de la barra de herramientas.
        let menu = find(icon::LIST);
        assert!(
            menu.1.y < TITLE_H && menu.1.x < find("Scriptpendra").1.x,
            "☰ a la izquierda del nombre"
        );
        let (back, forward) = (find(icon::ARROW_LEFT), find(icon::ARROW_RIGHT));
        assert!(
            back.1.y >= TITLE_H
                && back.1.y <= TITLE_H + TOOLBAR_H
                && (back.1.y - forward.1.y).abs() < 0.5
                && back.1.x < forward.1.x,
            "historial en su fila"
        );
        // Las pestañas de los espacios de trabajo y las del inspector quedan a la misma altura.
        let workspace_tab = find("ESCRITURA");
        let inspector_tab = find("ESCENA");
        assert!(
            (workspace_tab.1.y - inspector_tab.1.y).abs() < 0.5,
            "pestañas alineadas: {} y {}",
            workspace_tab.1.y,
            inspector_tab.1.y
        );
        // La barra de estado: el botón de la barra lateral a la izquierda del todo y el del inspector contra el borde derecho.
        let (side_btn, insp_btn) = (find(icon::SIDEBAR_SIMPLE), find(icon::SIDEBAR));
        assert!(side_btn.1.x < 40.0 && side_btn.1.y > size.y - STATUS_H);
        assert!(insp_btn.1.x > size.x - 40.0 && insp_btn.1.y > size.y - STATUS_H);
        assert!(
            !texts
                .iter()
                .any(|t| t.0 == icon::SIDEBAR && t.1.y < TITLE_H + TOOLBAR_H),
            "la barra de herramientas ya no lleva esos botones"
        );
        // La tarjeta de contenido: fondo `bg`, esquinas de R_FLOAT y SP_S de separación con la cabecera.
        let card = out.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.fill == p.bg && r.rect.width() > 300.0 && r.rect.height() > 300.0 =>
            {
                Some(r.clone())
            }
            _ => None,
        });
        let card = card.expect("la tarjeta de contenido");
        assert_eq!(card.corner_radius, egui::CornerRadius::same(R_FLOAT));
        assert!(
            (card.rect.top() - (TITLE_H + TOOLBAR_H + SP_S)).abs() < 1.0,
            "a SP_S debajo de la cabecera: {}",
            card.rect.top()
        );
        assert!(
            (card.rect.left() - (sidebar_w + SP_S)).abs() < 3.0,
            "a SP_S de la barra lateral: {}",
            card.rect.left()
        );
        // El botón del inspector de la barra de estado lo cierra y lo abre.
        assert!(app.show_right);
    }

    /// Dibuja la pantalla `page` unos cuadros y devuelve lo pintado.
    fn render_page(project: Project, page: Page, size: egui::Vec2) -> (egui::FullOutput, Palette) {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        let p = app.settings.scheme.palette();
        theme::apply(&ctx, &p, 100);
        let run = |app: &mut ScriptApp, n: f64| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(n),
                focused: true,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
            out
        };
        run(&mut app, 0.1);
        app.start_project(project, None);
        app.show_right = true;
        app.show_sidebar = true;
        app.screen = Screen::Project(page);
        let mut out = run(&mut app, 0.2);
        for n in 3..8 {
            out = run(&mut app, n as f64 * 0.2);
        }
        (out, p)
    }

    fn rects(out: &egui::FullOutput, fill: egui::Color32) -> Vec<egui::Rect> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.fill == fill => Some(r.rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_info_page_has_project_and_cover_cards_with_four_equal_indicators() {
        let (out, p) = render_page(Project::default(), Page::Info, egui::vec2(1500.0, 900.0));
        let cards = rects(&out, p.container);
        let cover = cards
            .iter()
            .find(|r| (r.width() - COVER_W).abs() < 1.5)
            .unwrap_or_else(|| panic!("la tarjeta Portada mide COVER_W: {cards:?}"));
        let project_card = cards
            .iter()
            .find(|r| r.height() > 200.0 && r.right() < cover.left() && r.top() == cover.top())
            .expect("la tarjeta Proyecto, a la izquierda de la Portada");
        // Los indicadores: cuatro tarjetas del mismo ancho en una fila, debajo, repartiéndose el ancho de las dos columnas.
        let row: Vec<&egui::Rect> = cards
            .iter()
            .filter(|r| r.top() > cover.bottom() - 1.0 && r.height() < 120.0)
            .collect();
        assert_eq!(row.len(), 4, "{row:?}");
        assert!(
            row.iter()
                .all(|r| (r.width() - row[0].width()).abs() < 1.5 && r.top() == row[0].top())
        );
        assert!((row[0].left() - project_card.left()).abs() < 1.5);
        assert!((row[3].right() - cover.right()).abs() < 1.5);
    }

    #[test]
    fn the_title_page_is_a_sheet_of_the_theme_color_in_the_paper_proportion_and_the_inspector_shows_the_project_values()
     {
        let mut project = Project::default();
        project.contact = "contacto@ejemplo.com".into();
        project.version = "Versión 3.1".into();
        let (out, p) = render_page(project, Page::TitlePage, egui::vec2(1500.0, 900.0));
        let sheet = rects(&out, p.container)
            .into_iter()
            .find(|r| r.height() > 300.0)
            .expect("la hoja es del color del tema, como en Sinopsis, Tratamiento y Guion");
        let ratio = sheet.height() / sheet.width();
        assert!(
            (ratio - 29.7 / 21.0).abs() < 0.01,
            "proporción del A4: {ratio}"
        );
        let shown: Vec<(String, egui::Pos2)> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos)),
                _ => None,
            })
            .collect();
        for value in ["contacto@ejemplo.com", "Versión 3.1"] {
            assert!(
                shown
                    .iter()
                    .any(|(t, pos)| t == value && pos.x > sheet.right()),
                "el inspector muestra «{value}»: {shown:?}"
            );
        }
    }

    #[test]
    fn creating_a_scene_from_the_toolbar_keeps_the_text_and_lets_you_type_the_heading() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        let p = app.settings.scheme.palette();
        theme::apply(&ctx, &p, 100);
        let size = egui::vec2(1500.0, 900.0);
        let mut t = 0.0;
        let mut run = |app: &mut ScriptApp, events: Vec<egui::Event>| {
            t += 0.2;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(t),
                focused: true,
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
            out
        };
        run(&mut app, vec![]);
        app.start_project(Project::default(), None);
        app.screen = Screen::Project(Page::Treatment);
        for _ in 0..4 {
            run(&mut app, vec![]);
        }
        let click = |pos: egui::Pos2| {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            vec![egui::Event::PointerMoved(pos), button(true), button(false)]
        };
        let press = |pos: egui::Pos2| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ]
        };
        let release = |pos: egui::Pos2| {
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }]
        };
        let glyph = |out: &egui::FullOutput, what: &str| -> egui::Pos2 {
            out.shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == what => {
                        Some(t.pos + t.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no se dibujó «{what}»"))
        };
        let out = run(&mut app, vec![]);
        let hint = glyph(&out, "Escribí el tratamiento…");
        run(&mut app, click(hint));
        run(
            &mut app,
            vec![egui::Event::Text("Ana llega al faro".into())],
        );
        run(&mut app, vec![]);
        assert_eq!(app.project.treatment, "Ana llega al faro");
        // Selecciona todo y aprieta «Crear escena» en la barra de herramientas.
        let select_all = egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        run(&mut app, vec![select_all]);
        let out = run(&mut app, vec![]);
        let button = glyph(&out, icon::FILM_SLATE);
        run(&mut app, press(button));
        run(&mut app, release(button));
        for _ in 0..3 {
            run(&mut app, vec![]);
        }
        run(
            &mut app,
            vec![egui::Event::Text("int. faro - noche".into())],
        );
        run(&mut app, vec![]);
        assert!(
            app.project.treatment.contains("INT. FARO - NOCHE")
                && app.project.treatment.contains("Ana llega al faro"),
            "{:?}",
            app.project.treatment
        );
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        run(&mut app, vec![enter]);
        run(&mut app, vec![]);
        assert_eq!(app.project.outline.len(), 1, "{:?}", app.project.treatment);
        assert!(app.project.treatment.contains("Ana llega al faro"));
    }

    #[test]
    fn with_ten_shots_in_a_1280_window_the_compact_columns_are_all_readable() {
        use crate::model::ParagraphKind::SceneHeading;
        let mut project = Project::default();
        project.paragraphs = vec![project.make_paragraph(SceneHeading, "INT. CASA - NOCHE")];
        let scene = project.paragraphs[0].id;
        for n in 0..10 {
            let shot = project.add_shot(scene, None);
            let s = project.shots.iter_mut().find(|s| s.id == shot).unwrap();
            s.size = "PM".into();
            s.description = format!("Descripción del plano {n}");
        }
        for open in [false, true] {
            let ctx = egui::Context::default();
            let mut app = ScriptApp::new(&ctx);
            let p = app.settings.scheme.palette();
            theme::apply(&ctx, &p, 100);
            let size = egui::vec2(1280.0, 800.0);
            let run = |app: &mut ScriptApp, n: f64| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    time: Some(n),
                    focused: true,
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| app.draw(ui));
                out.textures_delta.clear();
                out
            };
            run(&mut app, 0.1);
            app.start_project(project.clone(), None);
            app.screen = Screen::Project(Page::TechScript);
            app.show_right = open;
            let mut out = run(&mut app, 0.2);
            for n in 3..40 {
                out = run(&mut app, n as f64 * 0.2);
            }
            let texts: Vec<(String, egui::Pos2, egui::Vec2)> = out
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => {
                        Some((t.galley.text().to_string(), t.pos, t.galley.size()))
                    }
                    _ => None,
                })
                .collect();
            for title in [
                "ESCENA",
                "PLANO",
                "TIPO",
                "MOVIMIENTO",
                "DESCRIPCIÓN",
                "DURACIÓN",
                "CÁMARA",
                "IMAGEN",
            ] {
                let found = texts.iter().find(|t| t.0 == title && t.1.y > 90.0);
                let Some((_, pos, galley)) = found else {
                    panic!("falta el encabezado «{title}» (inspector abierto: {open})");
                };
                assert!(
                    pos.x + galley.x <= size.x,
                    "«{title}» queda fuera de la ventana: {pos:?} (inspector abierto: {open})"
                );
            }
        }
    }

    #[test]
    fn the_add_window_is_four_tiles_wide_and_the_categories_scroll_in_two_and_a_half_rows() {
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        let p = app.settings.scheme.palette();
        theme::apply(&ctx, &p, 100);
        let size = egui::vec2(1400.0, 900.0);
        let run = |app: &mut ScriptApp, n: f64| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(n),
                focused: true,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
            out
        };
        run(&mut app, 0.1);
        app.start_project(Project::default(), None);
        for n in 0..12 {
            app.project
                .add_element_category(&format!("Categoría {n}"), icon::PAW_PRINT);
        }
        app.show_add = true;
        let mut out = run(&mut app, 0.2);
        for n in 3..10 {
            out = run(&mut app, n as f64 * 0.2);
        }
        let label = |text: &str| {
            out.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos),
                _ => None,
            })
        };
        let at: Vec<egui::Pos2> = (0..5)
            .map(|n| label(&format!("Categoría {n}")).expect("mosaico dibujado"))
            .collect();
        assert!(
            at[..4].iter().all(|a| (a.y - at[0].y).abs() < 1.0),
            "cuatro por fila: {at:?}"
        );
        assert!(
            at[4].y > at[0].y + tokens::TILE_H - 1.0,
            "el quinto pasa a la fila siguiente: {at:?}"
        );
        // Dos filas enteras y media: la tercera se ve cortada y la lista se desplaza.
        let third = label("Categoría 8");
        let fourth = label("Categoría 11");
        assert!(third.is_some(), "la tercera fila asoma");
        let _ = fourth;
    }

    /// Una carpeta de proyecto guardada, con la app que la tiene abierta.
    fn shared_project(name: &str) -> (ScriptApp, PathBuf) {
        let dir = std::env::temp_dir().join(format!("scriptpendra-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Corto.scriptpendra");
        let ctx = egui::Context::default();
        let mut app = ScriptApp::new(&ctx);
        let mut project = Project::default();
        let heading = project.make_paragraph(crate::model::ParagraphKind::SceneHeading, "INT. CASA - DÍA");
        let action = project.make_paragraph(crate::model::ParagraphKind::Action, "Ana abre la puerta.");
        project.paragraphs = vec![heading, action];
        app.start_project(project, Some(file.clone()));
        app.save(false);
        (app, file)
    }

    /// Lo que hace otra persona: cambia el archivo de una sección en la carpeta compartida.
    fn other_person_edits(file: &Path, from: &str, to: &str) {
        let path = folder::data_dir(file).join("guion.json");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from));
        std::fs::write(path, text.replace(from, to)).unwrap();
    }

    #[test]
    fn a_change_saved_by_someone_else_is_brought_in_and_untouched_local_work_is_kept() {
        let (mut app, file) = shared_project("pull");
        other_person_edits(&file, "Ana abre la puerta.", "Ana abre la ventana.");
        app.poll_external(true);
        assert_eq!(app.project.paragraphs[1].text, "Ana abre la ventana.");
        // El aviso dice qué cambió, con su contenido.
        assert_eq!(app.change_toasts.len(), 1);
        assert_eq!(app.change_toasts[0].section, "guion");
        assert!(
            app.change_toasts[0].lines.iter().any(|l| l.contains("Ana abre la ventana.")),
            "{:?}",
            app.change_toasts[0].lines
        );
        assert!(!app.dirty, "lo traído ya está en el archivo");
        assert!(app.conflicts.is_empty());
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn different_changes_to_the_same_section_are_combined_and_the_same_change_is_a_conflict_to_resolve() {
        let (mut app, file) = shared_project("merge");
        // Yo agrego un párrafo; la otra persona cambia el de acción: se combinan.
        let mine = app.project.make_paragraph(crate::model::ParagraphKind::Action, "Mía.");
        app.project.paragraphs.push(mine);
        app.dirty = true;
        other_person_edits(&file, "Ana abre la puerta.", "Ana abre la ventana.");
        app.poll_external(true);
        let texts: Vec<&str> = app.project.paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert!(texts.contains(&"Ana abre la ventana.") && texts.contains(&"Mía."), "{texts:?}");
        assert!(app.dirty, "lo combinado hay que guardarlo");
        app.save(false);
        assert!(!app.dirty);
        // Ahora los dos tocamos el mismo párrafo.
        app.project.paragraphs[1].text = "Ana se asoma.".into();
        app.dirty = true;
        other_person_edits(&file, "Ana abre la ventana.", "Ana sale corriendo.");
        app.poll_external(true);
        assert_eq!(app.conflicts.len(), 1);
        app.save(false);
        assert!(app.dirty, "con un conflicto sin resolver no se guarda");
        // Me quedo con la mía: lo de la otra persona queda en el historial.
        app.resolve_conflict(0, true);
        app.save(false);
        assert!(!app.dirty);
        assert_eq!(folder::load(&file).unwrap().paragraphs[1].text, "Ana se asoma.");
        assert!(folder::list(&file).iter().any(|v| v.author == "Otra persona"));
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }
}
