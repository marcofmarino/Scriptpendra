use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParagraphKind {
    SceneHeading,
    Action,
    Character,
    Parenthetical,
    Dialogue,
    Transition,
    /// Letra de una canción.
    Lyrics,
    /// Nota entre líneas: una anotación para quien lee el guion, que no es parte de la película.
    Note,
    /// Título de un acto («ACTO I»).
    Act,
    /// Título de una secuencia.
    Sequence,
}

impl ParagraphKind {
    pub const ALL: [ParagraphKind; 10] = [
        ParagraphKind::SceneHeading,
        ParagraphKind::Action,
        ParagraphKind::Character,
        ParagraphKind::Parenthetical,
        ParagraphKind::Dialogue,
        ParagraphKind::Transition,
        ParagraphKind::Lyrics,
        ParagraphKind::Note,
        ParagraphKind::Act,
        ParagraphKind::Sequence,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ParagraphKind::SceneHeading => "Encabezado de escena",
            ParagraphKind::Action => "Acción",
            ParagraphKind::Character => "Personaje",
            ParagraphKind::Parenthetical => "Acotación",
            ParagraphKind::Dialogue => "Diálogo",
            ParagraphKind::Transition => "Transición",
            ParagraphKind::Lyrics => "Letra",
            ParagraphKind::Note => "Nota entre líneas",
            ParagraphKind::Act => "Acto",
            ParagraphKind::Sequence => "Secuencia",
        }
    }

    /// Los tipos que siempre van en mayúsculas.
    pub fn is_uppercase(self) -> bool {
        matches!(
            self,
            ParagraphKind::SceneHeading
                | ParagraphKind::Character
                | ParagraphKind::Transition
                | ParagraphKind::Act
                | ParagraphKind::Sequence
        )
    }

    /// Tipo del párrafo que se crea al pulsar Enter al final de este.
    pub fn after_enter(self) -> ParagraphKind {
        match self {
            ParagraphKind::Character | ParagraphKind::Parenthetical => ParagraphKind::Dialogue,
            // Después de lo que dice un personaje, responde otro (en un renglón vacío, el editor vuelve a Acción).
            ParagraphKind::Dialogue => ParagraphKind::Character,
            ParagraphKind::Transition => ParagraphKind::SceneHeading,
            // Una canción sigue línea tras línea; un acto o una secuencia abren con una escena.
            ParagraphKind::Lyrics => ParagraphKind::Lyrics,
            ParagraphKind::Act | ParagraphKind::Sequence => ParagraphKind::SceneHeading,
            _ => ParagraphKind::Action,
        }
    }

    /// Tipo al que cambia un párrafo vacío al pulsar Tab.
    pub fn after_tab(self) -> ParagraphKind {
        match self {
            ParagraphKind::SceneHeading => ParagraphKind::Action,
            ParagraphKind::Action => ParagraphKind::Character,
            ParagraphKind::Character => ParagraphKind::Parenthetical,
            ParagraphKind::Transition => ParagraphKind::SceneHeading,
            ParagraphKind::Dialogue => ParagraphKind::Parenthetical,
            ParagraphKind::Parenthetical => ParagraphKind::Dialogue,
            ParagraphKind::Lyrics | ParagraphKind::Note => ParagraphKind::Action,
            ParagraphKind::Act => ParagraphKind::Sequence,
            ParagraphKind::Sequence => ParagraphKind::Act,
        }
    }

    /// ¿Es parte de lo que se filma? Las notas y los títulos de acto o secuencia no cuentan para la duración.
    pub fn is_filmed(self) -> bool {
        !matches!(
            self,
            ParagraphKind::Note | ParagraphKind::Act | ParagraphKind::Sequence
        )
    }
}

/// A qué cosa del proyecto apunta un nombre escrito en el guion. Los ids no cambian al renombrar, así que dos
/// personajes con el mismo nombre nunca se confunden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityRef {
    Character(u64),
    Object(u64),
    Element(u64),
    /// Un documento de tipo «Mundo».
    World(u64),
    Location(u64),
}

/// Un tramo del texto de un párrafo (en caracteres) vinculado a algo del proyecto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: EntityRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    #[serde(default)]
    pub id: u64,
    pub kind: ParagraphKind,
    pub text: String,
    /// Nombres del texto que apuntan a un personaje, objeto, elemento o mundo.
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoryRole {
    #[default]
    Secondary,
    Primary,
    Antagonist,
    Minor,
}

impl StoryRole {
    pub const ALL: [StoryRole; 4] = [
        StoryRole::Primary,
        StoryRole::Antagonist,
        StoryRole::Secondary,
        StoryRole::Minor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StoryRole::Primary => "Protagonista",
            StoryRole::Antagonist => "Antagonista",
            StoryRole::Secondary => "Secundario",
            StoryRole::Minor => "Menor",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterProfile {
    pub id: u64,
    pub name: String,
    pub role: StoryRole,
    pub age: String,
    pub gender: String,
    pub one_liner: String,
    pub notes: String,
    pub photo: Option<PathBuf>,
    /// Número de elenco: así se nombra al personaje en los planes y las hojas de llamado.
    pub cast_number: Option<u32>,
    /// Quién lo interpreta (un contacto del proyecto).
    pub actor: Option<u64>,
    /// Campos de la ficha completa, por clave (ver `profile.rs`).
    pub fields: BTreeMap<String, String>,
}

/// Una persona de la agenda: elenco, equipo, contacto de una locación. En la agenda de la app (fuera de los
/// proyectos) cada una tiene un id estable; el proyecto guarda una copia de las que usa.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contact {
    pub id: u64,
    /// El id con que está en la agenda de la app, si se la guardó ahí.
    pub agenda_id: Option<u64>,
    pub name: String,
    pub role: String,
    pub department: String,
    pub phone: String,
    pub email: String,
    /// Días en que no puede trabajar.
    pub unavailable: Vec<DateRange>,
}

/// Un rango de fechas (AAAA-MM-DD), de `from` a `to` inclusive.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

impl DateRange {
    /// Lo usa el plan de rodaje para avisar de quien programó un día que no puede.
    #[allow(dead_code)]
    pub fn contains(&self, date: &str) -> bool {
        self.from.as_str() <= date && date <= self.to.as_str()
    }

    pub fn label(&self) -> String {
        if self.from == self.to {
            self.from.clone()
        } else {
            format!("{} a {}", self.from, self.to)
        }
    }
}

/// Lee una lista de fechas escrita a mano: «2026-11-03 a 2026-11-05; 2026-11-10». Lo que no es una fecha se ignora.
pub fn parse_date_ranges(text: &str) -> Vec<DateRange> {
    let is_date = |s: &str| {
        let parts: Vec<&str> = s.split('-').collect();
        parts.len() == 3
            && parts[0].len() == 4
            && parts[1].len() == 2
            && parts[2].len() == 2
            && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
    };
    text.split([';', ',', '\n'])
        .filter_map(|part| {
            let (a, b) = part
                .split_once(" a ")
                .map_or((part.trim(), part.trim()), |(a, b)| (a.trim(), b.trim()));
            (is_date(a) && is_date(b)).then(|| DateRange {
                from: a.min(b).to_string(),
                to: a.max(b).to_string(),
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermitStatus {
    #[default]
    Pending,
    Requested,
    Approved,
}

impl PermitStatus {
    pub const ALL: [PermitStatus; 3] = [
        PermitStatus::Pending,
        PermitStatus::Requested,
        PermitStatus::Approved,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PermitStatus::Pending => "Pendiente",
            PermitStatus::Requested => "Pedido",
            PermitStatus::Approved => "Aprobado",
        }
    }
}

/// El lugar real donde se filma. Varias locaciones del guion pueden filmarse en el mismo lugar (el living, el pasillo y
/// el baño en una sola casa), y eso es lo que el plan de rodaje agrupa.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShootLocation {
    pub id: u64,
    pub name: String,
    pub address: String,
    pub contact: Option<u64>,
    pub access_hours: String,
    pub parking: String,
    pub bathrooms: String,
    pub permit: PermitStatus,
    pub permit_date: String,
    /// Costo del permiso, en centavos.
    pub permit_cost_cents: Option<i64>,
    pub photos: Vec<PathBuf>,
    pub floor_plan: Option<PathBuf>,
    /// Cuántos metros de ancho mide lo que muestra el plano: da la escala para dibujar encima.
    pub plan_width_m: Option<f32>,
    /// Dónde queda, para calcular la salida y la puesta del sol sin internet (grados; el oeste y el sur son negativos).
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub circuits: String,
    pub power_watts: Option<u32>,
    pub generator: bool,
    pub noise_notes: String,
    /// Hospital o guardia más cercana.
    pub hospital: String,
    pub notes: String,
}

/// Ficha de una locación (se crea al abrirla por primera vez).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocationProfile {
    pub id: u64,
    pub name: String,
    pub one_liner: String,
    pub fields: BTreeMap<String, String>,
    /// El lugar real donde se filma por defecto.
    pub shoot_location: Option<u64>,
}

/// Objeto del proyecto (un arma, una carta, una llave…).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjectItem {
    pub id: u64,
    pub name: String,
    pub one_liner: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocKind {
    Folder,
    Text,
    World,
    MindMap,
    Gallery,
    /// Tarjetas para escribir, reordenar y ver en una línea de tiempo.
    Cards,
    /// Mapa de relaciones entre personajes, locaciones y otras cosas.
    Relations,
}

impl DocKind {
    pub fn label(self) -> &'static str {
        match self {
            DocKind::Folder => "Carpeta",
            DocKind::Text => "Texto",
            DocKind::World => "Mundo",
            DocKind::MindMap => "Mapa mental",
            DocKind::Gallery => "Galería de imágenes",
            DocKind::Cards => "Tarjetas",
            DocKind::Relations => "Mapa de relaciones",
        }
    }

    pub fn default_name(self) -> &'static str {
        match self {
            DocKind::Folder => "Nueva carpeta",
            DocKind::Text => "Nuevo texto",
            DocKind::World => "Nuevo mundo",
            DocKind::MindMap => "Nuevo mapa mental",
            DocKind::Gallery => "Nueva galería",
            DocKind::Cards => "Nuevas tarjetas",
            DocKind::Relations => "Nuevo mapa de relaciones",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MindNode {
    pub id: u64,
    pub parent: Option<u64>,
    pub text: String,
}

/// Una tarjeta de historia: un título, lo que pasa y en qué momento, con un color para agruparlas.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoryCard {
    pub id: u64,
    pub title: String,
    pub text: String,
    /// Cuándo ocurre («Acto I», «0:15», «Día 2»), para la línea de tiempo.
    pub moment: String,
    /// Índice del color en la paleta de tarjetas.
    pub color: u8,
}

/// Un nodo del mapa de relaciones: algo del proyecto (personaje, locación, objeto…) o una nota libre, con su lugar.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelNode {
    pub id: u64,
    pub target: Option<EntityRef>,
    /// El texto de una nota libre (si tiene `target`, el nombre sale de la ficha).
    pub text: String,
    pub x: f32,
    pub y: f32,
}

/// Una relación entre dos nodos, con su rótulo («hermana de», «trabaja en»).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelEdge {
    pub id: u64,
    pub from: u64,
    pub to: u64,
    pub label: String,
}

/// Documento libre del proyecto: carpeta, nota, mundo, mapa mental o galería.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: u64,
    pub kind: DocKind,
    pub name: String,
    /// Carpeta que lo contiene (`None` = raíz).
    #[serde(default)]
    pub parent: Option<u64>,
    /// Cuerpo de un texto o notas de un mundo.
    #[serde(default)]
    pub text: String,
    /// Descripción corta de un mundo.
    #[serde(default)]
    pub description: String,
    /// Campos de la ficha de un mundo, por clave (ver `profile.rs`).
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub nodes: Vec<MindNode>,
    #[serde(default)]
    pub images: Vec<PathBuf>,
    #[serde(default)]
    pub cards: Vec<StoryCard>,
    #[serde(default)]
    pub rel_nodes: Vec<RelNode>,
    #[serde(default)]
    pub rel_edges: Vec<RelEdge>,
}

const BREAKDOWN_VERSION: u32 = 2;

/// Los nombres más cortos no se vinculan solos dentro de acciones y diálogos: serían palabras corrientes.
const MIN_AUTO_LINK_CHARS: usize = 3;

fn yes() -> bool {
    true
}

/// En qué categorías los nombres guardados se muestran en mayúsculas dentro de acciones y diálogos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameCase {
    pub characters: bool,
    pub locations: bool,
    pub objects: bool,
    pub worlds: bool,
}

impl Default for NameCase {
    fn default() -> Self {
        // Solo los nombres de los personajes van en mayúsculas por defecto.
        Self {
            characters: true,
            locations: false,
            objects: false,
            worlds: false,
        }
    }
}

/// Foto de las opciones de mayúsculas del proyecto, lista para consultar por cada nombre vinculado.
#[derive(Clone, Debug, Default)]
pub struct CaseOptions {
    pub base: Option<NameCase>,
    /// Categoría de elementos → en mayúsculas.
    pub categories: std::collections::HashMap<u64, bool>,
    /// Elemento → su categoría.
    pub element_category: std::collections::HashMap<u64, u64>,
    /// Para los nombres que no apuntan a nada.
    pub unlinked: bool,
    /// Valor de las categorías de elementos que no se encuentren.
    pub fallback: bool,
    /// Los nombres se muestran exactamente como están escritos (el tratamiento no usa mayúsculas ni capitalización).
    pub as_written: bool,
}

impl CaseOptions {
    /// Todo igual.
    pub fn uniform(upper: bool) -> Self {
        Self {
            base: Some(NameCase {
                characters: upper,
                locations: upper,
                objects: upper,
                worlds: upper,
            }),
            categories: Default::default(),
            element_category: Default::default(),
            unlinked: upper,
            fallback: upper,
            as_written: false,
        }
    }

    /// Sin cambio de mayúsculas en ningún nombre.
    pub fn as_written() -> Self {
        Self {
            as_written: true,
            ..Self::uniform(false)
        }
    }

    /// ¿Va en mayúsculas el nombre al que apunta `target`?
    pub fn is_upper(&self, target: Option<EntityRef>) -> bool {
        let base = self.base.unwrap_or_default();
        match target {
            None => self.unlinked,
            Some(EntityRef::Character(_)) => base.characters,
            Some(EntityRef::Location(_)) => base.locations,
            Some(EntityRef::Object(_)) => base.objects,
            Some(EntityRef::World(_)) => base.worlds,
            Some(EntityRef::Element(id)) => self
                .element_category
                .get(&id)
                .and_then(|c| self.categories.get(c))
                .copied()
                .unwrap_or(self.fallback),
        }
    }
}

/// De dónde salen los elementos de una categoría del desglose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CategorySource {
    /// Elementos añadidos a mano o marcando texto del guion.
    #[default]
    Custom,
    /// Los personajes del proyecto.
    Characters,
    /// Las locaciones del proyecto.
    Locations,
    /// Los objetos del proyecto.
    Objects,
}

/// Categoría del desglose de producción (con su color), como las de Starc.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakdownCategory {
    pub id: u64,
    pub name: String,
    pub color: [u8; 3],
    #[serde(default)]
    pub source: CategorySource,
}

/// Cómo se resuelve un elemento para el rodaje.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Acquisition {
    #[default]
    Pending,
    Have,
    Rent,
    Buy,
    Build,
}

impl Acquisition {
    pub const ALL: [Acquisition; 5] = [
        Acquisition::Pending,
        Acquisition::Have,
        Acquisition::Rent,
        Acquisition::Buy,
        Acquisition::Build,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Acquisition::Pending => "Pendiente",
            Acquisition::Have => "Conseguido",
            Acquisition::Rent => "Alquilar",
            Acquisition::Buy => "Comprar",
            Acquisition::Build => "Fabricar",
        }
    }
}

/// Elemento del desglose: lo que se necesita para el rodaje (una camioneta, un vestuario, un efecto…).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakdownItem {
    pub id: u64,
    pub category: u64,
    pub name: String,
    #[serde(default)]
    pub status: Acquisition,
    #[serde(default)]
    pub quantity: u32,
    #[serde(default)]
    pub notes: String,
    /// Si el elemento es un personaje, una locación o un objeto del proyecto, a cuál. Se sigue por id, no por
    /// nombre: renombrar no lo separa de su fila y dos con el mismo nombre no se mezclan.
    #[serde(default)]
    pub entity: Option<EntityRef>,
    /// Lo que cuesta conseguirlo, en centavos: en total, o por unidad si `unit_cost`.
    #[serde(default)]
    pub cost_cents: Option<i64>,
    /// El costo es de cada unidad: el total es el costo por la cantidad.
    #[serde(default)]
    pub unit_cost: bool,
    /// Quién se encarga de conseguirlo (un contacto del proyecto).
    #[serde(default)]
    pub responsible: Option<u64>,
    /// Escenas (el id de su encabezado) a las que se asignó a mano, además de donde está marcado en el guion.
    #[serde(default)]
    pub extra_scenes: Vec<u64>,
}

impl BreakdownItem {
    /// Lo que cuesta en total: el costo por la cantidad si es por unidad (con cantidad 0 cuenta una), o el costo solo.
    pub fn total_cents(&self) -> Option<i64> {
        self.cost_cents.map(|c| {
            if self.unit_cost {
                c * i64::from(self.quantity.max(1))
            } else {
                c
            }
        })
    }
}

/// Marca en el guion: un texto de un párrafo ligado a un elemento del desglose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakdownTag {
    pub id: u64,
    pub paragraph: u64,
    pub quote: String,
    pub item: u64,
    /// Cuántos hay en la escena de esta marca (cinco extras en una escena y treinta en otra), si se indicó.
    #[serde(default)]
    pub quantity: Option<u32>,
}

/// Una fila del desglose: un elemento (guardado o salido de un personaje, locación u objeto) y sus escenas.
#[derive(Clone, Debug)]
pub struct BreakdownRow {
    pub category: u64,
    pub name: String,
    /// El personaje, locación u objeto al que corresponde la fila (si es de una categoría automática).
    pub entity: Option<EntityRef>,
    /// Posición en `breakdown_items`, si ya tiene datos guardados.
    pub item: Option<usize>,
    /// Números de las escenas donde aparece, en orden.
    pub scenes: Vec<usize>,
    /// Viene de un personaje, locación u objeto del proyecto.
    pub auto: bool,
}

/// Un elemento en la hoja de desglose de una escena.
#[derive(Clone, Debug)]
pub struct SheetItem {
    pub category: u64,
    pub name: String,
    /// El elemento guardado, si ya tiene datos.
    pub item_id: Option<u64>,
    /// Cuántos hay en esta escena, si se indicó.
    pub scene_quantity: Option<u32>,
    /// Cuántos hay en total.
    pub quantity: u32,
    pub status: Acquisition,
    pub notes: String,
    /// Viene de un personaje, locación u objeto del proyecto (no se marcó en el texto).
    pub auto: bool,
}

/// Una fila de la lista de compras y alquileres.
#[derive(Clone, Debug)]
pub struct ShoppingRow {
    pub item: u64,
    pub category: u64,
    pub name: String,
    pub quantity: u32,
    pub status: Acquisition,
    /// Números de las escenas donde se usa.
    pub scenes: Vec<String>,
    pub cost_cents: Option<i64>,
    pub responsible: Option<u64>,
    pub notes: String,
}

/// Lo que el lugar de rodaje de una locación del guion aporta al desglose: está conseguida si el permiso está aprobado, y
/// trae su costo y la persona de contacto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocationDeal {
    pub status: Option<Acquisition>,
    pub cost_cents: Option<i64>,
    pub responsible: Option<u64>,
}

/// Una locación con las escenas en las que transcurre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationInfo {
    pub id: u64,
    pub name: String,
    pub scenes: usize,
}

/// Nombre sugerido al escribir una acción.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    pub name: String,
    /// Los nombres de personaje son propios: siempre llevan mayúscula inicial.
    pub character: bool,
    /// A qué apunta. Las locaciones (que viven en los encabezados) todavía no tienen vínculo.
    pub target: Option<EntityRef>,
    /// Lo que distingue a dos con el mismo nombre (descripción o rol).
    pub detail: String,
}

/// Categoría de elementos creada por la persona (Fauna, Vehículos, Facciones…): aparece en la barra lateral
/// como los personajes o las locaciones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementCategory {
    pub id: u64,
    pub name: String,
    /// Glifo del icono elegido (ver `icon::PICKABLE`).
    pub icon: String,
    /// Ficha de los elementos de esta categoría. Sin personalizar usa la ficha por defecto.
    #[serde(default)]
    pub template: Option<Vec<TemplateSection>>,
    /// Sus nombres van en mayúsculas dentro de acciones y diálogos.
    #[serde(default = "yes")]
    pub uppercase: bool,
}

impl ElementCategory {
    /// Las secciones de la ficha: las propias o, si todavía no se tocó, las de por defecto.
    pub fn sections(&self) -> Vec<TemplateSection> {
        self.template
            .clone()
            .unwrap_or_else(|| crate::profile::owned(crate::profile::ELEMENT))
    }
}

/// Un campo de la ficha de una categoría propia.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateField {
    /// Clave estable con la que se guarda el valor en cada elemento.
    pub key: String,
    pub label: String,
    /// Alto del campo de texto, en filas.
    pub rows: usize,
}

/// Una sección de la ficha de una categoría propia.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateSection {
    pub title: String,
    pub fields: Vec<TemplateField>,
}

/// Un elemento de una categoría propia (un lobo, una facción, una nave…).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Element {
    pub id: u64,
    pub category: u64,
    pub name: String,
    pub one_liner: String,
    pub photo: Option<PathBuf>,
    pub fields: BTreeMap<String, String>,
}

/// Herramienta con la que se hizo un trazo del boceto de un plano.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SketchTool {
    Pen,
    Eraser,
    Line,
    Rect,
    Ellipse,
    /// Una flecha: para marcar el movimiento de la cámara o de un personaje.
    Arrow,
}

/// Un trazo del boceto: sus puntos (en proporciones del lienzo, de 0 a 1), el color y el grosor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchStroke {
    pub tool: SketchTool,
    pub points: Vec<(f32, f32)>,
    pub color: [u8; 4],
    pub width: f32,
}

/// Qué es cada cosa dibujada en un bloqueo de cámara.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    /// Una persona (con su personaje, si se lo vincula): un círculo con la dirección en que mira.
    #[default]
    Actor,
    /// Un mueble o un objeto: un rectángulo o un círculo (`BlockItem::shape`).
    Object,
    /// Una pared: un segmento grueso.
    Wall,
    /// Una puerta o ventana sobre una pared (`BlockItem::opening` y `BlockItem::wall`).
    Opening,
    /// Una cámara, con su ángulo de visión.
    Camera,
    /// Una luz de la planta de luces: apunta hacia donde mira y tiene potencia y temperatura de color.
    Light,
    /// Un rebote, una bandera o un difusor de la planta de luces (`BlockItem::modifier`).
    Modifier,
    /// Un micrófono de la planta de sonido: boom, corbatero, escondido o en pie (`BlockItem::mic`).
    Mic,
}

/// Forma de un mueble u objeto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectShape {
    #[default]
    Rect,
    Round,
    // Muebles y objetos con su propia figura, vistos desde arriba. El frente mira hacia +x.
    Table,
    Chair,
    Armchair,
    Sofa,
    Bed,
    Desk,
    Shelf,
    Wardrobe,
    Tv,
    Plant,
    Rug,
    Car,
}

impl ObjectShape {
    /// Los muebles y objetos con nombre y figura propios: forma, nombre y medidas de fábrica (profundidad, ancho), en metros.
    pub const FURNITURE: [(ObjectShape, &'static str, f32, f32); 12] = [
        (ObjectShape::Table, "Mesa", 0.9, 1.6),
        (ObjectShape::Chair, "Silla", 0.5, 0.5),
        (ObjectShape::Armchair, "Sillón", 0.9, 0.85),
        (ObjectShape::Sofa, "Sofá", 0.9, 2.0),
        (ObjectShape::Bed, "Cama", 2.0, 1.5),
        (ObjectShape::Desk, "Escritorio", 0.7, 1.4),
        (ObjectShape::Shelf, "Estante", 0.35, 1.0),
        (ObjectShape::Wardrobe, "Placard", 0.6, 1.2),
        (ObjectShape::Tv, "Televisor", 0.12, 1.0),
        (ObjectShape::Plant, "Planta", 0.5, 0.5),
        (ObjectShape::Rug, "Alfombra", 1.4, 2.0),
        (ObjectShape::Car, "Auto", 4.5, 1.8),
    ];

    /// El nombre del mueble, si es uno de los de la lista.
    pub fn furniture_name(self) -> Option<&'static str> {
        Self::FURNITURE
            .iter()
            .find(|(s, ..)| *s == self)
            .map(|(_, name, ..)| *name)
    }

    pub fn label(self) -> &'static str {
        match self {
            ObjectShape::Rect => "Rectangular",
            ObjectShape::Round => "Redondo",
            other => other.furniture_name().unwrap_or("Objeto"),
        }
    }
}

/// Puerta o ventana.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpeningKind {
    #[default]
    Door,
    Window,
}

/// Aparato de una luz.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightFixture {
    #[default]
    Fresnel,
    Panel,
    Tube,
    Softbox,
    Hmi,
    Practical,
    Other,
}

// Lo usan el dibujo y el inspector de las próximas etapas de las plantas.
#[allow(dead_code)]
impl LightFixture {
    pub const ALL: [LightFixture; 7] = [
        Self::Fresnel,
        Self::Panel,
        Self::Tube,
        Self::Softbox,
        Self::Hmi,
        Self::Practical,
        Self::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fresnel => "Fresnel",
            Self::Panel => "Panel LED",
            Self::Tube => "Tubo",
            Self::Softbox => "Softbox",
            Self::Hmi => "HMI",
            Self::Practical => "Práctica",
            Self::Other => "Otro",
        }
    }

    /// Apertura total del haz, en grados.
    pub fn beam_deg(self) -> f32 {
        match self {
            Self::Fresnel | Self::Other => 40.0,
            Self::Panel => 100.0,
            Self::Tube => 120.0,
            Self::Softbox => 90.0,
            Self::Hmi => 30.0,
            Self::Practical => 360.0,
        }
    }

    /// Alcance dibujado del haz, en metros.
    pub fn reach_m(self) -> f32 {
        if self == Self::Practical { 1.5 } else { 3.0 }
    }

    /// Potencia (W) y temperatura de color (K) de partida: puntos de partida editables, no especificaciones reales.
    pub fn watts_kelvin(self) -> (u32, u32) {
        match self {
            Self::Fresnel => (650, 5600),
            Self::Panel => (100, 5600),
            Self::Tube => (40, 5600),
            Self::Softbox => (300, 5600),
            Self::Hmi => (1200, 5600),
            Self::Practical => (60, 2700),
            Self::Other => (300, 5600),
        }
    }
}

/// Función de una luz en la escena.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightRole {
    #[default]
    Key,
    Fill,
    Back,
    Ambient,
    Practical,
    Effect,
}

impl LightRole {
    pub const ALL: [LightRole; 6] = [
        Self::Key,
        Self::Fill,
        Self::Back,
        Self::Ambient,
        Self::Practical,
        Self::Effect,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Key => "Principal",
            Self::Fill => "Relleno",
            Self::Back => "Contraluz",
            Self::Ambient => "Ambiente",
            Self::Practical => "Práctica",
            Self::Effect => "Efecto",
        }
    }
}

/// Lo que se sabe de una luz (`BlockItem::light`). La potencia y la temperatura viven en `BlockItem::watts` y `kelvin`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightSpec {
    pub fixture: LightFixture,
    pub role: LightRole,
    /// Apertura total del haz, en grados.
    pub beam_deg: f32,
    /// Alcance dibujado del haz, en metros.
    pub reach_m: f32,
    /// De 0 a 100 %.
    pub intensity: f32,
    /// Color de la gelatina, si hay.
    pub gel: Option<[u8; 3]>,
    pub height_m: f32,
    /// «Ventana», «lámpara de pie»…
    pub motivation: String,
    /// Vínculo al catálogo de equipos.
    pub equipment: Option<u64>,
}

impl LightSpec {
    pub fn new(fixture: LightFixture) -> Self {
        Self {
            fixture,
            role: LightRole::Key,
            beam_deg: fixture.beam_deg(),
            reach_m: fixture.reach_m(),
            intensity: 100.0,
            gel: None,
            height_m: 2.0,
            motivation: String::new(),
            equipment: None,
        }
    }
}

impl Default for LightSpec {
    fn default() -> Self {
        Self::new(LightFixture::Fresnel)
    }
}

/// Rebote, bandera o difusor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierKind {
    #[default]
    Bounce,
    Flag,
    Diffuser,
}

impl ModifierKind {
    pub const ALL: [ModifierKind; 3] = [Self::Bounce, Self::Flag, Self::Diffuser];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bounce => "Rebote",
            Self::Flag => "Bandera",
            Self::Diffuser => "Difusor",
        }
    }
}

/// Cara reflectante de un rebote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierFace {
    #[default]
    White,
    Silver,
    Gold,
    /// Negro (negativo).
    Black,
}

impl ModifierFace {
    pub const ALL: [ModifierFace; 4] = [Self::White, Self::Silver, Self::Gold, Self::Black];

    pub fn label(self) -> &'static str {
        match self {
            Self::White => "Blanco",
            Self::Silver => "Plata",
            Self::Gold => "Dorado",
            Self::Black => "Negro",
        }
    }
}

/// Lo que se sabe de un modificador de luz (`BlockItem::modifier`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModifierSpec {
    pub kind: ModifierKind,
    /// Solo en rebotes.
    pub face: ModifierFace,
    /// Largo, en metros.
    pub w: f32,
}

impl Default for ModifierSpec {
    fn default() -> Self {
        Self {
            kind: ModifierKind::Bounce,
            face: ModifierFace::White,
            w: 1.0,
        }
    }
}

/// Tipo de micrófono.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicKind {
    #[default]
    Boom,
    /// Corbatero.
    Lavalier,
    /// Oculto.
    Plant,
    /// En pie.
    Stand,
}

impl MicKind {
    pub const ALL: [MicKind; 4] = [Self::Boom, Self::Lavalier, Self::Plant, Self::Stand];

    pub fn label(self) -> &'static str {
        match self {
            Self::Boom => "Boom",
            Self::Lavalier => "Corbatero",
            Self::Plant => "Oculto",
            Self::Stand => "En pie",
        }
    }

    /// Patrón con que sale cada tipo.
    pub fn default_pattern(self) -> MicPattern {
        match self {
            Self::Boom => MicPattern::Shotgun,
            Self::Lavalier => MicPattern::Omni,
            Self::Plant | Self::Stand => MicPattern::Cardioid,
        }
    }

    /// Alcance dibujado de partida, en metros.
    pub fn default_reach_m(self) -> f32 {
        match self {
            Self::Boom => 1.5,
            Self::Lavalier => 0.5,
            Self::Plant | Self::Stand => 1.0,
        }
    }
}

/// Patrón polar de un micrófono.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicPattern {
    Omni,
    #[default]
    Cardioid,
    Supercardioid,
    Hypercardioid,
    /// Lobular.
    Shotgun,
    Figure8,
}

// Lo usan el dibujo y el inspector de las próximas etapas de las plantas.
#[allow(dead_code)]
impl MicPattern {
    pub const ALL: [MicPattern; 6] = [
        Self::Omni,
        Self::Cardioid,
        Self::Supercardioid,
        Self::Hypercardioid,
        Self::Shotgun,
        Self::Figure8,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Omni => "Omnidireccional",
            Self::Cardioid => "Cardioide",
            Self::Supercardioid => "Supercardioide",
            Self::Hypercardioid => "Hipercardioide",
            Self::Shotgun => "Lobular",
            Self::Figure8 => "Figura 8",
        }
    }
}

/// Lo que se sabe de un micrófono (`BlockItem::mic`). La pista del grabador vive en `BlockItem::track`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MicSpec {
    pub kind: MicKind,
    pub pattern: MicPattern,
    /// Alcance dibujado del patrón, en metros.
    pub reach_m: f32,
    pub wireless: bool,
    /// Solo corbatero: la persona que lo lleva (id del elemento).
    pub attached_to: Option<u64>,
    /// Solo boom: dónde está el operador, en metros; la cápsula es la posición del elemento.
    pub operator: Option<(f32, f32)>,
}

impl MicSpec {
    pub fn new(kind: MicKind) -> Self {
        Self {
            kind,
            pattern: kind.default_pattern(),
            reach_m: kind.default_reach_m(),
            wireless: kind == MicKind::Lavalier,
            attached_to: None,
            operator: None,
        }
    }
}

impl Default for MicSpec {
    fn default() -> Self {
        Self::new(MicKind::Boom)
    }
}

fn default_cone_len() -> f32 {
    5.0
}

fn default_cam_height() -> f32 {
    1.5
}

fn default_support() -> String {
    "Trípode".to_string()
}

/// Algo del bloqueo. Las medidas están en metros, vistas desde arriba, y el ángulo en grados (0° = hacia la derecha,
/// creciendo hacia abajo).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockItem {
    pub id: u64,
    pub kind: BlockKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rot: f32,
    pub label: String,
    #[serde(default)]
    pub character: Option<u64>,
    /// Ángulo de visión de una cámara, en grados (el de las cámaras sin lente cargado).
    #[serde(default)]
    pub fov: f32,
    /// Distancia focal de la lente de una cámara, en mm. En 0, vale el ángulo de visión de `fov`.
    #[serde(default)]
    pub lens_mm: f32,
    /// Cuerpo de cámara (sensor) con que se calcula el ángulo; sin él, un sensor full frame.
    #[serde(default)]
    pub body: Option<u64>,
    /// Posiciones en otros instantes del bloqueo, por orden de tiempo. La de `x`, `y`, `rot` es la del segundo 0.
    #[serde(default)]
    pub keys: Vec<BlockKey>,
    /// El equipo registrado (una fila de Equipo de sonido o de fotografía) que es esta cosa: el mismo equipo se usa en
    /// distintos lugares y escenas.
    #[serde(default)]
    pub equipment: Option<u64>,
    /// Potencia de una luz, en watts.
    #[serde(default)]
    pub watts: u32,
    /// Temperatura de color de una luz, en kelvin.
    #[serde(default)]
    pub kelvin: u32,
    /// Notas libres. Antes guardaba la función de una luz o el tipo de un micrófono: `migrate_legacy_note` lo pasa a los
    /// campos con tipo al cargar.
    #[serde(default)]
    pub note: String,
    /// Pista del grabador a la que va un micrófono (0: sin asignar).
    #[serde(default)]
    pub track: u32,
    /// Forma de un objeto.
    #[serde(default)]
    pub shape: ObjectShape,
    /// Puerta o ventana, y la pared (id del elemento) donde está.
    #[serde(default)]
    pub opening: OpeningKind,
    #[serde(default)]
    pub wall: Option<u64>,
    /// Largo del cono dibujado de una cámara, en metros.
    #[serde(default = "default_cone_len")]
    pub cone_len_m: f32,
    /// Cómo se sostiene una cámara (trípode, mano, gimbal…), a qué altura (m) y con qué ángulo vertical (°, + mira hacia arriba).
    #[serde(default = "default_support")]
    pub support: String,
    #[serde(default = "default_cam_height")]
    pub cam_height_m: f32,
    #[serde(default)]
    pub tilt_deg: f32,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub locked: bool,
    /// Antes, un interruptor para todo el recorrido: al cargar pasa a la suavidad de cada posición (`BlockKey::smooth`).
    #[serde(default, skip_serializing)]
    pub curved: bool,
    #[serde(default)]
    pub light: Option<LightSpec>,
    #[serde(default)]
    pub modifier: Option<ModifierSpec>,
    #[serde(default)]
    pub mic: Option<MicSpec>,
}

impl Default for BlockItem {
    fn default() -> Self {
        Self {
            id: 0,
            kind: BlockKind::default(),
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            rot: 0.0,
            label: String::new(),
            character: None,
            fov: 0.0,
            lens_mm: 0.0,
            body: None,
            keys: Vec::new(),
            equipment: None,
            watts: 0,
            kelvin: 0,
            note: String::new(),
            track: 0,
            shape: ObjectShape::default(),
            opening: OpeningKind::default(),
            wall: None,
            cone_len_m: default_cone_len(),
            support: default_support(),
            cam_height_m: default_cam_height(),
            tilt_deg: 0.0,
            hidden: false,
            locked: false,
            curved: false,
            light: None,
            modifier: None,
            mic: None,
        }
    }
}

/// Dónde está algo (y hacia dónde mira) en un instante del bloqueo, en segundos desde el comienzo.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockKey {
    pub t: f32,
    pub x: f32,
    pub y: f32,
    pub rot: f32,
    /// Qué pasa en ese momento («Juana se sienta»).
    #[serde(default)]
    pub label: String,
    /// Cuántos segundos se queda quieto en esta posición antes de seguir.
    #[serde(default)]
    pub hold: f32,
    /// Llega a esta posición frenando.
    #[serde(default)]
    pub ease_in: bool,
    /// Sale de esta posición acelerando.
    #[serde(default)]
    pub ease_out: bool,
    /// La lente de una cámara en este instante, en mm (un cambio de zoom, por ejemplo un efecto Vértigo). Sin valor, no
    /// cambia: sigue la interpolación entre las que sí tienen.
    #[serde(default)]
    pub lens_mm: Option<f32>,
    /// El recorrido pasa por esta posición con una curva suave y no con una esquina.
    #[serde(default)]
    pub smooth: bool,
    /// El tirador de curvatura de una posición suave: de la posición al punto de control hacia adelante (el de atrás es el
    /// simétrico). Sin valor, la curva se calcula sola.
    #[serde(default)]
    pub handle: Option<(f32, f32)>,
    /// Los puntos del recorrido entre la posición anterior (o la de partida) y esta, en orden. No son posiciones: no tienen
    /// tiempo, solo dan forma al trayecto.
    #[serde(default)]
    pub via: Vec<Waypoint>,
    /// Dónde está el operador de un boom en este instante, en metros. Sin valor, no cambia: sigue la interpolación entre las
    /// posiciones que sí lo tienen.
    #[serde(default)]
    pub operator: Option<(f32, f32)>,
}

/// Un punto sobre el trayecto de algo que se mueve: esquina (recta) o curva suave, como un nodo de Inkscape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Waypoint {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub smooth: bool,
    /// Su tirador de curvatura (ver `BlockKey::handle`).
    #[serde(default)]
    pub handle: Option<(f32, f32)>,
}

impl BlockKey {
    pub fn new(t: f32, x: f32, y: f32, rot: f32) -> Self {
        Self {
            t,
            x,
            y,
            rot,
            ..Self::default()
        }
    }
}

/// Dos instantes más cerca que esto son el mismo.
pub const KEY_EPSILON: f32 = 0.05;
/// Cuánto dura un bloqueo recién creado, en segundos.
pub const DEFAULT_BLOCK_SECONDS: f32 = 10.0;
/// Cuánto adelante del centro de su persona está un corbatero, en metros.
pub const LAVALIER_OFFSET_M: f32 = 0.1;

/// Avance de 0 a 1 con la aceleración del comienzo (`ease_out` de la posición de la que sale) y la frenada del final
/// (`ease_in` de la posición a la que llega).
fn eased(f: f32, accelerate: bool, decelerate: bool) -> f32 {
    match (accelerate, decelerate) {
        (false, false) => f,
        (true, true) => f * f * (3.0 - 2.0 * f),
        (true, false) => f * f,
        (false, true) => f * (2.0 - f),
    }
}

/// Los nodos de un recorrido (lugar y si son suaves) y el lugar de cada posición entre ellos.
pub struct PathNodes {
    /// Cada nodo: dónde está y si es suave.
    pub nodes: Vec<((f32, f32), bool)>,
    /// El tirador de curvatura que se le dio a cada nodo, si se le dio.
    pub handles: Vec<Option<(f32, f32)>>,
    /// Qué es cada nodo (la partida no es ninguno de los dos).
    pub refs: Vec<Option<PathNodeRef>>,
    /// El lugar de cada posición entre los nodos (la de partida es la 0).
    pub at: Vec<usize>,
}

/// Un nodo suave del trayecto con su lugar y su tirador de curvatura.
pub type SmoothHandle = (PathNodeRef, (f32, f32), (f32, f32));

/// Un nodo del trayecto que se puede tocar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathNodeRef {
    /// Un punto intermedio: la posición a la que llega su tramo y cuál es dentro de él.
    Via(usize, usize),
    /// Una posición guardada.
    Key(usize),
}

/// Cuántos tramos tiene cada curva entre dos nodos del recorrido.
const CURVE_STEPS: usize = 12;

/// El punto que está a la fracción `f` (0 a 1) del largo de una línea de puntos.
fn along(line: &[(f32, f32)], f: f32) -> (f32, f32) {
    let lengths: Vec<f32> = line
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .collect();
    let total: f32 = lengths.iter().sum();
    let Some(first) = line.first() else {
        return (0.0, 0.0);
    };
    if total <= 1e-6 {
        return *first;
    }
    let mut left = f.clamp(0.0, 1.0) * total;
    for (n, len) in lengths.iter().enumerate() {
        if left <= *len || n == lengths.len() - 1 {
            let u = if *len > 1e-6 {
                (left / len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (a, b) = (line[n], line[n + 1]);
            return (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
        }
        left -= len;
    }
    *line.last().unwrap_or(first)
}

impl BlockItem {
    /// Las paredes y aberturas no se mueven, y un corbatero que lleva una persona va con ella.
    pub fn animatable(&self) -> bool {
        !matches!(self.kind, BlockKind::Wall | BlockKind::Opening) && !self.is_attached_lavalier()
    }

    /// ¿Es un corbatero unido a una persona? No tiene posiciones propias.
    pub fn is_attached_lavalier(&self) -> bool {
        self.kind == BlockKind::Mic
            && self
                .mic
                .as_ref()
                .is_some_and(|m| m.kind == MicKind::Lavalier && m.attached_to.is_some())
    }

    /// El punto de recorrido `i`: el de partida (`i` = 0) o la posición `i - 1`, como (t, x, y, giro).
    fn knot(&self, i: usize) -> (f32, f32, f32, f32) {
        match i.checked_sub(1).and_then(|k| self.keys.get(k)) {
            Some(k) => (k.t, k.x, k.y, k.rot),
            None => (0.0, self.x, self.y, self.rot),
        }
    }

    /// Posición y giro en el instante `t`. Entre dos posiciones se avanza en línea recta, o por una curva suave si
    /// `curved`; la posición guardada puede tener una espera (`hold`) y aceleración o frenada (`ease_out`, `ease_in`).
    /// El giro toma el camino corto. Es la pose propia: para un corbatero, ver `pose_in`.
    pub fn pose_at(&self, t: f32) -> (f32, f32, f32) {
        let n = self.keys.len();
        for i in 1..=n {
            let (kt, ..) = self.knot(i);
            if t > kt {
                continue;
            }
            let (prev, from, to) = (self.knot(i - 1), self.knot(i - 1), self.knot(i));
            let (leave, arrive) = (self.keys.get(i.wrapping_sub(2)), &self.keys[i - 1]);
            let hold = leave.map_or(0.0, |k| k.hold.max(0.0));
            let start = (prev.0 + hold).min(to.0);
            let span = to.0 - start;
            let raw = if span <= f32::EPSILON {
                1.0
            } else {
                ((t - start) / span).clamp(0.0, 1.0)
            };
            let f = eased(raw, leave.is_some_and(|k| k.ease_out), arrive.ease_in);
            let (x, y) = if self.is_straight() {
                (from.1 + (to.1 - from.1) * f, from.2 + (to.2 - from.2) * f)
            } else {
                along(&self.segment_curve(i - 1), f)
            };
            let turn = (to.3 - from.3 + 540.0).rem_euclid(360.0) - 180.0;
            return (x, y, (from.3 + turn * f).rem_euclid(360.0));
        }
        let (_, x, y, rot) = self.knot(n);
        (x, y, rot)
    }

    /// ¿Va todo el recorrido en línea recta entre posiciones (sin puntos intermedios ni curvas)?
    fn is_straight(&self) -> bool {
        self.keys.iter().all(|k| !k.smooth && k.via.is_empty())
    }

    /// Los nodos de todo el recorrido, en orden: la partida, los puntos intermedios y cada posición, con si son suaves; y el
    /// lugar de cada posición en esa lista (la de partida es la 0).
    pub fn path_nodes(&self) -> PathNodes {
        let mut out = PathNodes {
            nodes: vec![((self.x, self.y), false)],
            handles: vec![None],
            refs: vec![None],
            at: vec![0],
        };
        for (k, key) in self.keys.iter().enumerate() {
            for (n, v) in key.via.iter().enumerate() {
                out.nodes.push(((v.x, v.y), v.smooth));
                out.handles.push(v.handle);
                out.refs.push(Some(PathNodeRef::Via(k, n)));
            }
            out.nodes.push(((key.x, key.y), key.smooth));
            out.handles.push(key.handle);
            out.refs.push(Some(PathNodeRef::Key(k)));
            out.at.push(out.nodes.len() - 1);
        }
        out
    }

    /// La tangente (de Hermite) de un nodo suave: la que se le dio con su tirador (tres veces el tirador, como en una curva
    /// de Bézier) o, si no, la de Catmull-Rom con sus vecinos.
    fn smooth_tangent(path: &PathNodes, j: usize) -> (f32, f32) {
        if let Some(h) = path.handles[j] {
            return (h.0 * 3.0, h.1 * 3.0);
        }
        let prev = path.nodes[j.saturating_sub(1)].0;
        let next = path.nodes[(j + 1).min(path.nodes.len() - 1)].0;
        ((next.0 - prev.0) * 0.5, (next.1 - prev.1) * 0.5)
    }

    /// Los nodos suaves del trayecto con su tirador de curvatura: qué nodo es, dónde está y hacia dónde apunta su tirador (un
    /// tercio de su tangente); el tirador de atrás es el simétrico.
    pub fn smooth_handles(&self) -> Vec<SmoothHandle> {
        let path = self.path_nodes();
        (0..path.nodes.len())
            .filter(|j| path.nodes[*j].1)
            .filter_map(|j| {
                let m = Self::smooth_tangent(&path, j);
                path.refs[j].map(|r| (r, path.nodes[j].0, (m.0 / 3.0, m.1 / 3.0)))
            })
            .collect()
    }

    /// El trayecto entre la posición `segment` y la siguiente, como una línea de muchos puntos: recto entre esquinas y con una
    /// curva (Hermite) donde algún extremo es suave.
    pub fn segment_curve(&self, segment: usize) -> Vec<(f32, f32)> {
        let path = self.path_nodes();
        let (Some(&from), Some(&to)) = (path.at.get(segment), path.at.get(segment + 1)) else {
            return Vec::new();
        };
        let mut out: Vec<(f32, f32)> = Vec::new();
        for k in from..to {
            let (p0, s0) = path.nodes[k];
            let (p1, s1) = path.nodes[k + 1];
            if !s0 && !s1 {
                out.push(p0);
                continue;
            }
            let tangent = |j: usize, smooth: bool| {
                if smooth {
                    Self::smooth_tangent(&path, j)
                } else {
                    (p1.0 - p0.0, p1.1 - p0.1)
                }
            };
            let (m0, m1) = (tangent(k, s0), tangent(k + 1, s1));
            for step in 0..CURVE_STEPS {
                let u = step as f32 / CURVE_STEPS as f32;
                let (u2, u3) = (u * u, u * u * u);
                let (h00, h10, h01, h11) = (
                    2.0 * u3 - 3.0 * u2 + 1.0,
                    u3 - 2.0 * u2 + u,
                    -2.0 * u3 + 3.0 * u2,
                    u3 - u2,
                );
                out.push((
                    h00 * p0.0 + h10 * m0.0 + h01 * p1.0 + h11 * m1.0,
                    h00 * p0.1 + h10 * m0.1 + h01 * p1.1 + h11 * m1.1,
                ));
            }
        }
        out.push(path.nodes[to].0);
        out
    }

    /// La lente de una cámara en el instante `t`: la de partida, que cambia en las posiciones que tienen la suya (un cambio de
    /// zoom) avanzando en línea recta en el tiempo de una a la siguiente.
    pub fn lens_at(&self, t: f32) -> f32 {
        let mut prev = (0.0_f32, self.lens_mm);
        for key in self.keys.iter().filter(|k| k.lens_mm.is_some()) {
            let mm = key.lens_mm.unwrap_or(self.lens_mm);
            if t <= key.t {
                let span = key.t - prev.0;
                let f = if span <= f32::EPSILON {
                    1.0
                } else {
                    ((t - prev.0) / span).clamp(0.0, 1.0)
                };
                return prev.1 + (mm - prev.1) * f;
            }
            prev = (key.t, mm);
        }
        prev.1
    }

    /// Cambia la lente en el instante `t`: en el segundo 0, la de partida; en otro, la de la posición de ese instante (que se
    /// crea, donde está la cosa en ese momento, si no la hay).
    pub fn set_lens_at(&mut self, t: f32, mm: f32) {
        if t < KEY_EPSILON {
            self.lens_mm = mm;
            return;
        }
        let (x, y, rot) = self.pose_at(t);
        self.set_pose_at(t, x, y, rot);
        if let Some(i) = self.key_at(t) {
            self.keys[i].lens_mm = Some(mm);
        }
    }

    /// Dónde está el operador de un boom en el instante `t`: el de partida, que cambia en las posiciones que tienen el suyo
    /// avanzando en línea recta en el tiempo de una a la siguiente. `None` si no es un boom o no tiene operador colocado.
    pub fn operator_at(&self, t: f32) -> Option<(f32, f32)> {
        let start = self.mic.as_ref()?.operator?;
        let mut prev = (0.0_f32, start);
        for key in self.keys.iter().filter(|k| k.operator.is_some()) {
            let at = key.operator.unwrap_or(start);
            if t <= key.t {
                let span = key.t - prev.0;
                let f = if span <= f32::EPSILON {
                    1.0
                } else {
                    ((t - prev.0) / span).clamp(0.0, 1.0)
                };
                return Some((
                    prev.1.0 + (at.0 - prev.1.0) * f,
                    prev.1.1 + (at.1 - prev.1.1) * f,
                ));
            }
            prev = (key.t, at);
        }
        Some(prev.1)
    }

    /// Ubica al operador de un boom en el instante `t`: en el segundo 0 (o si no se anima), su lugar de partida; en otro, el de
    /// la posición de ese instante (que se crea, donde está la cosa en ese momento, si no la hay).
    pub fn set_operator_at(&mut self, t: f32, at: (f32, f32)) {
        if t < KEY_EPSILON || !self.animatable() {
            if let Some(spec) = self.mic.as_mut() {
                spec.operator = Some(at);
            }
            return;
        }
        // Si el operador no tenía lugar propio, parte de donde se lo dibujaba (detrás de la cápsula).
        let (x0, y0, rot0) = (self.x, self.y, self.rot);
        if let Some(spec) = self.mic.as_mut()
            && spec.operator.is_none()
        {
            let r = rot0.to_radians();
            spec.operator = Some((x0 - 1.2 * r.cos(), y0 - 1.2 * r.sin()));
        }
        let (x, y, rot) = self.pose_at(t);
        self.set_pose_at(t, x, y, rot);
        if let Some(i) = self.key_at(t) {
            self.keys[i].operator = Some(at);
        }
    }

    /// Borra la posición del instante `t` y deja las demás: si es una guardada, se quita; si es la de partida y hay otras, la
    /// siguiente pasa a ser la de partida. Devuelve si borró algo. En un instante sin posición no hace nada.
    pub fn delete_position_at(&mut self, t: f32) -> bool {
        if let Some(i) = self.key_at(t) {
            self.keys.remove(i);
            return true;
        }
        if t < KEY_EPSILON && !self.keys.is_empty() {
            let first = self.keys.remove(0);
            (self.x, self.y, self.rot) = (first.x, first.y, first.rot);
            if let Some(mm) = first.lens_mm {
                self.lens_mm = mm;
            }
            if let (Some(op), Some(spec)) = (first.operator, self.mic.as_mut()) {
                spec.operator = Some(op);
            }
            return true;
        }
        false
    }

    /// Ubica la cosa en el instante `t`. En el segundo 0 (o si no se anima) cambia su posición de partida; en otro
    /// instante crea una posición o corrige la que ya hay ahí.
    pub fn set_pose_at(&mut self, t: f32, x: f32, y: f32, rot: f32) {
        if t < KEY_EPSILON || !self.animatable() {
            (self.x, self.y, self.rot) = (x, y, rot);
        } else if let Some(i) = self.key_at(t) {
            let key = &mut self.keys[i];
            (key.x, key.y, key.rot) = (x, y, rot);
        } else {
            self.keys.push(BlockKey::new(t, x, y, rot));
            self.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
        }
    }

    /// La posición guardada justo en el instante `t`, si la hay.
    pub fn key_at(&self, t: f32) -> Option<usize> {
        self.keys.iter().position(|k| (k.t - t).abs() < KEY_EPSILON)
    }

    /// Pasa el contenido viejo de `note` (la función de una luz o el tipo de un micrófono) a los campos con tipo, y deja
    /// `note` vacío. Una luz o un micrófono sin datos con tipo los recibe con sus valores iniciales.
    pub fn migrate_legacy_note(&mut self) {
        // El viejo interruptor de recorrido curvo pasa a la suavidad de cada posición.
        if self.curved {
            self.keys.iter_mut().for_each(|k| k.smooth = true);
            self.curved = false;
        }
        let note = std::mem::take(&mut self.note);
        let word = note.trim().to_lowercase();
        match self.kind {
            BlockKind::Light if self.light.is_none() => {
                if word == "rebote o bandera" {
                    self.kind = BlockKind::Modifier;
                    self.modifier = Some(ModifierSpec {
                        w: if self.w > 0.0 { self.w } else { 1.0 },
                        ..ModifierSpec::default()
                    });
                    (self.watts, self.kelvin) = (0, 0);
                    return;
                }
                let practical = word == "práctica" || word == "practica";
                let mut spec = LightSpec::new(if practical {
                    LightFixture::Practical
                } else {
                    LightFixture::Fresnel
                });
                spec.role = match word.as_str() {
                    "relleno" => LightRole::Fill,
                    "contraluz" => LightRole::Back,
                    "fondo" | "ambiente" => LightRole::Ambient,
                    "práctica" | "practica" => LightRole::Practical,
                    "efecto" => LightRole::Effect,
                    _ => LightRole::Key,
                };
                // El haz que se dibuja hasta ahora es `fov`; se conserva.
                if self.fov > 0.0 && !practical {
                    spec.beam_deg = self.fov;
                }
                self.light = Some(spec);
            }
            BlockKind::Mic if self.mic.is_none() => {
                let kind = match word.as_str() {
                    "corbatero" => MicKind::Lavalier,
                    "oculto" => MicKind::Plant,
                    _ => MicKind::Boom,
                };
                self.mic = Some(MicSpec::new(kind));
            }
            BlockKind::Modifier if self.modifier.is_none() => {
                self.modifier = Some(ModifierSpec::default())
            }
            // Lo que ya tiene datos con tipo conserva su nota como nota libre.
            _ => self.note = note,
        }
    }
}

/// Dónde queda el corbatero de una persona: `LAVALIER_OFFSET_M` adelante de su centro.
pub fn lavalier_spot(person: &BlockItem) -> (f32, f32) {
    let (sin, cos) = person.rot.to_radians().sin_cos();
    (
        person.x + LAVALIER_OFFSET_M * cos,
        person.y + LAVALIER_OFFSET_M * sin,
    )
}

/// La posición y el giro de `item` en el instante `t` entre las cosas `items`: la propia, salvo en un corbatero unido a una
/// persona, que está donde esa persona en `t`, `LAVALIER_OFFSET_M` adelante de su centro.
pub fn pose_in(items: &[BlockItem], item: &BlockItem, t: f32) -> (f32, f32, f32) {
    let person = item
        .mic
        .as_ref()
        .filter(|_| item.is_attached_lavalier())
        .and_then(|m| m.attached_to)
        .and_then(|id| {
            items
                .iter()
                .find(|i| i.id == id && i.kind == BlockKind::Actor)
        });
    match person {
        Some(person) => {
            let (x, y, rot) = person.pose_at(t);
            let (sin, cos) = rot.to_radians().sin_cos();
            (
                x + LAVALIER_OFFSET_M * cos,
                y + LAVALIER_OFFSET_M * sin,
                rot,
            )
        }
        None => item.pose_at(t),
    }
}

/// El bloqueo de una escena: un plano visto desde arriba con quién está dónde y desde dónde se filma.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Blocking {
    pub id: u64,
    /// Escena del guion a la que pertenece: el id de su encabezado.
    pub scene: u64,
    pub name: String,
    pub items: Vec<BlockItem>,
    /// Cuánto dura el movimiento, en segundos.
    pub duration: f32,
}

// Lo usan el dibujo y el inspector de las próximas etapas de las plantas.
#[allow(dead_code)]
impl BlockItem {
    /// Una cosa nueva de `kind` con las medidas de siempre para su tipo, y sus datos con tipo en su valor inicial.
    pub fn template(
        kind: BlockKind,
        id: u64,
        x: f32,
        y: f32,
        label: String,
        track: u32,
    ) -> BlockItem {
        let (w, h, fov) = match kind {
            BlockKind::Actor => (0.6, 0.6, 0.0),
            BlockKind::Object => (1.2, 0.7, 0.0),
            BlockKind::Wall => (3.0, 0.15, 0.0),
            BlockKind::Opening => (0.9, 0.15, 0.0),
            BlockKind::Camera => (0.5, 0.35, 40.0),
            BlockKind::Light => (0.4, 0.4, LightFixture::Fresnel.beam_deg()),
            BlockKind::Modifier => (1.0, 0.1, 0.0),
            BlockKind::Mic => (0.3, 0.3, 0.0),
        };
        let (watts, kelvin) = match kind {
            BlockKind::Light => LightFixture::Fresnel.watts_kelvin(),
            _ => (0, 0),
        };
        BlockItem {
            id,
            kind,
            x,
            y,
            w,
            h,
            label,
            fov,
            lens_mm: if kind == BlockKind::Camera { 35.0 } else { 0.0 },
            watts,
            kelvin,
            track,
            light: (kind == BlockKind::Light).then(LightSpec::default),
            modifier: (kind == BlockKind::Modifier).then(ModifierSpec::default),
            mic: (kind == BlockKind::Mic).then(MicSpec::default),
            ..BlockItem::default()
        }
    }
}

impl Blocking {
    /// El nombre con que sale la próxima cosa de `kind`: la siguiente letra libre para una cámara (A, B, C…), y L1, R1 o M1
    /// para una luz, un modificador o un micrófono. Las demás no llevan.
    pub fn next_label(&self, kind: BlockKind) -> String {
        let same = self.items.iter().filter(|i| i.kind == kind).count();
        match kind {
            BlockKind::Camera => {
                let used: Vec<&str> = self
                    .items
                    .iter()
                    .filter(|i| i.kind == kind)
                    .map(|i| i.label.as_str())
                    .collect();
                ('A'..='Z')
                    .map(String::from)
                    .find(|l| !used.contains(&l.as_str()))
                    .unwrap_or_else(|| format!("C{}", used.len() + 1))
            }
            BlockKind::Light => format!("L{}", same + 1),
            BlockKind::Modifier => format!("R{}", same + 1),
            BlockKind::Mic => format!("M{}", same + 1),
            _ => String::new(),
        }
    }

    /// La pista del grabador con que sale el próximo micrófono.
    pub fn next_track(&self, kind: BlockKind) -> u32 {
        if kind == BlockKind::Mic {
            self.items.iter().filter(|i| i.kind == kind).count() as u32 + 1
        } else {
            0
        }
    }

    /// Dónde está una cosa de este bloqueo en el instante `t` (ver `pose_in`).
    #[allow(dead_code)]
    pub fn pose_of(&self, item: &BlockItem, t: f32) -> (f32, f32, f32) {
        pose_in(&self.items, item, t)
    }
}

/// Lo que la producción sabe de una escena y el texto no dice. Se guarda con el id del encabezado como clave, así que
/// sobrevive a que la escena cambie de lugar o de nombre.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneMeta {
    /// Número fijo de la escena («12», «12A»). Vacío mientras la numeración no esté bloqueada.
    pub number: String,
    /// Una escena agregada después de bloquear: su letra es provisoria y la app la recalcula según su lugar en el
    /// guion hasta que se fije.
    pub provisional: bool,
    /// Octavos de página, si se corrigieron a mano; si no, se calculan de las líneas de la escena.
    pub eighths: Option<u16>,
    /// Día de la historia en que transcurre («Día 2»).
    pub story_day: Option<u32>,
    /// Resumen de una línea.
    pub summary: String,
    /// Tiempo estimado de rodaje, en minutos, si se cargó a mano.
    pub est_minutes: Option<u32>,
    pub notes: String,
    /// El lugar donde se filma esta escena, si no es el de su locación del guion.
    pub shoot_location: Option<u64>,
}

/// Colores de las revisiones, en el orden habitual de la industria (el blanco es el guion original).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevisionColor {
    White,
    #[default]
    Blue,
    Pink,
    Yellow,
    Green,
    Goldenrod,
    Buff,
    Salmon,
    Cherry,
}

impl RevisionColor {
    pub const ALL: [RevisionColor; 9] = [
        RevisionColor::White,
        RevisionColor::Blue,
        RevisionColor::Pink,
        RevisionColor::Yellow,
        RevisionColor::Green,
        RevisionColor::Goldenrod,
        RevisionColor::Buff,
        RevisionColor::Salmon,
        RevisionColor::Cherry,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RevisionColor::White => "Blanca",
            RevisionColor::Blue => "Azul",
            RevisionColor::Pink => "Rosa",
            RevisionColor::Yellow => "Amarilla",
            RevisionColor::Green => "Verde",
            RevisionColor::Goldenrod => "Dorada",
            RevisionColor::Buff => "Crema",
            RevisionColor::Salmon => "Salmón",
            RevisionColor::Cherry => "Cereza",
        }
    }

    /// El color de la revisión para marcar en pantalla (más saturado que el tono de la hoja).
    pub fn strong(self) -> [u8; 3] {
        match self {
            RevisionColor::White => [150, 150, 150],
            RevisionColor::Blue => [60, 130, 235],
            RevisionColor::Pink => [235, 90, 160],
            RevisionColor::Yellow => [215, 180, 20],
            RevisionColor::Green => [50, 165, 80],
            RevisionColor::Goldenrod => [205, 150, 20],
            RevisionColor::Buff => [185, 155, 105],
            RevisionColor::Salmon => [235, 115, 85],
            RevisionColor::Cherry => [205, 40, 80],
        }
    }

    /// El tono suave con que se tiñen las hojas de esa revisión.
    pub fn tint(self) -> [u8; 3] {
        match self {
            RevisionColor::White => [255, 255, 255],
            RevisionColor::Blue => [214, 230, 255],
            RevisionColor::Pink => [255, 222, 235],
            RevisionColor::Yellow => [255, 248, 190],
            RevisionColor::Green => [214, 240, 214],
            RevisionColor::Goldenrod => [247, 228, 168],
            RevisionColor::Buff => [243, 232, 208],
            RevisionColor::Salmon => [255, 218, 204],
            RevisionColor::Cherry => [245, 200, 206],
        }
    }
}

/// Una revisión del guion: desde que se crea, lo que se cambia queda marcado respecto de cómo estaba.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Revision {
    pub id: u64,
    pub name: String,
    pub color: RevisionColor,
    /// Fecha en que se creó (AAAA-MM-DD).
    pub date: String,
    /// Cómo estaba cada párrafo al crearla (tipo y texto), para saber cuáles cambiaron o son nuevos.
    pub baseline: BTreeMap<u64, (ParagraphKind, String)>,
}

/// Un plano del guion técnico.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shot {
    pub id: u64,
    /// Escena del guion a la que pertenece: el id de su encabezado.
    pub scene: u64,
    /// Tipo de plano (general, medio, primer plano…).
    pub size: String,
    pub movement: String,
    pub description: String,
    /// Lo que se dice en el plano.
    pub dialogue: String,
    /// Sonido, efectos o música del plano.
    pub sound: String,
    /// Duración aproximada del plano (por ejemplo «5», «5 s» o «0:05»).
    pub duration: String,
    /// Imagen del storyboard.
    pub image: Option<PathBuf>,
    /// Los trazos del boceto que dio esa imagen, para poder retocarlo. Vacío si la imagen vino de un archivo.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sketch: Vec<SketchStroke>,
    /// Bloqueo de cámara con el que se vincula el plano y la cámara de ese bloqueo que lo filma.
    pub blocking: Option<u64>,
    pub camera: Option<u64>,
    /// Puesta de luces con la que se vincula el plano (todavía sin pantalla propia).
    pub lighting: Option<u64>,
    /// Valores de las columnas que añadió el usuario, por id de columna.
    pub extra: BTreeMap<u64, String>,
    /// Lente propia del plano, en mm; sin ella, la de su cámara.
    #[serde(default)]
    pub lens_mm: Option<f32>,
    /// Trípode, mano, gimbal, slider, dolly, grúa, dron…
    #[serde(default)]
    pub support: String,
    /// Normal, picado, contrapicado, cenital…
    #[serde(default)]
    pub angle: String,
    /// Los personajes que están en cuadro (ids de personaje).
    #[serde(default)]
    pub in_frame: Vec<u64>,
    /// En qué orden se filma, si se ordenó por emplazamiento.
    #[serde(default)]
    pub shoot_order: Option<u32>,
    /// Tramo del movimiento del bloqueo que cubre el plano, en segundos (desde, hasta); sin él, todo el bloqueo.
    #[serde(default)]
    pub range: Option<(f32, f32)>,
    /// Lo de la planta (micrófonos, luces…) que se agregó trabajando con este plano filtrado, por id de la cosa.
    #[serde(default)]
    pub items: Vec<u64>,
}

/// Tipos de plano: la abreviatura que se escribe en la tabla y su nombre completo.
pub const SHOT_SIZES: [(&str, &str); 12] = [
    ("GPG", "Gran plano general"),
    ("PG", "Plano general"),
    ("PE", "Plano entero"),
    ("PA", "Plano americano"),
    ("PM", "Plano medio"),
    ("PMC", "Plano medio corto"),
    ("PML", "Plano medio largo"),
    ("PP", "Primer plano"),
    ("PPP", "Primerísimo primer plano"),
    ("PD", "Plano detalle"),
    ("OTS", "Sobre el hombro"),
    ("POV", "Plano subjetivo"),
];

/// Qué muestra una columna de la tabla del guion técnico.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnKind {
    Scene,
    Number,
    Size,
    Movement,
    Description,
    Dialogue,
    Sound,
    Duration,
    /// La cámara del bloqueo que filma el plano.
    Camera,
    Storyboard,
    /// Código del plano: «12A».
    Code,
    Lens,
    Support,
    Angle,
    InFrame,
    ShootOrder,
    Estimate,
    /// Una columna de texto que armó el usuario.
    Custom,
}

impl ColumnKind {
    pub fn default_name(self) -> &'static str {
        match self {
            ColumnKind::Scene => "Escena",
            ColumnKind::Number => "Plano",
            ColumnKind::Size => "Tipo de plano",
            ColumnKind::Movement => "Movimiento",
            ColumnKind::Description => "Descripción",
            ColumnKind::Dialogue => "Diálogo",
            ColumnKind::Sound => "Sonido",
            ColumnKind::Duration => "Duración",
            ColumnKind::Camera => "Cámara",
            ColumnKind::Storyboard => "Storyboard",
            ColumnKind::Code => "Código",
            ColumnKind::Lens => "Lente",
            ColumnKind::Support => "Soporte",
            ColumnKind::Angle => "Angulación",
            ColumnKind::InFrame => "En cuadro",
            ColumnKind::ShootOrder => "Orden de rodaje",
            ColumnKind::Estimate => "Tiempo",
            ColumnKind::Custom => "Columna",
        }
    }

    /// Las columnas que se agregan y se quitan a gusto (las demás son las de siempre).
    pub const OPTIONAL: [ColumnKind; 7] = [
        ColumnKind::Code,
        ColumnKind::Lens,
        ColumnKind::Support,
        ColumnKind::Angle,
        ColumnKind::InFrame,
        ColumnKind::ShootOrder,
        ColumnKind::Estimate,
    ];

    pub fn is_optional(self) -> bool {
        Self::OPTIONAL.contains(&self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShotColumn {
    pub id: u64,
    pub kind: ColumnKind,
    pub name: String,
    /// Ancho que el usuario le dio arrastrando el borde; sin él, el ancho de siempre.
    #[serde(default)]
    pub width: Option<f32>,
}

/// Una escena de la escaleta: su encabezado está escrito en el tratamiento y le pertenece todo el texto hasta el
/// siguiente encabezado (o el final).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineScene {
    pub id: u64,
    /// El encabezado tal como irá en el guion («INT. CASA - NOCHE»).
    pub heading: String,
    /// El encabezado del guion en el que se importó esta escena, para no importarla dos veces.
    #[serde(default)]
    pub imported: Option<u64>,
}

/// Comentario asociado a un párrafo del guion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    pub paragraph: u64,
    pub text: String,
    #[serde(default)]
    pub resolved: bool,
    /// Texto seleccionado al comentar; sin él, el comentario es sobre todo el párrafo.
    #[serde(default)]
    pub quote: Option<String>,
    /// Es un comentario del tratamiento (no de un párrafo del guion): `paragraph` no se usa.
    #[serde(default)]
    pub in_treatment: bool,
}

/// Qué es un punto de la curva dramática.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DramaKind {
    Start,
    Inciting,
    TurnOne,
    Midpoint,
    TurnTwo,
    Climax,
    Resolution,
    Peak,
    Valley,
    #[default]
    Custom,
}

impl DramaKind {
    pub const ALL: [DramaKind; 8] = [
        DramaKind::Start,
        DramaKind::TurnOne,
        DramaKind::TurnTwo,
        DramaKind::Climax,
        DramaKind::Resolution,
        DramaKind::Peak,
        DramaKind::Valley,
        DramaKind::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DramaKind::Start => "Inicio",
            DramaKind::Inciting => "Incidente desencadenante",
            DramaKind::TurnOne => "Punto de giro",
            DramaKind::Midpoint => "Punto medio",
            DramaKind::TurnTwo => "Punto de giro",
            DramaKind::Climax => "Clímax",
            DramaKind::Resolution => "Resolución",
            DramaKind::Peak => "Pico",
            DramaKind::Valley => "Valle",
            DramaKind::Custom => "Punto personalizado",
        }
    }
}

/// Un punto de la curva dramática: dónde está en la historia (`x`, de 0 a 1), cuánta tensión tiene (`y`, de 0 a 1) y lo que se
/// anota para él.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DramaPoint {
    pub id: u64,
    pub kind: DramaKind,
    pub label: String,
    pub text: String,
    pub x: f32,
    pub y: f32,
}

/// Cómo se llama en la nube una imagen del proyecto.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudImage {
    /// SHA-256 del archivo, en hexadecimal.
    pub hash: String,
    /// jpg, png o webp.
    pub ext: String,
}

/// Cuántos ids tiene cada persona en su tramo (ver `Project::use_id_block`).
pub const ID_BLOCK: u64 = 1_000_000_000;

/// Los roles que tienen su propia lista de pedidos por escena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// El guion sonoro: lo que cada escena pide de sonido.
    Sound,
    /// El equipo de sonido que se necesita.
    SoundGear,
    /// El equipo de fotografía: cámaras, lentes, luces y soportes.
    PhotoGear,
    Art,
}

impl Role {
    pub fn title(self) -> &'static str {
        match self {
            Role::Sound => "Guion sonoro",
            Role::SoundGear => "Equipo de sonido",
            Role::PhotoGear => "Equipos de fotografía",
            Role::Art => "Dirección de arte",
        }
    }

    /// Los tipos de fila.
    pub fn kinds(self) -> &'static [&'static str] {
        match self {
            Role::Sound => &["Diálogo", "Ambiente", "Efecto", "Foley", "Música", "Silencio"],
            Role::SoundGear => &[
                "Micrófono",
                "Boom",
                "Inalámbrico",
                "Grabador",
                "Mezclador",
                "Auriculares",
                "Accesorio",
                "Otro",
            ],
            Role::PhotoGear => &[
                "Cámara",
                "Lente",
                "Luz",
                "Modificador",
                "Soporte",
                "Filtro",
                "Otro",
            ],
            Role::Art => &[
                "Utilería",
                "Escenografía",
                "Vestuario",
                "Maquillaje",
                "Vehículo",
                "Otro",
            ],
        }
    }

    pub fn statuses(self) -> &'static [&'static str] {
        match self {
            Role::Sound => &["Pendiente", "Grabado", "Editado", "Mezclado"],
            Role::SoundGear | Role::PhotoGear => {
                &["Pendiente", "Reservado", "Conseguido", "Listo en set"]
            }
            Role::Art => &["Pendiente", "En búsqueda", "Conseguido", "Listo en set"],
        }
    }

    /// Los encabezados de las columnas: tipo, título, detalle, extra, estado y notas.
    pub fn headers(self) -> [&'static str; 6] {
        match self {
            Role::Sound => ["Tipo", "Sonido", "Entra / sale", "Fuente / archivo", "Estado", "Notas"],
            Role::SoundGear | Role::PhotoGear => {
                ["Tipo", "Equipo", "Detalle", "Responsable", "Estado", "Notas"]
            }
            Role::Art => ["Categoría", "Elemento", "Descripción", "Responsable", "Estado", "Notas"],
        }
    }
}

/// Una fila de la pantalla de un rol (guion sonoro, Dirección de Arte): algo que una escena pide, con su tipo, su detalle, a
/// cargo de quién está y cómo va. Cada rol llena sus propias filas, pensando en que cada uno trabaje en su lugar.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoleEntry {
    pub id: u64,
    /// Escena del guion (id de su encabezado); sin escena, es algo general de la película.
    pub scene: Option<u64>,
    pub kind: String,
    pub title: String,
    pub detail: String,
    /// Fuente o archivo (sonido) / responsable (arte).
    pub extra: String,
    pub status: String,
    pub notes: String,
    /// Antes guardaba el elemento del Desglose de esta fila; hoy el Desglose se arma agrupando por nombre.
    pub breakdown_item: Option<u64>,
    /// Cuántos hay (0 vale uno).
    pub quantity: u32,
    /// Si la fila la creó la planta (por lo que hay en ella): «rol|escena|nombre». Se actualiza y se quita sola.
    pub plan_key: Option<String>,
    /// A qué se vincula en el Desglose: un objeto clave del proyecto o una categoría propia.
    pub link: Option<RoleLink>,
}

/// El vínculo de una fila de un rol con el Desglose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleLink {
    /// Es el mismo que este objeto clave (comparten fila y escenas en el Desglose).
    Object(u64),
    /// Va a esta categoría del Desglose (por ejemplo una propia) y no a la de su tipo.
    Category(u64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub title: String,
    pub author: String,
    pub contact: String,
    pub version: String,
    pub logline: String,
    pub synopsis: String,
    pub treatment: String,
    /// Nombres del tratamiento que apuntan a un personaje, locación, objeto, elemento o mundo.
    pub treatment_links: Vec<Link>,
    /// Nombres de la sinopsis que apuntan a un personaje, locación, objeto, elemento o mundo.
    pub synopsis_links: Vec<Link>,
    /// Guion técnico: los planos de todas las escenas.
    pub shots: Vec<Shot>,
    /// Bloqueos de cámara, uno o más por escena.
    pub blockings: Vec<Blocking>,
    /// Datos de producción de cada escena, por id de su encabezado.
    pub scenes: BTreeMap<u64, SceneMeta>,
    /// Con la numeración bloqueada, las escenas conservan su número: una nueva entre la 12 y la 13 es la 12A y una
    /// borrada queda omitida en vez de renumerar las demás.
    pub scene_numbers_locked: bool,
    /// Revisiones del guion, de la más vieja a la más nueva.
    pub revisions: Vec<Revision>,
    /// Los días de rodaje y el orden en que se filman las escenas (ver `schedule.rs`).
    pub shoot_days: Vec<crate::schedule::ShootDay>,
    pub schedule: Vec<crate::schedule::StripEntry>,
    /// Cuánto dura una jornada de rodaje, en minutos, y cuánto se calcula por octavo de página si la escena no tiene
    /// un tiempo propio.
    pub day_minutes: u32,
    pub minutes_per_eighth: u32,
    /// Diferencia de la hora local con UTC, en minutos (Argentina: −180). Se usa para la salida y la puesta del sol.
    pub utc_offset_minutes: i32,
    /// Los cuerpos de cámara del proyecto (cada uno con su sensor) y los minutos que se calculan por emplazamiento y por
    /// plano para estimar cuánto lleva filmar una escena.
    pub camera_bodies: Vec<crate::optics::CameraBody>,
    /// El cuerpo (sensor) con que nacen las cámaras nuevas; sin él, full frame.
    pub camera_body_default: Option<u64>,
    /// Ya se le preguntó a la persona qué cámara usa (al poner la primera).
    pub camera_body_asked: bool,
    pub minutes_per_setup: u32,
    pub minutes_per_shot: u32,
    /// Proporción del cuadro de la película (ancho, alto): para el storyboard.
    pub aspect: crate::optics::Aspect,
    /// Colores de las tiras: interior de día, exterior de día, interior de noche y exterior de noche.
    pub strip_colors: [[u8; 3]; 4],
    /// Los lugares reales de rodaje y la gente del proyecto.
    pub shoot_locations: Vec<ShootLocation>,
    pub contacts: Vec<Contact>,
    /// Las imágenes que ya están en la nube: ruta (relativa a la carpeta del proyecto) -> huella y tipo en la nube.
    pub cloud_images: BTreeMap<String, CloudImage>,
    /// Los elementos del Desglose que reflejan las listas de Dirección de arte, sonido y fotografía: clave «categoría|nombre».
    pub role_breakdown: BTreeMap<String, u64>,
    /// Los puntos de la curva dramática.
    pub drama: Vec<DramaPoint>,
    /// El guion sonoro: lo que cada escena pide de sonido.
    pub sound_cues: Vec<RoleEntry>,
    /// El equipo de sonido que se necesita.
    pub sound_gear: Vec<RoleEntry>,
    /// El equipo de fotografía: cámaras y luces que se tienen o se necesitan.
    pub photo_gear: Vec<RoleEntry>,
    /// Lo que Dirección de Arte necesita en cada escena.
    pub art_items: Vec<RoleEntry>,
    /// La revisión que marca los cambios; sin ninguna, nada se marca.
    pub active_revision: Option<u64>,
    /// Agregar «(CONT.)» al maquetar cuando un personaje retoma la palabra después de una acción.
    pub auto_cont: bool,
    /// Columnas de la tabla del guion técnico, en el orden en que se muestran.
    pub shot_columns: Vec<ShotColumn>,
    /// En la vista de storyboard, no mostrar los planos que todavía no tienen imagen.
    pub storyboard_only_images: bool,
    /// La numeración de planos vuelve a 1 en cada escena (si no, es continua en todo el guion).
    pub shots_restart: bool,
    /// Escaleta: las escenas armadas con fragmentos del tratamiento.
    pub outline: Vec<OutlineScene>,
    pub characters: Vec<CharacterProfile>,
    pub paragraphs: Vec<Paragraph>,
    pub comments: Vec<Comment>,
    pub documents: Vec<Document>,
    pub location_profiles: Vec<LocationProfile>,
    pub objects: Vec<ObjectItem>,
    pub element_categories: Vec<ElementCategory>,
    pub elements: Vec<Element>,
    pub breakdown_categories: Vec<BreakdownCategory>,
    pub breakdown_items: Vec<BreakdownItem>,
    pub breakdown_tags: Vec<BreakdownTag>,
    /// Versión de las categorías del desglose, para migrar proyectos viejos.
    pub breakdown_version: u32,
    /// Qué nombres van en mayúsculas, por categoría.
    pub name_case: NameCase,
    /// Si ya se tomó la opción global anterior como punto de partida (proyectos de antes de las opciones por categoría).
    pub name_case_migrated: bool,
    /// Formato anterior: locaciones añadidas a mano por nombre. Al abrir se pasan a fichas con id y queda vacío.
    pub extra_locations: Vec<String>,
    /// Imagen de portada que se ve en la pantalla de inicio.
    pub cover: Option<PathBuf>,
    /// Carpeta del archivo del proyecto: contra ella se resuelven las rutas relativas de las imágenes.
    #[serde(skip)]
    pub base_dir: Option<PathBuf>,
    next_id: u64,
}

impl Default for Project {
    fn default() -> Self {
        let mut p = Self {
            title: String::from("Sin título"),
            author: String::new(),
            contact: String::new(),
            version: String::new(),
            logline: String::new(),
            synopsis: String::new(),
            treatment: String::new(),
            treatment_links: Vec::new(),
            synopsis_links: Vec::new(),
            outline: Vec::new(),
            shots: Vec::new(),
            blockings: Vec::new(),
            scenes: BTreeMap::new(),
            scene_numbers_locked: false,
            revisions: Vec::new(),
            shoot_days: Vec::new(),
            schedule: Vec::new(),
            day_minutes: 600,
            minutes_per_eighth: 10,
            utc_offset_minutes: -180,
            camera_bodies: Vec::new(),
            camera_body_default: None,
            camera_body_asked: false,
            minutes_per_setup: 20,
            minutes_per_shot: 5,
            aspect: crate::optics::Aspect::default(),
            strip_colors: crate::schedule::DEFAULT_STRIP_COLORS,
            shoot_locations: Vec::new(),
            contacts: Vec::new(),
            drama: Vec::new(),
            role_breakdown: BTreeMap::new(),
            cloud_images: BTreeMap::new(),
            sound_cues: Vec::new(),
            sound_gear: Vec::new(),
            photo_gear: Vec::new(),
            art_items: Vec::new(),
            active_revision: None,
            auto_cont: true,
            shot_columns: Vec::new(),
            shots_restart: false,
            storyboard_only_images: false,
            characters: Vec::new(),
            paragraphs: Vec::new(),
            comments: Vec::new(),
            documents: Vec::new(),
            location_profiles: Vec::new(),
            objects: Vec::new(),
            element_categories: Vec::new(),
            elements: Vec::new(),
            breakdown_categories: Vec::new(),
            breakdown_items: Vec::new(),
            breakdown_tags: Vec::new(),
            breakdown_version: 0,
            name_case: NameCase::default(),
            name_case_migrated: false,
            extra_locations: Vec::new(),
            cover: None,
            base_dir: None,
            next_id: 1,
        };
        let first = p.make_paragraph(ParagraphKind::SceneHeading, "");
        p.paragraphs.push(first);
        p.ensure_breakdown_defaults();
        p
    }
}

pub struct SceneInfo {
    pub number: usize,
    pub id: u64,
    pub heading: String,
    pub snippet: String,
    pub words: usize,
    pub seconds: f32,
}

#[derive(Default)]
pub struct KindStats {
    pub words: usize,
    pub occurrences: usize,
}

pub struct Stats {
    pub words: usize,
    pub chars_with_spaces: usize,
    pub chars_without_spaces: usize,
    pub pages: f32,
    pub seconds: f32,
    pub per_kind: Vec<(ParagraphKind, KindStats)>,
}

pub struct CharacterActivity {
    /// `None` para encabezados de personaje que todavía no se vincularon a una ficha.
    pub id: Option<u64>,
    pub name: String,
    pub dialogues: usize,
    pub words: usize,
    pub scenes: usize,
}

/// Líneas de página que ocupa un párrafo (aprox., para estimar páginas y duración).
fn page_lines(p: &Paragraph) -> f32 {
    let wrap = |cols: usize| -> f32 {
        p.text
            .lines()
            .map(|l| (l.chars().count().max(1) as f32 / cols as f32).ceil())
            .sum::<f32>()
            .max(1.0)
    };
    match p.kind {
        ParagraphKind::SceneHeading | ParagraphKind::Transition => 2.0,
        ParagraphKind::Action => wrap(60) + 1.0,
        ParagraphKind::Character => 1.0,
        ParagraphKind::Parenthetical => wrap(25),
        ParagraphKind::Dialogue | ParagraphKind::Lyrics => wrap(35) + 1.0,
        ParagraphKind::Note | ParagraphKind::Act | ParagraphKind::Sequence => 0.0,
    }
}

const LINES_PER_PAGE: f32 = 55.0;

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

impl Project {
    pub fn make_paragraph(&mut self, kind: ParagraphKind, text: &str) -> Paragraph {
        let id = self.next_id;
        self.next_id += 1;
        Paragraph {
            id,
            kind,
            text: text.to_string(),
            links: Vec::new(),
        }
    }

    /// Asigna ids a los párrafos que no tengan (proyectos antiguos) y garantiza al menos uno.
    pub fn normalize(&mut self) {
        // Los datos de una escena que ya no está en el guion se pierden, salvo su número si estaba fijo (queda omitida).
        let alive: std::collections::HashSet<u64> = self
            .paragraphs
            .iter()
            .filter(|p| p.kind == ParagraphKind::SceneHeading)
            .map(|p| p.id)
            .collect();
        self.scenes
            .retain(|id, m| alive.contains(id) || !m.number.is_empty());
        self.prune_production_links();
        self.prune_schedule();
        self.sync_scene_numbers();
        for b in &mut self.blockings {
            for item in &mut b.items {
                item.migrate_legacy_note();
            }
            let last = b
                .items
                .iter()
                .flat_map(|i| i.keys.iter().map(|k| k.t))
                .fold(0.0, f32::max);
            b.duration = if b.duration > 0.0 {
                b.duration.max(last)
            } else {
                DEFAULT_BLOCK_SECONDS.max(last)
            };
        }
        // Los tipos de plano escritos con su nombre completo pasan a su abreviatura («Plano general» → «PG»).
        for shot in &mut self.shots {
            if let Some((abbr, _)) = SHOT_SIZES
                .iter()
                .find(|(_, full)| full.eq_ignore_ascii_case(shot.size.trim()))
            {
                shot.size = abbr.to_string();
            }
        }
        let mut max = self.paragraphs.iter().map(|p| p.id).max().unwrap_or(0);
        for p in &mut self.paragraphs {
            if p.id == 0 {
                max += 1;
                p.id = max;
            }
        }
        let max = max.max(self.comments.iter().map(|c| c.id).max().unwrap_or(0));
        let max = max.max(self.documents.iter().map(|d| d.id).max().unwrap_or(0));
        let max = max.max(
            self.element_categories
                .iter()
                .map(|c| c.id)
                .max()
                .unwrap_or(0),
        );
        let max = max.max(self.elements.iter().map(|e| e.id).max().unwrap_or(0));
        let max = max.max(
            self.breakdown_categories
                .iter()
                .map(|c| c.id)
                .max()
                .unwrap_or(0),
        );
        let max = max.max(
            self.outline
                .iter()
                .flat_map(|o| std::iter::once(o.id))
                .max()
                .unwrap_or(0),
        );
        let max = max.max(self.shots.iter().map(|s| s.id).max().unwrap_or(0));
        let max = max.max(
            self.blockings
                .iter()
                .flat_map(|b| std::iter::once(b.id).chain(b.items.iter().map(|i| i.id)))
                .max()
                .unwrap_or(0),
        );
        let max = max.max(self.revisions.iter().map(|r| r.id).max().unwrap_or(0));
        let max = max.max(self.shoot_locations.iter().map(|l| l.id).max().unwrap_or(0));
        let max = max.max(self.shoot_days.iter().map(|d| d.id).max().unwrap_or(0));
        let max = max.max(self.contacts.iter().map(|c| c.id).max().unwrap_or(0));
        let max = max.max(self.shot_columns.iter().map(|c| c.id).max().unwrap_or(0));
        let max = max.max(self.breakdown_items.iter().map(|i| i.id).max().unwrap_or(0));
        let max = max.max(self.breakdown_tags.iter().map(|t| t.id).max().unwrap_or(0));
        let max = max.max(
            self.documents
                .iter()
                .flat_map(|d| d.nodes.iter().map(|n| n.id))
                .max()
                .unwrap_or(0),
        );
        self.next_id = self.next_id.max(max + 1);
        if self.paragraphs.is_empty() {
            let p = self.make_paragraph(ParagraphKind::SceneHeading, "");
            self.paragraphs.push(p);
        }
        self.ensure_breakdown_defaults();
        self.prune_camera_links();
        // Proyectos viejos: fichas y objetos sin id, y personajes que solo existían como texto.
        let high = self
            .characters
            .iter()
            .map(|c| c.id)
            .chain(self.objects.iter().map(|o| o.id))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(high + 1);
        for i in 0..self.characters.len() {
            if self.characters[i].id == 0 {
                self.characters[i].id = self.next_id;
                self.next_id += 1;
            }
        }
        for i in 0..self.objects.len() {
            if self.objects[i].id == 0 {
                self.objects[i].id = self.next_id;
                self.next_id += 1;
            }
        }
        // Locaciones: las fichas guardadas por nombre y las añadidas a mano pasan a tener id.
        let high = self
            .location_profiles
            .iter()
            .map(|l| l.id)
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(high + 1);
        for i in 0..self.location_profiles.len() {
            if self.location_profiles[i].id == 0 {
                self.location_profiles[i].id = self.next_id;
                self.next_id += 1;
            }
        }
        for name in std::mem::take(&mut self.extra_locations) {
            if self.location_id_by_name(&name).is_none() {
                self.new_location(&name);
            }
        }
        // Desglose: los elementos de personajes, locaciones y objetos pasan a seguirse por id.
        for i in 0..self.breakdown_items.len() {
            if self.breakdown_items[i].entity.is_some() {
                continue;
            }
            let name = self.breakdown_items[i].name.clone();
            let category = self.breakdown_items[i].category;
            let Some(source) = self
                .breakdown_categories
                .iter()
                .find(|c| c.id == category)
                .map(|c| c.source)
            else {
                continue;
            };
            self.breakdown_items[i].entity = match source {
                CategorySource::Characters => {
                    Some(EntityRef::Character(self.ensure_character(&name)))
                }
                CategorySource::Locations => Some(EntityRef::Location(self.ensure_location(&name))),
                CategorySource::Objects => Some(EntityRef::Object(self.ensure_object(&name))),
                CategorySource::Custom => None,
            };
        }
        self.auto_link_all();
        self.sync_links();
        self.purge_breakdown_items();
    }

    pub fn scenes(&self) -> Vec<SceneInfo> {
        let mut out: Vec<SceneInfo> = Vec::new();
        let mut lines = 0.0;
        for p in &self.paragraphs {
            if p.kind == ParagraphKind::SceneHeading {
                if let Some(last) = out.last_mut() {
                    last.seconds = lines / LINES_PER_PAGE * 60.0;
                }
                lines = 0.0;
                out.push(SceneInfo {
                    number: out.len() + 1,
                    id: p.id,
                    heading: p.text.clone(),
                    snippet: String::new(),
                    words: 0,
                    seconds: 0.0,
                });
            }
            let Some(scene) = out.last_mut() else {
                continue;
            };
            if p.kind == ParagraphKind::Action && scene.snippet.is_empty() {
                scene.snippet = p.text.clone();
            }
            scene.words += word_count(&p.text);
            lines += page_lines(p);
        }
        if let Some(last) = out.last_mut() {
            last.seconds = lines / LINES_PER_PAGE * 60.0;
        }
        out
    }

    pub fn add_comment(&mut self, paragraph: u64, text: &str, quote: Option<String>) {
        let id = self.next_id;
        self.next_id += 1;
        self.comments.push(Comment {
            id,
            paragraph,
            text: text.to_string(),
            resolved: false,
            quote,
            in_treatment: false,
        });
    }

    pub fn add_treatment_comment(&mut self, text: &str, quote: Option<String>) {
        self.add_comment(0, text, quote);
        if let Some(c) = self.comments.last_mut() {
            c.in_treatment = true;
        }
    }

    /// Ruta real de una imagen: las relativas se buscan en la carpeta del proyecto.
    pub fn resolve_path(&self, path: &std::path::Path) -> PathBuf {
        match (&self.base_dir, path.is_relative()) {
            (Some(base), true) => base.join(path),
            _ => path.to_path_buf(),
        }
    }

    /// Todas las rutas de imágenes del proyecto (portada, fotos y galerías).
    pub fn image_paths_mut(&mut self) -> Vec<&mut PathBuf> {
        let mut out: Vec<&mut PathBuf> = Vec::new();
        if let Some(c) = self.cover.as_mut() {
            out.push(c);
        }
        for c in &mut self.characters {
            if let Some(p) = c.photo.as_mut() {
                out.push(p);
            }
        }
        for e in &mut self.elements {
            if let Some(p) = e.photo.as_mut() {
                out.push(p);
            }
        }
        for d in &mut self.documents {
            out.extend(d.images.iter_mut());
        }
        for shot in &mut self.shots {
            if let Some(p) = shot.image.as_mut() {
                out.push(p);
            }
        }
        for place in &mut self.shoot_locations {
            out.extend(place.photos.iter_mut());
            if let Some(p) = place.floor_plan.as_mut() {
                out.push(p);
            }
        }
        out
    }

    /// Saca las rutas de imágenes que apuntan fuera de la carpeta del proyecto (absolutas o con «..»). Lo que llega de otra
    /// persona (por la carpeta compartida o la nube) no puede hacer que el programa copie, muestre o suba archivos de esta
    /// computadora. Devuelve cuántas sacó.
    pub fn drop_foreign_image_paths(&mut self) -> usize {
        let mut dropped = 0;
        for path in self.image_paths_mut() {
            let foreign = path.is_absolute()
                || path.has_root()
                || path.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_)));
            if foreign {
                *path = PathBuf::new();
                dropped += 1;
            }
        }
        self.cloud_images.retain(|k, _| {
            let p = std::path::Path::new(k);
            !(p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)))
        });
        dropped
    }

    /// Cambia cuando cambia el conjunto de imágenes; sirve para saber si hay algo nuevo que copiar sin revisar el
    /// disco en cada cuadro.
    pub fn image_signature(&mut self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for p in self.image_paths_mut() {
            p.hash(&mut h);
        }
        h.finish()
    }

    /// Las opciones de mayúsculas de este proyecto.
    pub fn case_options(&self) -> CaseOptions {
        CaseOptions {
            base: Some(self.name_case),
            categories: self
                .element_categories
                .iter()
                .map(|c| (c.id, c.uppercase))
                .collect(),
            element_category: self.elements.iter().map(|e| (e.id, e.category)).collect(),
            unlinked: false,
            fallback: true,
            as_written: false,
        }
    }

    pub fn character(&self, id: u64) -> Option<&CharacterProfile> {
        self.characters.iter().find(|c| c.id == id)
    }

    pub fn character_id_by_name(&self, name: &str) -> Option<u64> {
        let name = clean_character_name(name);
        self.characters
            .iter()
            .find(|c| clean_character_name(&c.name) == name)
            .map(|c| c.id)
    }

    pub fn object_id_by_name(&self, name: &str) -> Option<u64> {
        self.objects
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(name.trim()))
            .map(|o| o.id)
    }

    /// Crea una ficha de personaje y devuelve su id.
    pub fn new_character(&mut self, name: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.characters.push(CharacterProfile {
            id,
            name: clean_character_name(name),
            ..Default::default()
        });
        id
    }

    /// El personaje con ese nombre; si no hay ninguno, lo crea. Con varios del mismo nombre devuelve el primero.
    pub fn ensure_character(&mut self, name: &str) -> u64 {
        match self.character_id_by_name(name) {
            Some(id) => id,
            None => self.new_character(name),
        }
    }

    /// A quién pertenece un encabezado de personaje: a quien apunta su vínculo o, si todavía no lo tiene, al
    /// único personaje con ese nombre.
    fn cue_character(&self, p: &Paragraph) -> Option<u64> {
        if let Some(Link {
            target: EntityRef::Character(id),
            ..
        }) = p.links.first()
        {
            return Some(*id);
        }
        let name = clean_character_name(&p.text);
        let mut same = self
            .characters
            .iter()
            .filter(|c| clean_character_name(&c.name) == name);
        match (same.next(), same.next()) {
            (Some(c), None) => Some(c.id),
            _ => None,
        }
    }

    /// Cuántas veces habla el personaje (cantidad de encabezados de personaje que le pertenecen).
    pub fn character_cues(&self, id: u64) -> usize {
        self.paragraphs
            .iter()
            .filter(|p| p.kind == ParagraphKind::Character && self.cue_character(p) == Some(id))
            .count()
    }

    /// Elimina un personaje: su ficha y todas sus intervenciones (el encabezado de personaje y las
    /// acotaciones y diálogos que le siguen). Sus nombres dentro de acciones y diálogos quedan como texto.
    /// Devuelve cuántos párrafos del guion se borraron.
    pub fn remove_character(&mut self, id: u64) -> usize {
        let mut doomed: Vec<u64> = Vec::new();
        let mut inside = false;
        for p in &self.paragraphs {
            match p.kind {
                ParagraphKind::Character => inside = self.cue_character(p) == Some(id),
                ParagraphKind::Parenthetical | ParagraphKind::Dialogue => {}
                _ => inside = false,
            }
            if inside {
                doomed.push(p.id);
            }
        }
        // El guion nunca queda sin párrafos.
        if doomed.len() >= self.paragraphs.len() {
            doomed.pop();
        }
        self.characters.retain(|c| c.id != id);
        self.paragraphs.retain(|p| !doomed.contains(&p.id));
        self.comments.retain(|c| !doomed.contains(&c.paragraph));
        self.drop_links_to(EntityRef::Character(id));
        self.purge_breakdown_items();
        doomed.len()
    }

    /// Deja la ficha del personaje en blanco, conservando su identidad y su nombre.
    pub fn reset_character(&mut self, id: u64) {
        if let Some(c) = self.characters.iter_mut().find(|c| c.id == id) {
            *c = CharacterProfile {
                id: c.id,
                name: c.name.clone(),
                ..Default::default()
            };
        }
    }

    // ---------------------------------------------------------------- vínculos del texto con el proyecto

    pub fn entity_name(&self, target: EntityRef) -> Option<&str> {
        match target {
            EntityRef::Character(id) => self
                .characters
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.name.as_str()),
            EntityRef::Object(id) => self
                .objects
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.name.as_str()),
            EntityRef::Element(id) => self
                .elements
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.name.as_str()),
            EntityRef::World(id) => self
                .documents
                .iter()
                .find(|d| d.id == id && d.kind == DocKind::World)
                .map(|d| d.name.as_str()),
            EntityRef::Location(id) => self
                .location_profiles
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.name.as_str()),
        }
    }

    /// Lo que distingue a una cosa de otra con el mismo nombre: su descripción o, si no tiene, su rol.
    fn entity_detail(&self, target: EntityRef) -> String {
        let text = match target {
            EntityRef::Character(id) => self.character(id).map(|c| {
                if !c.one_liner.trim().is_empty() {
                    c.one_liner.clone()
                } else {
                    c.role.label().to_string()
                }
            }),
            EntityRef::Object(id) => self
                .objects
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.one_liner.clone()),
            EntityRef::Element(id) => self
                .elements
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.one_liner.clone()),
            EntityRef::World(id) => self
                .documents
                .iter()
                .find(|d| d.id == id)
                .map(|d| d.description.clone()),
            EntityRef::Location(id) => self
                .location_profiles
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.one_liner.clone()),
        };
        let text = text.unwrap_or_default();
        let text = text.trim();
        if text.chars().count() > 40 {
            format!("{}…", text.chars().take(40).collect::<String>())
        } else {
            text.to_string()
        }
    }

    /// El nombre para mostrar en listas: si otra cosa del mismo tipo se llama igual, se añade lo que las distingue.
    pub fn label_for(&self, target: EntityRef) -> String {
        let Some(name) = self.entity_name(target) else {
            return String::new();
        };
        let same_kind =
            |other: EntityRef| std::mem::discriminant(&other) == std::mem::discriminant(&target);
        let all: Vec<EntityRef> = self
            .characters
            .iter()
            .map(|c| EntityRef::Character(c.id))
            .chain(self.objects.iter().map(|o| EntityRef::Object(o.id)))
            .chain(self.elements.iter().map(|e| EntityRef::Element(e.id)))
            .chain(
                self.documents
                    .iter()
                    .filter(|d| d.kind == DocKind::World)
                    .map(|d| EntityRef::World(d.id)),
            )
            .chain(
                self.location_profiles
                    .iter()
                    .map(|l| EntityRef::Location(l.id)),
            )
            .collect();
        // Todos los del mismo tipo que se llaman igual, en orden (incluido el propio).
        let group: Vec<EntityRef> = all
            .iter()
            .copied()
            .filter(|t| {
                same_kind(*t)
                    && self
                        .entity_name(*t)
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .collect();
        if group.len() < 2 {
            return name.to_string();
        }
        let detail = self.entity_detail(target);
        let shared = group
            .iter()
            .filter(|t| **t != target && self.entity_detail(**t) == detail)
            .count()
            > 0;
        let position = group.iter().position(|t| *t == target).unwrap_or(0) + 1;
        match (detail.is_empty(), shared) {
            (true, _) => format!("{name} ({position})"),
            (false, true) => format!("{name} ({detail} · {position})"),
            (false, false) => format!("{name} ({detail})"),
        }
    }

    /// Quita todos los vínculos que apuntan a `target` (el texto queda como está).
    pub fn drop_links_to(&mut self, target: EntityRef) {
        for p in &mut self.paragraphs {
            p.links.retain(|l| l.target != target);
        }
        self.treatment_links.retain(|l| l.target != target);
        self.synopsis_links.retain(|l| l.target != target);
    }

    /// Nombres que se pueden vincular solos: los que tienen un único dueño entre personajes, objetos, elementos
    /// y mundos.
    fn unique_link_names(&self) -> Vec<(Vec<char>, EntityRef)> {
        let mut by_name: std::collections::HashMap<String, Vec<EntityRef>> =
            std::collections::HashMap::new();
        for m in self.mention_names() {
            if let Some(t) = m.target {
                by_name.entry(m.name.to_lowercase()).or_default().push(t);
            }
        }
        let mut out: Vec<(Vec<char>, EntityRef)> = by_name
            .into_iter()
            .filter(|(name, owners)| {
                owners.len() == 1 && name.chars().count() >= MIN_AUTO_LINK_CHARS
            })
            .map(|(name, owners)| (name.chars().collect(), owners[0]))
            .collect();
        out.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
        out
    }

    /// Vincula lo que haya sin vincular en un párrafo: el encabezado de personaje (creando la ficha si falta) y
    /// los nombres que coinciden exactamente con un solo personaje, objeto, elemento o mundo.
    pub fn auto_link_paragraph(&mut self, idx: usize) {
        let kind = self.paragraphs[idx].kind;
        match kind {
            ParagraphKind::Character => {
                if !self.paragraphs[idx].links.is_empty() {
                    return;
                }
                let text = self.paragraphs[idx].text.clone();
                let name = clean_character_name(&text);
                if name.is_empty() {
                    return;
                }
                let owners: Vec<u64> = self
                    .characters
                    .iter()
                    .filter(|c| clean_character_name(&c.name) == name)
                    .map(|c| c.id)
                    .collect();
                let id = match owners.len() {
                    0 => self.new_character(&name),
                    1 => owners[0],
                    _ => return, // dos con el mismo nombre: hay que elegir cuál
                };
                let lead = text.chars().take_while(|c| c.is_whitespace()).count();
                let name_end = match text.find('(') {
                    Some(b) => text[..b].trim_end().chars().count(),
                    None => text.trim_end().chars().count(),
                };
                if name_end > lead {
                    self.paragraphs[idx].links = vec![Link {
                        start: lead,
                        end: name_end,
                        target: EntityRef::Character(id),
                    }];
                }
            }
            ParagraphKind::SceneHeading => {
                if !self.paragraphs[idx].links.is_empty() {
                    return;
                }
                let text = self.paragraphs[idx].text.clone();
                let Some((start, end)) = heading_location_span(&text) else {
                    return;
                };
                let name: String = text.chars().skip(start).take(end - start).collect();
                let owners: Vec<u64> = self
                    .location_profiles
                    .iter()
                    .filter(|l| l.name.eq_ignore_ascii_case(name.trim()))
                    .map(|l| l.id)
                    .collect();
                let id = match owners.len() {
                    0 => self.new_location(&name),
                    1 => owners[0],
                    _ => return, // dos locaciones con el mismo nombre: hay que elegir cuál
                };
                self.paragraphs[idx].links = vec![Link {
                    start,
                    end,
                    target: EntityRef::Location(id),
                }];
            }
            ParagraphKind::Action | ParagraphKind::Dialogue => {
                let names = self.unique_link_names();
                let p = &mut self.paragraphs[idx];
                link_names(&names, &p.text, &mut p.links);
            }
            _ => {}
        }
    }

    /// Vincula los nombres de la sinopsis que coinciden con un solo personaje, locación, objeto, elemento o mundo.
    pub fn auto_link_synopsis(&mut self) {
        let names = self.unique_link_names();
        link_names(&names, &self.synopsis, &mut self.synopsis_links);
    }

    /// Vincula todo el guion. Primero los encabezados de personaje y de escena (que crean las fichas que falten) y
    /// después el texto, así una mención que aparece antes del primer encabezado de ese personaje también se vincula.
    /// Vincula los nombres del tratamiento que coinciden con un solo personaje, locación, objeto, elemento o mundo.
    pub fn auto_link_treatment(&mut self) {
        let names = self.unique_link_names();
        link_names(&names, &self.treatment, &mut self.treatment_links);
    }

    /// Vincula la locación de un encabezado escrito en el tratamiento (caracteres `start..end`), creando su ficha
    /// si falta, igual que en el guion.
    pub fn link_treatment_heading(&mut self, start: usize, end: usize) {
        let heading: String = self
            .treatment
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect();
        let Some((s, e)) = heading_location_span(&heading) else {
            return;
        };
        let name: String = heading.chars().skip(s).take(e - s).collect();
        let owners: Vec<u64> = self
            .location_profiles
            .iter()
            .filter(|l| l.name.eq_ignore_ascii_case(name.trim()))
            .map(|l| l.id)
            .collect();
        let id = match owners.len() {
            0 => self.new_location(&name),
            1 => owners[0],
            _ => return,
        };
        let link = Link {
            start: start + s,
            end: start + e,
            target: EntityRef::Location(id),
        };
        self.treatment_links
            .retain(|l| !(l.start < link.end && l.end > link.start));
        self.treatment_links.push(link);
        self.treatment_links.sort_by_key(|l| l.start);
    }

    pub fn auto_link_all(&mut self) {
        for i in 0..self.paragraphs.len() {
            if matches!(
                self.paragraphs[i].kind,
                ParagraphKind::Character | ParagraphKind::SceneHeading
            ) {
                self.auto_link_paragraph(i);
            }
        }
        for i in 0..self.paragraphs.len() {
            if matches!(
                self.paragraphs[i].kind,
                ParagraphKind::Action | ParagraphKind::Dialogue
            ) {
                self.auto_link_paragraph(i);
            }
        }
        self.auto_link_treatment();
    }

    /// Reescribe, en cada tramo vinculado, el nombre actual de lo que apunta. Se usa tras renombrar y tras
    /// deshacer o rehacer, así el guion siempre muestra los nombres vigentes. Los vínculos a cosas que ya no
    /// existen se sueltan (el texto queda).
    pub fn sync_links(&mut self) {
        let mut names: std::collections::HashMap<EntityRef, String> =
            std::collections::HashMap::new();
        for c in &self.characters {
            names.insert(EntityRef::Character(c.id), clean_character_name(&c.name));
        }
        for o in &self.objects {
            names.insert(EntityRef::Object(o.id), o.name.trim().to_string());
        }
        for e in &self.elements {
            names.insert(EntityRef::Element(e.id), e.name.trim().to_string());
        }
        for d in self.documents.iter().filter(|d| d.kind == DocKind::World) {
            names.insert(EntityRef::World(d.id), d.name.trim().to_string());
        }
        for l in &self.location_profiles {
            names.insert(EntityRef::Location(l.id), l.name.trim().to_string());
        }
        for p in &mut self.paragraphs {
            sync_text(p.kind, &mut p.text, &mut p.links, &names);
        }
        sync_text(
            ParagraphKind::Action,
            &mut self.treatment,
            &mut self.treatment_links,
            &names,
        );
        sync_text(
            ParagraphKind::Action,
            &mut self.synopsis,
            &mut self.synopsis_links,
            &names,
        );
    }

    // ---------------------------------------------------------------- rangos de texto entre párrafos

    /// Ordena dos posiciones `(id de párrafo, carácter)` según su lugar en el guion y devuelve `(índice, carácter)`.
    pub fn order_positions(
        &self,
        a: (u64, usize),
        b: (u64, usize),
    ) -> Option<((usize, usize), (usize, usize))> {
        let ia = self.paragraphs.iter().position(|p| p.id == a.0)?;
        let ib = self.paragraphs.iter().position(|p| p.id == b.0)?;
        let (ca, cb) = (
            a.1.min(self.paragraphs[ia].text.chars().count()),
            b.1.min(self.paragraphs[ib].text.chars().count()),
        );
        Some(if (ia, ca) <= (ib, cb) {
            ((ia, ca), (ib, cb))
        } else {
            ((ib, cb), (ia, ca))
        })
    }

    /// El texto entre dos posiciones, con un salto de línea entre párrafos.
    pub fn range_text(&self, a: (u64, usize), b: (u64, usize)) -> String {
        let Some(((i1, c1), (i2, c2))) = self.order_positions(a, b) else {
            return String::new();
        };
        let slice = |i: usize, from: usize, to: Option<usize>| -> String {
            let chars = self.paragraphs[i].text.chars().skip(from);
            match to {
                Some(to) => chars.take(to.saturating_sub(from)).collect(),
                None => chars.collect(),
            }
        };
        if i1 == i2 {
            return slice(i1, c1, Some(c2));
        }
        let mut parts = vec![slice(i1, c1, None)];
        parts.extend((i1 + 1..i2).map(|i| self.paragraphs[i].text.clone()));
        parts.push(slice(i2, 0, Some(c2)));
        parts.join("\n")
    }

    /// Borra el texto entre dos posiciones: lo que queda del primer párrafo se une con lo que queda del último.
    /// Los comentarios y marcas de los párrafos que desaparecen pasan al primero. Devuelve dónde quedó el cursor.
    pub fn delete_range(&mut self, a: (u64, usize), b: (u64, usize)) -> (u64, usize) {
        let Some(((i1, c1), (i2, c2))) = self.order_positions(a, b) else {
            return a;
        };
        let first_id = self.paragraphs[i1].id;
        if i1 == i2 {
            let para = &mut self.paragraphs[i1];
            let old = para.text.clone();
            let chars: Vec<char> = old.chars().collect();
            para.text = chars[..c1].iter().chain(chars[c2..].iter()).collect();
            adjust_links(&mut para.links, &old, &para.text);
            return (first_id, c1);
        }
        let last = self.paragraphs[i2].clone();
        let head: Vec<char> = self.paragraphs[i1].text.chars().take(c1).collect();
        let tail: Vec<char> = last.text.chars().skip(c2).collect();
        let (tail_links, _) = (split_links(&last.links, c2, 0).1, ());
        let removed: Vec<u64> = self.paragraphs[i1 + 1..=i2].iter().map(|p| p.id).collect();
        let first = &mut self.paragraphs[i1];
        first.links.retain(|l| l.end <= c1);
        first.links.extend(shifted_links(&tail_links, c1));
        first.text = head.into_iter().chain(tail).collect();
        let text = first.text.clone();
        validate_links(&mut first.links, &text);
        self.paragraphs.drain(i1 + 1..=i2);
        for c in self
            .comments
            .iter_mut()
            .filter(|c| removed.contains(&c.paragraph))
        {
            c.paragraph = first_id;
        }
        for t in self
            .breakdown_tags
            .iter_mut()
            .filter(|t| removed.contains(&t.paragraph))
        {
            t.paragraph = first_id;
        }
        (first_id, c1)
    }

    /// Pega texto de varias líneas respetando su estructura: cada línea no vacía pasa a ser un párrafo del tipo que
    /// le corresponde (encabezado, personaje, diálogo, acotación, transición o acción). Si el párrafo donde se pega
    /// está vacío, toma el tipo de la primera línea; si no, la primera línea se une a lo que ya había. Devuelve dónde
    /// quedó el cursor.
    pub fn paste_text_at(&mut self, pid: u64, at: usize, text: &str) -> (u64, usize) {
        let blocks = classify_pasted(text);
        if blocks.len() < 2 {
            let single = blocks.first().map_or(String::new(), |(_, t)| t.clone());
            return self.insert_text_at(pid, at, &single);
        }
        let Some(idx) = self.paragraphs.iter().position(|p| p.id == pid) else {
            return (pid, at);
        };
        if self.paragraphs[idx].text.is_empty() {
            self.paragraphs[idx].kind = blocks[0].0;
        }
        let joined = blocks
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let cursor = self.insert_text_at(pid, at, &joined);
        for (n, (kind, _)) in blocks.iter().enumerate().skip(1) {
            if let Some(para) = self.paragraphs.get_mut(idx + n) {
                para.kind = *kind;
            }
        }
        cursor
    }

    /// Inserta texto en una posición. Cada salto de línea abre un párrafo nuevo (de acción); lo que había después de
    /// la posición queda al final del último. Devuelve dónde quedó el cursor.
    pub fn insert_text_at(&mut self, pid: u64, at: usize, text: &str) -> (u64, usize) {
        let Some(idx) = self.paragraphs.iter().position(|p| p.id == pid) else {
            return (pid, at);
        };
        let at = at.min(self.paragraphs[idx].text.chars().count());
        let lines: Vec<&str> = text.split('\n').collect();
        let upper = |kind: ParagraphKind, s: &str| {
            if kind.is_uppercase() {
                s.to_uppercase()
            } else {
                s.to_string()
            }
        };

        let kind = self.paragraphs[idx].kind;
        let old = self.paragraphs[idx].text.clone();
        let chars: Vec<char> = old.chars().collect();
        let first = upper(kind, lines[0]);
        if lines.len() == 1 {
            let para = &mut self.paragraphs[idx];
            para.text = chars[..at].iter().collect::<String>()
                + &first
                + &chars[at..].iter().collect::<String>();
            adjust_links(&mut para.links, &old, &para.text);
            return (pid, at + first.chars().count());
        }
        // Varias líneas: el primer párrafo conserva lo anterior a la posición más la primera línea.
        let tail_text: String = chars[at..].iter().collect();
        let (head_links, tail_links) = split_links(&self.paragraphs[idx].links, at, 0);
        self.paragraphs[idx].text = chars[..at].iter().collect::<String>() + &first;
        self.paragraphs[idx].links = head_links;
        let mut at_index = idx;
        let mut cursor = (pid, at + first.chars().count());
        for (n, line) in lines.iter().enumerate().skip(1) {
            let last = n == lines.len() - 1;
            let shown = upper(ParagraphKind::Action, line);
            let body = if last {
                shown.clone() + &tail_text
            } else {
                shown.clone()
            };
            let mut para = self.make_paragraph(ParagraphKind::Action, &body);
            if last {
                para.links = shifted_links(&tail_links, shown.chars().count());
                cursor = (para.id, shown.chars().count());
            }
            at_index += 1;
            self.paragraphs.insert(at_index, para);
        }
        cursor
    }

    // ---------------------------------------------------------------- tratamiento y escaleta

    /// Lo que se escribe entre paréntesis la primera vez que un personaje aparece en el tratamiento: su edad y su
    /// descripción, tal como están en la ficha. Vacío si la ficha no tiene ninguna de las dos.
    pub fn character_intro(&self, id: u64) -> String {
        let Some(c) = self.character(id) else {
            return String::new();
        };
        let age = c.age.trim();
        let age = if !age.is_empty() && age.chars().all(|ch| ch.is_ascii_digit()) {
            format!("{age} años")
        } else {
            age.to_string()
        };
        let about = c.one_liner.trim();
        match (age.is_empty(), about.is_empty()) {
            (true, true) => String::new(),
            (false, true) => age,
            (true, false) => about.to_string(),
            (false, false) => format!("{age}, {about}"),
        }
    }

    /// ¿Ya aparece el personaje en el tratamiento?
    pub fn treatment_mentions(&self, id: u64) -> bool {
        self.treatment_links
            .iter()
            .any(|l| l.target == EntityRef::Character(id))
    }

    /// Lo escrito entre paréntesis justo después de la primera mención de cada personaje con la ficha en blanco
    /// (edad y descripción) pasa a su ficha. Solo completa lo que esté vacío. Devuelve si cambió alguna ficha.
    pub fn learn_character_intros(&mut self) -> bool {
        let chars: Vec<char> = self.treatment.chars().collect();
        let mut found: Vec<(u64, String)> = Vec::new();
        for l in &self.treatment_links {
            let EntityRef::Character(id) = l.target else {
                continue;
            };
            if found.iter().any(|(i, _)| *i == id) {
                continue; // solo cuenta la primera mención
            }
            // « (…)» pegado al nombre, en la misma línea.
            let mut at = l.end;
            while chars.get(at) == Some(&' ') {
                at += 1;
            }
            if chars.get(at) != Some(&'(') || at == l.end {
                continue;
            }
            if let Some(close) = chars[at + 1..].iter().position(|c| *c == ')' || *c == '\n') {
                if chars[at + 1 + close] == ')' {
                    found.push((
                        id,
                        chars[at + 1..at + 1 + close]
                            .iter()
                            .collect::<String>()
                            .trim()
                            .to_string(),
                    ));
                }
            }
        }
        let mut changed = false;
        for (id, content) in found {
            let Some(c) = self.characters.iter_mut().find(|c| c.id == id) else {
                continue;
            };
            if content.is_empty() {
                continue;
            }
            // «32 años, bibliotecaria»: la edad es lo que lleva números, antes de la primera coma.
            let (head, tail) = match content.split_once(',') {
                Some((h, t)) => (h.trim().to_string(), t.trim().to_string()),
                None => (content.clone(), String::new()),
            };
            let (age, about) =
                if head.chars().any(|ch| ch.is_ascii_digit()) && head.chars().count() <= 14 {
                    (head.trim_end_matches("años").trim().to_string(), tail)
                } else {
                    (String::new(), content.clone())
                };
            if c.age.trim().is_empty() && !age.is_empty() {
                c.age = age;
                changed = true;
            }
            if c.one_liner.trim().is_empty() && !about.is_empty() {
                c.one_liner = about;
                changed = true;
            }
        }
        changed
    }

    // ---------------------------------------------------------------- guion técnico

    /// Las columnas de la tabla: si todavía no hay, las de siempre.
    pub fn ensure_shot_columns(&mut self) {
        if self.shot_columns.is_empty() {
            for kind in [
                ColumnKind::Scene,
                ColumnKind::Number,
                ColumnKind::Size,
                ColumnKind::Movement,
                ColumnKind::Description,
                ColumnKind::Dialogue,
                ColumnKind::Sound,
                ColumnKind::Duration,
                ColumnKind::Camera,
                ColumnKind::Storyboard,
            ] {
                let id = self.next_id;
                self.next_id += 1;
                self.shot_columns.push(ShotColumn {
                    id,
                    kind,
                    name: kind.default_name().to_string(),
                    width: None,
                });
            }
        } else if !self
            .shot_columns
            .iter()
            .any(|c| c.kind == ColumnKind::Camera)
        {
            // Proyectos de antes del bloqueo: la columna de la cámara entra justo antes del storyboard.
            let id = self.next_id;
            self.next_id += 1;
            let at = self
                .shot_columns
                .iter()
                .position(|c| c.kind == ColumnKind::Storyboard)
                .unwrap_or(self.shot_columns.len());
            let kind = ColumnKind::Camera;
            self.shot_columns.insert(
                at,
                ShotColumn {
                    id,
                    kind,
                    name: kind.default_name().to_string(),
                    width: None,
                },
            );
        }
    }

    pub fn add_shot_column(&mut self, name: &str) -> u64 {
        self.ensure_shot_columns();
        let id = self.next_id;
        self.next_id += 1;
        self.shot_columns.push(ShotColumn {
            id,
            kind: ColumnKind::Custom,
            name: name.trim().to_string(),
            width: None,
        });
        id
    }

    /// Cuántos planos tienen algo escrito en esa columna.
    pub fn column_values(&self, column: u64) -> usize {
        self.shots
            .iter()
            .filter(|s| s.extra.get(&column).is_some_and(|v| !v.trim().is_empty()))
            .count()
    }

    /// Agrega una de las columnas predefinidas (lente, soporte…) antes del storyboard, si todavía no está.
    pub fn add_builtin_column(&mut self, kind: ColumnKind) -> Option<u64> {
        self.ensure_shot_columns();
        if !kind.is_optional() || self.shot_columns.iter().any(|c| c.kind == kind) {
            return None;
        }
        let id = self.alloc_id();
        let at = self
            .shot_columns
            .iter()
            .position(|c| c.kind == ColumnKind::Storyboard)
            .unwrap_or(self.shot_columns.len());
        self.shot_columns.insert(
            at,
            ShotColumn {
                id,
                kind,
                name: kind.default_name().to_string(),
                width: None,
            },
        );
        Some(id)
    }

    /// Quita una columna propia o una predefinida opcional, con lo escrito en ella. Las de siempre no se quitan.
    pub fn remove_shot_column(&mut self, id: u64) -> bool {
        if !self
            .shot_columns
            .iter()
            .any(|c| c.id == id && (c.kind == ColumnKind::Custom || c.kind.is_optional()))
        {
            return false;
        }
        self.shot_columns.retain(|c| c.id != id);
        for shot in &mut self.shots {
            shot.extra.remove(&id);
        }
        true
    }

    pub fn move_shot_column(&mut self, id: u64, delta: isize) -> bool {
        let Some(i) = self.shot_columns.iter().position(|c| c.id == id) else {
            return false;
        };
        let Some(j) = i
            .checked_add_signed(delta)
            .filter(|j| *j < self.shot_columns.len())
        else {
            return false;
        };
        self.shot_columns.swap(i, j);
        true
    }

    // ---------------------------------------------------------------- bloqueo de cámara

    /// Crea un bloqueo en una escena con un nombre («Bloqueo 1», «Bloqueo 2»…) y devuelve su id.
    pub fn add_blocking(&mut self, scene: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let n = self.blockings.iter().filter(|b| b.scene == scene).count() + 1;
        self.blockings.push(Blocking {
            id,
            scene,
            name: format!("Bloqueo {n}"),
            items: Vec::new(),
            duration: DEFAULT_BLOCK_SECONDS,
        });
        id
    }

    /// Quita un bloqueo; los planos que lo usaban quedan sin cámara.
    pub fn remove_blocking(&mut self, id: u64) {
        self.blockings.retain(|b| b.id != id);
        for shot in self.shots.iter_mut().filter(|s| s.blocking == Some(id)) {
            shot.blocking = None;
            shot.camera = None;
        }
    }

    /// Pone algo en el bloqueo, en el punto (`x`, `y`) en metros, con las medidas de siempre para su tipo. A las
    /// cámaras les da la siguiente letra libre. Devuelve su id.
    #[cfg(test)]
    pub fn add_block_item(
        &mut self,
        blocking: u64,
        kind: BlockKind,
        x: f32,
        y: f32,
    ) -> Option<u64> {
        self.add_block_item_with(blocking, kind, x, y, |_| {})
    }

    /// Como `add_block_item`, y `edit` ajusta la cosa nueva (aparato, giro, medidas…) antes de ponerla: así entra al
    /// bloqueo, y al historial, de una sola vez.
    pub fn add_block_item_with(
        &mut self,
        blocking: u64,
        kind: BlockKind,
        x: f32,
        y: f32,
        edit: impl FnOnce(&mut BlockItem),
    ) -> Option<u64> {
        let id = self.next_id;
        let b = self.blockings.iter().find(|b| b.id == blocking)?;
        let mut item = BlockItem::template(kind, id, x, y, b.next_label(kind), b.next_track(kind));
        if kind == BlockKind::Camera {
            item.body = self.camera_body_default;
        }
        edit(&mut item);
        self.next_id += 1;
        self.blockings
            .iter_mut()
            .find(|b| b.id == blocking)?
            .items
            .push(item);
        Some(id)
    }

    /// Pone copias de `items` en el bloqueo, corridas `dx`, `dy` metros (también sus posiciones guardadas y el operador de un
    /// boom). Cada copia tiene su id; las cámaras, luces y micrófonos toman el nombre o la letra siguiente libre, y las
    /// referencias entre lo copiado (la pared de una puerta, la persona de un corbatero) se rehacen hacia las copias. Devuelve
    /// los ids nuevos.
    pub fn paste_block_items(
        &mut self,
        blocking: u64,
        items: &[BlockItem],
        dx: f32,
        dy: f32,
    ) -> Vec<u64> {
        if !self.blockings.iter().any(|b| b.id == blocking) {
            return Vec::new();
        }
        let map: std::collections::HashMap<u64, u64> = items
            .iter()
            .map(|i| {
                let id = self.next_id;
                self.next_id += 1;
                (i.id, id)
            })
            .collect();
        let Some(b) = self.blockings.iter_mut().find(|b| b.id == blocking) else {
            return Vec::new();
        };
        let mut added = Vec::new();
        for original in items {
            let mut item = original.clone();
            item.id = map[&original.id];
            item.x += dx;
            item.y += dy;
            for key in &mut item.keys {
                key.x += dx;
                key.y += dy;
            }
            // Lo que apuntaba a algo copiado apunta a su copia; lo que apuntaba a algo de afuera sigue, si existe acá.
            let exists = |id: u64, b: &Blocking| b.items.iter().any(|i| i.id == id);
            item.wall = item
                .wall
                .and_then(|w| map.get(&w).copied().or_else(|| exists(w, b).then_some(w)));
            if let Some(mic) = item.mic.as_mut() {
                mic.attached_to = mic
                    .attached_to
                    .and_then(|a| map.get(&a).copied().or_else(|| exists(a, b).then_some(a)));
                if let Some(op) = mic.operator.as_mut() {
                    *op = (op.0 + dx, op.1 + dy);
                }
            }
            if matches!(
                item.kind,
                BlockKind::Camera | BlockKind::Light | BlockKind::Modifier | BlockKind::Mic
            ) {
                item.label = b.next_label(item.kind);
            }
            if item.kind == BlockKind::Mic {
                item.track = b.next_track(item.kind);
            }
            added.push(item.id);
            b.items.push(item);
        }
        added
    }

    /// Quita algo del bloqueo; los planos que lo filmaban pierden la cámara.
    pub fn remove_block_item(&mut self, blocking: u64, item: u64) {
        if let Some(b) = self.blockings.iter_mut().find(|b| b.id == blocking) {
            b.items.retain(|i| i.id != item);
        }
        for shot in self
            .shots
            .iter_mut()
            .filter(|s| s.blocking == Some(blocking) && s.camera == Some(item))
        {
            shot.camera = None;
        }
    }

    /// Las cámaras de una escena, para vincularlas a un plano: (bloqueo, cámara, cómo se la nombra).
    pub fn scene_cameras(&self, scene: u64) -> Vec<(u64, u64, String)> {
        self.blockings
            .iter()
            .filter(|b| b.scene == scene)
            .flat_map(|b| {
                b.items
                    .iter()
                    .filter(|i| i.kind == BlockKind::Camera)
                    .map(move |i| (b.id, i.id, format!("{} · {}", i.label, b.name)))
            })
            .collect()
    }

    /// Las cámaras vinculadas a algún plano pierden el vínculo si su bloqueo o la cámara ya no existen.
    pub fn prune_camera_links(&mut self) {
        let valid: Vec<(u64, u64)> = self
            .blockings
            .iter()
            .flat_map(|b| {
                b.items
                    .iter()
                    .filter(|i| i.kind == BlockKind::Camera)
                    .map(move |i| (b.id, i.id))
            })
            .collect();
        for shot in &mut self.shots {
            if let (Some(b), Some(c)) = (shot.blocking, shot.camera) {
                if !valid.contains(&(b, c)) {
                    shot.blocking = None;
                    shot.camera = None;
                }
            }
        }
    }

    /// Las escenas del guion, en orden: el id y el texto de cada encabezado.
    pub fn script_scenes(&self) -> Vec<(u64, String)> {
        self.paragraphs
            .iter()
            .filter(|p| p.kind == ParagraphKind::SceneHeading)
            .map(|p| (p.id, p.text.clone()))
            .collect()
    }

    /// El número de cada escena del guion, en orden: el de su posición, o el fijo si la numeración está bloqueada.
    pub fn scene_labels(&self) -> Vec<(u64, String)> {
        self.script_scenes()
            .iter()
            .enumerate()
            .map(|(n, (id, _))| {
                let fixed = self
                    .scenes
                    .get(id)
                    .map(|m| m.number.clone())
                    .filter(|s| self.scene_numbers_locked && !s.is_empty());
                (*id, fixed.unwrap_or_else(|| (n + 1).to_string()))
            })
            .collect()
    }

    /// Fija la numeración actual: 1, 2, 3… y de ahí en más no cambia.
    pub fn lock_scene_numbers(&mut self) {
        for meta in self.scenes.values_mut() {
            meta.number.clear();
            meta.provisional = false;
        }
        for (n, (id, _)) in self.script_scenes().into_iter().enumerate() {
            self.scenes.entry(id).or_default().number = (n + 1).to_string();
        }
        self.scene_numbers_locked = true;
    }

    /// Suelta la numeración: vuelve a ser la posición de cada escena.
    pub fn unlock_scene_numbers(&mut self) {
        self.scene_numbers_locked = false;
        for meta in self.scenes.values_mut() {
            meta.number.clear();
            meta.provisional = false;
        }
    }

    /// Cuántas escenas tienen todavía una letra provisoria.
    pub fn provisional_scene_count(&self) -> usize {
        if !self.scene_numbers_locked {
            return 0;
        }
        self.script_scenes()
            .iter()
            .filter(|(id, _)| self.scenes.get(id).is_some_and(|m| m.provisional))
            .count()
    }

    /// Las letras provisorias pasan a ser fijas: desde ahora no cambian aunque se agreguen escenas entre medio.
    pub fn fix_scene_numbers(&mut self) {
        self.sync_scene_numbers();
        for meta in self.scenes.values_mut() {
            meta.provisional = false;
        }
    }

    /// Con la numeración bloqueada, da número a las escenas que todavía no lo tienen. Las que se agregaron después de
    /// bloquear son provisorias: toman la letra que les corresponde por su lugar (la que sigue a la escena anterior:
    /// 12 → 12A → 12B; antes de la primera, A1, B1…) y se recalculan cada vez, así los números siempre se leen de corrido.
    /// Una escena fija borrada queda omitida y su número no se reutiliza.
    pub fn sync_scene_numbers(&mut self) {
        if !self.scene_numbers_locked {
            return;
        }
        let ids: Vec<u64> = self.script_scenes().into_iter().map(|(id, _)| id).collect();
        // Una escena provisoria que se borró no deja rastro (ni sus datos); las nuevas arrancan provisorias.
        self.scenes
            .retain(|id, m| ids.contains(id) || !m.provisional);
        for id in &ids {
            let meta = self.scenes.entry(*id).or_default();
            if meta.number.is_empty() {
                meta.provisional = true;
            }
        }
        // Los números de las escenas fijas, vivas u omitidas: ninguno se repite.
        let taken: std::collections::HashSet<String> = self
            .scenes
            .values()
            .filter(|m| !m.provisional)
            .map(|m| m.number.clone())
            .collect();
        let fixed_at = |s: &Self, id: u64| {
            s.scenes
                .get(&id)
                .filter(|m| !m.provisional)
                .map(|m| m.number.clone())
        };
        let mut i = 0;
        let mut previous: Option<String> = None;
        while i < ids.len() {
            if let Some(n) = fixed_at(self, ids[i]) {
                previous = Some(n);
                i += 1;
                continue;
            }
            // Una tanda de escenas provisorias entre dos fijas.
            let end = (i..ids.len())
                .find(|&j| fixed_at(self, ids[j]).is_some())
                .unwrap_or(ids.len());
            let next = ids.get(end).and_then(|id| fixed_at(self, *id));
            let labels = labels_between(previous.as_deref(), next.as_deref(), end - i, &taken);
            for (id, label) in ids[i..end].iter().zip(labels) {
                self.scenes.entry(*id).or_default().number = label;
            }
            i = end;
        }
    }

    /// Reserva un id nuevo, único en todo el proyecto.
    pub(crate) fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Crea un lugar de rodaje y devuelve su id.
    pub fn add_shoot_location(&mut self, name: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let name = if name.trim().is_empty() {
            format!("Locación de rodaje {}", self.shoot_locations.len() + 1)
        } else {
            name.trim().to_string()
        };
        self.shoot_locations.push(ShootLocation {
            id,
            name,
            ..Default::default()
        });
        id
    }

    /// Quita un lugar de rodaje; las escenas y locaciones del guion que lo tenían quedan sin lugar.
    pub fn remove_shoot_location(&mut self, id: u64) {
        self.shoot_locations.retain(|l| l.id != id);
        self.prune_production_links();
    }

    /// Agrega un contacto y devuelve su id.
    /// Agrega una fila vacía a la lista de un rol y devuelve su id.
    pub fn add_role_entry(&mut self, role: Role, scene: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let entry = RoleEntry {
            id,
            scene,
            kind: role.kinds()[0].to_string(),
            status: role.statuses()[0].to_string(),
            ..Default::default()
        };
        self.role_entries_mut(role).push(entry);
        id
    }

    /// Quita una fila de un rol (el Desglose se actualiza solo con `sync_equipment`).
    pub fn remove_role_entry(&mut self, role: Role, id: u64) {
        self.role_entries_mut(role).retain(|e| e.id != id);
    }

    /// Qué lista y qué fila le corresponde a algo de la planta: (rol, tipo de fila, nombre). Solo lo que se tiene que conseguir o
    /// tener a mano: muebles, luces, modificadores, cámaras y micrófonos.
    fn plan_item_entry(item: &BlockItem) -> Option<(Role, &'static str, String)> {
        match item.kind {
            BlockKind::Object => item
                .shape
                .furniture_name()
                .map(|n| (Role::Art, "Escenografía", n.to_string())),
            BlockKind::Light => item.light.as_ref().map(|l| {
                (Role::PhotoGear, "Luz", format!("Luz {}", l.fixture.label()))
            }),
            BlockKind::Modifier => item.modifier.as_ref().map(|m| {
                (Role::PhotoGear, "Modificador", m.kind.label().to_string())
            }),
            BlockKind::Camera => Some((Role::PhotoGear, "Cámara", format!("Cámara {}", item.label))),
            BlockKind::Mic => item.mic.as_ref().map(|m| {
                let kind = match m.kind {
                    MicKind::Boom => "Boom",
                    MicKind::Lavalier => "Inalámbrico",
                    _ => "Micrófono",
                };
                (Role::SoundGear, kind, format!("Micrófono {}", m.kind.label()))
            }),
            _ => None,
        }
    }

    /// Lo que hay en las plantas pasa a las listas de Dirección de arte, sonido y fotografía: cada cosa sin equipo registrado
    /// es una fila de su escena (se actualiza y se quita sola con la planta), y las que tienen equipo registrado no suman filas
    /// (usan la del equipo). Devuelve si cambió algo.
    fn sync_plan_to_roles(&mut self) -> bool {
        let mut wanted: BTreeMap<String, (Role, &'static str, String, u64, u32)> = BTreeMap::new();
        for b in &self.blockings {
            let mut per: BTreeMap<String, (Role, &'static str, String, u32)> = BTreeMap::new();
            for item in b.items.iter().filter(|i| !i.hidden) {
                if item.equipment.is_some_and(|e| {
                    [Role::SoundGear, Role::PhotoGear, Role::Art]
                        .iter()
                        .any(|r| self.role_entries(*r).iter().any(|x| x.id == e))
                }) {
                    continue;
                }
                if let Some((role, kind, name)) = Self::plan_item_entry(item) {
                    let key = format!("{role:?}|{}|{name}", b.scene);
                    per.entry(key).or_insert((role, kind, name, 0)).3 += 1;
                }
            }
            for (key, (role, kind, name, n)) in per {
                let e = wanted.entry(key).or_insert((role, kind, name, b.scene, 0));
                e.4 = e.4.max(n);
            }
        }
        let mut changed = false;
        for role in [Role::Art, Role::SoundGear, Role::PhotoGear] {
            let before = self.role_entries(role).len();
            self.role_entries_mut(role).retain(|e| {
                e.plan_key.as_ref().is_none_or(|k| wanted.contains_key(k))
            });
            changed |= before != self.role_entries(role).len();
        }
        for (key, (role, kind, name, scene, quantity)) in wanted {
            let existing = self
                .role_entries(role)
                .iter()
                .position(|e| e.plan_key.as_deref() == Some(key.as_str()));
            match existing {
                Some(i) => {
                    let e = &mut self.role_entries_mut(role)[i];
                    if e.quantity != quantity {
                        e.quantity = quantity;
                        changed = true;
                    }
                }
                None => {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.role_entries_mut(role).push(RoleEntry {
                        id,
                        scene: Some(scene),
                        kind: kind.to_string(),
                        title: name,
                        status: role.statuses()[0].to_string(),
                        quantity,
                        plan_key: Some(key),
                        ..Default::default()
                    });
                    changed = true;
                }
            }
        }
        changed
    }

    /// Qué categoría del Desglose le toca a una fila de un rol: siempre una de las que el Desglose ya trae («Maquillaje y
    /// peluquería», «Arte y decorado»…), para no crear categorías repetidas con otro nombre.
    fn role_category(role: Role, kind: &str) -> &'static str {
        match role {
            Role::Art => match kind {
                "Utilería" => "Utilería",
                "Vestuario" => "Vestuario",
                "Maquillaje" => "Maquillaje y peluquería",
                "Vehículo" => "Vehículos",
                _ => "Arte y decorado",
            },
            Role::PhotoGear => "Equipo técnico",
            Role::SoundGear | Role::Sound => "Sonido y música",
        }
    }

    /// Refleja en el Desglose las listas de Dirección de arte, sonido y fotografía: un elemento por nombre (y categoría), con
    /// las escenas de todas sus filas y de las cosas de la planta que usan ese equipo. Una fila sin título se llama como su
    /// tipo. Lo que se quita de las listas sale del Desglose. Devuelve si cambió algo.
    fn sync_roles_to_breakdown(&mut self) -> bool {
        // Las escenas donde cada equipo registrado se usa en una planta.
        let mut used: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for b in &self.blockings {
            for item in b.items.iter().filter(|i| !i.hidden) {
                if let Some(e) = item.equipment {
                    let scenes = used.entry(e).or_default();
                    if !scenes.contains(&b.scene) {
                        scenes.push(b.scene);
                    }
                }
            }
        }
        // Un elemento del Desglose por clave: nombre y categoría, o el objeto clave al que se vinculó la fila.
        struct Want {
            category: Option<u64>,
            category_name: &'static str,
            name: String,
            entity: Option<EntityRef>,
            scenes: Vec<u64>,
            manual: u32,
            auto: u32,
            acquired: bool,
        }
        let mut wanted: BTreeMap<String, Want> = BTreeMap::new();
        for role in [Role::Art, Role::SoundGear, Role::PhotoGear] {
            for e in self.role_entries(role) {
                let name = if e.title.trim().is_empty() {
                    e.kind.clone()
                } else {
                    e.title.trim().to_string()
                };
                // El vínculo de la fila: un objeto clave del proyecto o una categoría propia del Desglose.
                let object = match e.link {
                    Some(RoleLink::Object(id)) if self.objects.iter().any(|o| o.id == id) => Some(id),
                    _ => None,
                };
                let category = match e.link {
                    Some(RoleLink::Category(id)) if self.breakdown_categories.iter().any(|c| c.id == id) => {
                        Some(id)
                    }
                    _ => None,
                };
                let category_name = Self::role_category(role, &e.kind);
                let key = match (object, category) {
                    (Some(id), _) => format!("obj|{id}"),
                    (None, Some(id)) => format!("#{id}|{name}"),
                    _ => format!("{category_name}|{name}"),
                };
                let want = wanted.entry(key).or_insert_with(|| Want {
                    category,
                    category_name,
                    name,
                    entity: object.map(EntityRef::Object),
                    scenes: Vec::new(),
                    manual: 0,
                    auto: 0,
                    acquired: false,
                });
                let scenes: Vec<u64> = e
                    .scene
                    .into_iter()
                    .chain(used.get(&e.id).into_iter().flatten().copied())
                    .collect();
                for sc in scenes {
                    if !want.scenes.contains(&sc) {
                        want.scenes.push(sc);
                    }
                }
                if e.plan_key.is_some() {
                    want.auto = want.auto.max(e.quantity.max(1));
                } else {
                    want.manual += e.quantity.max(1);
                }
                want.acquired =
                    want.acquired || matches!(e.status.as_str(), "Conseguido" | "Listo en set");
            }
        }
        let mut changed = false;
        let stale: Vec<String> = self
            .role_breakdown
            .keys()
            .filter(|k| !wanted.contains_key(*k))
            .cloned()
            .collect();
        for key in stale {
            if let Some(id) = self.role_breakdown.remove(&key)
                && let Some(idx) = self.breakdown_items.iter().position(|i| i.id == id)
            {
                self.remove_breakdown_item(idx);
            }
            changed = true;
        }
        for (key, want) in wanted {
            let category = match (want.entity, want.category) {
                // Un objeto clave va a la categoría de los objetos clave.
                (Some(_), _) => self
                    .breakdown_categories
                    .iter()
                    .find(|c| c.source == CategorySource::Objects)
                    .map(|c| c.id),
                (None, Some(id)) => Some(id),
                (None, None) => Some(match self
                    .breakdown_categories
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(want.category_name))
                {
                    Some(c) => c.id,
                    None => {
                        changed = true;
                        self.add_breakdown_category(
                            want.category_name,
                            match want.category_name {
                                "Utilería" => [0xFF, 0xCC, 0x80],
                                "Vestuario" => [0xF0, 0x62, 0x92],
                                "Maquillaje y peluquería" => [0xE5, 0x73, 0x73],
                                "Vehículos" => [0x4D, 0xB6, 0xAC],
                                "Arte y decorado" => [0xFF, 0xD5, 0x4F],
                                "Sonido y música" => [0x79, 0x86, 0xCB],
                                _ => [0x90, 0xA4, 0xAE],
                            },
                        )
                    }
                }),
            };
            let Some(category) = category else {
                continue;
            };
            let quantity = (want.manual + want.auto).max(1);
            let status = if want.acquired { Acquisition::Have } else { Acquisition::Pending };
            let idx = self
                .role_breakdown
                .get(&key)
                .and_then(|id| self.breakdown_items.iter().position(|i| i.id == *id));
            match idx {
                Some(i) => {
                    let item = &mut self.breakdown_items[i];
                    if item.category != category
                        || item.quantity != quantity
                        || item.extra_scenes != want.scenes
                        || item.status != status
                    {
                        item.category = category;
                        item.quantity = quantity;
                        item.extra_scenes = want.scenes;
                        item.status = status;
                        changed = true;
                    }
                }
                // Un objeto clave que ya tiene su elemento en el Desglose lo usa: solo se le suman las escenas.
                None if want.entity.is_some()
                    && self.breakdown_items.iter().any(|i| i.entity == want.entity) =>
                {
                    if let Some(item) = self.breakdown_items.iter_mut().find(|i| i.entity == want.entity) {
                        for sc in want.scenes {
                            if !item.extra_scenes.contains(&sc) {
                                item.extra_scenes.push(sc);
                                changed = true;
                            }
                        }
                    }
                }
                None => {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.breakdown_items.push(BreakdownItem {
                        id,
                        category,
                        name: want.name,
                        status,
                        quantity,
                        notes: String::new(),
                        entity: want.entity,
                        cost_cents: None,
                        unit_cost: false,
                        responsible: None,
                        extra_scenes: want.scenes,
                    });
                    self.role_breakdown.insert(key, id);
                    changed = true;
                }
            }
        }
        // Las categorías sueltas que se crearon antes con otros nombres («Maquillaje», «Sonido»…) y quedaron vacías se quitan.
        let before = self.breakdown_categories.len();
        let used: Vec<u64> = self.breakdown_items.iter().map(|i| i.category).collect();
        self.breakdown_categories.retain(|c| {
            let legacy = ["Escenografía", "Iluminación", "Fotografía", "Sonido", "Maquillaje"]
                .iter()
                .any(|n| c.name == *n);
            !(legacy && c.source == CategorySource::Custom && !used.contains(&c.id))
        });
        changed |= before != self.breakdown_categories.len();
        changed
    }

    /// Mantiene al día lo que se comparte entre plantas, listas de los roles y Desglose. Devuelve si cambió algo.
    pub fn sync_equipment(&mut self) -> bool {
        let a = self.sync_plan_to_roles();
        let b = self.sync_roles_to_breakdown();
        a || b
    }

    pub fn role_entries(&self, role: Role) -> &[RoleEntry] {
        match role {
            Role::Sound => &self.sound_cues,
            Role::SoundGear => &self.sound_gear,
            Role::PhotoGear => &self.photo_gear,
            Role::Art => &self.art_items,
        }
    }

    pub fn role_entries_mut(&mut self, role: Role) -> &mut Vec<RoleEntry> {
        match role {
            Role::Sound => &mut self.sound_cues,
            Role::SoundGear => &mut self.sound_gear,
            Role::PhotoGear => &mut self.photo_gear,
            Role::Art => &mut self.art_items,
        }
    }

    pub fn add_contact(&mut self, name: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.contacts.push(Contact {
            id,
            name: name.trim().to_string(),
            ..Default::default()
        });
        id
    }

    /// Quita un contacto; lo que lo nombraba (locaciones, personajes) queda sin él.
    pub fn remove_contact(&mut self, id: u64) {
        self.contacts.retain(|c| c.id != id);
        self.prune_production_links();
    }

    /// Suelta los vínculos a lugares de rodaje y contactos que ya no existen.
    pub fn prune_production_links(&mut self) {
        let places: Vec<u64> = self.shoot_locations.iter().map(|l| l.id).collect();
        let people: Vec<u64> = self.contacts.iter().map(|c| c.id).collect();
        for meta in self.scenes.values_mut() {
            meta.shoot_location = meta.shoot_location.filter(|id| places.contains(id));
        }
        for profile in &mut self.location_profiles {
            profile.shoot_location = profile.shoot_location.filter(|id| places.contains(id));
        }
        for place in &mut self.shoot_locations {
            place.contact = place.contact.filter(|id| people.contains(id));
        }
        for c in &mut self.characters {
            c.actor = c.actor.filter(|id| people.contains(id));
        }
        for item in &mut self.breakdown_items {
            item.responsible = item.responsible.filter(|id| people.contains(id));
        }
    }

    /// Dónde se filma una escena: el lugar que se le dio a ella o, si no, el de su locación del guion.
    pub fn scene_shoot_location(&self, scene: u64) -> Option<u64> {
        if let Some(id) = self.scenes.get(&scene).and_then(|m| m.shoot_location) {
            return Some(id);
        }
        let heading = self
            .paragraphs
            .iter()
            .find(|p| p.id == scene && p.kind == ParagraphKind::SceneHeading)?;
        let profile = self.heading_location_id(heading)?;
        self.location_profiles
            .iter()
            .find(|l| l.id == profile)
            .and_then(|l| l.shoot_location)
    }

    /// Las escenas que se filman en un lugar: (número, encabezado, octavos).
    pub fn shoot_location_scenes(&self, place: u64) -> Vec<(String, String, u16)> {
        let labels: std::collections::HashMap<u64, String> =
            self.scene_labels().into_iter().collect();
        self.script_scenes()
            .into_iter()
            .filter(|(id, _)| self.scene_shoot_location(*id) == Some(place))
            .map(|(id, heading)| {
                (
                    labels.get(&id).cloned().unwrap_or_default(),
                    heading,
                    self.scene_eighths(id),
                )
            })
            .collect()
    }

    /// Las líneas de página de una escena (de su encabezado al siguiente).
    fn scene_lines(&self, id: u64) -> f32 {
        let mut inside = false;
        let mut lines = 0.0;
        for p in &self.paragraphs {
            if p.kind == ParagraphKind::SceneHeading {
                if inside {
                    break;
                }
                inside = p.id == id;
            }
            if inside {
                lines += page_lines(p);
            }
        }
        lines
    }

    /// Octavos de página que ocupa una escena: una página son 8/8, se redondea hacia arriba al octavo y el mínimo es
    /// 1/8. Si se corrigió a mano, vale lo corregido.
    pub fn scene_eighths(&self, id: u64) -> u16 {
        if let Some(n) = self.scenes.get(&id).and_then(|m| m.eighths) {
            return n;
        }
        ((self.scene_lines(id) / LINES_PER_PAGE * 8.0).ceil() as u16).max(1)
    }

    /// Los personajes de una escena: los que hablan y los que se nombran con vínculo, en orden de aparición, con su
    /// número de elenco y quién los interpreta.
    pub fn scene_cast_detail(&self, id: u64) -> Vec<(Option<u32>, String, Option<String>)> {
        self.scene_cast_ids(id)
            .iter()
            .filter_map(|c| self.character(*c))
            .map(|c| {
                let actor = c
                    .actor
                    .and_then(|a| self.contacts.iter().find(|x| x.id == a))
                    .map(|x| x.name.clone());
                (c.cast_number, clean_character_name(&c.name), actor)
            })
            .collect()
    }

    /// Los ids de los personajes de una escena, en orden de aparición.
    pub fn scene_cast_ids(&self, id: u64) -> Vec<u64> {
        let mut inside = false;
        let mut ids: Vec<u64> = Vec::new();
        for p in &self.paragraphs {
            if p.kind == ParagraphKind::SceneHeading {
                if inside {
                    break;
                }
                inside = p.id == id;
                continue;
            }
            if !inside {
                continue;
            }
            let mut add = |c: u64| {
                if !ids.contains(&c) {
                    ids.push(c);
                }
            };
            if p.kind == ParagraphKind::Character {
                if let Some(c) = self.cue_character(p) {
                    add(c);
                }
            }
            for link in &p.links {
                if let EntityRef::Character(c) = link.target {
                    add(c);
                }
            }
        }
        ids
    }

    /// Los personajes de una escena como se muestran en una lista: «3. ANA».
    pub fn scene_cast(&self, id: u64) -> Vec<String> {
        self.scene_cast_detail(id)
            .into_iter()
            .map(|(number, name, _)| match number {
                Some(n) => format!("{n}. {name}"),
                None => name,
            })
            .collect()
    }

    /// Una sugerencia de resumen: la primera oración de la primera acción de la escena.
    pub fn scene_summary_hint(&self, id: u64) -> String {
        let mut inside = false;
        for p in &self.paragraphs {
            if p.kind == ParagraphKind::SceneHeading {
                if inside {
                    break;
                }
                inside = p.id == id;
            } else if inside && p.kind == ParagraphKind::Action && !p.text.trim().is_empty() {
                let text = p.text.trim();
                let end = text.find(['.', '!', '?']).map_or(text.len(), |i| i + 1);
                return text[..end].trim().to_string();
            }
        }
        String::new()
    }

    /// El día de la historia que sugiere el encabezado: el de la escena anterior si dice CONTINUO, MÁS TARDE…
    pub fn story_day_hint(&self, id: u64) -> Option<u32> {
        let scenes = self.script_scenes();
        let at = scenes.iter().position(|(s, _)| *s == id)?;
        let (_, _, time) = heading_parts(&scenes[at].1);
        let continues = ["CONTINUO", "MÁS TARDE", "MOMENTOS DESPUÉS", "SEGUIDO"]
            .iter()
            .any(|w| time.contains(w));
        if !continues || at == 0 {
            return None;
        }
        self.scenes.get(&scenes[at - 1].0).and_then(|m| m.story_day)
    }

    /// Los números de las escenas que ya no están en el guion («12 OMITIDA»), en orden.
    #[cfg(test)]
    pub fn omitted_scene_numbers(&self) -> Vec<String> {
        if !self.scene_numbers_locked {
            return Vec::new();
        }
        let alive: std::collections::HashSet<u64> =
            self.script_scenes().into_iter().map(|(id, _)| id).collect();
        let mut out: Vec<String> = self
            .scenes
            .iter()
            .filter(|(id, m)| !alive.contains(id) && !m.number.is_empty())
            .map(|(_, m)| m.number.clone())
            .collect();
        out.sort_by_key(|n| scene_number_key(n));
        out
    }

    /// Añade un plano a una escena, después del último que ya tenga. Con `after` va justo debajo de ese plano.
    pub fn add_shot(&mut self, scene: u64, after: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let at = match after.and_then(|a| self.shots.iter().position(|s| s.id == a)) {
            Some(i) => i + 1,
            None => self
                .shots
                .iter()
                .rposition(|s| s.scene == scene)
                .map_or(self.shots.len(), |i| i + 1),
        };
        self.shots.insert(
            at,
            Shot {
                id,
                scene,
                ..Default::default()
            },
        );
        id
    }

    pub fn remove_shot(&mut self, id: u64) {
        self.shots.retain(|s| s.id != id);
    }

    /// Sube o baja un plano entre los de su misma escena.
    pub fn move_shot(&mut self, id: u64, delta: isize) -> bool {
        let Some(i) = self.shots.iter().position(|s| s.id == id) else {
            return false;
        };
        let scene = self.shots[i].scene;
        let same: Vec<usize> = (0..self.shots.len())
            .filter(|&k| self.shots[k].scene == scene)
            .collect();
        let Some(pos) = same.iter().position(|&k| k == i) else {
            return false;
        };
        let Some(&j) = pos.checked_add_signed(delta).and_then(|q| same.get(q)) else {
            return false;
        };
        self.shots.swap(i, j);
        true
    }

    /// Número de cada plano. Es continuo en todo el guion (escenas en el orden del guion y, al final, los planos de
    /// escenas que ya no existen) salvo que `shots_restart` pida volver a 1 en cada escena.
    pub fn shot_numbers(&self) -> std::collections::HashMap<u64, usize> {
        let mut out = std::collections::HashMap::new();
        let scenes: Vec<u64> = self.script_scenes().into_iter().map(|(id, _)| id).collect();
        let mut count = 0;
        let number =
            |out: &mut std::collections::HashMap<u64, usize>, shot: &Shot, count: &mut usize| {
                *count += 1;
                out.insert(shot.id, *count);
            };
        for scene in &scenes {
            if self.shots_restart {
                count = 0;
            }
            for shot in self.shots.iter().filter(|s| s.scene == *scene) {
                number(&mut out, shot, &mut count);
            }
        }
        if self.shots_restart {
            count = 0;
        }
        for shot in self.shots.iter().filter(|s| !scenes.contains(&s.scene)) {
            number(&mut out, shot, &mut count);
        }
        out
    }

    /// Crea una escena de la escaleta y devuelve su id.
    pub fn add_outline_scene(&mut self, heading: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.outline.push(OutlineScene {
            id,
            heading: heading.trim().to_uppercase(),
            imported: None,
        });
        id
    }

    /// Dónde está cada escena de la escaleta en el tratamiento (en el orden de `outline`): el comienzo (en caracteres) de la
    /// línea de su encabezado. Si varias escenas tienen el mismo encabezado (INT. LIVING - NOCHE dos veces), cada una toma
    /// la aparición que le corresponde, en orden. Sin esa línea la escena no existe.
    pub fn outline_positions(&self) -> Vec<Option<usize>> {
        self.outline_positions_in(&self.treatment)
    }

    /// Como `outline_positions`, sobre otro texto (el de antes de una edición).
    fn outline_positions_in(&self, text: &str) -> Vec<Option<usize>> {
        let mut lines: std::collections::HashMap<&str, Vec<usize>> = Default::default();
        let mut start = 0;
        for line in text.split('\n') {
            lines.entry(line).or_default().push(start);
            start += line.chars().count() + 1;
        }
        let mut taken: std::collections::HashMap<&str, usize> = Default::default();
        self.outline
            .iter()
            .map(|scene| {
                if scene.heading.is_empty() {
                    return None;
                }
                let n = taken.entry(scene.heading.as_str()).or_default();
                let found = lines.get(scene.heading.as_str())?.get(*n).copied();
                *n += 1;
                found
            })
            .collect()
    }

    /// Crea la escena de la escaleta cuyo encabezado empieza en `at` (en caracteres) y la ubica entre las que tienen el
    /// mismo encabezado según cuántas aparecen antes en el texto. Devuelve su id.
    pub fn add_outline_scene_at(&mut self, heading: &str, at: usize) -> u64 {
        let id = self.add_outline_scene(heading);
        let heading = self.outline[self.outline.len() - 1].heading.clone();
        let mut before = 0;
        let mut start = 0;
        for line in self.treatment.split('\n') {
            if start >= at {
                break;
            }
            if line == heading {
                before += 1;
            }
            start += line.chars().count() + 1;
        }
        let new = self.outline.pop().expect("recién creada");
        let mut seen = 0;
        let mut index = self.outline.len();
        for (i, o) in self.outline.iter().enumerate() {
            if o.heading == heading {
                if seen == before {
                    index = i;
                    break;
                }
                seen += 1;
                index = i + 1;
            }
        }
        self.outline.insert(index, new);
        id
    }

    /// La escaleta sigue al tratamiento: las escenas van en el orden en que están escritas y la que pierde su
    /// encabezado deja de existir (el texto no se toca).
    pub fn sort_outline_by_text(&mut self) {
        let positions = self.outline_positions();
        let mut keyed: Vec<(usize, OutlineScene)> = std::mem::take(&mut self.outline)
            .into_iter()
            .zip(positions)
            .filter_map(|(o, pos)| pos.map(|pos| (pos, o)))
            .collect();
        keyed.sort_by_key(|(pos, _)| *pos);
        self.outline = keyed.into_iter().map(|(_, o)| o).collect();
    }

    /// Lo que abarca cada escena (ya ordenadas) en el tratamiento: desde su encabezado hasta el siguiente o el final.
    pub fn outline_blocks(&self) -> Vec<(usize, usize)> {
        let total = self.treatment.chars().count();
        let starts: Vec<usize> = self
            .outline_positions()
            .into_iter()
            .map(|pos| pos.unwrap_or(total))
            .collect();
        (0..starts.len())
            .map(|k| (starts[k], starts.get(k + 1).copied().unwrap_or(total)))
            .collect()
    }

    /// Si el texto cambió dentro de la línea de un encabezado de escena, la escena sigue a ese encabezado con el
    /// nombre nuevo. Se llama con el texto de antes y el de después de cada edición.
    pub fn follow_heading_edits(&mut self, old: &str, new: &str) {
        let (o, n): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
        let prefix = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
        let suffix = o[prefix..]
            .iter()
            .rev()
            .zip(n[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let (old_end, new_end) = (o.len() - suffix, n.len() - suffix);
        if new_end == prefix && old_end == prefix {
            return;
        }
        let mut line_start = 0;
        for line in old.split('\n') {
            let line_end = line_start + line.chars().count();
            if prefix >= line_start && old_end <= line_end && !n[prefix..new_end].contains(&'\n') {
                let positions = self.outline_positions_in(old);
                let at = positions.iter().position(|pos| *pos == Some(line_start));
                if let Some(scene) = at.and_then(|k| self.outline.get_mut(k)) {
                    let tail = n[line_start..]
                        .iter()
                        .take_while(|c| **c != '\n')
                        .collect::<String>();
                    if !tail.trim().is_empty() {
                        scene.heading = tail;
                    }
                }
                return;
            }
            line_start = line_end + 1;
        }
    }

    /// Sube o baja una escena de la escaleta cambiando de lugar su texto en el tratamiento: el bloque que va desde su
    /// encabezado hasta el siguiente se intercambia con el de la escena vecina. Devuelve si hubo cambio.
    pub fn move_outline_scene(&mut self, idx: usize, delta: isize) -> bool {
        self.sort_outline_by_text();
        let Some(other) = idx
            .checked_add_signed(delta)
            .filter(|j| *j < self.outline.len())
        else {
            return false;
        };
        let (lo, hi) = (idx.min(other), idx.max(other));
        let positions = self.outline_positions();
        let heading_at = |_: &Self, k: usize| positions[k];
        let (Some(a), Some(b)) = (heading_at(self, lo), heading_at(self, hi)) else {
            return false;
        };
        let total = self.treatment.chars().count();
        let end = (hi + 1..self.outline.len())
            .find_map(|k| heading_at(self, k))
            .filter(|e| *e > b)
            .unwrap_or(total);
        if !(a < b && b < end) {
            return false;
        }
        let chars: Vec<char> = self.treatment.chars().collect();
        let first: String = chars[a..b].iter().collect();
        let second: String = chars[b..end].iter().collect();
        // Si el segundo bloque cierra el texto sin salto de línea, el salto pasa a separarlos.
        let (second, first) = if end == total && !second.ends_with('\n') {
            (
                format!("{second}\n"),
                first.trim_end_matches('\n').to_string(),
            )
        } else {
            (second, first)
        };
        let second_len = second.chars().count();
        let remap = |l: &Link| -> Link {
            if l.start >= a && l.end <= b {
                Link {
                    start: l.start + second_len,
                    end: l.end + second_len,
                    ..*l
                }
            } else if l.start >= b && l.end <= end {
                Link {
                    start: l.start - b + a,
                    end: l.end - b + a,
                    ..*l
                }
            } else {
                *l
            }
        };
        self.treatment_links = self.treatment_links.iter().map(remap).collect();
        self.treatment = chars[..a].iter().collect::<String>()
            + &second
            + &first
            + &chars[end..].iter().collect::<String>();
        let text = self.treatment.clone();
        validate_links(&mut self.treatment_links, &text);
        self.treatment_links.sort_by_key(|l| l.start);
        self.sort_outline_by_text();
        true
    }

    /// Importa al guion los encabezados de las escenas de la escaleta que todavía no estén, sin texto, al final del
    /// guion y en el orden de la escaleta. Si el guion está en blanco (un único encabezado vacío), el primero ocupa
    /// su lugar. Devuelve cuántos encabezados se añadieron.
    pub fn import_outline_into_script(&mut self) -> usize {
        self.sort_outline_by_text();
        let mut added = 0;
        let blank = self.paragraphs.len() == 1
            && self.paragraphs[0].kind == ParagraphKind::SceneHeading
            && self.paragraphs[0].text.trim().is_empty();
        for i in 0..self.outline.len() {
            let already = self.outline[i]
                .imported
                .is_some_and(|pid| self.paragraphs.iter().any(|p| p.id == pid));
            if already || self.outline[i].heading.trim().is_empty() {
                continue;
            }
            let heading = self.outline[i].heading.clone();
            let pid = if blank && added == 0 {
                self.paragraphs[0].text = heading;
                self.paragraphs[0].links.clear();
                self.paragraphs[0].id
            } else {
                let para = self.make_paragraph(ParagraphKind::SceneHeading, &heading);
                let pid = para.id;
                self.paragraphs.push(para);
                pid
            };
            self.outline[i].imported = Some(pid);
            if let Some(idx) = self.paragraphs.iter().position(|p| p.id == pid) {
                self.auto_link_paragraph(idx);
            }
            added += 1;
        }
        added
    }

    /// Id del encabezado de la escena a la que pertenece el párrafo `pid`.
    pub fn scene_of(&self, pid: u64) -> Option<u64> {
        let idx = self.paragraphs.iter().position(|p| p.id == pid)?;
        self.paragraphs[..=idx]
            .iter()
            .rev()
            .find(|p| p.kind == ParagraphKind::SceneHeading)
            .map(|p| p.id)
    }

    /// Nombres de las fichas de personaje, sin repetir (mayúsculas).
    pub fn known_characters(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.characters {
            let n = clean_character_name(&c.name);
            if !n.is_empty() && !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }

    /// Las locaciones del proyecto con la cantidad de escenas de cada una. Una escena cuenta para la locación a la que
    /// apunta su encabezado o, si todavía no está vinculado, para la única que se llama como él.
    /// Los ids de las escenas cuyo encabezado nombra esta locación del guion.
    pub fn scenes_in_location(&self, location: u64) -> Vec<u64> {
        self.paragraphs
            .iter()
            .filter(|p| {
                p.kind == ParagraphKind::SceneHeading
                    && self.heading_location_id(p) == Some(location)
            })
            .map(|p| p.id)
            .collect()
    }

    /// Cuántas escenas de la escaleta todavía no están en el guion.
    pub fn pending_outline_scenes(&self) -> usize {
        self.outline
            .iter()
            .filter(|o| {
                !o.heading.trim().is_empty()
                    && !o
                        .imported
                        .is_some_and(|pid| self.paragraphs.iter().any(|q| q.id == pid))
            })
            .count()
    }

    pub fn locations(&self) -> Vec<LocationInfo> {
        let mut counts: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for p in self
            .paragraphs
            .iter()
            .filter(|p| p.kind == ParagraphKind::SceneHeading)
        {
            if let Some(id) = self.heading_location_id(p) {
                *counts.entry(id).or_default() += 1;
            }
        }
        self.location_profiles
            .iter()
            .map(|l| LocationInfo {
                id: l.id,
                name: l.name.clone(),
                scenes: counts.get(&l.id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// A qué locación pertenece un encabezado de escena: su vínculo o, sin él, la única con ese nombre.
    fn heading_location_id(&self, p: &Paragraph) -> Option<u64> {
        if let Some(Link {
            target: EntityRef::Location(id),
            ..
        }) = p.links.first()
        {
            return Some(*id);
        }
        let name = heading_location(&p.text);
        let mut same = self
            .location_profiles
            .iter()
            .filter(|l| l.name.eq_ignore_ascii_case(&name));
        match (same.next(), same.next()) {
            (Some(l), None) => Some(l.id),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn location(&self, id: u64) -> Option<&LocationProfile> {
        self.location_profiles.iter().find(|l| l.id == id)
    }

    pub fn location_mut(&mut self, id: u64) -> Option<&mut LocationProfile> {
        self.location_profiles.iter_mut().find(|l| l.id == id)
    }

    pub fn location_id_by_name(&self, name: &str) -> Option<u64> {
        self.location_profiles
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(name.trim()))
            .map(|l| l.id)
    }

    /// Crea una ficha de locación y devuelve su id.
    pub fn new_location(&mut self, name: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.location_profiles.push(LocationProfile {
            id,
            name: name.trim().to_uppercase(),
            ..Default::default()
        });
        id
    }

    /// La locación con ese nombre; si no hay ninguna, la crea.
    pub fn ensure_location(&mut self, name: &str) -> u64 {
        match self.location_id_by_name(name) {
            Some(id) => id,
            None => self.new_location(name),
        }
    }

    /// Añade una locación. El nombre por defecto se numera para poder distinguirlas, pero después se puede repetir.
    pub fn add_location(&mut self, base: &str) -> u64 {
        let mut name = base.to_uppercase();
        let mut n = 2;
        while self.location_profiles.iter().any(|l| l.name == name) {
            name = format!("{} {n}", base.to_uppercase());
            n += 1;
        }
        self.new_location(&name)
    }

    /// Elimina una locación; los encabezados que la nombraban conservan su texto.
    pub fn remove_location(&mut self, id: u64) {
        self.location_profiles.retain(|l| l.id != id);
        self.drop_links_to(EntityRef::Location(id));
        self.purge_breakdown_items();
    }

    /// Escenas (número y encabezado) que transcurren en la locación `id`.
    pub fn scenes_at(&self, id: u64) -> Vec<(usize, String)> {
        let mut number = 0;
        let mut out = Vec::new();
        for p in self
            .paragraphs
            .iter()
            .filter(|p| p.kind == ParagraphKind::SceneHeading)
        {
            number += 1;
            if self.heading_location_id(p) == Some(id) {
                out.push((number, p.text.clone()));
            }
        }
        out
    }

    /// El objeto con ese nombre; si no hay ninguno, lo crea.
    pub fn ensure_object(&mut self, name: &str) -> u64 {
        if let Some(id) = self.object_id_by_name(name) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.objects.push(ObjectItem {
            id,
            name: name.trim().to_string(),
            ..Default::default()
        });
        id
    }

    /// Elimina un objeto clave; sus nombres en el guion quedan como texto.
    pub fn remove_object(&mut self, id: u64) {
        self.objects.retain(|o| o.id != id);
        self.drop_links_to(EntityRef::Object(id));
        self.purge_breakdown_items();
    }

    /// Añade un objeto. El nombre por defecto se numera para poder distinguirlos, pero después se puede repetir.
    /// Devuelve su id.
    pub fn add_object(&mut self, base: &str) -> u64 {
        let mut name = base.to_string();
        let mut n = 2;
        while self
            .objects
            .iter()
            .any(|o| o.name.eq_ignore_ascii_case(&name))
        {
            name = format!("{base} {n}");
            n += 1;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.objects.push(ObjectItem {
            id,
            name,
            ..Default::default()
        });
        self.auto_link_all();
        id
    }

    /// Crea una categoría de elementos y devuelve su id.
    pub fn add_element_category(&mut self, name: &str, icon: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.element_categories.push(ElementCategory {
            id,
            name: name.trim().to_string(),
            icon: icon.to_string(),
            template: None,
            uppercase: false,
        });
        id
    }

    /// Borra una categoría con todos sus elementos.
    pub fn remove_element_category(&mut self, id: u64) {
        let doomed: Vec<u64> = self
            .elements
            .iter()
            .filter(|e| e.category == id)
            .map(|e| e.id)
            .collect();
        for e in doomed {
            self.drop_links_to(EntityRef::Element(e));
        }
        self.elements.retain(|e| e.category != id);
        self.element_categories.retain(|c| c.id != id);
    }

    /// Plantilla editable de la categoría (la crea a partir de la de por defecto si hacía falta).
    pub fn template_mut(&mut self, category: u64) -> Option<&mut Vec<TemplateSection>> {
        let cat = self
            .element_categories
            .iter_mut()
            .find(|c| c.id == category)?;
        if cat.template.is_none() {
            cat.template = Some(crate::profile::owned(crate::profile::ELEMENT));
        }
        cat.template.as_mut()
    }

    pub fn add_template_section(&mut self, category: u64, title: &str) {
        if let Some(t) = self.template_mut(category) {
            t.push(TemplateSection {
                title: title.trim().to_string(),
                fields: Vec::new(),
            });
        }
    }

    pub fn add_template_field(&mut self, category: u64, section: usize, label: &str) {
        let key = format!("f{}", self.next_id);
        self.next_id += 1;
        if let Some(sec) = self.template_mut(category).and_then(|t| t.get_mut(section)) {
            sec.fields.push(TemplateField {
                key,
                label: label.trim().to_string(),
                rows: 2,
            });
        }
    }

    /// Cuántos elementos de la categoría tienen contenido en el campo `field` de la sección (o en toda la
    /// sección si `field` es `None`).
    pub fn template_usage(&self, category: u64, section: usize, field: Option<usize>) -> usize {
        let Some(sec) = self
            .element_categories
            .iter()
            .find(|c| c.id == category)
            .map(|c| c.sections())
            .and_then(|t| t.get(section).cloned())
        else {
            return 0;
        };
        let keys: Vec<&str> = match field {
            Some(f) => sec
                .fields
                .get(f)
                .map(|f| f.key.as_str())
                .into_iter()
                .collect(),
            None => sec.fields.iter().map(|f| f.key.as_str()).collect(),
        };
        self.elements
            .iter()
            .filter(|e| {
                e.category == category
                    && keys
                        .iter()
                        .any(|k| e.fields.get(*k).is_some_and(|v| !v.trim().is_empty()))
            })
            .count()
    }

    /// Quita un campo (o una sección entera) de la ficha, con su contenido en todos los elementos.
    pub fn remove_template_part(&mut self, category: u64, section: usize, field: Option<usize>) {
        let keys: Vec<String> = {
            let Some(t) = self.template_mut(category) else {
                return;
            };
            let Some(sec) = t.get_mut(section) else {
                return;
            };
            match field {
                Some(f) if f < sec.fields.len() => vec![sec.fields.remove(f).key],
                Some(_) => return,
                None => t
                    .remove(section)
                    .fields
                    .into_iter()
                    .map(|f| f.key)
                    .collect(),
            }
        };
        for e in self.elements.iter_mut().filter(|e| e.category == category) {
            for k in &keys {
                e.fields.remove(k);
            }
        }
    }

    /// Vuelve a la ficha por defecto. Lo escrito en campos que ya no existan queda guardado pero sin mostrarse.
    pub fn reset_template(&mut self, category: u64) {
        if let Some(c) = self
            .element_categories
            .iter_mut()
            .find(|c| c.id == category)
        {
            c.template = None;
        }
    }

    /// Crea un elemento en la categoría con un nombre que no se repita dentro de ella.
    pub fn add_element(&mut self, category: u64, base: &str) -> Option<u64> {
        if !self.element_categories.iter().any(|c| c.id == category) {
            return None;
        }
        let mut name = base.trim().to_string();
        let mut n = 2;
        while self
            .elements
            .iter()
            .any(|e| e.category == category && e.name.eq_ignore_ascii_case(&name))
        {
            name = format!("{} {n}", base.trim());
            n += 1;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.elements.push(Element {
            id,
            category,
            name,
            ..Default::default()
        });
        Some(id)
    }

    /// Nombres que el editor sugiere mientras se escribe: personajes, objetos, elementos, mundos y locaciones.
    /// Dos cosas con el mismo nombre aparecen las dos, cada una con su vínculo.
    pub fn mention_names(&self) -> Vec<Mention> {
        let mut out: Vec<Mention> = Vec::new();
        let mut push = |name: &str, character: bool, target: EntityRef, project: &Project| {
            let name = name.trim();
            if !name.is_empty() {
                out.push(Mention {
                    name: name.to_string(),
                    character,
                    target: Some(target),
                    detail: project.entity_detail(target),
                });
            }
        };
        for c in &self.characters {
            push(
                &clean_character_name(&c.name),
                true,
                EntityRef::Character(c.id),
                self,
            );
        }
        for o in &self.objects {
            push(&o.name, false, EntityRef::Object(o.id), self);
        }
        for e in &self.elements {
            push(&e.name, false, EntityRef::Element(e.id), self);
        }
        for d in self.documents.iter().filter(|d| d.kind == DocKind::World) {
            push(&d.name, false, EntityRef::World(d.id), self);
        }
        for l in &self.location_profiles {
            push(&l.name, false, EntityRef::Location(l.id), self);
        }
        out
    }

    // ---------------------------------------------------------------- desglose

    /// Crea las categorías iniciales del desglose si todavía no hay ninguna.
    pub fn ensure_breakdown_defaults(&mut self) {
        if !self.breakdown_categories.is_empty() {
            self.migrate_breakdown();
            return;
        }
        self.breakdown_version = BREAKDOWN_VERSION;
        let defaults: [(&str, [u8; 3], CategorySource); 14] = [
            ("Elenco", [0xB3, 0x9D, 0xDB], CategorySource::Characters),
            ("Locaciones", [0x81, 0xC7, 0x84], CategorySource::Locations),
            ("Objetos clave", [0xFF, 0xB7, 0x4D], CategorySource::Objects),
            ("Utilería", [0xFF, 0xCC, 0x80], CategorySource::Custom),
            ("Figuración", [0x4F, 0xC3, 0xF7], CategorySource::Custom),
            ("Vestuario", [0xF0, 0x62, 0x92], CategorySource::Custom),
            (
                "Maquillaje y peluquería",
                [0xE5, 0x73, 0x73],
                CategorySource::Custom,
            ),
            ("Vehículos", [0x4D, 0xB6, 0xAC], CategorySource::Custom),
            ("Animales", [0xAE, 0xD5, 0x81], CategorySource::Custom),
            ("Especialistas", [0xFF, 0x8A, 0x65], CategorySource::Custom),
            (
                "Efectos especiales",
                [0xBA, 0x68, 0xC8],
                CategorySource::Custom,
            ),
            (
                "Sonido y música",
                [0x79, 0x86, 0xCB],
                CategorySource::Custom,
            ),
            (
                "Arte y decorado",
                [0xFF, 0xD5, 0x4F],
                CategorySource::Custom,
            ),
            ("Equipo técnico", [0x90, 0xA4, 0xAE], CategorySource::Custom),
        ];
        for (name, color, source) in defaults {
            let id = self.next_id;
            self.next_id += 1;
            self.breakdown_categories.push(BreakdownCategory {
                id,
                name: name.to_string(),
                color,
                source,
            });
        }
    }

    /// Proyectos de la primera versión del desglose: «Objetos» pasa a llamarse «Objetos clave» y se añade
    /// «Utilería» para los objetos corrientes (solo una vez, para no volver a crearla si se la borra).
    fn migrate_breakdown(&mut self) {
        if self.breakdown_version >= BREAKDOWN_VERSION {
            return;
        }
        self.breakdown_version = BREAKDOWN_VERSION;
        let Some(pos) = self
            .breakdown_categories
            .iter()
            .position(|c| c.source == CategorySource::Objects)
        else {
            return;
        };
        if self.breakdown_categories[pos].name == "Objetos" {
            self.breakdown_categories[pos].name = "Objetos clave".to_string();
        }
        if !self
            .breakdown_categories
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case("Utilería"))
        {
            let id = self.next_id;
            self.next_id += 1;
            self.breakdown_categories.insert(
                pos + 1,
                BreakdownCategory {
                    id,
                    name: "Utilería".to_string(),
                    color: [0xFF, 0xCC, 0x80],
                    source: CategorySource::Custom,
                },
            );
        }
    }

    pub fn add_breakdown_category(&mut self, name: &str, color: [u8; 3]) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.breakdown_categories.push(BreakdownCategory {
            id,
            name: name.trim().to_string(),
            color,
            source: CategorySource::Custom,
        });
        id
    }

    /// Borra una categoría propia con sus elementos y marcas. Las automáticas no se pueden borrar.
    pub fn remove_breakdown_category(&mut self, id: u64) -> bool {
        let removable = self
            .breakdown_categories
            .iter()
            .any(|c| c.id == id && c.source == CategorySource::Custom);
        if !removable {
            return false;
        }
        let items: Vec<u64> = self
            .breakdown_items
            .iter()
            .filter(|i| i.category == id)
            .map(|i| i.id)
            .collect();
        self.breakdown_tags.retain(|t| !items.contains(&t.item));
        self.breakdown_items.retain(|i| i.category != id);
        self.breakdown_categories.retain(|c| c.id != id);
        true
    }

    pub fn find_breakdown_item(&self, category: u64, name: &str) -> Option<usize> {
        let name = name.trim();
        self.breakdown_items.iter().position(|i| {
            i.category == category && i.entity.is_none() && i.name.eq_ignore_ascii_case(name)
        })
    }

    /// El elemento del desglose de un personaje, locación u objeto (lo crea si falta).
    pub fn breakdown_item_for_entity(&mut self, category: u64, entity: EntityRef) -> Option<usize> {
        if let Some(i) = self
            .breakdown_items
            .iter()
            .position(|i| i.entity == Some(entity))
        {
            return Some(i);
        }
        let name = self.entity_name(entity)?.to_string();
        let id = self.next_id;
        self.next_id += 1;
        self.breakdown_items.push(BreakdownItem {
            id,
            category,
            name,
            status: Acquisition::Pending,
            quantity: 0,
            notes: String::new(),
            entity: Some(entity),
            cost_cents: None,
            responsible: None,
            unit_cost: false,
            extra_scenes: Vec::new(),
        });
        Some(self.breakdown_items.len() - 1)
    }

    /// El elemento guardado de una fila del desglose, creándolo si hace falta.
    pub fn breakdown_item_for_row(&mut self, row: &BreakdownRow) -> Option<usize> {
        match row.entity {
            Some(entity) => self.breakdown_item_for_entity(row.category, entity),
            None => self.breakdown_item_for(row.category, &row.name),
        }
    }

    /// Devuelve el elemento del desglose con ese nombre (lo crea si falta). En las categorías automáticas se busca el
    /// personaje, locación u objeto con ese nombre (o se crea) y el elemento queda ligado a él por id.
    pub fn breakdown_item_for(&mut self, category: u64, name: &str) -> Option<usize> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return None;
        }
        let source = self
            .breakdown_categories
            .iter()
            .find(|c| c.id == category)?
            .source;
        let entity = match source {
            CategorySource::Characters => Some(EntityRef::Character(self.ensure_character(&name))),
            CategorySource::Locations => Some(EntityRef::Location(self.ensure_location(&name))),
            CategorySource::Objects => Some(EntityRef::Object(self.ensure_object(&name))),
            CategorySource::Custom => None,
        };
        if let Some(entity) = entity {
            return self.breakdown_item_for_entity(category, entity);
        }
        if let Some(i) = self.find_breakdown_item(category, &name) {
            return Some(i);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.breakdown_items.push(BreakdownItem {
            id,
            category,
            name,
            status: Acquisition::Pending,
            quantity: 0,
            notes: String::new(),
            entity: None,
            cost_cents: None,
            responsible: None,
            unit_cost: false,
            extra_scenes: Vec::new(),
        });
        Some(self.breakdown_items.len() - 1)
    }

    /// Quita los elementos del desglose que apuntaban a algo que ya no existe, con sus marcas.
    pub fn purge_breakdown_items(&mut self) {
        let gone: Vec<u64> = self
            .breakdown_items
            .iter()
            .filter(|i| i.entity.is_some_and(|e| self.entity_name(e).is_none()))
            .map(|i| i.id)
            .collect();
        if gone.is_empty() {
            return;
        }
        self.breakdown_tags.retain(|t| !gone.contains(&t.item));
        self.breakdown_items.retain(|i| !gone.contains(&i.id));
    }

    /// Marca `quote` dentro del párrafo `paragraph` como `name` de la categoría. Con `all_mentions` marca todas las
    /// apariciones del texto en el guion. Devuelve cuántas marcas nuevas se crearon.
    pub fn add_breakdown_tag(
        &mut self,
        paragraph: u64,
        quote: &str,
        category: u64,
        name: &str,
        all_mentions: bool,
    ) -> usize {
        let quote = quote.trim();
        if quote.is_empty() {
            return 0;
        }
        let Some(idx) = self.breakdown_item_for(category, name) else {
            return 0;
        };
        let item = self.breakdown_items[idx].id;
        let targets: Vec<u64> = if all_mentions {
            let needle = quote.to_lowercase();
            self.paragraphs
                .iter()
                .filter(|p| {
                    p.kind != ParagraphKind::SceneHeading && p.text.to_lowercase().contains(&needle)
                })
                .map(|p| p.id)
                .collect()
        } else {
            vec![paragraph]
        };
        let mut created = 0;
        for pid in targets {
            // En el párrafo original se usa el texto tal cual; en los demás, el que aparece escrito allí.
            let text = self
                .paragraphs
                .iter()
                .find(|p| p.id == pid)
                .map(|p| p.text.clone())
                .unwrap_or_default();
            let found = if pid == paragraph && text.contains(quote) {
                Some(quote.to_string())
            } else {
                find_ignore_case(&text, quote)
            };
            let Some(found) = found else { continue };
            if self
                .breakdown_tags
                .iter()
                .any(|t| t.paragraph == pid && t.item == item && t.quote == found)
            {
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.breakdown_tags.push(BreakdownTag {
                id,
                paragraph: pid,
                quote: found,
                item,
                quantity: None,
            });
            created += 1;
        }
        created
    }

    /// Qué escena (id de su encabezado) contiene cada párrafo.
    fn paragraph_scenes(&self) -> std::collections::HashMap<u64, u64> {
        let mut out = std::collections::HashMap::new();
        let mut current = None;
        for p in &self.paragraphs {
            if p.kind == ParagraphKind::SceneHeading {
                current = Some(p.id);
            }
            if let Some(scene) = current {
                out.insert(p.id, scene);
            }
        }
        out
    }

    /// Cuántos hay de un elemento en una escena, si se indicó en alguna de sus marcas.
    pub fn scene_quantity(&self, item: u64, scene: u64) -> Option<u32> {
        let scenes = self.paragraph_scenes();
        let marks: Vec<u32> = self
            .breakdown_tags
            .iter()
            .filter(|t| t.item == item && scenes.get(&t.paragraph) == Some(&scene))
            .filter_map(|t| t.quantity)
            .collect();
        (!marks.is_empty()).then(|| marks.iter().sum())
    }

    /// Indica cuántos hay de un elemento en una escena. Se guarda en la primera marca del elemento en esa escena; un
    /// elemento que aparece solo porque es un personaje, una locación o un objeto del proyecto no tiene marcas.
    /// Devuelve si se pudo guardar.
    pub fn set_scene_quantity(&mut self, item: u64, scene: u64, quantity: Option<u32>) -> bool {
        let scenes = self.paragraph_scenes();
        let mut found = false;
        for tag in self
            .breakdown_tags
            .iter_mut()
            .filter(|t| t.item == item && scenes.get(&t.paragraph) == Some(&scene))
        {
            tag.quantity = if found { None } else { quantity };
            found = true;
        }
        found
    }

    /// Lo que lleva la hoja de desglose de una escena: cada elemento que aparece en ella, con su categoría.
    pub fn scene_sheet_items(&self, scene: u64) -> Vec<SheetItem> {
        let Some(position) = self.script_scenes().iter().position(|(id, _)| *id == scene) else {
            return Vec::new();
        };
        self.breakdown_rows(false)
            .into_iter()
            .filter(|r| r.scenes.contains(&(position + 1)))
            .map(|r| {
                let item = r.item.map(|i| &self.breakdown_items[i]);
                SheetItem {
                    category: r.category,
                    name: r.name.clone(),
                    item_id: item.map(|i| i.id),
                    scene_quantity: item.and_then(|i| self.scene_quantity(i.id, scene)),
                    quantity: item.map_or(0, |i| i.quantity),
                    status: item.map_or(Acquisition::Pending, |i| i.status),
                    notes: item.map(|i| i.notes.clone()).unwrap_or_default(),
                    auto: r.auto,
                }
            })
            .collect()
    }

    /// Lista de compras y alquileres: los elementos guardados con alguno de esos estados, de las categorías que se
    /// consiguen (los personajes y las locaciones no se compran), en el orden de las categorías.
    pub fn shopping_list(&self, statuses: &[Acquisition]) -> Vec<ShoppingRow> {
        let labels: Vec<String> = self.scene_labels().into_iter().map(|(_, n)| n).collect();
        let rows = self.breakdown_rows(false);
        let mut out = Vec::new();
        for cat in self.breakdown_categories.iter().filter(|c| {
            !matches!(
                c.source,
                CategorySource::Characters | CategorySource::Locations
            )
        }) {
            for row in rows.iter().filter(|r| r.category == cat.id) {
                let Some(item) = row.item.map(|i| &self.breakdown_items[i]) else {
                    continue;
                };
                if !statuses.contains(&item.status) {
                    continue;
                }
                out.push(ShoppingRow {
                    item: item.id,
                    category: cat.id,
                    name: row.name.clone(),
                    quantity: item.quantity,
                    status: item.status,
                    scenes: row
                        .scenes
                        .iter()
                        .filter_map(|n| labels.get(n - 1).cloned())
                        .collect(),
                    cost_cents: item.total_cents(),
                    responsible: item.responsible,
                    notes: item.notes.clone(),
                });
            }
        }
        out
    }

    /// Lo que trae el lugar de rodaje asignado a una locación del guion (si tiene uno): el desglose lo muestra mientras la
    /// fila no tenga un dato propio.
    pub fn location_deal(&self, entity: EntityRef) -> Option<LocationDeal> {
        let EntityRef::Location(id) = entity else {
            return None;
        };
        let place = self
            .location_profiles
            .iter()
            .find(|l| l.id == id)?
            .shoot_location
            .and_then(|s| self.shoot_locations.iter().find(|l| l.id == s))?;
        Some(LocationDeal {
            status: (place.permit == PermitStatus::Approved).then_some(Acquisition::Have),
            cost_cents: place.permit_cost_cents,
            responsible: place.contact,
        })
    }

    /// Lo que cuesta lo que pide el guion, por categoría: (categoría, centavos, cuántos elementos tienen costo). Es una
    /// suma de lo cargado, no un presupuesto de producción.
    pub fn budget_by_category(&self) -> Vec<(u64, i64, usize)> {
        let rows = self.breakdown_rows(false);
        self.breakdown_categories
            .iter()
            .map(|c| {
                let mut costs: Vec<i64> = self
                    .breakdown_items
                    .iter()
                    .filter(|i| i.category == c.id)
                    .filter_map(|i| i.total_cents())
                    .collect();
                // Las locaciones con lugar de rodaje suman su costo (una sola vez por lugar) si la fila no tiene uno propio.
                if c.source == CategorySource::Locations {
                    let mut counted: Vec<u64> = Vec::new();
                    for row in rows.iter().filter(|r| r.category == c.id) {
                        let own = row
                            .item
                            .and_then(|i| self.breakdown_items[i].cost_cents)
                            .is_some();
                        let Some(EntityRef::Location(id)) = row.entity else {
                            continue;
                        };
                        let place = self
                            .location_profiles
                            .iter()
                            .find(|l| l.id == id)
                            .and_then(|l| l.shoot_location);
                        if let (false, Some(place), Some(cost)) = (
                            own,
                            place,
                            self.location_deal(EntityRef::Location(id))
                                .and_then(|d| d.cost_cents),
                        ) && !counted.contains(&place)
                        {
                            counted.push(place);
                            costs.push(cost);
                        }
                    }
                }
                (c.id, costs.iter().sum(), costs.len())
            })
            .filter(|(_, _, n)| *n > 0)
            .collect()
    }

    pub fn remove_breakdown_item(&mut self, idx: usize) {
        if idx >= self.breakdown_items.len() {
            return;
        }
        let id = self.breakdown_items.remove(idx).id;
        self.breakdown_tags.retain(|t| t.item != id);
    }

    /// Todas las filas del desglose agrupadas por categoría (en el orden de `breakdown_categories`), cada
    /// una con las escenas donde aparece. `alphabetical` ordena por nombre; si no, por aparición en el guion.
    pub fn breakdown_rows(&self, alphabetical: bool) -> Vec<BreakdownRow> {
        // Escena (número) de cada párrafo.
        let mut scene_of: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        let mut current = 0;
        for p in &self.paragraphs {
            if p.kind == ParagraphKind::SceneHeading {
                current += 1;
            }
            scene_of.insert(p.id, current);
        }

        let mut rows: Vec<BreakdownRow> = Vec::new();
        let mut by_entity: std::collections::HashMap<EntityRef, usize> =
            std::collections::HashMap::new();
        let category_of = |source: CategorySource| {
            self.breakdown_categories
                .iter()
                .find(|c| c.source == source)
                .map(|c| c.id)
        };

        // Una fila por cada personaje, locación y objeto del proyecto: se identifican por id, no por nombre.
        let entities: Vec<(CategorySource, EntityRef)> = self
            .characters
            .iter()
            .map(|c| (CategorySource::Characters, EntityRef::Character(c.id)))
            .chain(
                self.location_profiles
                    .iter()
                    .map(|l| (CategorySource::Locations, EntityRef::Location(l.id))),
            )
            .chain(
                self.objects
                    .iter()
                    .map(|o| (CategorySource::Objects, EntityRef::Object(o.id))),
            )
            .collect();
        for (source, entity) in entities {
            let Some(category) = category_of(source) else {
                continue;
            };
            by_entity.insert(entity, rows.len());
            rows.push(BreakdownRow {
                category,
                name: self.label_for(entity),
                entity: Some(entity),
                item: None,
                scenes: Vec::new(),
                auto: true,
            });
        }

        // Escenas donde aparece cada uno: sus encabezados, y los nombres vinculados en el texto.
        let touch = |rows: &mut Vec<BreakdownRow>, entity: EntityRef, scene: Option<usize>| {
            if let (Some(&r), Some(n)) = (by_entity.get(&entity), scene) {
                if n > 0 && !rows[r].scenes.contains(&n) {
                    rows[r].scenes.push(n);
                }
            }
        };
        for p in &self.paragraphs {
            let scene = scene_of.get(&p.id).copied();
            match p.kind {
                ParagraphKind::Character => {
                    if let Some(id) = self.cue_character(p) {
                        touch(&mut rows, EntityRef::Character(id), scene);
                    }
                }
                ParagraphKind::SceneHeading => {
                    if let Some(id) = self.heading_location_id(p) {
                        touch(&mut rows, EntityRef::Location(id), scene);
                    }
                }
                _ => {
                    for l in &p.links {
                        if matches!(
                            l.target,
                            EntityRef::Character(_) | EntityRef::Location(_) | EntityRef::Object(_)
                        ) {
                            touch(&mut rows, l.target, scene);
                        }
                    }
                }
            }
        }

        // Los elementos guardados: los de un personaje, locación u objeto completan su fila; los demás son filas propias.
        let mut custom_row: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut item_row: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for (idx, item) in self.breakdown_items.iter().enumerate() {
            if !self
                .breakdown_categories
                .iter()
                .any(|c| c.id == item.category)
            {
                continue;
            }
            match item.entity {
                Some(entity) => {
                    if let Some(&r) = by_entity.get(&entity) {
                        rows[r].item = Some(idx);
                        item_row.insert(idx, r);
                    }
                }
                None => {
                    custom_row.insert(idx, rows.len());
                    item_row.insert(idx, rows.len());
                    rows.push(BreakdownRow {
                        category: item.category,
                        name: item.name.clone(),
                        entity: None,
                        item: Some(idx),
                        scenes: Vec::new(),
                        auto: false,
                    });
                }
            }
        }
        // Sus marcas en el guion.
        for tag in &self.breakdown_tags {
            let Some(idx) = self.breakdown_items.iter().position(|i| i.id == tag.item) else {
                continue;
            };
            let scene = scene_of.get(&tag.paragraph).copied();
            match self.breakdown_items[idx].entity {
                Some(entity) => touch(&mut rows, entity, scene),
                None => {
                    if let (Some(&r), Some(n)) = (custom_row.get(&idx), scene) {
                        if n > 0 && !rows[r].scenes.contains(&n) {
                            rows[r].scenes.push(n);
                        }
                    }
                }
            }
        }
        // Las escenas que se asignaron a mano en el desglose.
        for (idx, item) in self.breakdown_items.iter().enumerate() {
            let Some(&r) = item_row.get(&idx) else {
                continue;
            };
            for scene in &item.extra_scenes {
                if let Some(&n) = scene_of.get(scene)
                    && n > 0
                    && !rows[r].scenes.contains(&n)
                {
                    rows[r].scenes.push(n);
                }
            }
        }
        for row in &mut rows {
            row.scenes.sort_unstable();
        }
        let order: Vec<u64> = self.breakdown_categories.iter().map(|c| c.id).collect();
        rows.sort_by(|a, b| {
            let (ca, cb) = (
                order.iter().position(|c| *c == a.category),
                order.iter().position(|c| *c == b.category),
            );
            ca.cmp(&cb).then_with(|| {
                if alphabetical {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                } else {
                    let first = |r: &BreakdownRow| r.scenes.first().copied().unwrap_or(usize::MAX);
                    first(a)
                        .cmp(&first(b))
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                }
            })
        });
        rows
    }

    /// Crea un documento (dentro de    /// Crea un documento (dentro de `parent` si es una carpeta) y devuelve su id.
    pub fn add_document(&mut self, kind: DocKind, parent: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let parent = parent.filter(|p| {
            self.documents
                .iter()
                .any(|d| d.id == *p && d.kind == DocKind::Folder)
        });
        let mut doc = Document {
            id,
            kind,
            name: kind.default_name().to_string(),
            parent,
            text: String::new(),
            description: String::new(),
            fields: BTreeMap::new(),
            nodes: Vec::new(),
            images: Vec::new(),
            cards: Vec::new(),
            rel_nodes: Vec::new(),
            rel_edges: Vec::new(),
        };
        if kind == DocKind::MindMap {
            let root = self.next_id;
            self.next_id += 1;
            doc.nodes.push(MindNode {
                id: root,
                parent: None,
                text: "Idea central".to_string(),
            });
        }
        self.documents.push(doc);
        id
    }

    /// Borra un documento y, si es una carpeta, todo lo que contiene.
    pub fn delete_document(&mut self, id: u64) {
        let mut doomed = vec![id];
        let mut i = 0;
        while i < doomed.len() {
            let current = doomed[i];
            doomed.extend(
                self.documents
                    .iter()
                    .filter(|d| d.parent == Some(current))
                    .map(|d| d.id),
            );
            i += 1;
        }
        for d in &doomed {
            self.drop_links_to(EntityRef::World(*d));
        }
        self.documents.retain(|d| !doomed.contains(&d.id));
    }

    pub fn add_mind_node(&mut self, doc: u64, parent: u64) -> Option<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let d = self.documents.iter_mut().find(|d| d.id == doc)?;
        d.nodes.push(MindNode {
            id,
            parent: Some(parent),
            text: "Nueva idea".to_string(),
        });
        Some(id)
    }

    /// Cada persona numera lo que crea en su propio tramo de ids (el `slot`, de 1 en adelante, de a mil millones), así dos
    /// personas que trabajan a la vez en copias del mismo proyecto nunca le dan el mismo id a cosas distintas. Sigue desde el
    /// último id que ya haya usado en su tramo.
    pub fn use_id_block(&mut self, slot: u64) {
        fn top(value: &serde_json::Value, from: u64, to: u64, found: &mut u64) {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        if k == "id"
                            && let Some(n) = v.as_u64()
                            && (from..to).contains(&n)
                        {
                            *found = (*found).max(n + 1);
                        }
                        top(v, from, to, found);
                    }
                }
                serde_json::Value::Array(items) => items.iter().for_each(|v| top(v, from, to, found)),
                _ => {}
            }
        }
        let from = slot.max(1) * ID_BLOCK;
        let mut next = from;
        if let Ok(value) = serde_json::to_value(&*self) {
            top(&value, from, from + ID_BLOCK, &mut next);
        }
        self.next_id = next;
    }

    /// La curva básica de tres actos, con sus dos puntos de giro, valles, picos y clímax, si todavía no hay ninguna.
    /// Devuelve si la creó.
    pub fn ensure_drama(&mut self) -> bool {
        // El incidente desencadenante y el punto medio ya no son parte de la curva básica.
        let before = self.drama.len();
        self.drama
            .retain(|d| !matches!(d.kind, DramaKind::Inciting | DramaKind::Midpoint));
        if !self.drama.is_empty() {
            return self.drama.len() != before;
        }
        let base = [
            (DramaKind::Start, 0.0, 0.10),
            (DramaKind::TurnOne, 0.25, 0.50),
            (DramaKind::Valley, 0.40, 0.30),
            (DramaKind::Peak, 0.55, 0.65),
            (DramaKind::Valley, 0.65, 0.38),
            (DramaKind::TurnTwo, 0.75, 0.78),
            (DramaKind::Climax, 0.90, 1.0),
            (DramaKind::Resolution, 1.0, 0.35),
        ];
        for (kind, x, y) in base {
            let id = self.next_id;
            self.next_id += 1;
            self.drama.push(DramaPoint {
                id,
                kind,
                label: kind.label().to_string(),
                text: String::new(),
                x,
                y,
            });
        }
        true
    }

    /// Agrega un punto a la curva dramática en el mayor hueco entre dos puntos y devuelve su id.
    pub fn add_drama_point(&mut self, kind: DramaKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.drama.sort_by(|a, b| a.x.total_cmp(&b.x));
        let (mut x, mut y, mut gap) = (0.5, 0.5, -1.0);
        for pair in self.drama.windows(2) {
            if pair[1].x - pair[0].x > gap {
                gap = pair[1].x - pair[0].x;
                x = (pair[0].x + pair[1].x) / 2.0;
                y = (pair[0].y + pair[1].y) / 2.0;
            }
        }
        y = match kind {
            DramaKind::Peak => (y + 0.25).min(1.0),
            DramaKind::Valley => (y - 0.25).max(0.0),
            _ => y,
        };
        self.drama.push(DramaPoint {
            id,
            kind,
            label: kind.label().to_string(),
            text: String::new(),
            x,
            y,
        });
        id
    }

    /// Agrega una tarjeta al final de un documento de tarjetas.
    pub fn add_story_card(&mut self, doc: u64) -> Option<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let d = self.documents.iter_mut().find(|d| d.id == doc)?;
        d.cards.push(StoryCard {
            id,
            title: "Nueva tarjeta".to_string(),
            ..Default::default()
        });
        Some(id)
    }

    /// Agrega un nodo al mapa de relaciones (algo del proyecto o una nota libre) y devuelve su id.
    pub fn add_rel_node(
        &mut self,
        doc: u64,
        target: Option<EntityRef>,
        at: (f32, f32),
    ) -> Option<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let text = if target.is_none() { "Nota".to_string() } else { String::new() };
        let d = self.documents.iter_mut().find(|d| d.id == doc)?;
        d.rel_nodes.push(RelNode {
            id,
            target,
            text,
            x: at.0,
            y: at.1,
        });
        Some(id)
    }

    /// Relaciona dos nodos del mapa (una sola vez por par y sentido). Devuelve el id de la relación.
    pub fn add_rel_edge(&mut self, doc: u64, from: u64, to: u64) -> Option<u64> {
        if from == to {
            return None;
        }
        let id = self.next_id;
        let d = self.documents.iter_mut().find(|d| d.id == doc)?;
        if d.rel_edges.iter().any(|e| e.from == from && e.to == to) {
            return None;
        }
        d.rel_edges.push(RelEdge {
            id,
            from,
            to,
            label: String::new(),
        });
        self.next_id += 1;
        Some(id)
    }

    /// Segundos transcurridos al comenzar cada párrafo (una página de guion es aproximadamente un minuto) y la
    /// duración total. Es lo que dibuja la línea de tiempo junto a la hoja.
    pub fn timeline(&self) -> (Vec<f32>, f32) {
        let mut at = 0.0;
        let mut starts = Vec::with_capacity(self.paragraphs.len());
        for p in &self.paragraphs {
            starts.push(at);
            at += page_lines(p) / LINES_PER_PAGE * 60.0;
        }
        (starts, at)
    }

    pub fn stats(&self) -> Stats {
        let mut per_kind: Vec<(ParagraphKind, KindStats)> = ParagraphKind::ALL
            .iter()
            .map(|k| (*k, KindStats::default()))
            .collect();
        let (mut words, mut with, mut without, mut lines) = (0, 0, 0, 0.0);
        for p in &self.paragraphs {
            let w = word_count(&p.text);
            words += w;
            with += p.text.chars().count();
            without += p.text.chars().filter(|c| !c.is_whitespace()).count();
            lines += page_lines(p);
            if let Some((_, s)) = per_kind.iter_mut().find(|(k, _)| *k == p.kind) {
                s.words += w;
                if !p.text.trim().is_empty() {
                    s.occurrences += 1;
                }
            }
        }
        let pages = lines / LINES_PER_PAGE;
        Stats {
            words,
            chars_with_spaces: with,
            chars_without_spaces: without,
            pages,
            seconds: pages * 60.0,
            per_kind,
        }
    }

    pub fn character_activity(&self) -> Vec<CharacterActivity> {
        let mut out: Vec<CharacterActivity> = Vec::new();
        let mut current: Option<usize> = None;
        let mut last_scene: Vec<usize> = Vec::new();
        let mut scene_no = 0;
        for p in &self.paragraphs {
            match p.kind {
                ParagraphKind::SceneHeading => {
                    scene_no += 1;
                    current = None;
                }
                ParagraphKind::Character => {
                    let name = clean_character_name(&p.text);
                    if name.is_empty() {
                        current = None;
                        continue;
                    }
                    // Un encabezado vinculado cuenta para su personaje; si no, para el nombre suelto.
                    let id = self.cue_character(p);
                    let shown = id
                        .and_then(|i| self.character(i))
                        .map_or(name.clone(), |c| clean_character_name(&c.name));
                    let idx = match out.iter().position(|c| {
                        if id.is_some() {
                            c.id == id
                        } else {
                            c.id.is_none() && c.name == shown
                        }
                    }) {
                        Some(i) => i,
                        None => {
                            out.push(CharacterActivity {
                                id,
                                name: shown,
                                dialogues: 0,
                                words: 0,
                                scenes: 0,
                            });
                            last_scene.push(0);
                            out.len() - 1
                        }
                    };
                    out[idx].dialogues += 1;
                    if last_scene[idx] != scene_no {
                        last_scene[idx] = scene_no;
                        out[idx].scenes += 1;
                    }
                    current = Some(idx);
                }
                ParagraphKind::Dialogue => {
                    if let Some(i) = current {
                        out[i].words += word_count(&p.text);
                    }
                }
                _ => {}
            }
        }
        out.sort_by(|a, b| b.words.cmp(&a.words));
        out
    }

    /// Cada párrafo con el texto que se ve y se exporta: en la acción y el diálogo, los nombres según las reglas de
    /// mayúsculas; en el resto, el texto tal cual.
    pub fn shown_paragraphs(&self) -> Vec<(ParagraphKind, String)> {
        let mentions = self.mention_names();
        let case = self.case_options();
        // Dentro de un diálogo los nombres nunca van todo en mayúsculas: es lo que dice el personaje, no una indicación.
        let spoken = CaseOptions::uniform(false);
        self.paragraphs
            .iter()
            .map(|p| {
                let text = if matches!(p.kind, ParagraphKind::Action | ParagraphKind::Dialogue) {
                    let rules = if p.kind == ParagraphKind::Dialogue {
                        &spoken
                    } else {
                        &case
                    };
                    styled_mentions_text(&p.text, &p.links, &mentions, rules)
                } else {
                    p.text.trim_end().to_string()
                };
                (p.kind, text)
            })
            .collect()
    }

    /// Abre una revisión: de ahí en más, lo que se cambie en el guion queda marcado. Toma el color que sigue al de la
    /// anterior en el orden habitual (azul, rosa, amarillo…). Devuelve su id.
    pub fn new_revision(&mut self, name: &str, date: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let color = match self.revisions.last() {
            Some(last) => {
                let at = RevisionColor::ALL
                    .iter()
                    .position(|c| *c == last.color)
                    .unwrap_or(0);
                RevisionColor::ALL[(at + 1) % RevisionColor::ALL.len()]
            }
            None => RevisionColor::Blue,
        };
        let name = if name.trim().is_empty() {
            format!("Revisión {}", self.revisions.len() + 1)
        } else {
            name.trim().to_string()
        };
        let baseline = self
            .paragraphs
            .iter()
            .map(|p| (p.id, (p.kind, p.text.clone())))
            .collect();
        self.revisions.push(Revision {
            id,
            name,
            color,
            date: date.to_string(),
            baseline,
        });
        self.active_revision = Some(id);
        id
    }

    /// La revisión que está marcando cambios.
    pub fn active_revision(&self) -> Option<&Revision> {
        self.active_revision
            .and_then(|id| self.revisions.iter().find(|r| r.id == id))
    }

    /// Los párrafos nuevos o cambiados desde que se abrió la revisión activa. Un párrafo que vuelve a quedar como estaba
    /// deja de estar marcado.
    pub fn revised_paragraphs(&self) -> std::collections::HashSet<u64> {
        let Some(rev) = self.active_revision() else {
            return Default::default();
        };
        self.paragraphs
            .iter()
            .filter(|p| {
                rev.baseline
                    .get(&p.id)
                    .is_none_or(|(kind, text)| *kind != p.kind || *text != p.text)
            })
            .map(|p| p.id)
            .collect()
    }

    /// Qué personajes retoman la palabra después de una acción o una transición: llevan «(CONT.)» al maquetar, sin
    /// guardarlo en el texto. Una escena nueva corta la cuenta, y dos nombres seguidos no cuentan como retomar.
    pub fn cont_marks(&self) -> Vec<bool> {
        let mut last: Option<String> = None;
        let mut interrupted = false;
        self.paragraphs
            .iter()
            .map(|p| match p.kind {
                ParagraphKind::SceneHeading | ParagraphKind::Act | ParagraphKind::Sequence => {
                    last = None;
                    interrupted = false;
                    false
                }
                ParagraphKind::Action | ParagraphKind::Transition => {
                    interrupted = true;
                    false
                }
                ParagraphKind::Character => {
                    let name = clean_character_name(&p.text);
                    let again = self.auto_cont
                        && interrupted
                        && last.as_deref() == Some(name.as_str())
                        && !name.is_empty()
                        && !p.text.to_uppercase().contains("CONT");
                    last = Some(name);
                    interrupted = false;
                    again
                }
                _ => false,
            })
            .collect()
    }

    /// Reemplaza el guion por los párrafos importados. Los personajes y las locaciones que nombran se crean si no
    /// existen. Con un número en cada encabezado, la numeración queda bloqueada con esos números.
    pub fn replace_script(
        &mut self,
        blocks: Vec<(ParagraphKind, String)>,
        numbers: Vec<Option<String>>,
    ) {
        self.paragraphs = blocks
            .into_iter()
            .map(|(kind, text)| self.make_paragraph(kind, &text))
            .collect();
        self.comments.retain(|c| c.in_treatment);
        self.breakdown_tags.clear();
        self.scenes.clear();
        self.scene_numbers_locked = false;
        let headings = self.script_scenes();
        if !headings.is_empty()
            && numbers.len() == headings.len()
            && numbers
                .iter()
                .all(|n| n.as_ref().is_some_and(|n| !n.is_empty()))
        {
            for ((id, _), number) in headings.iter().zip(&numbers) {
                self.scenes.entry(*id).or_default().number = number.clone().unwrap_or_default();
            }
            self.scene_numbers_locked = true;
        }
        self.auto_link_all();
        self.normalize();
    }

    /// Exporta a texto plano con el formato clásico de guion.
    pub fn to_plain_text(&self) -> String {
        let mentions = self.mention_names();
        let case = self.case_options();
        // Dentro de un diálogo los nombres nunca van todo en mayúsculas: es lo que dice el personaje, no una indicación.
        let spoken = CaseOptions::uniform(false);
        let mut out = format!("{}\n", self.title.to_uppercase());
        if !self.author.is_empty() {
            out += &format!("por {}\n", self.author);
        }
        let mut scene = 0;
        let conts = self.cont_marks();
        let labels = self.scene_labels();
        for (index, p) in self.paragraphs.iter().enumerate() {
            let shown;
            let text = if matches!(p.kind, ParagraphKind::Action | ParagraphKind::Dialogue) {
                let rules = if p.kind == ParagraphKind::Dialogue {
                    &spoken
                } else {
                    &case
                };
                shown = styled_mentions_text(&p.text, &p.links, &mentions, rules);
                shown.as_str()
            } else {
                p.text.trim_end()
            };
            match p.kind {
                ParagraphKind::SceneHeading => {
                    scene += 1;
                    let number = labels
                        .get(scene - 1)
                        .map_or(scene.to_string(), |(_, n)| n.clone());
                    out += &format!("\n{number}. {}\n\n", text.to_uppercase());
                }
                ParagraphKind::Action => out += &format!("{text}\n\n"),
                ParagraphKind::Character => {
                    let again = if conts.get(index).copied().unwrap_or(false) {
                        " (CONT.)"
                    } else {
                        ""
                    };
                    out += &format!("{:>26}{again}\n", text.to_uppercase())
                }
                ParagraphKind::Parenthetical => out += &format!("{:>22}({text})\n", ""),
                ParagraphKind::Dialogue => {
                    for line in text.lines() {
                        out += &format!("{:>16}{line}\n", "");
                    }
                    out.push('\n');
                }
                ParagraphKind::Transition => out += &format!("{:>60}\n\n", text.to_uppercase()),
                ParagraphKind::Lyrics => {
                    for line in text.lines() {
                        out += &format!("{:>16}~{line}\n", "");
                    }
                    out.push('\n');
                }
                ParagraphKind::Note => out += &format!("[[{text}]]\n\n"),
                ParagraphKind::Act | ParagraphKind::Sequence => {
                    out += &format!("\n{:^60}\n\n", text.to_uppercase())
                }
            }
        }
        out
    }
}

/// Busca `needle` sin distinguir mayúsculas y devuelve el texto tal como está escrito en `haystack`.
fn find_ignore_case(haystack: &str, needle: &str) -> Option<String> {
    let chars: Vec<char> = haystack.chars().collect();
    let lower: Vec<char> = haystack.to_lowercase().chars().collect();
    if chars.len() != lower.len() {
        return haystack.find(needle).map(|_| needle.to_string());
    }
    let n: Vec<char> = needle.to_lowercase().chars().collect();
    if n.is_empty() || n.len() > lower.len() {
        return None;
    }
    (0..=lower.len() - n.len())
        .find(|&i| lower[i..i + n.len()] == n[..])
        .map(|i| chars[i..i + n.len()].iter().collect())
}

/// Cómo se muestra un nombre guardado dentro de un texto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseRule {
    Upper,
    /// Mayúscula inicial en cada palabra importante («Juan Carlos»).
    Title,
    Lower,
}

/// Aparición de un nombre guardado dentro de un texto (posiciones en caracteres).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MentionSpan {
    pub start: usize,
    pub end: usize,
    pub character: bool,
    /// Cambio de mayúsculas al mostrarlo; `None` si se deja como se escribió.
    pub rule: Option<CaseRule>,
}

/// Cómo se muestra cada nombre guardado en `text`: los tramos vinculados (personajes, objetos, elementos, mundos) y,
/// mientras las locaciones no tengan vínculo, las locaciones encontradas por su nombre. Con `uppercase` todos van en
/// mayúsculas; si no, los personajes llevan mayúscula inicial y los demás se dejan como se escribieron (salvo los que
/// están enteros en mayúsculas, que pasan a minúsculas porque esa es la huella del modo «todo mayúsculas»).
///
/// El texto guardado no cambia: al alternar la opción se ve distinto todo el guion, lo nuevo y lo ya escrito.
pub fn mention_spans(
    text: &str,
    links: &[Link],
    mentions: &[Mention],
    case: &CaseOptions,
) -> Vec<MentionSpan> {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    if chars.len() != lower.len() {
        return Vec::new();
    }
    let rule_for =
        |start: usize, end: usize, character: bool, uppercase: bool| -> Option<CaseRule> {
            if case.as_written {
                None
            } else if uppercase {
                Some(CaseRule::Upper)
            } else if character {
                Some(CaseRule::Title)
            } else {
                let letters: Vec<char> = chars[start..end]
                    .iter()
                    .copied()
                    .filter(|c| c.is_alphabetic())
                    .collect();
                (letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase()))
                    .then_some(CaseRule::Lower)
            }
        };
    let mut spans: Vec<MentionSpan> = Vec::new();
    for l in links {
        if l.start < l.end
            && l.end <= chars.len()
            && !spans.iter().any(|s| l.start < s.end && l.end > s.start)
        {
            let character = matches!(l.target, EntityRef::Character(_));
            let upper = case.is_upper(Some(l.target));
            spans.push(MentionSpan {
                start: l.start,
                end: l.end,
                character,
                rule: rule_for(l.start, l.end, character, upper),
            });
        }
    }
    let mut ordered: Vec<&Mention> = mentions.iter().filter(|m| m.target.is_none()).collect();
    ordered.sort_by_key(|m| std::cmp::Reverse(m.name.chars().count()));
    for mention in ordered {
        let name: Vec<char> = mention.name.to_lowercase().chars().collect();
        if name.is_empty() || name.len() > lower.len() {
            continue;
        }
        for i in 0..=lower.len() - name.len() {
            let end = i + name.len();
            let boundaries = (i == 0 || !chars[i - 1].is_alphanumeric())
                && (end == chars.len() || !chars[end].is_alphanumeric());
            if !boundaries
                || lower[i..end] != name[..]
                || spans.iter().any(|s| i < s.end && end > s.start)
            {
                continue;
            }
            spans.push(MentionSpan {
                start: i,
                end,
                character: mention.character,
                rule: rule_for(i, end, mention.character, case.is_upper(mention.target)),
            });
        }
    }
    spans.sort_by_key(|s| s.start);
    spans
}

fn map_one(c: char, upper: bool) -> char {
    let mut it: Vec<char> = if upper {
        c.to_uppercase().collect()
    } else {
        c.to_lowercase().collect()
    };
    if it.len() == 1 { it.remove(0) } else { c }
}

/// Aplica una regla de mayúsculas conservando el largo (carácter por carácter).
pub fn apply_case(chars: &[char], rule: CaseRule) -> String {
    match rule {
        CaseRule::Upper => chars.iter().map(|c| map_one(*c, true)).collect(),
        CaseRule::Lower => chars.iter().map(|c| map_one(*c, false)).collect(),
        CaseRule::Title => {
            const SMALL: [&str; 8] = ["de", "del", "la", "las", "los", "el", "y", "e"];
            let mut out = String::new();
            let mut word: Vec<char> = Vec::new();
            let mut first_word = true;
            let flush = |word: &mut Vec<char>, out: &mut String, first_word: &mut bool| {
                if word.is_empty() {
                    return;
                }
                let lower: String = word.iter().map(|c| map_one(*c, false)).collect();
                if !*first_word && SMALL.contains(&lower.as_str()) {
                    out.push_str(&lower);
                } else {
                    let mut it = lower.chars();
                    if let Some(f) = it.next() {
                        out.push(map_one(f, true));
                        out.push_str(it.as_str());
                    }
                }
                *first_word = false;
                word.clear();
            };
            for &c in chars {
                if c.is_alphanumeric() {
                    word.push(c);
                } else {
                    flush(&mut word, &mut out, &mut first_word);
                    out.push(c);
                }
            }
            flush(&mut word, &mut out, &mut first_word);
            out
        }
    }
}

/// El texto con los nombres guardados mostrados según la opción de mayúsculas (para exportar).
pub fn styled_mentions_text(
    text: &str,
    links: &[Link],
    mentions: &[Mention],
    case: &CaseOptions,
) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for span in mention_spans(text, links, mentions, case) {
        out.extend(&chars[at..span.start]);
        match span.rule {
            Some(rule) => out.push_str(&apply_case(&chars[span.start..span.end], rule)),
            None => out.extend(&chars[span.start..span.end]),
        }
        at = span.end;
    }
    out.extend(&chars[at..]);
    out
}

/// Cómo se escribe el nombre actual de algo vinculado dentro de un párrafo. En un encabezado de personaje va en
/// mayúsculas; en acciones y diálogos, los personajes con mayúscula inicial y lo demás respetando si el texto
/// anterior empezaba con mayúscula.
fn linked_text(kind: ParagraphKind, target: EntityRef, name: &str, previous: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    match (kind, target) {
        (ParagraphKind::Character | ParagraphKind::SceneHeading, _) => {
            apply_case(&chars, CaseRule::Upper)
        }
        (_, EntityRef::Character(_)) => apply_case(&chars, CaseRule::Title),
        _ => {
            let lower = apply_case(&chars, CaseRule::Lower);
            if previous.chars().next().is_some_and(char::is_uppercase) {
                let mut it = lower.chars();
                it.next().map_or(String::new(), |f| {
                    f.to_uppercase().collect::<String>() + it.as_str()
                })
            } else {
                lower
            }
        }
    }
}

/// Reescribe cada tramo vinculado de un texto con el nombre actual de lo que apunta (ver `Project::sync_links`).
fn sync_text(
    kind: ParagraphKind,
    text: &mut String,
    links: &mut Vec<Link>,
    names: &std::collections::HashMap<EntityRef, String>,
) {
    if links.is_empty() {
        return;
    }
    links.retain(|l| names.get(&l.target).is_some_and(|n| !n.is_empty()));
    links.sort_by_key(|l| l.start);
    let mut chars: Vec<char> = text.chars().collect();
    let mut delta: isize = 0;
    let mut changed = false;
    for l in links.iter_mut() {
        let (s, e) = (
            (l.start as isize + delta) as usize,
            (l.end as isize + delta) as usize,
        );
        if e > chars.len() || s > e {
            continue;
        }
        let target_name = &names[&l.target];
        let current: String = chars[s..e].iter().collect();
        if current.to_lowercase() == target_name.to_lowercase() {
            l.start = s;
            l.end = e;
            continue;
        }
        let replacement: Vec<char> = linked_text(kind, l.target, target_name, &current)
            .chars()
            .collect();
        delta += replacement.len() as isize - (e - s) as isize;
        chars.splice(s..e, replacement.iter().copied());
        l.start = s;
        l.end = s + replacement.len();
        changed = true;
    }
    if changed {
        *text = chars.into_iter().collect();
    }
}

/// Vincula, dentro de `text`, los nombres que coinciden exactamente con un solo dueño (palabras completas, sin
/// distinguir mayúsculas) y que todavía no estén cubiertos por otro vínculo.
fn link_names(names: &[(Vec<char>, EntityRef)], text: &str, links: &mut Vec<Link>) {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    if chars.len() != lower.len() {
        return;
    }
    for (name, target) in names {
        if name.len() > lower.len() {
            continue;
        }
        for i in 0..=lower.len() - name.len() {
            let end = i + name.len();
            let boundaries = (i == 0 || !chars[i - 1].is_alphanumeric())
                && (end == chars.len() || !chars[end].is_alphanumeric());
            if boundaries
                && lower[i..end] == name[..]
                && !links.iter().any(|l| i < l.end && end > l.start)
            {
                links.push(Link {
                    start: i,
                    end,
                    target: *target,
                });
            }
        }
    }
    links.sort_by_key(|l| l.start);
}

/// Descarta los vínculos que ya no son una palabra completa dentro de `text`.
pub fn validate_links(links: &mut Vec<Link>, text: &str) {
    let n: Vec<char> = text.chars().collect();
    links.retain(|l| {
        l.start < l.end
            && l.end <= n.len()
            && (l.start == 0 || !n[l.start - 1].is_alphanumeric())
            && (l.end == n.len() || !n[l.end].is_alphanumeric())
    });
}

/// Acomoda los vínculos de un párrafo a un cambio de su texto. Se busca la parte que cambió (lo que no coincide
/// entre el principio y el final) y los vínculos de después se corren; los que quedan tocados por el cambio, o que
/// dejan de ser una palabra completa, se sueltan.
pub fn adjust_links(links: &mut Vec<Link>, old: &str, new: &str) {
    if links.is_empty() {
        return;
    }
    let o: Vec<char> = old.chars().collect();
    let n: Vec<char> = new.chars().collect();
    let mut prefix = 0;
    while prefix < o.len() && prefix < n.len() && o[prefix] == n[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < o.len() - prefix
        && suffix < n.len() - prefix
        && o[o.len() - 1 - suffix] == n[n.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_end = o.len() - suffix;
    let delta = n.len() as isize - o.len() as isize;
    links.retain_mut(|l| {
        if l.end <= prefix {
            true
        } else if l.start >= old_end {
            l.start = (l.start as isize + delta) as usize;
            l.end = (l.end as isize + delta) as usize;
            true
        } else {
            false
        }
    });
    validate_links(links, new);
}

/// Reparte los vínculos al partir un párrafo en el carácter `at`; `skip` es la cantidad de caracteres que se
/// recortan al comienzo de la parte de abajo. Los que cruzan el corte se sueltan.
pub fn split_links(links: &[Link], at: usize, skip: usize) -> (Vec<Link>, Vec<Link>) {
    let head = links.iter().copied().filter(|l| l.end <= at).collect();
    let tail = links
        .iter()
        .copied()
        .filter(|l| l.start >= at + skip)
        .map(|l| Link {
            start: l.start - at - skip,
            end: l.end - at - skip,
            target: l.target,
        })
        .collect();
    (head, tail)
}

/// Pasa los vínculos de un párrafo que se une al final de otro de largo `offset`.
pub fn shifted_links(links: &[Link], offset: usize) -> Vec<Link> {
    links
        .iter()
        .map(|l| Link {
            start: l.start + offset,
            end: l.end + offset,
            target: l.target,
        })
        .collect()
}

/// Normaliza el nombre de un personaje: mayúsculas y sin extensiones como `(V.O.)` o `(CONT'D)`.
pub fn clean_character_name(raw: &str) -> String {
    let base = raw.split('(').next().unwrap_or("");
    base.trim().to_uppercase()
}

/// Dónde está la locación dentro de un encabezado de escena (posiciones en caracteres):
/// `INT. CASA DE ANA - NOCHE` → el tramo `CASA DE ANA`. `None` si todavía no hay nombre.
pub fn heading_location_span(heading: &str) -> Option<(usize, usize)> {
    let chars: Vec<char> = heading.chars().collect();
    let upper: String = chars.iter().map(|c| map_one(*c, true)).collect();
    let mut start = 0;
    for prefix in [
        "INT./EXT.",
        "INT/EXT.",
        "INT./EXT",
        "INT/EXT",
        "I/E.",
        "INT.",
        "EXT.",
        "INT ",
        "EXT ",
    ] {
        if upper.trim_start().starts_with(prefix) {
            start =
                upper.chars().take_while(|c| c.is_whitespace()).count() + prefix.chars().count();
            break;
        }
    }
    while start < chars.len() && chars[start].is_whitespace() {
        start += 1;
    }
    let upper_chars: Vec<char> = upper.chars().collect();
    let mut end = chars.len();
    // Hasta « - » (momento del día); un « -» al final también corta.
    let mut i = start;
    while i < chars.len() {
        if upper_chars[i] == ' '
            && upper_chars.get(i + 1) == Some(&'-')
            && matches!(upper_chars.get(i + 2), Some(' ') | None)
        {
            end = i;
        }
        i += 1;
    }
    while end > start && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    (end > start).then_some((start, end))
}

/// Extrae la locación de un encabezado: `INT. CASA DE ANA - NOCHE` → `CASA DE ANA`.
pub fn heading_location(heading: &str) -> String {
    let mut s = heading.trim().to_uppercase();
    for prefix in [
        "INT./EXT.",
        "INT/EXT.",
        "INT./EXT",
        "INT/EXT",
        "I/E.",
        "INT.",
        "EXT.",
        "INT ",
        "EXT ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim().to_string();
            break;
        }
    }
    match s.rfind(" - ") {
        Some(i) => s[..i].trim().to_string(),
        None => s,
    }
}

/// Las partes de un encabezado: («INT.», lugar, momento): `INT. CASA DE ANA - NOCHE` → («INT.», «CASA DE ANA», «NOCHE»).
pub fn heading_parts(heading: &str) -> (String, String, String) {
    let upper = heading.trim().to_uppercase();
    const PREFIXES: [(&str, &str); 9] = [
        ("INT./EXT.", "INT./EXT."),
        ("INT/EXT.", "INT./EXT."),
        ("INT./EXT", "INT./EXT."),
        ("INT/EXT", "INT./EXT."),
        ("I/E.", "INT./EXT."),
        ("INT.", "INT."),
        ("EXT.", "EXT."),
        ("INT ", "INT."),
        ("EXT ", "EXT."),
    ];
    let (kind, rest) = PREFIXES
        .iter()
        .find_map(|(prefix, label)| upper.strip_prefix(prefix).map(|r| (label.to_string(), r)))
        .unwrap_or((String::new(), upper.as_str()));
    let rest = rest.trim();
    match rest
        .rfind(" - ")
        .map(|i| (i, 3))
        .or_else(|| rest.rfind(" – ").map(|i| (i, " – ".len())))
    {
        Some((i, len)) => (
            kind,
            rest[..i].trim().to_string(),
            rest[i + len..].trim().to_string(),
        ),
        None => (kind, rest.to_string(), String::new()),
    }
}

/// Los octavos de página como se escriben en un plan de rodaje: «3/8», «1», «1 3/8».
pub fn fmt_eighths(eighths: u16) -> String {
    let (whole, rest) = (eighths / 8, eighths % 8);
    match (whole, rest) {
        (0, r) => format!("{r}/8"),
        (w, 0) => w.to_string(),
        (w, r) => format!("{w} {r}/8"),
    }
}

/// Lee los octavos que se escribieron: «3/8», «1 3/8», «2» (páginas enteras). Vacío es «calcular solo».
pub fn parse_eighths(text: &str) -> Option<u16> {
    let text = text.trim();
    let mut total = 0u16;
    let mut any = false;
    for part in text.split_whitespace() {
        match part.split_once('/') {
            Some((a, b)) => {
                let (a, b): (u16, u16) = (a.parse().ok()?, b.parse().ok()?);
                if b != 8 {
                    return None;
                }
                total += a;
            }
            None => total += part.parse::<u16>().ok()? * 8,
        }
        any = true;
    }
    (any && total > 0).then_some(total)
}

/// Parte un número de escena en (letras de antes, cifras, letras de después): «12A» → ("", 12, "A"), «A1» → ("A", 1, "").
fn split_scene_number(number: &str) -> (String, u32, String) {
    let before: String = number
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let rest = &number[before.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let after = rest[digits.len()..].to_string();
    (before, digits.parse().unwrap_or(0), after)
}

/// Para ordenar números de escena como en el guion: A1 antes que 1, 12 antes que 12A, 12A antes que 12B, 12Z antes que
/// 12AA y 12A antes que 13.
fn scene_number_key(number: &str) -> (u32, String, usize, String) {
    let (before, n, after) = split_scene_number(number);
    (
        n,
        if before.is_empty() {
            String::from("~")
        } else {
            before
        },
        after.len(),
        after,
    )
}

/// Los `count` números de una tanda de escenas nuevas entre la escena fija anterior y la siguiente, en orden y sin
/// repetir ninguno de los fijos. Si entre las dos fijas no cabe ninguna letra (12 y 12A), se cuelgan de la siguiente
/// con letra doble (12AA, 12AB…).
fn labels_between(
    previous: Option<&str>,
    next: Option<&str>,
    count: usize,
    taken: &std::collections::HashSet<String>,
) -> Vec<String> {
    if previous.is_none() && next.is_none() {
        return (1..=count).map(|n| n.to_string()).collect();
    }
    let mut labels = Vec::with_capacity(count);
    let mut current = previous.map(str::to_string);
    for _ in 0..count {
        let mut candidate = next_scene_number(current.as_deref());
        while taken.contains(&candidate) {
            candidate = next_scene_number(Some(&candidate));
        }
        current = Some(candidate.clone());
        labels.push(candidate);
    }
    let fits = next.is_none_or(|n| {
        labels
            .last()
            .is_none_or(|l| scene_number_key(l) < scene_number_key(n))
    });
    if fits {
        return labels;
    }
    let anchor = next.unwrap_or_default();
    let mut doubles = Vec::with_capacity(count);
    let mut letters = "A".to_string();
    while doubles.len() < count {
        let candidate = format!("{anchor}{letters}");
        if !taken.contains(&candidate) {
            doubles.push(candidate);
        }
        letters = next_letters(&letters);
    }
    doubles
}

/// La letra siguiente: A → B, Z → AA.
fn next_letters(letters: &str) -> String {
    let mut chars: Vec<char> = letters.chars().collect();
    for i in (0..chars.len()).rev() {
        if chars[i] == 'Z' {
            chars[i] = 'A';
        } else {
            chars[i] = (chars[i] as u8 + 1) as char;
            return chars.into_iter().collect();
        }
    }
    format!("A{}", chars.into_iter().collect::<String>())
}

/// El número que sigue al de la escena anterior sin ser el de la escena siguiente: se le agrega o se le sube la letra.
fn next_scene_number(previous: Option<&str>) -> String {
    let Some(previous) = previous else {
        return "A1".to_string();
    };
    let (before, n, after) = split_scene_number(previous);
    if !before.is_empty() {
        return format!("{}{n}{after}", next_letters(&before));
    }
    if after.is_empty() {
        format!("{n}A")
    } else {
        format!("{n}{}", next_letters(&after))
    }
}

/// Reparte un texto pegado en párrafos con su tipo: ignora las líneas en blanco (que solo separan bloques) y los
/// espacios de sobra, y reconoce encabezados de escena, transiciones, personajes con sus diálogos y acotaciones.
/// Lo que escribe en mayúsculas lo que el tipo pide en mayúsculas.
pub fn classify_pasted(text: &str) -> Vec<(ParagraphKind, String)> {
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let mut out = Vec::new();
    let mut speaking = false;
    for (n, line) in lines.iter().enumerate() {
        if line.is_empty() {
            speaking = false;
            continue;
        }
        let next_is_text = lines.get(n + 1).is_some_and(|l| !l.is_empty());
        let (kind, shown) = if speaking {
            if line.starts_with('(') && line.ends_with(')') {
                // El editor dibuja los paréntesis aparte: el texto va sin ellos.
                (
                    ParagraphKind::Parenthetical,
                    line[1..line.len() - 1].trim().to_string(),
                )
            } else {
                (ParagraphKind::Dialogue, line.clone())
            }
        } else if let Some(heading) = scene_heading_of(line) {
            (ParagraphKind::SceneHeading, heading)
        } else if looks_like_transition(line) {
            (
                ParagraphKind::Transition,
                line.trim_start_matches('>').trim().to_uppercase(),
            )
        } else if next_is_text && looks_like_character_cue(line) {
            speaking = true;
            (ParagraphKind::Character, line.to_uppercase())
        } else {
            (ParagraphKind::Action, line.clone())
        };
        out.push((kind, shown));
    }
    out
}

pub(crate) fn is_all_caps(s: &str) -> bool {
    s.chars().any(char::is_alphabetic) && !s.chars().any(char::is_lowercase)
}

/// El encabezado de escena, si la línea lo es (admite número de escena delante y el punto de «.INT.» forzado).
pub(crate) fn scene_heading_of(line: &str) -> Option<String> {
    let forced = line
        .strip_prefix('.')
        .filter(|r| r.starts_with(|c: char| c.is_alphabetic()));
    let without_number = line
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .trim_start_matches(['.', ')', '-', ' ']);
    let candidate = forced.unwrap_or(if without_number.len() < line.len() {
        without_number
    } else {
        line
    });
    let upper = candidate.to_uppercase();
    const STARTS: [&str; 8] = [
        "INT./EXT.",
        "INT/EXT",
        "INT.",
        "EXT.",
        "INT ",
        "EXT ",
        "INTERIOR ",
        "EXTERIOR ",
    ];
    let matches = forced.is_some() || STARTS.iter().any(|s| upper.starts_with(s));
    matches.then(|| {
        let mut heading = upper;
        for (short, full) in [
            ("INT ", "INT. "),
            ("EXT ", "EXT. "),
            ("INTERIOR ", "INT. "),
            ("EXTERIOR ", "EXT. "),
        ] {
            if let Some(rest) = heading.strip_prefix(short) {
                heading = format!("{full}{rest}");
                break;
            }
        }
        heading
    })
}

pub(crate) fn looks_like_transition(line: &str) -> bool {
    if line.starts_with('>') && !line.ends_with('<') {
        return true;
    }
    is_all_caps(line)
        && (line.ends_with(':') && line.len() <= 30
            || [
                "CORTE A", "CORTA A", "FUNDIDO", "FADE ", "DISUELVE", "CUT TO",
            ]
            .iter()
            .any(|s| line.starts_with(s)))
}

/// Un nombre solo en mayúsculas, corto y sin punto final, con o sin extensión: «JUAN», «ANA (V.O.)».
pub(crate) fn looks_like_character_cue(line: &str) -> bool {
    let name = match line.find('(') {
        Some(i) if line.ends_with(')') => line[..i].trim(),
        _ => line,
    };
    is_all_caps(name)
        && name.chars().count() <= 32
        && name.split(' ').count() <= 4
        && !name.ends_with(['.', '!', '?', ':', ',', ';'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with(items: &[(ParagraphKind, &str)]) -> Project {
        let mut p = Project::default();
        p.paragraphs.clear();
        for (k, t) in items {
            let para = p.make_paragraph(*k, t);
            p.paragraphs.push(para);
        }
        p
    }

    #[test]
    fn the_outline_follows_the_order_of_the_text_and_moving_a_scene_moves_its_text() {
        let mut p = Project::default();
        p.new_character("Ana");
        p.treatment = "INT. FARO - NOCHE\nAna sube.\nINT. PLAYA - DÍA\nAna camina.\nINT. CASA - DÍA\nAna duerme.".into();
        // Se crean en otro orden que el del texto.
        p.add_outline_scene("INT. CASA - DÍA");
        p.add_outline_scene("INT. FARO - NOCHE");
        p.add_outline_scene("INT. PLAYA - DÍA");
        p.sort_outline_by_text();
        let heads = |p: &Project| {
            p.outline
                .iter()
                .map(|o| o.heading.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            heads(&p),
            ["INT. FARO - NOCHE", "INT. PLAYA - DÍA", "INT. CASA - DÍA"]
        );

        // Subir la última escena intercambia los bloques de texto.
        assert!(p.move_outline_scene(2, -1));
        assert_eq!(
            p.treatment,
            "INT. FARO - NOCHE\nAna sube.\nINT. CASA - DÍA\nAna duerme.\nINT. PLAYA - DÍA\nAna camina."
        );
        assert_eq!(
            heads(&p),
            ["INT. FARO - NOCHE", "INT. CASA - DÍA", "INT. PLAYA - DÍA"]
        );
        // Bajar la primera.
        assert!(p.move_outline_scene(0, 1));
        assert_eq!(
            p.treatment,
            "INT. CASA - DÍA\nAna duerme.\nINT. FARO - NOCHE\nAna sube.\nINT. PLAYA - DÍA\nAna camina."
        );
        // No se puede subir la primera.
        assert!(!p.move_outline_scene(0, -1));
    }

    #[test]
    fn enter_and_tab_flow() {
        use ParagraphKind::*;
        assert_eq!(SceneHeading.after_enter(), Action);
        assert_eq!(Action.after_tab(), Character);
        assert_eq!(Character.after_enter(), Dialogue);
        assert_eq!(Dialogue.after_tab(), Parenthetical);
        assert_eq!(Parenthetical.after_enter(), Dialogue);
        assert_eq!(Dialogue.after_enter(), Character);
        // Tab en un personaje vacío abre la acotación; en una acotación y en un diálogo, el otro.
        assert_eq!(Character.after_tab(), Parenthetical);
        assert_eq!(Parenthetical.after_tab(), Dialogue);
        assert_eq!(SceneHeading.after_tab(), Action);
        assert_eq!(Transition.after_tab(), SceneHeading);
        assert_eq!(Transition.after_enter(), SceneHeading);
    }

    #[test]
    fn location_from_heading() {
        assert_eq!(heading_location("int. casa de ana - noche"), "CASA DE ANA");
        assert_eq!(heading_location("EXT. PLAZA"), "PLAZA");
        assert_eq!(heading_location("INT./EXT. AUTO - DÍA"), "AUTO");
    }

    #[test]
    fn characters_are_unique_and_clean() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (Character, "ana"),
            (Character, "Luis (V.O.)"),
            (Character, "ANA"),
            (Character, " "),
        ]);
        p.auto_link_all();
        assert_eq!(p.known_characters(), vec!["ANA", "LUIS"]);
        assert_eq!(p.characters.len(), 2);
    }

    #[test]
    fn documents_nest_and_delete_with_folder() {
        let mut p = Project::default();
        let folder = p.add_document(DocKind::Folder, None);
        let note = p.add_document(DocKind::Text, Some(folder));
        let map = p.add_document(DocKind::MindMap, Some(note)); // `note` no es carpeta: va a la raíz
        assert_eq!(
            p.documents.iter().find(|d| d.id == map).unwrap().parent,
            None
        );
        assert_eq!(
            p.documents.iter().find(|d| d.id == note).unwrap().parent,
            Some(folder)
        );
        assert_eq!(
            p.documents
                .iter()
                .find(|d| d.id == map)
                .unwrap()
                .nodes
                .len(),
            1
        );
        p.delete_document(folder);
        assert_eq!(p.documents.len(), 1);
        assert_eq!(p.documents[0].id, map);
    }

    #[test]
    fn new_locations_get_numbered_names_and_can_be_renamed_freely() {
        let mut p = Project::default();
        let a = p.add_location("Nueva locación");
        let b = p.add_location("Nueva locación");
        assert_eq!(p.location(a).unwrap().name, "NUEVA LOCACIÓN");
        assert_eq!(p.location(b).unwrap().name, "NUEVA LOCACIÓN 2");
        p.location_mut(b).unwrap().name = "NUEVA LOCACIÓN".into(); // el nombre se puede repetir: se distinguen por id
        assert!(
            p.locations()
                .iter()
                .all(|l| l.name == "NUEVA LOCACIÓN" && l.scenes == 0)
        );
        assert_ne!(a, b);
    }

    #[test]
    fn headings_are_linked_to_locations_and_follow_renames() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. casa de ana - noche"),
            (Action, "Se oye una puerta."),
            (SceneHeading, "EXT. FARO - DÍA"),
            (SceneHeading, "INT. FARO - NOCHE"),
            (Action, "Ana vuelve a la casa de ana."),
        ]);
        p.auto_link_all();
        // Los encabezados crearon sus locaciones y quedaron vinculados sobre el nombre.
        assert_eq!(p.location_profiles.len(), 2);
        assert_eq!(
            p.paragraphs[0].links[0].end - p.paragraphs[0].links[0].start,
            "casa de ana".len()
        );
        let casa = p.location_id_by_name("CASA DE ANA").unwrap();
        let faro = p.location_id_by_name("FARO").unwrap();
        assert_eq!(
            p.scenes_at(faro)
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        // La mención dentro de la acción también se vinculó (a la misma locación).
        assert!(
            p.paragraphs[4]
                .links
                .iter()
                .any(|l| l.target == EntityRef::Location(casa))
        );

        p.location_mut(casa).unwrap().name = "LA CASONA".into();
        p.sync_links();
        assert_eq!(p.paragraphs[0].text, "INT. LA CASONA - noche"); // se conservan prefijo y momento
        assert_eq!(p.paragraphs[4].text, "Ana vuelve a la la casona.");
        assert_eq!(p.scenes_at(casa).len(), 1);
    }

    #[test]
    fn two_locations_with_the_same_name_stay_apart() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. FARO - DÍA"),
            (SceneHeading, "INT. FARO - NOCHE"),
        ]);
        let viejo = p.new_location("FARO");
        let nuevo = p.new_location("FARO");
        p.paragraphs[0].links = vec![Link {
            start: 5,
            end: 9,
            target: EntityRef::Location(viejo),
        }];
        p.paragraphs[1].links = vec![Link {
            start: 5,
            end: 9,
            target: EntityRef::Location(nuevo),
        }];
        p.auto_link_all(); // los dos ya están vinculados
        p.location_mut(nuevo).unwrap().name = "FARO NUEVO".into();
        p.sync_links();
        assert_eq!(p.paragraphs[0].text, "INT. FARO - DÍA");
        assert_eq!(p.paragraphs[1].text, "INT. FARO NUEVO - NOCHE");
        assert_eq!(p.scenes_at(viejo).len(), 1);
        assert_eq!(p.scenes_at(nuevo).len(), 1);
    }

    #[test]
    fn old_manual_locations_and_profiles_are_migrated() {
        let mut p = Project::default();
        p.extra_locations = vec!["TORRE".into()];
        p.location_profiles = vec![LocationProfile {
            name: "FARO".into(),
            one_liner: "Un faro".into(),
            ..Default::default()
        }];
        p.normalize();
        assert!(p.extra_locations.is_empty());
        assert_eq!(p.location_profiles.len(), 2);
        assert!(p.location_profiles.iter().all(|l| l.id != 0));
        assert_eq!(
            p.location(p.location_id_by_name("FARO").unwrap())
                .unwrap()
                .one_liner,
            "Un faro"
        );
    }

    #[test]
    fn heading_location_span_finds_the_place() {
        assert_eq!(
            heading_location_span("INT. CASA DE ANA - NOCHE"),
            Some((5, 16))
        );
        assert_eq!(heading_location_span("ext. plaza"), Some((5, 10)));
        assert_eq!(
            heading_location_span("INT./EXT. AUTO - DÍA"),
            Some((10, 14))
        );
        assert_eq!(heading_location_span("INT. "), None);
        assert_eq!(heading_location_span("INT. FARO -"), Some((5, 9)));
        assert_eq!(heading_location_span(""), None);
    }

    #[test]
    fn removing_a_character_removes_their_lines_only() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Character, "ANA"),
            (Parenthetical, "bajo"),
            (Dialogue, "hola"),
            (Action, "Luis entra."),
            (Character, "LUIS"),
            (Dialogue, "buenas"),
            (Character, "ANA"),
            (Dialogue, "chau"),
        ]);
        let pid = p.paragraphs[3].id;
        p.add_comment(pid, "revisar", None);
        p.auto_link_all(); // crea las fichas y vincula los encabezados
        let ana = p.character_id_by_name("ANA").unwrap();
        assert_eq!(p.character_cues(ana), 2);
        assert_eq!(p.remove_character(ana), 5);
        let kinds: Vec<_> = p.paragraphs.iter().map(|q| q.kind).collect();
        assert_eq!(kinds, vec![SceneHeading, Action, Character, Dialogue]);
        assert!(p.character(ana).is_none() && p.comments.is_empty());
        assert_eq!(p.known_characters(), vec!["LUIS"]);
    }

    #[test]
    fn reset_keeps_only_the_name() {
        let mut p = Project::default();
        let id = p.new_character("ANA");
        p.characters[0].age = "30".into();
        p.reset_character(id);
        assert_eq!(p.characters[0].name, "ANA");
        assert_eq!(p.characters[0].id, id);
        assert!(p.characters[0].age.is_empty());
    }

    fn mention(name: &str, character: bool) -> Mention {
        Mention {
            name: name.into(),
            character,
            target: None,
            detail: String::new(),
        }
    }

    fn category_id(p: &Project, name: &str) -> u64 {
        p.breakdown_categories
            .iter()
            .find(|c| c.name == name)
            .expect("categoría")
            .id
    }

    #[test]
    fn breakdown_collects_scenes_for_every_kind_of_element() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. CASA - DÍA"),
            (Action, "Ana toma la linterna vieja."),
            (SceneHeading, "EXT. CALLE - NOCHE"),
            (Action, "Pasa una camioneta roja."),
            (Character, "ANA"),
            (Dialogue, "hola"),
        ]);
        p.ensure_breakdown_defaults();
        p.add_object("Linterna");
        let vehicles = category_id(&p, "Vehículos");
        let para = p.paragraphs[3].id;
        assert_eq!(
            p.add_breakdown_tag(para, "camioneta roja", vehicles, "Camioneta", false),
            1
        );
        // No se repite la misma marca.
        assert_eq!(
            p.add_breakdown_tag(para, "camioneta roja", vehicles, "camioneta", false),
            0
        );

        let rows = p.breakdown_rows(false);
        let scenes = |cat: &str, name: &str| {
            let c = category_id(&p, cat);
            rows.iter()
                .find(|r| r.category == c && r.name.eq_ignore_ascii_case(name))
                .map(|r| r.scenes.clone())
        };
        assert_eq!(scenes("Vehículos", "Camioneta"), Some(vec![2]));
        assert_eq!(scenes("Objetos clave", "Linterna"), Some(vec![1])); // por mención en el texto
        assert_eq!(scenes("Elenco", "ANA"), Some(vec![1, 2])); // aparece en una acción y habla en la escena 2
        assert_eq!(scenes("Locaciones", "CASA"), Some(vec![1]));
        assert_eq!(scenes("Locaciones", "CALLE"), Some(vec![2]));
        let scene_two = p.script_scenes()[1].0;
        assert!(
            p.scene_sheet_items(scene_two)
                .iter()
                .any(|i| i.name == "Camioneta"),
            "la hoja de la escena 2 lleva la camioneta"
        );
    }

    #[test]
    fn marking_in_an_automatic_category_creates_the_project_element() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Action, "Sobre la mesa hay un Reloj. El reloj suena."),
        ]);
        p.ensure_breakdown_defaults();
        let props = category_id(&p, "Objetos clave");
        let para = p.paragraphs[1].id;
        assert_eq!(p.add_breakdown_tag(para, "Reloj", props, "Reloj", true), 1);
        assert!(p.objects.iter().any(|o| o.name == "Reloj"));
        // «Marcar todas las menciones»: no distingue mayúsculas, pero cada párrafo cuenta una vez.
        assert_eq!(p.breakdown_tags.len(), 1);
    }

    #[test]
    fn key_objects_and_props_are_different_categories() {
        let p = Project::default();
        let key = category_id(&p, "Objetos clave");
        let props = category_id(&p, "Utilería");
        assert_ne!(key, props);
        let source = |id| {
            p.breakdown_categories
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .source
        };
        assert_eq!(source(key), CategorySource::Objects);
        assert_eq!(source(props), CategorySource::Custom);
    }

    #[test]
    fn old_breakdown_is_migrated_once() {
        let mut p = Project::default();
        // Un proyecto de la primera versión: «Objetos» y sin «Utilería».
        p.breakdown_version = 0;
        p.breakdown_categories.retain(|c| c.name != "Utilería");
        p.breakdown_categories
            .iter_mut()
            .find(|c| c.source == CategorySource::Objects)
            .unwrap()
            .name = "Objetos".into();
        p.normalize();
        let names: Vec<&str> = p
            .breakdown_categories
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        let pos = names
            .iter()
            .position(|n| *n == "Objetos clave")
            .expect("renombrada");
        assert_eq!(names[pos + 1], "Utilería");
        // Si después la borran, no vuelve a aparecer.
        let id = category_id(&p, "Utilería");
        p.remove_breakdown_category(id);
        p.normalize();
        assert!(!p.breakdown_categories.iter().any(|c| c.name == "Utilería"));
    }

    #[test]
    fn removing_a_category_removes_its_items_and_tags() {
        use ParagraphKind::*;
        let mut p = project_with(&[(SceneHeading, "INT. A - DÍA"), (Action, "Una capa roja.")]);
        p.ensure_breakdown_defaults();
        let custom = p.add_breakdown_category("Capas", [1, 2, 3]);
        let para = p.paragraphs[1].id;
        p.add_breakdown_tag(para, "capa roja", custom, "Capa", false);
        assert!(p.remove_breakdown_category(custom));
        assert!(p.breakdown_tags.is_empty() && p.breakdown_items.is_empty());
        // Las automáticas no se borran.
        let cast = category_id(&p, "Elenco");
        assert!(!p.remove_breakdown_category(cast));
    }

    #[test]
    fn mentions_are_displayed_according_to_the_option_without_touching_the_text() {
        let mentions = vec![
            mention("JUAN CARLOS", true),
            mention("CASA DE ANA", false),
            mention("Llave maestra", false),
        ];
        let text = "juan carlos entra en la Casa de Ana con la LLAVE MAESTRA.";
        // Todo en mayúsculas: lo ya escrito cambia al marcar la opción.
        assert_eq!(
            styled_mentions_text(text, &[], &mentions, &CaseOptions::uniform(true)),
            "JUAN CARLOS entra en la CASA DE ANA con la LLAVE MAESTRA."
        );
        // Sin mayúsculas: el personaje es un nombre propio, «Casa de Ana» se respeta y lo que estaba en
        // mayúsculas (huella del otro modo) pasa a minúsculas.
        assert_eq!(
            styled_mentions_text(text, &[], &mentions, &CaseOptions::uniform(false)),
            "Juan Carlos entra en la Casa de Ana con la llave maestra."
        );
        // Palabras completas solamente.
        assert_eq!(
            styled_mentions_text(
                "anais",
                &[],
                &[mention("ANA", true)],
                &CaseOptions::uniform(true)
            ),
            "anais"
        );
    }

    #[test]
    fn case_is_decided_per_category() {
        use ParagraphKind::*;
        let mut p = project_with(&[(
            Action,
            "Ana entra a la casa de ana, ve el lobo y toma la llave.",
        )]);
        p.new_character("ANA");
        p.new_location("CASA DE ANA");
        p.add_object("llave");
        let fauna = p.add_element_category("Fauna", "X");
        p.add_element(fauna, "lobo");
        p.auto_link_all();
        assert_eq!(
            p.paragraphs[0].links.len(),
            4,
            "{:?}",
            p.paragraphs[0].links
        );
        let shown = |p: &Project| {
            styled_mentions_text(
                &p.paragraphs[0].text,
                &p.paragraphs[0].links,
                &p.mention_names(),
                &p.case_options(),
            )
        };
        // Personajes en mayúsculas; locaciones, objetos y la categoría propia, como se escribieron.
        p.name_case = NameCase {
            characters: true,
            locations: false,
            objects: false,
            worlds: false,
        };
        p.element_categories[0].uppercase = false;
        assert_eq!(
            shown(&p),
            "ANA entra a la casa de ana, ve el lobo y toma la llave."
        );
        // Se enciende solo la categoría propia.
        p.element_categories[0].uppercase = true;
        assert_eq!(
            shown(&p),
            "ANA entra a la casa de ana, ve el LOBO y toma la llave."
        );
        // Y ahora todo en mayúsculas menos los personajes.
        p.name_case = NameCase {
            characters: false,
            locations: true,
            objects: true,
            worlds: true,
        };
        assert_eq!(
            shown(&p),
            "Ana entra a la CASA DE ANA, ve el LOBO y toma la LLAVE."
        );
    }

    #[test]
    fn names_inside_dialogue_are_never_forced_to_uppercase() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. CASA - DÍA"),
            (Action, "Ana mira la llave."),
            (Character, "LUIS"),
            (Dialogue, "Hola, ana. ¿Viste la llave?"),
        ]);
        p.add_object("llave");
        p.new_character("ANA");
        p.auto_link_all();
        // Todo en mayúsculas activado en todas las categorías.
        p.name_case = NameCase {
            characters: true,
            locations: true,
            objects: true,
            worlds: true,
        };
        let text = p.to_plain_text();
        // En la acción sí; en el diálogo, como se dice: el personaje con su mayúscula inicial, el objeto tal cual.
        assert!(text.contains("ANA mira la LLAVE."), "{text}");
        assert!(text.contains("Hola, Ana. ¿Viste la llave?"), "{text}");
    }

    #[test]
    fn timeline_grows_with_the_script_and_matches_the_total_duration() {
        use ParagraphKind::*;
        let p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Action, "Una acción de prueba."),
            (Character, "ANA"),
            (Dialogue, "Hola."),
            (
                Action,
                &"Otra acción bastante más larga para que sume tiempo. ".repeat(20),
            ),
        ]);
        let (starts, total) = p.timeline();
        assert_eq!(starts.len(), p.paragraphs.len());
        assert_eq!(starts[0], 0.0);
        assert!(starts.windows(2).all(|w| w[1] > w[0]));
        assert!(total > *starts.last().unwrap());
        assert!((total - p.stats().seconds).abs() < 0.01);
    }

    #[test]
    fn deleting_a_range_across_paragraphs_joins_what_is_left() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (Action, "uno dos tres"),
            (Character, "ANA"),
            (Dialogue, "hola a todos"),
            (Action, "cuatro cinco"),
        ]);
        let ids: Vec<u64> = p.paragraphs.iter().map(|q| q.id).collect();
        p.add_comment(ids[2], "revisar", None);
        // Un vínculo en lo que sobrevive del último párrafo («cinco» = 7..12).
        p.paragraphs[3].links = vec![Link {
            start: 7,
            end: 12,
            target: EntityRef::Object(1),
        }];

        // Desde «uno |dos tres» hasta «cuatro| cinco» (en cualquier orden).
        assert_eq!(
            p.range_text((ids[0], 4), (ids[3], 6)),
            "dos tres\nANA\nhola a todos\ncuatro"
        );
        assert_eq!(
            p.range_text((ids[3], 6), (ids[0], 4)),
            "dos tres\nANA\nhola a todos\ncuatro"
        );
        let cursor = p.delete_range((ids[3], 6), (ids[0], 4));
        assert_eq!(cursor, (ids[0], 4));
        assert_eq!(p.paragraphs.len(), 1);
        assert_eq!(p.paragraphs[0].text, "uno  cinco");
        assert_eq!(p.paragraphs[0].kind, Action);
        // El vínculo sigue sobre «cinco» (ahora en 5..10) y el comentario pasó al párrafo que queda.
        assert_eq!(
            (p.paragraphs[0].links[0].start, p.paragraphs[0].links[0].end),
            (5, 10)
        );
        assert_eq!(p.comments[0].paragraph, ids[0]);

        // Dentro de un solo párrafo.
        let mut q = project_with(&[(Action, "abcdef")]);
        let id = q.paragraphs[0].id;
        assert_eq!(q.delete_range((id, 1), (id, 4)), (id, 1));
        assert_eq!(q.paragraphs[0].text, "aef");
    }

    #[test]
    fn inserting_text_with_line_breaks_opens_new_paragraphs() {
        use ParagraphKind::*;
        let mut p = project_with(&[(Action, "inicio fin")]);
        let id = p.paragraphs[0].id;
        let (last, at) = p.insert_text_at(id, 7, "uno\ndos\ntres");
        let texts: Vec<&str> = p.paragraphs.iter().map(|q| q.text.as_str()).collect();
        assert_eq!(texts, vec!["inicio uno", "dos", "tresfin"]);
        assert_eq!(at, 4); // justo después de «tres»
        assert_eq!(last, p.paragraphs[2].id);
        // Una línea sola se inserta en el mismo párrafo; en un encabezado queda en mayúsculas.
        let mut h = project_with(&[(SceneHeading, "INT. A")]);
        let hid = h.paragraphs[0].id;
        assert_eq!(h.insert_text_at(hid, 6, " - noche"), (hid, 14));
        assert_eq!(h.paragraphs[0].text, "INT. A - NOCHE");
    }

    #[test]
    fn treatment_names_are_linked_and_follow_renames() {
        let mut p = Project::default();
        let ana = p.new_character("ANA");
        p.add_location("Faro");
        p.treatment = "Ana llega al faro de noche. Más tarde, Ana se va.".into();
        p.auto_link_treatment();
        assert_eq!(p.treatment_links.len(), 3); // Ana, faro, Ana
        p.characters.iter_mut().find(|c| c.id == ana).unwrap().name = "MARTA".into();
        p.sync_links();
        assert_eq!(
            p.treatment,
            "Marta llega al faro de noche. Más tarde, Marta se va."
        );
        assert!(p.treatment_mentions(ana));
        // Al eliminar el personaje, sus vínculos desaparecen y el texto queda.
        p.remove_character(ana);
        assert!(!p.treatment_mentions(ana));
        assert!(p.treatment.contains("Marta llega"));
    }

    #[test]
    fn a_character_intro_comes_from_the_sheet_when_it_is_filled_in() {
        let mut p = Project::default();
        let id = p.new_character("ANA");
        assert_eq!(p.character_intro(id), ""); // ficha en blanco
        p.characters[0].age = "32".into();
        assert_eq!(p.character_intro(id), "32 años");
        p.characters[0].one_liner = "bibliotecaria".into();
        assert_eq!(p.character_intro(id), "32 años, bibliotecaria");
        p.characters[0].age = String::new();
        assert_eq!(p.character_intro(id), "bibliotecaria");
        p.characters[0].age = "treintañera".into();
        assert_eq!(p.character_intro(id), "treintañera, bibliotecaria");
    }

    #[test]
    fn what_is_typed_in_the_first_parentheses_fills_the_empty_sheet() {
        let mut p = Project::default();
        let ana = p.new_character("ANA");
        let luis = p.new_character("LUIS");
        p.characters[1].age = "50".into(); // Luis ya tiene la edad: no se pisa
        p.treatment =
            "Ana (32 años, bibliotecaria) conoce a Luis (joven, tímido). Ana (otra cosa) vuelve."
                .into();
        p.auto_link_treatment();
        assert!(p.learn_character_intros());
        let ana_sheet = p.character(ana).unwrap();
        assert_eq!(
            (ana_sheet.age.as_str(), ana_sheet.one_liner.as_str()),
            ("32", "bibliotecaria")
        );
        // Luis: «joven, tímido» no tiene números, así que entero es la descripción; la edad que ya tenía queda.
        let luis_sheet = p.character(luis).unwrap();
        assert_eq!(
            (luis_sheet.age.as_str(), luis_sheet.one_liner.as_str()),
            ("50", "joven, tímido")
        );
        // Solo cuenta la primera mención y lo que ya está lleno no cambia.
        assert!(!p.learn_character_intros());
        assert_eq!(p.character(ana).unwrap().one_liner, "bibliotecaria");
    }

    #[test]
    fn art_sound_and_photo_gear_show_up_in_the_breakdown_even_without_a_title_and_follow_their_rows() {
        let mut p = project_with(&[(ParagraphKind::SceneHeading, "INT. CASA - DÍA")]);
        let scene = p.script_scenes()[0].0;
        let id = p.add_role_entry(Role::Art, Some(scene));
        p.art_items[0].title = "Sillón".into();
        p.art_items[0].status = "Conseguido".into();
        // Sin título, la fila se llama como su tipo.
        let gear = p.add_role_entry(Role::SoundGear, Some(scene));
        let photo = p.add_role_entry(Role::PhotoGear, None);
        assert!(p.sync_equipment());
        assert!(!p.sync_equipment(), "sincronizar de nuevo no cambia nada");
        let rows = p.breakdown_rows(false);
        let seat = rows.iter().find(|r| r.name == "Sillón").expect("el elemento de arte");
        assert_eq!(seat.scenes, vec![1]);
        assert!(rows.iter().any(|r| r.name == "Micrófono"), "el equipo de sonido sin título");
        assert!(rows.iter().any(|r| r.name == "Cámara"), "el equipo de fotografía sin título");
        assert_eq!(p.breakdown_items.len(), 3);
        p.remove_role_entry(Role::Art, id);
        p.remove_role_entry(Role::SoundGear, gear);
        p.remove_role_entry(Role::PhotoGear, photo);
        assert!(p.sync_equipment());
        assert!(p.breakdown_items.is_empty());
    }

    #[test]
    fn plan_things_fill_the_lists_and_a_registered_piece_of_gear_is_one_item_in_every_scene_it_is_used() {
        let mut p = project_with(&[
            (ParagraphKind::SceneHeading, "INT. CASA - DÍA"),
            (ParagraphKind::SceneHeading, "EXT. FARO - NOCHE"),
        ]);
        let scenes: Vec<u64> = p.script_scenes().iter().map(|s| s.0).collect();
        let (b1, b2) = (p.add_blocking(scenes[0]), p.add_blocking(scenes[1]));
        let boom1 = p.add_block_item_with(b1, BlockKind::Mic, 1.0, 1.0, |i| i.mic = Some(MicSpec::new(MicKind::Boom))).unwrap();
        let boom2 = p.add_block_item_with(b2, BlockKind::Mic, 1.0, 1.0, |i| i.mic = Some(MicSpec::new(MicKind::Boom))).unwrap();
        p.sync_equipment();
        // Sin registrar: una fila por escena en Equipo de sonido, un solo elemento en el Desglose con las dos escenas.
        assert_eq!(p.sound_gear.len(), 2);
        let row = p.breakdown_rows(false).into_iter().find(|r| r.name == "Micrófono Boom").unwrap();
        assert_eq!(row.scenes, vec![1, 2]);
        // Registrado como un mismo equipo: una sola fila, usada en las dos escenas.
        let id = p.add_role_entry(Role::SoundGear, None);
        p.sound_gear.iter_mut().find(|e| e.id == id).unwrap().title = "Boom Rode".into();
        for (b, item) in [(b1, boom1), (b2, boom2)] {
            p.blockings.iter_mut().find(|x| x.id == b).unwrap().items.iter_mut().find(|i| i.id == item).unwrap().equipment = Some(id);
        }
        p.sync_equipment();
        assert_eq!(p.sound_gear.len(), 1);
        let row = p.breakdown_rows(false).into_iter().find(|r| r.name == "Boom Rode").unwrap();
        assert_eq!(row.scenes, vec![1, 2]);
        assert!(p.breakdown_rows(false).iter().all(|r| r.name != "Micrófono Boom"));
    }

    #[test]
    fn furniture_and_lights_in_the_plans_are_reflected_in_the_breakdown_and_leave_with_them() {
        let mut p = project_with(&[(ParagraphKind::SceneHeading, "INT. CASA - DÍA")]);
        let scene = p.script_scenes()[0].0;
        let b = p.add_blocking(scene);
        let sofa = p.add_block_item(b, BlockKind::Object, 1.0, 1.0).unwrap();
        p.add_block_item(b, BlockKind::Object, 3.0, 1.0);
        for blocking in p.blockings.iter_mut() {
            for item in blocking.items.iter_mut() {
                if item.id == sofa {
                    item.shape = ObjectShape::Sofa;
                }
            }
        }
        assert!(p.sync_equipment());
        assert!(!p.sync_equipment(), "sin cambios no toca nada");
        let rows = p.breakdown_rows(false);
        let row = rows.iter().find(|r| r.name == "Sofá").expect("el sofá figura en el Desglose");
        assert_eq!(row.scenes, vec![1]);
        p.remove_block_item(b, sofa);
        assert!(p.sync_equipment());
        assert!(p.breakdown_items.iter().all(|i| i.name != "Sofá"));
        assert!(p.art_items.iter().all(|e| e.title != "Sofá"), "la fila de la planta también se va");
    }

    #[test]
    fn role_rows_use_the_breakdown_categories_that_already_exist_and_leave_no_duplicates() {
        let mut p = project_with(&[(ParagraphKind::SceneHeading, "INT. CASA - DÍA")]);
        p.ensure_breakdown_defaults();
        let before = p.breakdown_categories.len();
        for (role, kind, title) in [
            (Role::Art, "Maquillaje", "Sangre"),
            (Role::Art, "Escenografía", "Sillón"),
            (Role::SoundGear, "Boom", "Boom Rode"),
            (Role::PhotoGear, "Luz", "Fresnel"),
        ] {
            let id = p.add_role_entry(role, None);
            let e = p.role_entries_mut(role).iter_mut().find(|e| e.id == id).unwrap();
            (e.kind, e.title) = (kind.to_string(), title.to_string());
        }
        // Una categoría suelta que se creó antes con otro nombre y quedó vacía se limpia.
        p.add_breakdown_category("Maquillaje", [1, 2, 3]);
        p.sync_equipment();
        let name_of = |p: &Project, item: &str| {
            let i = p.breakdown_items.iter().find(|i| i.name == item).unwrap();
            p.breakdown_categories.iter().find(|c| c.id == i.category).unwrap().name.clone()
        };
        assert_eq!(name_of(&p, "Sangre"), "Maquillaje y peluquería");
        assert_eq!(name_of(&p, "Sillón"), "Arte y decorado");
        assert_eq!(name_of(&p, "Boom Rode"), "Sonido y música");
        assert_eq!(name_of(&p, "Fresnel"), "Equipo técnico");
        assert_eq!(p.breakdown_categories.len(), before, "no se crearon categorías nuevas");
    }

    #[test]
    fn one_armchair_in_many_scenes_is_one_art_element_and_a_row_can_link_to_a_key_object_or_a_category() {
        let mut p = project_with(&[
            (ParagraphKind::SceneHeading, "INT. CASA - DÍA"),
            (ParagraphKind::SceneHeading, "INT. CASA - NOCHE"),
        ]);
        p.ensure_breakdown_defaults();
        let scenes: Vec<u64> = p.script_scenes().iter().map(|s| s.0).collect();
        let (b1, b2) = (p.add_blocking(scenes[0]), p.add_blocking(scenes[1]));
        let chair = |p: &mut Project, b| {
            p.add_block_item_with(b, BlockKind::Object, 1.0, 1.0, |i| i.shape = ObjectShape::Armchair).unwrap()
        };
        let (c1, c2) = (chair(&mut p, b1), chair(&mut p, b2));
        p.sync_equipment();
        assert_eq!(p.art_items.len(), 2, "sin registrar: una fila por escena");
        // Registrado como un mismo elemento único.
        let id = p.add_role_entry(Role::Art, None);
        p.art_items.iter_mut().find(|e| e.id == id).unwrap().title = "Sillón de la abuela".into();
        for (b, c) in [(b1, c1), (b2, c2)] {
            p.blockings.iter_mut().find(|x| x.id == b).unwrap().items.iter_mut().find(|i| i.id == c).unwrap().equipment = Some(id);
        }
        p.sync_equipment();
        assert_eq!(p.art_items.len(), 1);
        let rows = p.breakdown_rows(false);
        let row = rows.iter().find(|r| r.name == "Sillón de la abuela").unwrap();
        assert_eq!(row.scenes, vec![1, 2]);
        // Vinculado a una categoría propia.
        let mine = p.add_breakdown_category("Muebles de época", [9, 9, 9]);
        p.art_items[0].link = Some(RoleLink::Category(mine));
        p.sync_equipment();
        let item = p.breakdown_items.iter().find(|i| i.name == "Sillón de la abuela").unwrap();
        assert_eq!(item.category, mine);
        // Vinculado a un objeto clave: comparte su fila.
        let key = p.add_object("llave");
        p.art_items[0].link = Some(RoleLink::Object(key));
        p.sync_equipment();
        let rows = p.breakdown_rows(false);
        assert!(rows.iter().all(|r| r.name != "Sillón de la abuela"), "ya no es una fila aparte");
        let row = rows.iter().find(|r| r.entity == Some(EntityRef::Object(key))).unwrap();
        assert_eq!(row.scenes, vec![1, 2]);
        assert_eq!(rows.iter().filter(|r| r.entity == Some(EntityRef::Object(key))).count(), 1);
    }

    #[test]
    fn each_person_numbers_what_they_create_in_their_own_block() {
        let mut a = Project::default();
        let mut b = Project::default();
        a.use_id_block(3);
        b.use_id_block(7);
        let (x, y) = (a.add_role_entry(Role::Art, None), b.add_role_entry(Role::Art, None));
        assert_ne!(x, y);
        assert_eq!(x / ID_BLOCK, 3);
        assert_eq!(y / ID_BLOCK, 7);
        // Al volver a abrir, sigue desde el último id de su tramo.
        a.use_id_block(3);
        assert_eq!(a.add_role_entry(Role::Art, None), x + 1);
    }

    #[test]
    fn image_paths_pointing_outside_the_project_folder_are_dropped() {
        let mut p = Project::default();
        p.cover = Some(PathBuf::from("/home/victima/.ssh/id_rsa.png"));
        let mut doc = Document {
            id: 1,
            kind: DocKind::Gallery,
            name: String::new(),
            parent: None,
            text: String::new(),
            description: String::new(),
            fields: BTreeMap::new(),
            nodes: Vec::new(),
            images: vec![PathBuf::from("recursos/ok.png"), PathBuf::from("../../secreto.png")],
            cards: Vec::new(),
            rel_nodes: Vec::new(),
            rel_edges: Vec::new(),
        };
        doc.images.push(PathBuf::from("recursos/../../x.png"));
        p.documents.push(doc);
        assert_eq!(p.drop_foreign_image_paths(), 3);
        assert_eq!(p.cover, Some(PathBuf::new()));
        assert_eq!(p.documents[0].images[0], PathBuf::from("recursos/ok.png"));
        assert_eq!(p.drop_foreign_image_paths(), 0);
    }

    #[test]
    fn the_basic_drama_curve_has_two_turning_points_and_a_climax_and_takes_custom_points() {
        let mut p = Project::default();
        assert!(p.ensure_drama());
        assert!(!p.ensure_drama());
        let turns = p.drama.iter().filter(|d| matches!(d.kind, DramaKind::TurnOne | DramaKind::TurnTwo)).count();
        let climax = p.drama.iter().filter(|d| d.kind == DramaKind::Climax).count();
        assert_eq!((turns, climax), (2, 1));
        let before = p.drama.len();
        p.add_drama_point(DramaKind::Valley);
        p.add_drama_point(DramaKind::Peak);
        assert_eq!(p.drama.len(), before + 2);
    }

    #[test]
    fn each_role_keeps_its_own_entries_and_a_boom_operator_can_be_animated() {
        let mut p = Project::default();
        let a = p.add_role_entry(Role::Sound, None);
        let b = p.add_role_entry(Role::Art, None);
        assert_eq!((p.sound_cues.len(), p.art_items.len()), (1, 1));
        assert_eq!(p.sound_cues[0].kind, "Diálogo");
        assert_eq!(p.art_items[0].status, "Pendiente");
        assert_ne!(a, b);

        let mut boom = actor(1, 4.0, 4.0, 0.0);
        boom.kind = BlockKind::Mic;
        boom.mic = Some(MicSpec::new(MicKind::Boom));
        boom.set_operator_at(0.0, (1.0, 1.0));
        boom.set_operator_at(4.0, (5.0, 1.0));
        assert_eq!(boom.operator_at(0.0), Some((1.0, 1.0)));
        assert_eq!(boom.operator_at(2.0), Some((3.0, 1.0)));
        assert_eq!(boom.operator_at(9.0), Some((5.0, 1.0)));
    }

    #[test]
    fn two_scenes_with_the_same_heading_stay_two_scenes_in_the_outline() {
        let mut p = Project::default();
        p.treatment = "INT. LIVING - NOCHE\nuno\nEXT. CALLE - DÍA\ndos\nINT. LIVING - NOCHE\ntres".into();
        p.add_outline_scene_at("INT. LIVING - NOCHE", 0);
        p.add_outline_scene_at("EXT. CALLE - DÍA", 24);
        p.add_outline_scene_at("INT. LIVING - NOCHE", 45);
        p.sort_outline_by_text();
        assert_eq!(p.outline.len(), 3);
        assert_eq!(p.outline_positions(), vec![Some(0), Some(24), Some(45)]);
        assert_eq!(p.outline_blocks().len(), 3);
    }

    #[test]
    fn outline_scenes_are_imported_into_the_script_once() {
        use ParagraphKind::*;
        let mut p = Project::default(); // guion en blanco: un encabezado vacío
        p.treatment = "INT. CASA - NOCHE\nAna llega a casa.\nEXT. FARO - DÍA\nSube al faro.\nINT. FARO - NOCHE".into();
        p.add_outline_scene("int. casa - noche");
        p.add_outline_scene("EXT. FARO - DÍA");

        assert_eq!(p.import_outline_into_script(), 2);
        let kinds: Vec<_> = p.paragraphs.iter().map(|q| q.kind).collect();
        assert_eq!(kinds, vec![SceneHeading, SceneHeading]); // el vacío se reutilizó
        assert_eq!(p.paragraphs[0].text, "INT. CASA - NOCHE");
        assert_eq!(p.paragraphs[1].text, "EXT. FARO - DÍA");
        // Las locaciones de los encabezados quedan creadas y vinculadas.
        assert_eq!(p.location_profiles.len(), 2);
        assert_eq!(p.paragraphs[1].links.len(), 1);

        // Importar otra vez no repite; una escena nueva sí se agrega al final.
        assert_eq!(p.import_outline_into_script(), 0);
        p.add_outline_scene("INT. FARO - NOCHE");
        assert_eq!(p.import_outline_into_script(), 1);
        assert_eq!(p.paragraphs.last().unwrap().text, "INT. FARO - NOCHE");
        // Si se borra un encabezado importado, la escena vuelve a poder importarse.
        let gone = p.paragraphs[1].id;
        p.paragraphs.retain(|q| q.id != gone);
        assert_eq!(p.import_outline_into_script(), 1);
    }

    #[test]
    fn a_scene_lives_while_its_heading_is_in_the_text() {
        let mut p = Project::default();
        let text = "INT. FARO - NOCHE\nAna sube.\nLuego baja.\nINT. CASA - DÍA\nAna duerme.";
        p.treatment = text.into();
        p.add_outline_scene("INT. FARO - NOCHE");
        p.add_outline_scene("INT. CASA - DÍA");
        p.sort_outline_by_text();
        // La escena abarca todo hasta el siguiente encabezado o el final.
        assert_eq!(p.outline_blocks(), vec![(0, 40), (40, 67)]);

        // Editar el encabezado en el texto renombra la escena.
        let edited = text.replace("FARO - NOCHE", "FARO - DÍA");
        p.follow_heading_edits(text, &edited);
        p.treatment = edited.clone();
        p.sort_outline_by_text();
        assert_eq!(p.outline[0].heading, "INT. FARO - DÍA");

        // Borrar el encabezado borra la escena, no el texto.
        let gone = edited.replace("INT. CASA - DÍA\n", "");
        p.follow_heading_edits(&edited, &gone);
        p.treatment = gone.clone();
        p.sort_outline_by_text();
        assert_eq!(p.outline.len(), 1);
        assert!(p.treatment.contains("Ana duerme."));
    }

    #[test]
    fn shot_numbers_are_continuous_unless_the_scenes_restart_them() {
        use ParagraphKind::*;
        let mut p = Project::default();
        p.paragraphs = vec![
            p.make_paragraph(SceneHeading, "INT. CASA - DÍA"),
            p.make_paragraph(SceneHeading, "EXT. FARO - NOCHE"),
        ];
        let (a, b) = (p.paragraphs[0].id, p.paragraphs[1].id);
        let s1 = p.add_shot(a, None);
        let s2 = p.add_shot(a, None);
        let s3 = p.add_shot(b, None);
        // Un plano nuevo en la primera escena va con los de su escena, no al final.
        let s4 = p.add_shot(a, None);
        assert_eq!(
            p.shots.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![s1, s2, s4, s3]
        );
        let n = p.shot_numbers();
        assert_eq!([n[&s1], n[&s2], n[&s4], n[&s3]], [1, 2, 3, 4]);
        p.shots_restart = true;
        let n = p.shot_numbers();
        assert_eq!([n[&s1], n[&s2], n[&s4], n[&s3]], [1, 2, 3, 1]);

        // Subir y bajar solo cambia el orden dentro de la escena.
        assert!(p.move_shot(s4, -1));
        assert_eq!(
            p.shots.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![s1, s4, s2, s3]
        );
        assert!(!p.move_shot(s3, 1));
        assert!(!p.move_shot(s1, -1));
        // Un plano de una escena que ya no existe sigue numerado, al final.
        p.paragraphs.retain(|q| q.id != b);
        p.shots_restart = false;
        assert_eq!(p.shot_numbers()[&s3], 4);
    }

    #[test]
    fn shot_columns_can_be_added_moved_and_removed_and_full_plane_names_become_abbreviations() {
        let mut p = Project::default();
        p.ensure_shot_columns();
        assert_eq!(p.shot_columns.len(), 10);
        assert_eq!(p.shot_columns[0].kind, ColumnKind::Scene);
        let lens = p.add_shot_column(" Lente ");
        assert_eq!(p.shot_columns.last().unwrap().name, "Lente");
        let scene = p.paragraphs[0].id;
        let shot = p.add_shot(scene, None);
        p.shots[0].extra.insert(lens, "35 mm".into());
        assert_eq!(p.column_values(lens), 1);
        // Subir la columna nueva por encima de Storyboard; no se puede salir de la lista.
        assert!(p.move_shot_column(lens, -1));
        assert_eq!(p.shot_columns[9].id, lens);
        assert!(!p.move_shot_column(p.shot_columns[0].id, -1));
        // Las de siempre no se quitan; las propias sí, con su contenido.
        assert!(!p.remove_shot_column(p.shot_columns[0].id));
        assert!(p.remove_shot_column(lens));
        assert!(p.shots[0].extra.is_empty());
        let _ = shot;

        p.shots[0].size = "plano general".into();
        p.normalize();
        assert_eq!(p.shots[0].size, "PG");
    }

    #[test]
    fn title_case_keeps_small_words_lowercase() {
        let chars: Vec<char> = "MARÍA DE LA CRUZ".chars().collect();
        assert_eq!(apply_case(&chars, CaseRule::Title), "María de la Cruz");
    }

    #[test]
    fn element_categories_hold_their_own_elements() {
        let mut p = Project::default();
        let fauna = p.add_element_category("Fauna", "X");
        let ships = p.add_element_category("Naves", "Y");
        let wolf = p.add_element(fauna, "Lobo").unwrap();
        let wolf2 = p.add_element(fauna, "lobo").unwrap();
        p.add_element(ships, "Lobo").unwrap(); // en otra categoría el nombre se puede repetir
        assert_eq!(
            p.elements.iter().find(|e| e.id == wolf2).unwrap().name,
            "lobo 2"
        );
        p.elements.iter_mut().find(|e| e.id == wolf).unwrap().name = "Lobo gris".into(); // el nombre se edita directo
        assert!(
            p.mention_names()
                .iter()
                .any(|m| m.name == "Lobo gris" && !m.character && m.target.is_some())
        );
        p.remove_element_category(fauna);
        assert_eq!(p.elements.len(), 1);
        assert_eq!(p.add_element(9999, "nada"), None);
    }

    #[test]
    fn category_template_can_be_customised_and_cleans_values() {
        let mut p = Project::default();
        let cat = p.add_element_category("Fauna", "X");
        assert_eq!(
            p.element_categories[0].sections().len(),
            crate::profile::ELEMENT.len()
        );
        let wolf = p.add_element(cat, "Lobo").unwrap();

        p.add_template_section(cat, "Comportamiento");
        let last = p.element_categories[0].sections().len() - 1;
        p.add_template_field(cat, last, "Dieta");
        let key = p.element_categories[0].sections()[last].fields[0]
            .key
            .clone();
        p.elements
            .iter_mut()
            .find(|e| e.id == wolf)
            .unwrap()
            .fields
            .insert(key.clone(), "Carnívoro".into());

        assert_eq!(p.template_usage(cat, last, Some(0)), 1);
        assert_eq!(p.template_usage(cat, last, None), 1);
        p.remove_template_part(cat, last, Some(0));
        assert!(p.elements[0].fields.is_empty());
        assert!(p.element_categories[0].sections()[last].fields.is_empty());

        p.remove_template_part(cat, last, None); // la sección entera
        assert_eq!(
            p.element_categories[0].sections().len(),
            crate::profile::ELEMENT.len()
        );
        p.add_template_section(cat, "Otra");
        p.reset_template(cat);
        assert!(p.element_categories[0].template.is_none());
    }

    #[test]
    fn breakdown_rows_follow_renames_by_id_and_keep_twins_apart() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. FARO - DÍA"),
            (Character, "JUAN"),
            (Dialogue, "Hola"),
            (SceneHeading, "INT. FARO - NOCHE"),
            (Character, "JUAN"),
            (Dialogue, "Chau"),
        ]);
        p.ensure_breakdown_defaults();
        // Dos personajes llamados JUAN y dos locaciones llamadas FARO, cada encabezado apunta a uno.
        let (padre, hijo) = (p.new_character("JUAN"), p.new_character("JUAN"));
        let (viejo, nuevo) = (p.new_location("FARO"), p.new_location("FARO"));
        let link = |target, start, end| vec![Link { start, end, target }];
        p.paragraphs[0].links = link(EntityRef::Location(viejo), 5, 9);
        p.paragraphs[1].links = link(EntityRef::Character(padre), 0, 4);
        p.paragraphs[3].links = link(EntityRef::Location(nuevo), 5, 9);
        p.paragraphs[4].links = link(EntityRef::Character(hijo), 0, 4);
        p.characters
            .iter_mut()
            .find(|c| c.id == padre)
            .unwrap()
            .one_liner = "el padre".into();
        p.characters
            .iter_mut()
            .find(|c| c.id == hijo)
            .unwrap()
            .one_liner = "el hijo".into();

        // El desglose guarda datos del hijo (estado y cantidad), ligados a su id.
        let cast = p
            .breakdown_categories
            .iter()
            .find(|c| c.source == CategorySource::Characters)
            .unwrap()
            .id;
        let row_of = |p: &Project, id: u64| {
            p.breakdown_rows(false)
                .into_iter()
                .find(|r| r.entity == Some(EntityRef::Character(id)))
                .unwrap()
        };
        let row = row_of(&p, hijo);
        let idx = p.breakdown_item_for_row(&row).unwrap();
        p.breakdown_items[idx].status = Acquisition::Have;
        p.breakdown_items[idx].quantity = 3;

        // Las dos filas existen por separado, con sus escenas, aunque se llamen igual.
        let (rp, rh) = (row_of(&p, padre), row_of(&p, hijo));
        assert_eq!((rp.scenes.clone(), rh.scenes.clone()), (vec![1], vec![2]));
        assert!(
            rp.name.contains("el padre") && rh.name.contains("el hijo"),
            "{} / {}",
            rp.name,
            rh.name
        );
        assert!(rp.item.is_none() && rh.item.is_some());

        // Renombrar al hijo no pierde sus datos ni toca al padre: sigue siendo la misma fila por id.
        p.characters.iter_mut().find(|c| c.id == hijo).unwrap().name = "JUANITO".into();
        p.sync_links();
        let rh = row_of(&p, hijo);
        assert!(rh.name.starts_with("JUANITO"), "{}", rh.name);
        let item = &p.breakdown_items[rh.item.unwrap()];
        assert_eq!((item.status, item.quantity), (Acquisition::Have, 3));
        assert!(row_of(&p, padre).item.is_none());
        let _ = cast;

        // Las dos locaciones también son filas distintas, cada una con su escena.
        let lc = p
            .breakdown_categories
            .iter()
            .find(|c| c.source == CategorySource::Locations)
            .unwrap()
            .id;
        let places: Vec<BreakdownRow> = p
            .breakdown_rows(false)
            .into_iter()
            .filter(|r| r.category == lc)
            .collect();
        assert_eq!(places.len(), 2);
        assert!(places.iter().all(|r| r.scenes.len() == 1));

        // Al eliminar un personaje, su fila guardada desaparece.
        p.remove_character(hijo);
        assert!(
            p.breakdown_items
                .iter()
                .all(|i| i.entity != Some(EntityRef::Character(hijo)))
        );
    }

    #[test]
    fn old_breakdown_items_are_linked_to_their_entities_on_load() {
        use ParagraphKind::*;
        let mut p = project_with(&[(SceneHeading, "INT. FARO - DÍA"), (Character, "ANA")]);
        p.ensure_breakdown_defaults();
        let cc = p
            .breakdown_categories
            .iter()
            .find(|c| c.source == CategorySource::Characters)
            .unwrap()
            .id;
        let custom = p
            .breakdown_categories
            .iter()
            .find(|c| c.name == "Vehículos")
            .unwrap()
            .id;
        // Formato anterior: elementos guardados solo con nombre.
        let item = |id, category, name: &str| BreakdownItem {
            id,
            category,
            name: name.into(),
            status: Acquisition::Rent,
            quantity: 2,
            notes: String::new(),
            entity: None,
            cost_cents: None,
            responsible: None,
            unit_cost: false,
            extra_scenes: Vec::new(),
        };
        p.breakdown_items = vec![item(900, cc, "ANA"), item(901, custom, "Camioneta")];
        p.normalize();
        let ana = p.character_id_by_name("ANA").unwrap();
        assert_eq!(p.breakdown_items[0].entity, Some(EntityRef::Character(ana)));
        assert_eq!(p.breakdown_items[1].entity, None);
        // Los datos guardados se conservan.
        let row = p
            .breakdown_rows(false)
            .into_iter()
            .find(|r| r.entity == Some(EntityRef::Character(ana)))
            .unwrap();
        assert_eq!(p.breakdown_items[row.item.unwrap()].quantity, 2);
    }

    #[test]
    fn renaming_follows_every_link_and_keeps_twins_apart() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Character, "JUAN"),
            (Dialogue, "Hola"),
            (Action, "Pedro mira a Juan y a la llave."),
            (Character, "JUAN"),
            (Dialogue, "Chau"),
        ]);
        // Dos personajes se llaman JUAN: cada encabezado apunta a uno distinto.
        let father = p.new_character("JUAN");
        let son = p.new_character("JUAN");
        p.paragraphs[1].links = vec![Link {
            start: 0,
            end: 4,
            target: EntityRef::Character(father),
        }];
        p.paragraphs[4].links = vec![Link {
            start: 0,
            end: 4,
            target: EntityRef::Character(son),
        }];
        // «Juan» dentro de la acción es ambiguo: no se vincula solo.
        p.auto_link_all();
        assert!(p.paragraphs[3].links.is_empty());
        // La persona elige a cuál se refiere.
        p.paragraphs[3].links = vec![Link {
            start: 13,
            end: 17,
            target: EntityRef::Character(son),
        }];

        p.characters.iter_mut().find(|c| c.id == son).unwrap().name = "JUANITO".into();
        p.sync_links();
        assert_eq!(p.paragraphs[1].text, "JUAN"); // el otro Juan no se toca
        assert_eq!(p.paragraphs[4].text, "JUANITO");
        assert_eq!(p.paragraphs[3].text, "Pedro mira a Juanito y a la llave.");
        assert_eq!(
            (p.paragraphs[3].links[0].start, p.paragraphs[3].links[0].end),
            (13, 20)
        );

        // Y al volver a renombrarlo se sigue cumpliendo, aunque el nombre sea más corto.
        p.characters.iter_mut().find(|c| c.id == son).unwrap().name = "JU".into();
        p.sync_links();
        assert_eq!(p.paragraphs[3].text, "Pedro mira a Ju y a la llave.");
    }

    #[test]
    fn suffixes_like_vo_are_kept_when_renaming_a_cue() {
        use ParagraphKind::*;
        let mut p = project_with(&[(Character, "ANA (V.O.)"), (Dialogue, "hola")]);
        p.auto_link_all();
        let id = p.character_id_by_name("ANA").unwrap();
        p.characters.iter_mut().find(|c| c.id == id).unwrap().name = "MARTA".into();
        p.sync_links();
        assert_eq!(p.paragraphs[0].text, "MARTA (V.O.)");
    }

    #[test]
    fn names_are_linked_automatically_only_when_unique() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (Character, "ANA"),
            (Dialogue, "Hola"),
            (Action, "Ana toma la linterna de Ana."),
            (Action, "Luis mira a Ana."),
        ]);
        let lamp = p.add_object("linterna"); // crea y vincula lo ya escrito
        assert_eq!(p.characters.len(), 1); // el encabezado creó la ficha
        assert_eq!(p.paragraphs[0].links.len(), 1);
        let ana = p.character_id_by_name("ANA").unwrap();
        assert_eq!(p.paragraphs[2].links.len(), 3); // Ana, linterna, Ana
        assert!(
            p.paragraphs[2]
                .links
                .iter()
                .any(|l| l.target == EntityRef::Object(lamp))
        );
        assert!(
            p.paragraphs[3]
                .links
                .iter()
                .all(|l| l.target == EntityRef::Character(ana))
        );

        // Con un segundo ANA, los nombres nuevos ya no se vinculan solos.
        p.new_character("ANA");
        p.paragraphs[3].links.clear();
        p.auto_link_paragraph(3);
        assert!(p.paragraphs[3].links.is_empty());
    }

    #[test]
    fn links_follow_edits_and_are_dropped_when_they_stop_being_a_word() {
        let target = EntityRef::Character(1);
        let mut links = vec![Link {
            start: 5,
            end: 9,
            target,
        }];
        adjust_links(&mut links, "Hola Juan y", "Hola Juan y todos"); // agregar después
        assert_eq!((links[0].start, links[0].end), (5, 9));
        adjust_links(&mut links, "Hola Juan y todos", "Oye. Hola Juan y todos"); // agregar antes
        assert_eq!((links[0].start, links[0].end), (10, 14));
        adjust_links(
            &mut links,
            "Oye. Hola Juan y todos",
            "Oye. Hola Juan, y todos",
        ); // puntuación pegada: sigue
        assert_eq!(links.len(), 1);
        adjust_links(
            &mut links,
            "Oye. Hola Juan, y todos",
            "Oye. Hola Juanito, y todos",
        ); // la palabra cambió
        assert!(links.is_empty());

        let mut links = vec![Link {
            start: 5,
            end: 9,
            target,
        }];
        adjust_links(&mut links, "Hola Juan", "Hola Jian"); // se tocó por dentro
        assert!(links.is_empty());
    }

    #[test]
    fn splitting_and_merging_keep_the_links() {
        let t = EntityRef::Object(3);
        let links = vec![
            Link {
                start: 0,
                end: 3,
                target: t,
            },
            Link {
                start: 10,
                end: 14,
                target: t,
            },
        ];
        let (head, tail) = split_links(&links, 6, 1); // «abc defg|  hijk lmno»
        assert_eq!(
            head,
            vec![Link {
                start: 0,
                end: 3,
                target: t
            }]
        );
        assert_eq!(
            tail,
            vec![Link {
                start: 3,
                end: 7,
                target: t
            }]
        );
        assert_eq!(shifted_links(&tail, 6)[0].start, 9);
        // Un vínculo que cruza el corte se pierde.
        assert!(
            split_links(
                &[Link {
                    start: 4,
                    end: 8,
                    target: t
                }],
                6,
                0
            )
            .0
            .is_empty()
        );
    }

    #[test]
    fn scenes_split_on_headings() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Action, "Uno dos tres."),
            (SceneHeading, "EXT. B - NOCHE"),
            (Action, "Cuatro."),
        ]);
        let scenes = p.scenes();
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].snippet, "Uno dos tres.");
        assert_eq!(scenes[1].number, 2);
        p.auto_link_all();
        assert_eq!(p.locations().len(), 2);
    }

    #[test]
    fn activity_counts_dialogue_words() {
        use ParagraphKind::*;
        let p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Character, "ANA"),
            (Dialogue, "hola que tal"),
            (Character, "LUIS"),
            (Dialogue, "bien"),
        ]);
        let a = p.character_activity();
        assert_eq!(a[0].name, "ANA");
        assert_eq!(a[0].words, 3);
        assert_eq!(a[0].scenes, 1);
    }

    #[test]
    fn pasted_script_text_keeps_its_structure() {
        use ParagraphKind::*;
        let text = "1. int casa de ana - noche\r\n\r\n   Ana  espera junto a la ventana.\r\n\r\n\r\nANA (V.O.)\r\n(bajito)\r\nNo vino nadie.\r\nTodavía.\r\n\r\nSE ESCUCHA UN DISPARO.\r\n\r\nCORTE A:\r\n\r\nEXT. CALLE - DÍA\r\nJuan corre.";
        let kinds: Vec<_> = classify_pasted(text)
            .into_iter()
            .map(|(k, t)| (k, t))
            .collect();
        let expected = vec![
            (SceneHeading, "INT. CASA DE ANA - NOCHE"),
            (Action, "Ana espera junto a la ventana."),
            (Character, "ANA (V.O.)"),
            (Parenthetical, "bajito"),
            (Dialogue, "No vino nadie."),
            (Dialogue, "Todavía."),
            (Action, "SE ESCUCHA UN DISPARO."),
            (Transition, "CORTE A:"),
            (SceneHeading, "EXT. CALLE - DÍA"),
            (Action, "Juan corre."),
        ];
        assert_eq!(
            kinds,
            expected
                .into_iter()
                .map(|(k, t)| (k, t.to_string()))
                .collect::<Vec<_>>()
        );
        // Un nombre en mayúsculas sin nada debajo no es un personaje.
        assert_eq!(classify_pasted("FIN"), vec![(Action, "FIN".to_string())]);

        // Pegado en un párrafo vacío: toma el tipo de la primera línea y no deja párrafos en blanco.
        let mut p = project_with(&[(Action, "")]);
        let id = p.paragraphs[0].id;
        let (last, at) = p.paste_text_at(id, 0, "int. casa - día\n\nJUAN\nHola.");
        assert_eq!(
            p.paragraphs.iter().map(|q| q.kind).collect::<Vec<_>>(),
            vec![SceneHeading, Character, Dialogue]
        );
        assert_eq!(p.paragraphs[0].text, "INT. CASA - DÍA");
        assert_eq!((last, at), (p.paragraphs[2].id, 5));
        // En medio de una acción, la primera línea se une a lo que había y lo de después queda al final.
        let mut q = project_with(&[(Action, "Abre la puerta.")]);
        let id = q.paragraphs[0].id;
        q.paste_text_at(id, 5, "uno\ndos");
        assert_eq!(
            q.paragraphs
                .iter()
                .map(|x| x.text.clone())
                .collect::<Vec<_>>(),
            vec!["Abre uno", "dosla puerta."]
        );
    }

    #[test]
    fn notes_and_act_titles_take_no_screen_time_and_are_left_out_of_what_is_filmed() {
        use ParagraphKind::*;
        let base = project_with(&[(SceneHeading, "INT. A - DÍA"), (Action, "Entra Ana.")]);
        let with = project_with(&[
            (Act, "ACTO I"),
            (Sequence, "EL REGRESO"),
            (SceneHeading, "INT. A - DÍA"),
            (Note, "Revisar el ritmo de esta escena."),
            (Action, "Entra Ana."),
        ]);
        assert!((base.stats().seconds - with.stats().seconds).abs() < 0.01);
        assert!(
            Lyrics.is_filmed() && !Note.is_filmed() && !Act.is_filmed() && !Sequence.is_filmed()
        );
        // El texto exportado marca cada formato.
        let lyrics = project_with(&[(Lyrics, "La la la"), (Note, "ojo")]);
        let text = lyrics.to_plain_text();
        assert!(
            text.contains("~La la la") && text.contains("[[ojo]]"),
            "{text}"
        );
    }

    #[test]
    fn locked_scene_numbers_hold_and_new_scenes_take_the_letter_of_their_place() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Action, "uno"),
            (SceneHeading, "INT. B - DÍA"),
            (SceneHeading, "INT. C - DÍA"),
        ]);
        let labels = |p: &Project| {
            p.scene_labels()
                .into_iter()
                .map(|(_, n)| n)
                .collect::<Vec<_>>()
        };
        // Sin bloquear, el número es la posición y cambia al agregar.
        assert_eq!(labels(&p), ["1", "2", "3"]);
        p.lock_scene_numbers();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        assert_eq!(p.provisional_scene_count(), 0);

        // Una escena nueva entre la 2 y la 3 es la 2A.
        let first = p.make_paragraph(SceneHeading, "INT. D - NOCHE");
        let first_id = first.id;
        p.paragraphs.insert(3, first);
        p.sync_scene_numbers();
        assert_eq!(labels(&p), ["1", "2", "2A", "3"]);
        assert_eq!(p.provisional_scene_count(), 1);

        // Otra entre la 2 y la 2A toma la 2A por su lugar, y la que ya estaba pasa a ser la 2B.
        let second = p.make_paragraph(SceneHeading, "INT. E - NOCHE");
        let second_id = second.id;
        p.paragraphs.insert(3, second);
        p.sync_scene_numbers();
        assert_eq!(labels(&p), ["1", "2", "2A", "2B", "3"]);
        let number = |p: &Project, id: u64| p.scenes[&id].number.clone();
        assert_eq!(
            (number(&p, second_id), number(&p, first_id)),
            ("2A".to_string(), "2B".to_string())
        );

        // Antes de la primera: A1.
        let before = p.make_paragraph(SceneHeading, "INT. F - DÍA");
        p.paragraphs.insert(0, before);
        p.sync_scene_numbers();
        assert_eq!(labels(&p), ["A1", "1", "2", "2A", "2B", "3"]);

        // Una escena provisoria que se borra no deja rastro; una fija que se borra queda omitida y no renumera.
        p.paragraphs.retain(|q| q.id != first_id);
        p.sync_scene_numbers();
        assert_eq!(labels(&p), ["A1", "1", "2", "2A", "3"]);
        assert!(p.omitted_scene_numbers().is_empty());
        let gone = ids[1];
        p.paragraphs.retain(|q| q.id != gone);
        p.sync_scene_numbers();
        // La 2A era provisoria: con la 2 fuera, por su lugar pasa a ser la 1A.
        assert_eq!(labels(&p), ["A1", "1", "1A", "3"]);
        assert_eq!(p.omitted_scene_numbers(), ["2"]);
        let again = p.make_paragraph(SceneHeading, "INT. G - DÍA");
        p.paragraphs.push(again);
        p.sync_scene_numbers();
        assert_eq!(labels(&p).last().map(String::as_str), Some("3A"));

        // Fijar las letras: la 1A ya no cambia. Si se mete otra escena entre la 1 y la 1A no cabe ninguna letra
        // sencilla: se usa la doble.
        p.fix_scene_numbers();
        assert_eq!(p.provisional_scene_count(), 0);
        let squeezed = p.make_paragraph(SceneHeading, "INT. H - DÍA");
        let scenes = p.script_scenes();
        let at = p
            .paragraphs
            .iter()
            .position(|q| q.id == scenes[2].0)
            .unwrap();
        p.paragraphs.insert(at, squeezed);
        p.sync_scene_numbers();
        assert_eq!(labels(&p), ["A1", "1", "1AA", "1A", "3", "3A"]);

        // Desbloquear vuelve a la posición y se olvidan las omitidas.
        p.unlock_scene_numbers();
        assert_eq!(
            labels(&p),
            (1..=p.script_scenes().len())
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
        );
        assert!(p.omitted_scene_numbers().is_empty());
    }

    #[test]
    fn scene_numbers_grow_letter_by_letter_and_sort_like_the_script() {
        assert_eq!(next_scene_number(None), "A1");
        assert_eq!(next_scene_number(Some("12")), "12A");
        assert_eq!(next_scene_number(Some("12A")), "12B");
        assert_eq!(next_scene_number(Some("12Z")), "12AA");
        assert_eq!(next_scene_number(Some("A1")), "B1");
        let mut numbers = vec!["13", "12A", "1", "A1", "12", "12B"];
        numbers.sort_by_key(|n| scene_number_key(n));
        assert_eq!(numbers, ["A1", "1", "12", "12A", "12B", "13"]);
    }

    #[test]
    fn a_character_taking_the_floor_again_after_an_action_gets_cont() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Character, "ANA"),
            (Dialogue, "Hola."),
            (Action, "Mira la puerta."),
            (Character, "ANA"),
            (Dialogue, "¿Quién es?"),
            (Character, "JUAN"),
            (Dialogue, "Yo."),
            (Character, "ANA"),
            (Character, "ANA (CONT'D)"),
            (SceneHeading, "INT. B - DÍA"),
            (Character, "ANA"),
        ]);
        // Retoma después de una acción; no cuenta responder a otro personaje ni empezar escena nueva.
        assert_eq!(p.cont_marks().iter().filter(|m| **m).count(), 1);
        assert!(p.cont_marks()[4]);
        assert!(p.to_plain_text().contains("ANA (CONT.)"));
        p.auto_cont = false;
        assert!(p.cont_marks().iter().all(|m| !m));
    }

    #[test]
    fn a_revision_marks_what_changed_since_it_was_opened_and_the_next_one_starts_clean() {
        use ParagraphKind::*;
        let mut p = project_with(&[
            (SceneHeading, "INT. A - DÍA"),
            (Action, "uno"),
            (Action, "dos"),
        ]);
        assert!(
            p.revised_paragraphs().is_empty(),
            "sin revisión no se marca nada"
        );
        let first = p.new_revision("Primera entrega", "2026-10-30");
        assert_eq!(p.revisions[0].color, RevisionColor::Blue);
        assert!(p.revised_paragraphs().is_empty());

        // Cambiar un párrafo y agregar otro los marca; dejar uno como estaba, no.
        let ids: Vec<u64> = p.paragraphs.iter().map(|q| q.id).collect();
        p.paragraphs[1].text = "uno cambiado".into();
        let added = p.make_paragraph(Action, "nuevo");
        let added_id = added.id;
        p.paragraphs.push(added);
        let marked = p.revised_paragraphs();
        assert_eq!(marked, [ids[1], added_id].into_iter().collect());
        p.paragraphs[1].text = "uno".into();
        assert_eq!(p.revised_paragraphs(), [added_id].into_iter().collect());
        // Un cambio de tipo también cuenta.
        p.paragraphs[2].kind = Dialogue;
        assert!(p.revised_paragraphs().contains(&ids[2]));

        // La revisión siguiente toma el color que sigue y arranca sin marcas.
        let second = p.new_revision("", "2026-11-02");
        assert_eq!(
            (p.revisions[1].color, p.revisions[1].name.as_str()),
            (RevisionColor::Pink, "Revisión 2")
        );
        assert!(p.revised_paragraphs().is_empty());
        // Volver a la anterior vuelve a marcar sus cambios; dejar de marcar los apaga.
        p.active_revision = Some(first);
        assert_eq!(p.revised_paragraphs().len(), 2);
        p.active_revision = None;
        assert!(p.revised_paragraphs().is_empty());
        let _ = second;

        // Se guarda con el proyecto.
        let json = serde_json::to_string(&p).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.revisions, p.revisions);
    }

    #[test]
    fn an_old_project_with_typed_notes_opens_with_typed_fields_and_an_empty_note() {
        // JSON como lo guardaba la versión anterior: sin ninguno de los campos nuevos.
        let old = r#"{"id":1,"scene":9,"name":"","duration":10.0,"items":[
            {"id":1,"kind":"Light","x":0.0,"y":0.0,"w":0.4,"h":0.4,"rot":0.0,"label":"L1","fov":60.0,"watts":650,"kelvin":5600,"note":"Relleno"},
            {"id":2,"kind":"Light","x":0.0,"y":0.0,"w":0.4,"h":0.4,"rot":0.0,"label":"L2","fov":60.0,"watts":60,"kelvin":2700,"note":"Práctica"},
            {"id":3,"kind":"Light","x":0.0,"y":0.0,"w":0.8,"h":0.4,"rot":0.0,"label":"L3","fov":60.0,"watts":650,"kelvin":5600,"note":"Rebote o bandera"},
            {"id":4,"kind":"Mic","x":0.0,"y":0.0,"w":0.3,"h":0.3,"rot":0.0,"label":"M1","track":1,"note":"Boom"},
            {"id":5,"kind":"Mic","x":0.0,"y":0.0,"w":0.3,"h":0.3,"rot":0.0,"label":"M2","track":2,"note":"Corbatero"},
            {"id":6,"kind":"Mic","x":0.0,"y":0.0,"w":0.3,"h":0.3,"rot":0.0,"label":"M3","track":3,"note":"Oculto"},
            {"id":7,"kind":"Light","x":0.0,"y":0.0,"w":0.4,"h":0.4,"rot":0.0,"label":"L4","note":"Principal","keys":[{"t":2.0,"x":1.0,"y":1.0,"rot":0.0}]}
        ]}"#;
        let mut blocking: Blocking =
            serde_json::from_str(old).expect("el formato viejo tiene que seguir abriéndose");
        for item in &mut blocking.items {
            item.migrate_legacy_note();
        }
        let by = |id: u64| blocking.items.iter().find(|i| i.id == id).unwrap();
        assert!(
            blocking.items.iter().all(|i| i.note.is_empty()),
            "note queda vacío"
        );
        let fill = by(1).light.as_ref().unwrap();
        assert_eq!(
            (fill.role, fill.fixture),
            (LightRole::Fill, LightFixture::Fresnel)
        );
        let practical = by(2).light.as_ref().unwrap();
        assert_eq!(
            (practical.fixture, practical.role),
            (LightFixture::Practical, LightRole::Practical)
        );
        assert_eq!(by(3).kind, BlockKind::Modifier);
        assert_eq!(by(3).modifier.as_ref().unwrap().kind, ModifierKind::Bounce);
        assert!(by(3).light.is_none());
        for (id, kind) in [
            (4, MicKind::Boom),
            (5, MicKind::Lavalier),
            (6, MicKind::Plant),
        ] {
            let mic = by(id).mic.as_ref().unwrap();
            assert_eq!(
                (mic.kind, mic.pattern, mic.reach_m),
                (kind, kind.default_pattern(), kind.default_reach_m()),
                "mic {id}"
            );
        }
        assert_eq!(by(4).track, 1, "la pista se conserva");
        // Lo que ya existía se conserva: el haz que se dibujaba, la potencia y las posiciones (con los campos nuevos en su valor inicial).
        assert_eq!(
            (by(1).watts, by(1).kelvin, fill.beam_deg),
            (650, 5600, 60.0)
        );
        let keyed = by(7);
        assert_eq!(keyed.keys.len(), 1);
        assert_eq!(
            (
                keyed.keys[0].hold,
                keyed.keys[0].ease_in,
                keyed.keys[0].ease_out,
                keyed.keys[0].label.as_str()
            ),
            (0.0, false, false, "")
        );
        assert_eq!(
            (keyed.cone_len_m, keyed.hidden, keyed.locked, keyed.curved),
            (5.0, false, false, false)
        );
        // Migrar dos veces no cambia nada.
        let once = blocking.clone();
        for item in &mut blocking.items {
            item.migrate_legacy_note();
        }
        assert_eq!(once, blocking);
    }

    #[test]
    fn a_project_migrates_when_it_is_loaded_and_new_fields_survive_a_save() {
        let mut project = Project::default();
        let scene = project.paragraphs[0].id;
        let blocking = project.add_blocking(scene);
        let id = project
            .add_block_item(blocking, BlockKind::Light, 1.0, 1.0)
            .unwrap();
        {
            let item = project
                .blockings
                .iter_mut()
                .find(|b| b.id == blocking)
                .unwrap()
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .unwrap();
            item.light = None;
            item.note = "Contraluz".into();
        }
        project.normalize();
        let item = &project.blockings[0].items[0];
        assert_eq!(
            (item.light.as_ref().map(|l| l.role), item.note.as_str()),
            (Some(LightRole::Back), "")
        );
        let back: Project =
            serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        assert_eq!(back.blockings, project.blockings);
    }

    fn actor(id: u64, x: f32, y: f32, rot: f32) -> BlockItem {
        BlockItem {
            id,
            kind: BlockKind::Actor,
            x,
            y,
            rot,
            w: 0.6,
            h: 0.6,
            ..BlockItem::default()
        }
    }

    fn lavalier(id: u64, on: u64) -> BlockItem {
        BlockItem {
            id,
            kind: BlockKind::Mic,
            mic: Some(MicSpec {
                attached_to: Some(on),
                ..MicSpec::new(MicKind::Lavalier)
            }),
            ..BlockItem::default()
        }
    }

    #[test]
    fn a_lavalier_follows_its_person_at_every_instant_and_has_no_poses_of_its_own() {
        let mut person = actor(1, 1.0, 1.0, 0.0);
        person.set_pose_at(4.0, 5.0, 3.0, 90.0);
        person.set_pose_at(8.0, 5.0, 7.0, 90.0);
        let mut mic = lavalier(2, 1);
        assert!(!mic.animatable());
        mic.set_pose_at(4.0, 99.0, 99.0, 0.0);
        assert!(
            mic.keys.is_empty(),
            "un corbatero unido no guarda posiciones"
        );
        let blocking = Blocking {
            items: vec![person.clone(), mic.clone()],
            ..Blocking::default()
        };
        for t in [0.0, 1.0, 2.5, 4.0, 6.0, 8.0, 12.0] {
            let (px, py, rot) = person.pose_at(t);
            let (x, y, mic_rot) = blocking.pose_of(&mic, t);
            let (sin, cos) = rot.to_radians().sin_cos();
            assert!(
                (x - (px + LAVALIER_OFFSET_M * cos)).abs() < 1e-4
                    && (y - (py + LAVALIER_OFFSET_M * sin)).abs() < 1e-4,
                "t = {t}"
            );
            assert_eq!(mic_rot, rot);
            // Siempre a 0,1 m de su persona.
            assert!(
                (((x - px).powi(2) + (y - py).powi(2)).sqrt() - LAVALIER_OFFSET_M).abs() < 1e-4
            );
        }
        // En el segundo 0 la persona mira hacia la derecha: el corbatero queda 0,1 m a la derecha de su centro.
        let (x, y, _) = blocking.pose_of(&mic, 0.0);
        assert!((x - 1.1).abs() < 1e-4 && (y - 1.0).abs() < 1e-4);
        // Sin su persona (se la borró) usa su propia pose.
        let alone = Blocking {
            items: vec![mic.clone()],
            ..Blocking::default()
        };
        assert_eq!(alone.pose_of(&mic, 3.0), mic.pose_at(3.0));
    }

    #[test]
    fn hold_easing_and_curves_shape_the_movement() {
        let mut item = actor(1, 0.0, 0.0, 0.0);
        item.keys = vec![
            BlockKey::new(2.0, 4.0, 0.0, 0.0),
            BlockKey::new(6.0, 4.0, 4.0, 0.0),
        ];
        // Sin nada de lo nuevo, el avance es lineal como siempre.
        assert_eq!(item.pose_at(1.0), (2.0, 0.0, 0.0));
        assert_eq!(item.pose_at(4.0), (4.0, 2.0, 0.0));
        // Una espera: se queda quieta en la posición durante `hold` segundos y después sigue.
        item.keys[0].hold = 2.0;
        assert_eq!(item.pose_at(3.9), (4.0, 0.0, 0.0));
        assert_eq!(item.pose_at(4.0), (4.0, 0.0, 0.0));
        assert_eq!(item.pose_at(5.0), (4.0, 2.0, 0.0));
        assert_eq!(item.pose_at(6.0), (4.0, 4.0, 0.0));
        item.keys[0].hold = 0.0;
        // Frenar al llegar a la primera posición: a mitad de camino ya recorrió más de la mitad.
        item.keys[0].ease_in = true;
        assert!(item.pose_at(1.0).0 > 2.0 && item.pose_at(2.0).0 == 4.0);
        // Acelerar al salir de ella: a mitad de camino recorrió menos de la mitad.
        item.keys[0].ease_in = false;
        item.keys[0].ease_out = true;
        assert!(item.pose_at(4.0).1 < 2.0 && item.pose_at(6.0).1 == 4.0);
        // Las dos: simétrico, a mitad de camino va justo por la mitad.
        item.keys[1].ease_in = true;
        let mid = item.pose_at(4.0).1;
        assert!((mid - 2.0).abs() < 1e-4);
        // Curva: pasa por las posiciones y se aparta de la recta entre ellas.
        let mut curved = actor(2, 0.0, 0.0, 0.0);
        curved.keys = vec![
            BlockKey::new(2.0, 4.0, 0.0, 0.0),
            BlockKey::new(4.0, 4.0, 4.0, 0.0),
            BlockKey::new(6.0, 0.0, 4.0, 0.0),
        ];
        let straight = (0..=12)
            .map(|n| curved.pose_at(n as f32 * 0.5))
            .collect::<Vec<_>>();
        curved.keys.iter_mut().for_each(|k| k.smooth = true);
        for key in curved.keys.clone() {
            let (x, y, _) = curved.pose_at(key.t);
            assert!(
                (x - key.x).abs() < 1e-4 && (y - key.y).abs() < 1e-4,
                "no pasa por la posición de t = {}",
                key.t
            );
        }
        let bent = (0..=12)
            .map(|n| curved.pose_at(n as f32 * 0.5))
            .collect::<Vec<_>>();
        assert_ne!(straight, bent);
        assert_eq!(curved.pose_at(0.0), (0.0, 0.0, 0.0));
        assert_eq!(curved.pose_at(99.0), (0.0, 4.0, 0.0));
        // El giro sigue tomando el camino corto.
        let mut turner = actor(3, 0.0, 0.0, 350.0);
        turner.keys = vec![BlockKey::new(2.0, 0.0, 0.0, 10.0)];
        assert!((turner.pose_at(1.0).2 - 0.0).abs() < 1e-3);
    }

    #[test]
    fn every_light_fixture_and_mic_kind_has_initial_values() {
        for fixture in LightFixture::ALL {
            let spec = LightSpec::new(fixture);
            assert!(
                spec.beam_deg > 0.0
                    && spec.reach_m > 0.0
                    && spec.intensity == 100.0
                    && spec.height_m == 2.0,
                "{fixture:?}"
            );
        }
        assert_eq!(
            (
                LightFixture::Practical.beam_deg(),
                LightFixture::Practical.reach_m()
            ),
            (360.0, 1.5)
        );
        for (kind, pattern, reach) in [
            (MicKind::Boom, MicPattern::Shotgun, 1.5),
            (MicKind::Lavalier, MicPattern::Omni, 0.5),
            (MicKind::Plant, MicPattern::Cardioid, 1.0),
            (MicKind::Stand, MicPattern::Cardioid, 1.0),
        ] {
            let spec = MicSpec::new(kind);
            assert_eq!((spec.pattern, spec.reach_m), (pattern, reach), "{kind:?}");
        }
        assert_eq!(ModifierSpec::default().w, 1.0);
    }

    #[test]
    fn the_lens_changes_in_time_between_the_positions_that_carry_a_zoom_for_a_vertigo_effect() {
        let mut cam = BlockItem {
            kind: BlockKind::Camera,
            lens_mm: 24.0,
            ..BlockItem::default()
        };
        assert_eq!(cam.lens_at(3.0), 24.0, "sin posiciones, la de partida");
        cam.set_lens_at(0.0, 20.0);
        assert_eq!(cam.lens_mm, 20.0, "en 0 s cambia la de partida");
        cam.set_lens_at(4.0, 80.0);
        assert_eq!(
            cam.keys.len(),
            1,
            "en otro instante se crea la posición con su zoom"
        );
        assert_eq!(cam.keys[0].lens_mm, Some(80.0));
        assert_eq!(
            (
                cam.lens_at(0.0),
                cam.lens_at(2.0),
                cam.lens_at(4.0),
                cam.lens_at(9.0)
            ),
            (20.0, 50.0, 80.0, 80.0)
        );
        // Una posición sin zoom propio no cambia la lente: sigue la interpolación entre las que sí lo tienen.
        cam.keys.push(BlockKey::new(6.0, 1.0, 1.0, 0.0));
        cam.keys.push(BlockKey {
            lens_mm: Some(40.0),
            ..BlockKey::new(8.0, 2.0, 2.0, 0.0)
        });
        assert_eq!(cam.lens_at(6.0), 60.0);
        assert_eq!(cam.lens_at(8.0), 40.0);
        // Cambiarlo en una posición que ya está no crea otra.
        cam.set_lens_at(4.0, 100.0);
        assert_eq!(cam.keys.len(), 3);
        assert_eq!(cam.lens_at(4.0), 100.0);
    }

    #[test]
    fn deleting_the_position_at_an_instant_keeps_the_others_and_the_start_passes_to_the_next() {
        let mut cam = actor(1, 0.0, 0.0, 0.0);
        cam.keys = vec![
            BlockKey::new(2.0, 4.0, 0.0, 90.0),
            BlockKey::new(4.0, 4.0, 4.0, 180.0),
            BlockKey::new(6.0, 0.0, 4.0, 270.0),
        ];
        assert!(
            !cam.delete_position_at(3.0),
            "entre dos posiciones no hay nada que borrar"
        );
        assert_eq!(cam.keys.len(), 3);
        assert!(cam.delete_position_at(4.0));
        assert_eq!(cam.keys.iter().map(|k| k.t).collect::<Vec<_>>(), [2.0, 6.0]);
        // Borrar la de partida la cede a la siguiente.
        assert!(cam.delete_position_at(0.0));
        assert_eq!((cam.x, cam.y, cam.rot), (4.0, 0.0, 90.0));
        assert_eq!(cam.keys.len(), 1);
        assert!(
            !actor(2, 0.0, 0.0, 0.0).delete_position_at(0.0),
            "sin posiciones no hay qué ceder"
        );
    }

    #[test]
    fn points_on_the_path_shape_the_trail_and_each_one_is_a_corner_or_a_smooth_curve() {
        let mut item = actor(1, 0.0, 0.0, 0.0);
        item.keys = vec![BlockKey::new(4.0, 4.0, 0.0, 0.0)];
        assert_eq!(item.pose_at(2.0), (2.0, 0.0, 0.0), "recto por defecto");
        // Un punto intermedio en (2; 2): el camino pasa por él, con esquinas.
        item.keys[0].via.push(Waypoint {
            x: 2.0,
            y: 2.0,
            smooth: false,
            handle: None,
        });
        let corner = item.segment_curve(0);
        assert_eq!(corner, vec![(0.0, 0.0), (2.0, 2.0), (4.0, 0.0)]);
        let halfway = item.pose_at(2.0);
        assert!(
            (halfway.0 - 2.0).abs() < 1e-3 && (halfway.1 - 2.0).abs() < 1e-3,
            "a mitad del recorrido está en el punto: {halfway:?}"
        );
        // Suave: una curva que sigue pasando por el punto y por las posiciones, pero sin esquina.
        item.keys[0].via[0].smooth = true;
        let curve = item.segment_curve(0);
        assert!(
            curve.len() > 10
                && curve.first() == Some(&(0.0, 0.0))
                && curve.last() == Some(&(4.0, 0.0))
        );
        assert!(
            curve
                .iter()
                .any(|p| (p.0 - 2.0).abs() < 1e-3 && (p.1 - 2.0).abs() < 1e-3),
            "pasa por el punto"
        );
        assert!(
            curve.iter().all(|p| p.1 <= 2.0 + 1e-3),
            "no se pasa del punto más alto"
        );
        let (x, y, _) = item.pose_at(4.0);
        assert!(
            (x - 4.0).abs() < 1e-3 && y.abs() < 1e-3,
            "llega a la posición"
        );
        // También una posición se puede hacer suave.
        item.keys[0].smooth = true;
        assert!(item.segment_curve(0).len() > 10);
    }

    #[test]
    fn the_old_curved_switch_becomes_smooth_positions_on_load() {
        let mut item = actor(1, 0.0, 0.0, 0.0);
        item.keys = vec![
            BlockKey::new(2.0, 4.0, 0.0, 0.0),
            BlockKey::new(4.0, 4.0, 4.0, 0.0),
        ];
        item.curved = true;
        item.migrate_legacy_note();
        assert!(!item.curved && item.keys.iter().all(|k| k.smooth));
        let old = r#"{"id":1,"kind":"Actor","x":0.0,"y":0.0,"w":0.6,"h":0.6,"rot":0.0,"label":"","curved":true,"keys":[{"t":2.0,"x":4.0,"y":0.0,"rot":0.0}]}"#;
        let mut loaded: BlockItem = serde_json::from_str(old).unwrap();
        loaded.migrate_legacy_note();
        assert!(loaded.keys[0].smooth);
        assert!(
            !serde_json::to_string(&loaded).unwrap().contains("curved"),
            "ya no se guarda"
        );
    }

    #[test]
    fn a_curvature_handle_reshapes_the_curve_without_moving_the_nodes() {
        let mut item = actor(1, 0.0, 0.0, 0.0);
        item.keys = vec![BlockKey::new(4.0, 4.0, 0.0, 0.0)];
        item.keys[0].via.push(Waypoint {
            x: 2.0,
            y: 2.0,
            smooth: true,
            handle: None,
        });
        let auto = item.segment_curve(0);
        // Sin tirador propio, el tirador sale de la curva; con uno propio, la curva lo sigue.
        let (_, _, h) = item.smooth_handles()[0];
        assert!(h.0 > 0.0, "apunta hacia adelante: {h:?}");
        item.keys[0].via[0].handle = Some((0.0, 1.5));
        let custom = item.segment_curve(0);
        assert_ne!(auto, custom);
        assert_eq!(
            (custom.first(), custom.last()),
            (Some(&(0.0, 0.0)), Some(&(4.0, 0.0)))
        );
        assert!(
            custom
                .iter()
                .any(|p| (p.0 - 2.0).abs() < 1e-3 && (p.1 - 2.0).abs() < 1e-3),
            "sigue pasando por el nodo"
        );
        // Una posición suave también lleva tiradores; una esquina, no.
        assert_eq!(item.smooth_handles().len(), 1);
        item.keys[0].smooth = true;
        assert_eq!(item.smooth_handles().len(), 2);
        assert_eq!(item.smooth_handles()[1].0, PathNodeRef::Key(0));
    }

    #[test]
    fn a_script_location_shows_the_status_and_cost_of_its_shooting_place_in_the_breakdown() {
        use ParagraphKind::*;
        let mut p = project_with(&[(SceneHeading, "INT. CASA - DÍA"), (Action, "Ana entra.")]);
        p.ensure_breakdown_defaults();
        p.auto_link_all();
        let place = p.add_shoot_location("Casa de Pedro");
        let location = p.location_profiles[0].id;
        p.location_profiles[0].shoot_location = Some(place);
        let place_mut = p
            .shoot_locations
            .iter_mut()
            .find(|l| l.id == place)
            .unwrap();
        place_mut.permit = PermitStatus::Approved;
        place_mut.permit_cost_cents = Some(150_000);
        let deal = p.location_deal(EntityRef::Location(location)).unwrap();
        assert_eq!(deal.status, Some(Acquisition::Have));
        assert_eq!(deal.cost_cents, Some(150_000));
        // El presupuesto de la categoría Locaciones suma el costo del lugar, una sola vez aunque haya dos locaciones.
        let category = p
            .breakdown_categories
            .iter()
            .find(|c| c.source == CategorySource::Locations)
            .unwrap()
            .id;
        let total = |p: &Project| {
            p.budget_by_category()
                .into_iter()
                .find(|(c, _, _)| *c == category)
                .map(|(_, cents, _)| cents)
        };
        assert_eq!(total(&p), Some(150_000));
    }
}
