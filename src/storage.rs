use std::{
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::{model::Project, theme::Scheme};

pub const EXTENSION: &str = "spd";

/// Tamaño de papel de las hojas del guion y del tratamiento.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Paper {
    /// El de casi todo el mundo (Starc: plantilla internacional).
    #[default]
    A4,
    /// El de Estados Unidos.
    Letter,
}

/// Medidas de una hoja en pulgadas: el papel y sus márgenes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheet {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Sheet {
    pub fn text_w(self) -> f32 {
        self.width - self.left - self.right
    }

    pub fn text_h(self) -> f32 {
        self.height - self.top - self.bottom
    }

    /// Lo que se pierde de alto entre el texto de una hoja y el de la siguiente (margen de abajo y de arriba).
    pub fn margins_v(self) -> f32 {
        self.top + self.bottom
    }
}

impl Paper {
    pub const ALL: [Paper; 2] = [Paper::A4, Paper::Letter];

    pub fn label(self) -> &'static str {
        match self {
            Paper::A4 => "A4 (21 × 29,7 cm)",
            Paper::Letter => "Carta (21,6 × 27,9 cm)",
        }
    }

    /// A4: 1,5 in (3,8 cm) a la izquierda y unos 0,77 in a la derecha, que dejan exactamente 60 columnas como en Carta; 2,5 cm arriba y abajo.
    /// Carta: 1,5" a la izquierda y 1" en los demás lados (60 caracteres).
    pub fn sheet(self) -> Sheet {
        const CM: f32 = 1.0 / 2.54;
        match self {
            Paper::A4 => Sheet {
                width: 21.0 * CM,
                height: 29.7 * CM,
                left: 1.5,
                right: 21.0 * CM - 1.5 - 6.0,
                top: 2.5 * CM,
                bottom: 2.5 * CM,
            },
            Paper::Letter => Sheet {
                width: 8.5,
                height: 11.0,
                left: 1.5,
                right: 1.0,
                top: 1.0,
                bottom: 1.0,
            },
        }
    }
}
pub const DEFAULT_ZOOM: u32 = 115;
pub const DEFAULT_PANEL_WIDTH: f32 = 300.0;
/// Anchos mínimo y máximo de la barra lateral y de los paneles de la derecha.
pub const SIDEBAR_RANGE: std::ops::RangeInclusive<f32> = 220.0..=520.0;
pub const RIGHT_RANGE: std::ops::RangeInclusive<f32> = 240.0..=560.0;
/// Tamaño de la letra de las hojas, en puntos: el del formato (Courier Prime 12 en el guion, Arial 11 en el tratamiento).
pub const SCRIPT_PT: u32 = 12;
pub const TREATMENT_PT: u32 = 11;
/// Zoom de la vista de las hojas, en porcentaje: agranda o achica la hoja entera sin cambiar cuánto entra en ella.
pub const DEFAULT_VIEW_ZOOM: u32 = 100;
pub const VIEW_ZOOM_RANGE: (u32, u32) = (40, 300);
/// Cuánto cambia el zoom de la vista con cada paso de la rueda.
pub const VIEW_ZOOM_STEP: u32 = 5;
pub const ZOOM_RANGE: (u32, u32) = (70, 250);

/// La fecha de hoy como AAAA-MM-DD, en hora de Argentina (UTC-3).
pub fn today() -> String {
    let seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    date_at(seconds)
}

/// Segundos desde 1970 (UTC).
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64
}

/// «AAAA-MM-DD HH:MM» en UTC-3 de un instante dado en segundos desde 1970 UTC.
pub fn datetime_at(unix_seconds: i64) -> String {
    let local = unix_seconds - 3 * 3600;
    let secs = local.rem_euclid(86_400);
    format!("{} {:02}:{:02}", date_at(unix_seconds), secs / 3600, secs % 3600 / 60)
}

/// Un momento para leer de un vistazo: «Hoy 14:32», «Ayer 18:10» o «31/10 09:05».
pub fn friendly_time(unix_seconds: i64, now: i64) -> String {
    let full = datetime_at(unix_seconds);
    let (date, hour) = full.split_once(' ').unwrap_or((full.as_str(), ""));
    if date == date_at(now) {
        format!("Hoy {hour}")
    } else if date == date_at(now - 86_400) {
        format!("Ayer {hour}")
    } else {
        let mut parts = date.split('-');
        let (_, month, day) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        format!("{day}/{month} {hour}")
    }
}

/// El nombre del usuario del sistema, para firmar las versiones si la persona no puso otro.
pub fn os_user_name() -> String {
    ["USER", "USERNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_default()
}

/// La fecha (AAAA-MM-DD) en UTC-3 de un instante dado en segundos desde 1970 UTC.
fn date_at(unix_seconds: i64) -> String {
    let (year, month, day) = civil_from_days((unix_seconds - 3 * 3600).div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// Días desde 1970-01-01 a (año, mes, día) del calendario gregoriano.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// Guarda el proyecto como carpeta de secciones (ver `folder`): solo se escribe lo que cambió.
pub fn save(project: &Project, path: &Path) -> io::Result<()> {
    crate::folder::save(project, path).map(|_| ())
}

/// Abre un proyecto, en carpeta o de un solo archivo (el formato anterior).
pub fn load(path: &Path) -> io::Result<Project> {
    crate::folder::load(path)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub scheme: Scheme,
    /// Tamaño de la interfaz en porcentaje.
    pub ui_zoom: u32,
    /// Zoom de la vista de las hojas (guion y tratamiento), en porcentaje.
    pub view_zoom: u32,
    /// Papel de las hojas del guion y el tratamiento.
    pub paper: Paper,
    /// Versión de los ajustes; los anteriores a la 2 guardaban en «script_pt» el tamaño de la vista.
    pub settings_version: u32,
    pub autosave: bool,
    /// El nombre con que se firman las versiones guardadas del historial.
    pub user_name: String,
    /// Qué tramo de ids usa esta copia del programa (se sortea una vez): ver `Project::use_id_block`.
    pub user_id: u64,
    /// La nube (Supabase): su dirección y la clave pública. Ninguna es secreta; la clave secreta nunca se usa en el programa.
    pub cloud_url: String,
    pub cloud_anon_key: String,
    /// Nombres de personajes en negrita dentro del guion.
    pub bold_names: bool,
    /// Nombres de personajes en negrita dentro del tratamiento.
    pub treatment_bold: bool,
    /// Línea de tiempo con la duración aproximada junto a la hoja del guion.
    pub show_timeline: bool,
    /// Ancho de la barra lateral izquierda y de los paneles de la derecha (se arrastran desde el borde).
    pub sidebar_width: f32,
    pub right_width: f32,
    /// Opción global de antes de las mayúsculas por categoría: solo da el punto de partida a los proyectos viejos.
    pub mentions_uppercase: bool,
    /// «Reducir animaciones»: todas las duraciones pasan a 0 (reglas §6.7).
    pub reduce_motion: bool,
    pub recents: Vec<PathBuf>,
    /// Última pantalla abierta de cada proyecto (por ruta del archivo), para volver a ella al abrirlo.
    pub last_pages: std::collections::BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scheme: Scheme::Tierra,
            ui_zoom: DEFAULT_ZOOM,
            view_zoom: DEFAULT_VIEW_ZOOM,
            paper: Paper::A4,
            settings_version: 4,
            autosave: true,
            user_name: String::new(),
            user_id: 0,
            cloud_url: String::new(),
            cloud_anon_key: String::new(),
            bold_names: false,
            treatment_bold: false,
            show_timeline: true,
            sidebar_width: DEFAULT_PANEL_WIDTH,
            right_width: DEFAULT_PANEL_WIDTH,
            mentions_uppercase: true,
            reduce_motion: false,
            recents: Vec::new(),
            last_pages: Default::default(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("scriptpendra").join("settings.json"))
}

/// Dónde se guarda, sin pedir nada, un proyecto que todavía no tiene archivo (para no perder lo escrito).
pub fn recovery_path() -> Option<PathBuf> {
    config_path().map(|p| p.with_file_name("recuperado.scriptpendra"))
}

/// La agenda de contactos de la app, compartida entre proyectos (cada proyecto guarda una copia de los que usa).
fn agenda_path() -> Option<PathBuf> {
    config_path().map(|p| p.with_file_name("agenda.json"))
}

pub fn load_agenda() -> Vec<crate::model::Contact> {
    agenda_from_text(
        agenda_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .as_deref(),
    )
}

fn agenda_from_text(text: Option<&str>) -> Vec<crate::model::Contact> {
    text.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

pub fn save_agenda(contacts: &[crate::model::Contact]) {
    // Las pruebas no tocan la agenda real de quien las corre.
    if cfg!(test) {
        return;
    }
    let Some(path) = agenda_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(contacts) {
        let _ = fs::write(path, json);
    }
}

/// Un id para la agenda que no se repite entre proyectos ni entre máquinas.
pub fn new_agenda_id() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        .max(1)
}

impl Settings {
    pub fn load() -> Self {
        let mut settings = Self::from_text(
            config_path()
                .and_then(|p| fs::read_to_string(p).ok())
                .as_deref(),
        );
        // Cada copia del programa sortea una vez su tramo de ids, para que dos personas no repitan ids al trabajar a la vez.
        if settings.user_id == 0 {
            let nanos = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64);
            settings.user_id = (nanos ^ u64::from(std::process::id()).rotate_left(17)) % 1_000_000 + 1;
            settings.save();
        }
        settings
    }

    /// Lee los ajustes guardados (o los de fábrica si no hay o no se entienden).
    fn from_text(text: Option<&str>) -> Self {
        let mut settings: Settings = text
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        // Antes el «tamaño del texto» era en realidad el zoom de la hoja (20 = lo normal): pasa a ser el zoom.
        let saved: Option<serde_json::Value> = text.and_then(|s| serde_json::from_str(s).ok());
        if let Some(old) = saved
            .as_ref()
            .filter(|v| v.get("settings_version").is_none())
        {
            let px = old.get("script_pt").and_then(|n| n.as_u64()).unwrap_or(20) as u32;
            settings.view_zoom = px * 100 / 20;
            settings.settings_version = 2;
        }
        // Versión 3: un defecto dejó guardados anchos de paneles inflados por el contenido; se vuelve al ancho normal.
        if settings.settings_version < 3 {
            settings.sidebar_width = DEFAULT_PANEL_WIDTH;
            settings.right_width = DEFAULT_PANEL_WIDTH;
            settings.settings_version = 3;
        }
        // Versión 4: el autoguardado pasa a estar activado por defecto; lo que quedó guardado como apagado sin que nadie lo haya
        // elegido (era el valor de fábrica) se enciende una vez.
        if settings.settings_version < 4 {
            settings.autosave = true;
            settings.settings_version = 4;
        }
        settings.sanitize();
        settings
    }

    /// Un panel guardado justo en el ancho máximo es casi seguro un resto de un defecto anterior (el panel crecía solo
    /// hasta el tope): se vuelve al ancho normal. Los valores fuera de rango se recortan.
    pub fn sanitize(&mut self) {
        let fix = |width: &mut f32, range: &std::ops::RangeInclusive<f32>| {
            if *width >= *range.end() - 1.0 || !width.is_finite() {
                *width = DEFAULT_PANEL_WIDTH;
            }
            *width = width.clamp(*range.start(), *range.end());
        };
        self.view_zoom = self.view_zoom.clamp(VIEW_ZOOM_RANGE.0, VIEW_ZOOM_RANGE.1);
        fix(&mut self.sidebar_width, &SIDEBAR_RANGE);
        fix(&mut self.right_width, &RIGHT_RANGE);
    }

    pub fn save(&self) {
        // Las pruebas no tocan la configuración real de quien las corre.
        if cfg!(test) {
            return;
        }
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn add_recent(&mut self, path: &Path) {
        self.recents.retain(|p| p != path);
        self.recents.insert(0, path.to_path_buf());
        self.recents.truncate(12);
        self.save();
    }
}

/// Datos de una tarjeta de la pantalla de inicio.
pub struct RecentCard {
    pub path: PathBuf,
    pub title: String,
    pub logline: String,
    pub cover: Option<PathBuf>,
    pub modified: Option<SystemTime>,
}

pub fn recent_cards(settings: &Settings) -> Vec<RecentCard> {
    settings
        .recents
        .iter()
        .filter_map(|path| {
            let project = load(path).ok()?;
            let modified = fs::metadata(path).and_then(|m| m.modified()).ok();
            let cover = project.cover.as_deref().map(|c| project.resolve_path(c));
            Some(RecentCard {
                path: path.clone(),
                title: project.title,
                logline: project.logline,
                cover,
                modified,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind;

    #[test]
    fn projects_saved_before_links_get_ids_profiles_and_links_on_load() {
        // Formato anterior: sin ids en fichas ni objetos, y personajes que solo existían como texto.
        let json = r#"{
            "title": "Viejo",
            "paragraphs": [
                {"id": 1, "kind": "SceneHeading", "text": "INT. A - DÍA"},
                {"id": 2, "kind": "Character", "text": "ANA"},
                {"id": 3, "kind": "Action", "text": "Ana toma la llave."}
            ],
            "objects": [{"name": "llave"}]
        }"#;
        let path = std::env::temp_dir().join(format!("scriptpendra_old.{EXTENSION}"));
        fs::write(&path, json).unwrap();
        let p = load(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(p.characters.len(), 1); // el encabezado creó su ficha
        assert_ne!(p.characters[0].id, 0);
        assert_ne!(p.objects[0].id, 0);
        assert_eq!(p.paragraphs[1].links.len(), 1);
        assert_eq!(p.paragraphs[2].links.len(), 2); // Ana y llave
        // Tras renombrar, todo sigue al nombre nuevo.
        let mut p = p;
        p.characters[0].name = "MARTA".into();
        p.sync_links();
        assert_eq!(p.paragraphs[1].text, "MARTA");
        assert_eq!(p.paragraphs[2].text, "Marta toma la llave.");
    }

    #[test]
    fn a_panel_stuck_at_the_maximum_width_goes_back_to_normal() {
        let mut s = Settings {
            right_width: 560.0,
            sidebar_width: 252.75,
            ..Default::default()
        };
        s.sanitize();
        assert_eq!(s.right_width, DEFAULT_PANEL_WIDTH); // el que creció solo
        assert_eq!(s.sidebar_width, 252.75); // el que se achicó a mano se respeta
        let mut weird = Settings {
            right_width: f32::NAN,
            sidebar_width: 10.0,
            ..Default::default()
        };
        weird.sanitize();
        assert_eq!(
            (weird.right_width, weird.sidebar_width),
            (DEFAULT_PANEL_WIDTH, *SIDEBAR_RANGE.start())
        );
    }

    #[test]
    fn roundtrip() {
        let mut p = Project::default();
        p.title = "Prueba".into();
        let para = p.make_paragraph(ParagraphKind::Dialogue, "Hola");
        p.paragraphs.push(para);

        let path = std::env::temp_dir().join(format!("scriptpendra_test.{EXTENSION}"));
        save(&p, &path).unwrap();
        let loaded = load(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(p.title, loaded.title);
        assert_eq!(p.paragraphs, loaded.paragraphs);
    }

    #[test]
    fn settings_saved_before_the_font_and_view_split_keep_their_view_size() {
        // Antes «script_pt» era el tamaño de la vista (20 = 100 %): ahora es el zoom.
        let old = Settings::from_text(Some(r#"{"script_pt": 30, "autosave": true}"#));
        assert_eq!(old.view_zoom, 150);
        assert!(old.autosave);
        // Los ajustes nuevos se respetan tal cual.
        let new = Settings::from_text(Some(
            r#"{"settings_version": 2, "script_pt": 11, "view_zoom": 80}"#,
        ));
        assert_eq!(new.view_zoom, 80);
        // Sin archivo, o con basura, quedan los de fábrica.
        assert_eq!(Settings::from_text(None).view_zoom, 100);
        assert_eq!(Settings::from_text(Some("no es json")).view_zoom, 100);
    }

    #[test]
    fn the_earth_theme_is_called_tierra_again_and_old_saved_names_still_load() {
        let old = Settings::from_text(Some(r#"{"settings_version": 2, "scheme": "Chubut"}"#));
        assert_eq!(old.scheme, Scheme::Tierra);
        assert_eq!(Scheme::Tierra.name(), "Tierra");
        assert_eq!(Settings::default().scheme, Scheme::Tierra);
    }

    #[test]
    fn dates_are_computed_from_days_since_1970() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(20_756), (2026, 10, 30));
        assert_eq!(today().len(), 10);
        // A las 02:30 UTC del 30 todavía es el 29 en Argentina; a las 03:00 UTC ya es el 30.
        let midnight = 20_756 * 86_400;
        assert_eq!(date_at(midnight + 2 * 3600 + 1800), "2026-10-29");
        assert_eq!(date_at(midnight + 3 * 3600), "2026-10-30");
    }

    #[test]
    fn the_agenda_is_read_back_and_a_broken_file_is_an_empty_agenda() {
        let contacts = vec![crate::model::Contact {
            id: 1,
            agenda_id: Some(7),
            name: "Ana".into(),
            ..Default::default()
        }];
        let json = serde_json::to_string(&contacts).unwrap();
        assert_eq!(agenda_from_text(Some(&json)), contacts);
        assert!(
            agenda_from_text(Some("no es json")).is_empty() && agenda_from_text(None).is_empty()
        );
        assert_ne!(new_agenda_id(), 0);
    }

    #[test]
    fn a_moment_is_shown_as_date_and_time_in_argentina() {
        // 2026-10-31 15:30 UTC son las 12:30 en UTC-3.
        assert_eq!(datetime_at(1_793_460_600), "2026-10-31 12:30");
    }

    #[test]
    fn a_moment_reads_as_today_yesterday_or_a_date() {
        let now = 1_793_460_600; // 2026-10-31 12:30 en UTC-3
        assert_eq!(friendly_time(now - 3600, now), "Hoy 11:30");
        assert_eq!(friendly_time(now - 86_400, now), "Ayer 12:30");
        assert_eq!(friendly_time(now - 5 * 86_400, now), "26/10 12:30");
    }
}
