//! El proyecto como carpeta: en vez de un solo archivo grande, el proyecto se parte en secciones (guion, plantas, listas de
//! arte, sonido, fotografía…), cada una en su archivo. Así, si el proyecto vive en una carpeta compartida (Drive, Syncthing…),
//! solo se sincroniza lo que cambió y dos personas que trabajan en partes distintas casi nunca se pisan.
//!
//! ```text
//! Proyecto/
//!   Corto.scriptpendra        el índice: formato y lista de secciones
//!   Corto.datos/
//!     guion.json, plantas.json, …
//!     historial/              las versiones guardadas (ver `Version`)
//! ```
//!
//! Los proyectos viejos, de un solo archivo, se siguen abriendo; al guardar pasan al formato nuevo y el archivo viejo queda
//! respaldado.

use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::model::Project;

/// Versión del formato de carpeta.
pub const FORMAT: u64 = 2;
const FORMAT_KEY: &str = "scriptpendra_format";

/// En qué sección va cada campo del proyecto. Lo que no figura va a «general».
pub fn section_of(key: &str) -> &'static str {
    match key {
        "cloud_images" => "imagenes",
        "paragraphs" | "comments" | "revisions" | "active_revision" | "auto_cont"
        | "scene_numbers_locked" => "guion",
        "synopsis" | "synopsis_links" | "treatment" | "treatment_links" | "outline" | "drama"
        | "documents" | "logline" => "historia",
        "characters" | "location_profiles" | "objects" | "element_categories" | "elements"
        | "name_case" | "name_case_migrated" | "extra_locations" => "fichas",
        "shots" | "shot_columns" | "storyboard_only_images" | "shots_restart" | "camera_bodies"
        | "camera_body_default" | "camera_body_asked" | "aspect" => "tecnica",
        "blockings" => "plantas",
        "sound_cues" | "sound_gear" => "sonido",
        "photo_gear" => "fotografia",
        "art_items" => "arte",
        "breakdown_categories" | "breakdown_items" | "breakdown_tags" | "breakdown_version"
        | "role_breakdown" => "desglose",
        "shoot_days" | "schedule" | "day_minutes" | "minutes_per_eighth" | "utc_offset_minutes"
        | "minutes_per_setup" | "minutes_per_shot" | "strip_colors" | "shoot_locations"
        | "contacts" | "scenes" => "produccion",
        _ => "general",
    }
}

/// La carpeta de datos de un proyecto: «Corto.datos» junto a «Corto.scriptpendra».
pub fn data_dir(file: &Path) -> PathBuf {
    let stem = file.file_stem().map_or_else(|| "proyecto".into(), |s| s.to_string_lossy().into_owned());
    file.with_file_name(format!("{stem}.datos"))
}

/// El proyecto partido en secciones, cada una como JSON ordenado (el mismo proyecto da siempre los mismos bytes).
pub fn split(project: &Project) -> io::Result<BTreeMap<String, Vec<u8>>> {
    let Value::Object(all) = serde_json::to_value(project).map_err(io::Error::other)? else {
        return Err(io::Error::other("el proyecto no es un objeto"));
    };
    let mut parts: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for (key, value) in all {
        parts.entry(section_of(&key).to_string()).or_default().insert(key, value);
    }
    parts
        .into_iter()
        .map(|(name, map)| {
            serde_json::to_vec_pretty(&Value::Object(map))
                .map(|bytes| (name, bytes))
                .map_err(io::Error::other)
        })
        .collect()
}

/// Junta las secciones en un proyecto. Lo que falte toma su valor de fábrica.
pub fn join(sections: &BTreeMap<String, Vec<u8>>) -> io::Result<Project> {
    let mut all = Map::new();
    for bytes in sections.values() {
        let Value::Object(map) = serde_json::from_slice(bytes).map_err(io::Error::other)? else {
            return Err(io::Error::other("una sección no es un objeto"));
        };
        all.extend(map);
    }
    serde_json::from_value(Value::Object(all)).map_err(io::Error::other)
}

/// Escribe sin dejar un archivo a medias: primero a uno temporal y después se lo pone en su lugar.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Qué secciones se escribieron al guardar.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SaveReport {
    pub written: Vec<String>,
}

/// ¿El archivo es un proyecto de un solo archivo (el formato anterior)?
fn is_legacy(file: &Path) -> bool {
    fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| v.is_object() && v.get(FORMAT_KEY).is_none())
}

/// Guarda el proyecto como carpeta: solo se escriben las secciones que cambiaron.
pub fn save(project: &Project, file: &Path) -> io::Result<SaveReport> {
    let sections = split(project)?;
    let dir = data_dir(file);
    fs::create_dir_all(&dir)?;
    // El proyecto viejo, de un solo archivo, queda respaldado la primera vez.
    if file.exists() && is_legacy(file) {
        let backup = file.with_extension("antes-de-carpeta.json");
        if !backup.exists() {
            fs::copy(file, backup)?;
        }
    }
    let mut report = SaveReport::default();
    for (name, bytes) in &sections {
        let path = dir.join(format!("{name}.json"));
        if fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
            write_atomic(&path, bytes)?;
            report.written.push(name.clone());
        }
    }
    // Las secciones que ya no existen se quitan.
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            if path.is_file()
                && path.extension().is_some_and(|e| e == "json")
                && name.is_some_and(|n| !sections.contains_key(&n))
            {
                let _ = fs::remove_file(path);
            }
        }
    }
    let manifest = serde_json::to_vec_pretty(&serde_json::json!({
        FORMAT_KEY: FORMAT,
        "datos": dir.file_name().map(|n| n.to_string_lossy().into_owned()),
        "secciones": sections.keys().collect::<Vec<_>>(),
    }))
    .map_err(io::Error::other)?;
    if fs::read(file).ok().as_deref() != Some(manifest.as_slice()) {
        write_atomic(file, &manifest)?;
    }
    Ok(report)
}

/// Las secciones guardadas de un proyecto en carpeta.
pub fn read_sections(file: &Path) -> io::Result<BTreeMap<String, Vec<u8>>> {
    let mut out = BTreeMap::new();
    for entry in fs::read_dir(data_dir(file))?.flatten() {
        let path = entry.path();
        if path.is_file()
            && path.extension().is_some_and(|e| e == "json")
            && let Some(name) = path.file_stem()
        {
            out.insert(name.to_string_lossy().into_owned(), fs::read(&path)?);
        }
    }
    Ok(out)
}

/// Abre un proyecto: en carpeta, o de un solo archivo (el formato anterior).
pub fn load(file: &Path) -> io::Result<Project> {
    let bytes = fs::read(file)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    let mut project: Project = if value.get(FORMAT_KEY).is_some() {
        join(&read_sections(file)?)?
    } else {
        serde_json::from_value(value).map_err(io::Error::other)?
    };
    project.base_dir = file.parent().map(Path::to_path_buf);
    project.normalize();
    Ok(project)
}

// ------------------------------------------------------------------------------------------------
// Cambios de otras personas
// ------------------------------------------------------------------------------------------------

/// Cuándo se tocó cada archivo de sección (tamaño y fecha): sirve para saber, sin leerlos, si cambió algo en la carpeta.
pub type Stamps = BTreeMap<String, (u64, Option<std::time::SystemTime>)>;

pub fn stamps(file: &Path) -> Stamps {
    let mut out = Stamps::new();
    let manifest = fs::metadata(file).ok();
    out.insert(
        String::new(),
        (
            manifest.as_ref().map_or(0, |m| m.len()),
            manifest.and_then(|m| m.modified().ok()),
        ),
    );
    if let Ok(entries) = fs::read_dir(data_dir(file)) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().is_some_and(|e| e == "json")
                && let (Some(name), Ok(meta)) = (path.file_stem(), entry.metadata())
            {
                out.insert(
                    name.to_string_lossy().into_owned(),
                    (meta.len(), meta.modified().ok()),
                );
            }
        }
    }
    out
}

/// El mismo JSON ordenado con que se guardan las secciones.
pub fn pretty(value: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(value).unwrap_or_default()
}

fn id_of(value: &Value) -> Option<u64> {
    value.as_object()?.get("id")?.as_u64()
}

/// ¿Es una lista de cosas con id (párrafos, planos, filas…)? Esas se combinan una por una.
fn is_id_list(items: &[Value]) -> bool {
    !items.is_empty() && items.iter().all(|v| id_of(v).is_some())
}

/// La combinación de tres vías de un valor: `base` es lo que las dos copias tenían antes, `mine` lo que cambié yo y `theirs` lo
/// que cambió la otra persona. Lo que cambió uno solo se toma; lo que cambiaron los dos se combina por campo y, en las listas
/// de cosas con id, cosa por cosa. Si los dos cambiaron lo mismo de distinta forma queda un conflicto (en `conflicts`) y se
/// conserva lo mío.
fn merge_opt(
    base: Option<&Value>,
    mine: Option<&Value>,
    theirs: Option<&Value>,
    path: &str,
    conflicts: &mut Vec<String>,
) -> Option<Value> {
    if mine == theirs {
        return mine.cloned();
    }
    if mine == base {
        return theirs.cloned();
    }
    if theirs == base {
        return mine.cloned();
    }
    match (mine, theirs) {
        (Some(Value::Object(m)), Some(Value::Object(t))) => {
            let empty = Map::new();
            let b = base.and_then(Value::as_object).unwrap_or(&empty);
            let mut out = Map::new();
            let keys: std::collections::BTreeSet<&String> =
                m.keys().chain(t.keys()).chain(b.keys()).collect();
            for key in keys {
                // El contador de ids no se combina: cada persona usa el suyo (ver `Project::use_id_block`).
                if key == "next_id" {
                    if let Some(v) = m.get(key).or(t.get(key)) {
                        out.insert(key.clone(), v.clone());
                    }
                    continue;
                }
                let sub = format!("{path}/{key}");
                if let Some(v) = merge_opt(b.get(key), m.get(key), t.get(key), &sub, conflicts) {
                    out.insert(key.clone(), v);
                }
            }
            Some(Value::Object(out))
        }
        (Some(Value::Array(m)), Some(Value::Array(t)))
            if is_id_list(m) && is_id_list(t) && base.is_none_or(|b| b.as_array().is_some_and(|a| a.is_empty() || is_id_list(a))) =>
        {
            let b: Vec<Value> = base.and_then(Value::as_array).cloned().unwrap_or_default();
            let find = |list: &[Value], id: u64| list.iter().find(|v| id_of(v) == Some(id)).cloned();
            let mut order: Vec<u64> = m.iter().filter_map(id_of).collect();
            for id in t.iter().filter_map(id_of) {
                if !order.contains(&id) {
                    order.push(id);
                }
            }
            // Lo que estaba antes y ya no está en mi lista lo borré yo: entra al recorrido para decidir si la otra persona
            // lo cambió.
            for id in b.iter().filter_map(id_of) {
                if !order.contains(&id) {
                    order.push(id);
                }
            }
            let mut out = Vec::new();
            for id in order {
                let sub = format!("{path}#{id}");
                if let Some(v) = merge_opt(
                    find(&b, id).as_ref(),
                    find(m, id).as_ref(),
                    find(t, id).as_ref(),
                    &sub,
                    conflicts,
                ) {
                    out.push(v);
                }
            }
            Some(Value::Array(out))
        }
        _ => {
            conflicts.push(path.to_string());
            mine.cloned()
        }
    }
}

/// Combina una sección: lo que cambió cada uno sobre la base común. `Err` lleva los lugares donde los dos tocaron lo mismo.
pub fn merge_section(base: &[u8], mine: &[u8], theirs: &[u8]) -> Result<Vec<u8>, Vec<String>> {
    let parse = |b: &[u8]| serde_json::from_slice::<Value>(b).ok();
    let (Some(mine_v), Some(theirs_v)) = (parse(mine), parse(theirs)) else {
        return Err(vec!["la sección".to_string()]);
    };
    let base_v = parse(base);
    let mut conflicts = Vec::new();
    let merged = merge_opt(base_v.as_ref(), Some(&mine_v), Some(&theirs_v), "", &mut conflicts);
    match (conflicts.is_empty(), merged) {
        (true, Some(v)) => Ok(pretty(&v)),
        _ => Err(conflicts),
    }
}

/// Cómo se llama, en singular y plural, lo que guarda cada campo de una sección (para decir «agregó un plano»).
fn thing_name(key: &str) -> (&'static str, &'static str) {
    match key {
        "paragraphs" => ("párrafo", "párrafos"),
        "comments" => ("comentario", "comentarios"),
        "shots" => ("plano", "planos"),
        "blockings" => ("planta", "plantas"),
        "characters" => ("personaje", "personajes"),
        "location_profiles" => ("locación", "locaciones"),
        "objects" => ("objeto", "objetos"),
        "elements" => ("elemento", "elementos"),
        "documents" => ("documento", "documentos"),
        "outline" => ("escena del tratamiento", "escenas del tratamiento"),
        "drama" => ("punto de la curva", "puntos de la curva"),
        "sound_cues" => ("sonido", "sonidos"),
        "sound_gear" => ("equipo de sonido", "equipos de sonido"),
        "photo_gear" => ("equipo de fotografía", "equipos de fotografía"),
        "art_items" => ("elemento de arte", "elementos de arte"),
        "breakdown_items" => ("elemento del desglose", "elementos del desglose"),
        "breakdown_categories" => ("categoría", "categorías"),
        "shoot_days" => ("día de rodaje", "días de rodaje"),
        "shoot_locations" => ("lugar de rodaje", "lugares de rodaje"),
        "contacts" => ("contacto", "contactos"),
        _ => ("elemento", "elementos"),
    }
}

/// El texto con que se reconoce una cosa de una lista: su texto, título o nombre.
fn display_text(item: &Value) -> String {
    let obj = item.as_object();
    let text = ["text", "title", "name", "label", "heading", "description"]
        .iter()
        .filter_map(|k| obj?.get(*k)?.as_str())
        .map(str::trim)
        .find(|t| !t.is_empty())
        .unwrap_or("(sin texto)");
    let one_line: String = text.lines().next().unwrap_or("").chars().take(70).collect();
    if text.chars().count() > 70 { format!("{one_line}…") } else { one_line }
}

/// Qué cambió entre dos versiones de una sección, en frases cortas para mostrarle a la persona: «Agregó un párrafo: “Ana abre la
/// puerta.”», «Editó un plano: …», «Borró 2 párrafos». Como máximo `limit` frases. Lo que no se puede describir por cosa (un
/// valor suelto) sale como «Cambió …».
pub fn describe_change(before: &[u8], after: &[u8], limit: usize) -> Vec<String> {
    let parse = |b: &[u8]| serde_json::from_slice::<Value>(b).ok().and_then(|v| if let Value::Object(m) = v { Some(m) } else { None });
    let (Some(old), Some(new)) = (parse(before), parse(after)) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let keys: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    for key in keys {
        if key == "next_id" || old.get(key) == new.get(key) {
            continue;
        }
        let (one, many) = thing_name(key);
        match (old.get(key), new.get(key)) {
            (Some(Value::Array(a)), Some(Value::Array(b))) if is_id_list(b) || is_id_list(a) => {
                fn find(list: &[Value], id: u64) -> Option<&Value> {
                    list.iter().find(|v| id_of(v) == Some(id))
                }
                let added: Vec<&Value> = b.iter().filter(|v| id_of(v).is_some_and(|i| find(a, i).is_none())).collect();
                let changed: Vec<&Value> = b
                    .iter()
                    .filter(|v| id_of(v).is_some_and(|i| find(a, i).is_some_and(|o| o != *v)))
                    .collect();
                let removed = a.iter().filter(|v| id_of(v).is_some_and(|i| find(b, i).is_none())).count();
                for v in added.iter().take(3) {
                    lines.push(format!("Agregó {} «{}»", one, display_text(v)));
                }
                if added.len() > 3 {
                    lines.push(format!("…y {} {many} más", added.len() - 3));
                }
                for v in changed.iter().take(3) {
                    lines.push(format!("Editó {} «{}»", one, display_text(v)));
                }
                if changed.len() > 3 {
                    lines.push(format!("…y editó {} {many} más", changed.len() - 3));
                }
                if removed > 0 {
                    lines.push(if removed == 1 { format!("Borró un {one}") } else { format!("Borró {removed} {many}") });
                }
            }
            (_, Some(Value::String(text))) => {
                lines.push(format!("Cambió el texto: «{}»", display_text(&json!({"text": text}))));
            }
            _ => lines.push(format!("Cambió {}", key.replace('_', " "))),
        }
    }
    lines.truncate(limit);
    lines
}

// ------------------------------------------------------------------------------------------------
// Historial de versiones
// ------------------------------------------------------------------------------------------------

/// Una versión guardada: cuándo, quién, cómo se llama y qué contenido tenía cada sección. El contenido vive aparte, en
/// `historial/contenidos`, con el nombre de su huella: dos versiones con la misma sección comparten el archivo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Version {
    /// Nombre del archivo de la versión (sin la extensión).
    #[serde(skip)]
    pub id: String,
    /// Segundos desde 1970 (UTC).
    pub time: i64,
    pub author: String,
    /// El nombre que le dio la persona; vacío en las automáticas.
    pub name: String,
    pub auto: bool,
    /// Sección -> huella de su contenido.
    pub sections: BTreeMap<String, String>,
}

pub fn history_dir(file: &Path) -> PathBuf {
    data_dir(file).join("historial")
}

fn blobs_dir(file: &Path) -> PathBuf {
    history_dir(file).join("contenidos")
}

/// Una huella estable del contenido (FNV-1a de 64 bits y el largo): sirve de nombre de archivo y es igual en cualquier
/// sistema.
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}-{}", bytes.len())
}

fn slug(text: &str) -> String {
    let s: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "anonimo".to_string() } else { s.chars().take(24).collect() }
}

/// Guarda una versión del proyecto tal como está. Una versión automática idéntica a la última no se guarda (devuelve `None`).
pub fn snapshot(
    project: &Project,
    file: &Path,
    author: &str,
    name: &str,
    auto: bool,
    now: i64,
) -> io::Result<Option<Version>> {
    let sections = split(project)?;
    let prints: BTreeMap<String, String> =
        sections.iter().map(|(n, b)| (n.clone(), fingerprint(b))).collect();
    if auto && list(file).first().is_some_and(|v| v.sections == prints) {
        return Ok(None);
    }
    let blobs = blobs_dir(file);
    fs::create_dir_all(&blobs)?;
    for (n, bytes) in &sections {
        let path = blobs.join(format!("{}.json", prints[n]));
        if !path.exists() {
            write_atomic(&path, bytes)?;
        }
    }
    let version = Version {
        id: String::new(),
        time: now,
        author: author.trim().to_string(),
        name: name.trim().to_string(),
        auto,
        sections: prints,
    };
    // Un archivo por versión, con la fecha y el autor en el nombre: dos personas que guardan a la vez no se pisan.
    let base = format!("{now:012}-{}", slug(author));
    let mut id = base.clone();
    let mut n = 1;
    while history_dir(file).join(format!("{id}.json")).exists() {
        n += 1;
        id = format!("{base}-{n}");
    }
    let bytes = serde_json::to_vec_pretty(&version).map_err(io::Error::other)?;
    write_atomic(&history_dir(file).join(format!("{id}.json")), &bytes)?;
    Ok(Some(Version { id, ..version }))
}

/// Las versiones guardadas, de la más nueva a la más vieja.
pub fn list(file: &Path) -> Vec<Version> {
    let Ok(entries) = fs::read_dir(history_dir(file)) else {
        return Vec::new();
    };
    let mut out: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if !path.is_file() || path.extension().is_none_or(|x| x != "json") {
                return None;
            }
            let mut v: Version = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
            v.id = path.file_stem()?.to_string_lossy().into_owned();
            Some(v)
        })
        .collect();
    out.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| b.id.cmp(&a.id)));
    out
}

/// Qué versiones se descartan, con el tiempo: se conservan todas las del último día, una por día durante el último mes y una
/// por semana después. Las que tienen nombre (o las guardó la persona) no se descartan nunca. `versions` va de la más nueva a
/// la más vieja; devuelve los lugares de las que se sacan.
pub fn prune_plan(versions: &[Version], now: i64) -> Vec<usize> {
    const DAY: i64 = 86_400;
    let mut seen: HashSet<(u8, i64)> = HashSet::new();
    let mut drop = Vec::new();
    for (i, v) in versions.iter().enumerate() {
        let age = now - v.time;
        let day = (v.time - 3 * 3600).div_euclid(DAY);
        let keep = if !v.auto || !v.name.is_empty() || age <= DAY {
            true
        } else if age <= 30 * DAY {
            seen.insert((0, day))
        } else {
            seen.insert((1, day.div_euclid(7)))
        };
        if !keep {
            drop.push(i);
        }
    }
    drop
}

/// Aplica `prune_plan` y borra los contenidos que ninguna versión usa. Devuelve cuántas versiones sacó.
pub fn prune(file: &Path, now: i64) -> io::Result<usize> {
    let versions = list(file);
    let drop = prune_plan(&versions, now);
    for i in &drop {
        let _ = fs::remove_file(history_dir(file).join(format!("{}.json", versions[*i].id)));
    }
    let keep: HashSet<String> = versions
        .iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .flat_map(|(_, v)| v.sections.values().cloned())
        .collect();
    if let Ok(entries) = fs::read_dir(blobs_dir(file)) {
        for entry in entries.flatten() {
            let path = entry.path();
            let used = path.file_stem().is_some_and(|s| keep.contains(s.to_string_lossy().as_ref()));
            if !used {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(drop.len())
}

fn read_blob(file: &Path, print: &str) -> io::Result<Vec<u8>> {
    fs::read(blobs_dir(file).join(format!("{print}.json")))
}

/// Qué secciones de `version` son distintas de las del proyecto actual.
pub fn differing_sections(project: &Project, version: &Version) -> io::Result<Vec<String>> {
    let now: BTreeMap<String, String> =
        split(project)?.iter().map(|(n, b)| (n.clone(), fingerprint(b))).collect();
    let names: std::collections::BTreeSet<&String> =
        now.keys().chain(version.sections.keys()).collect();
    Ok(names
        .into_iter()
        .filter(|n| now.get(*n) != version.sections.get(*n))
        .cloned()
        .collect())
}

/// El proyecto con las secciones `only` (o todas, si es `None`) como estaban en `version`. No toca el disco.
pub fn restore(
    project: &Project,
    file: &Path,
    version: &Version,
    only: Option<&[String]>,
) -> io::Result<Project> {
    let mut sections = split(project)?;
    let names: Vec<String> = sections
        .keys()
        .chain(version.sections.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|n| only.is_none_or(|o| o.contains(n)))
        .collect();
    for name in names {
        match version.sections.get(&name) {
            Some(print) => {
                sections.insert(name, read_blob(file, print)?);
            }
            // La sección no existía en esa versión (estaba vacía): vuelve a su valor de fábrica.
            None => {
                sections.remove(&name);
            }
        }
    }
    let mut restored = join(&sections)?;
    restored.base_dir = project.base_dir.clone();
    restored.normalize();
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("scriptpendra-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> Project {
        let mut p = Project::default();
        p.title = "Corto".into();
        p.paragraphs = vec![
            p.make_paragraph(ParagraphKind::SceneHeading, "INT. CASA - DÍA"),
            p.make_paragraph(ParagraphKind::Action, "Ana abre la puerta."),
        ];
        p.add_role_entry(crate::model::Role::Art, None);
        p
    }

    /// ¿Es el mismo proyecto? Al abrir se ordenan algunas cosas (`normalize`), así que se compara contra eso.
    fn same(opened: &Project, original: &Project) -> bool {
        let mut expected = original.clone();
        expected.normalize();
        serde_json::to_value(opened).unwrap() == serde_json::to_value(&expected).unwrap()
    }

    #[test]
    fn a_project_survives_the_trip_through_a_folder_and_only_changed_sections_are_rewritten() {
        let dir = temp("folder");
        let file = dir.join("Corto.scriptpendra");
        let mut p = sample();
        let first = save(&p, &file).unwrap();
        assert!(first.written.contains(&"guion".to_string()));
        assert!(data_dir(&file).join("guion.json").exists());
        assert!(same(&load(&file).unwrap(), &p));
        // Sin cambios no se escribe nada.
        assert!(save(&p, &file).unwrap().written.is_empty());
        // Un cambio en el guion solo reescribe el guion.
        p.paragraphs[1].text = "Ana cierra la puerta.".into();
        assert_eq!(save(&p, &file).unwrap().written, vec!["guion".to_string()]);
        assert!(same(&load(&file).unwrap(), &p));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_old_single_file_project_opens_and_is_converted_on_save_keeping_a_backup() {
        let dir = temp("legacy");
        let file = dir.join("Viejo.scriptpendra");
        let p = sample();
        fs::write(&file, serde_json::to_string_pretty(&p).unwrap()).unwrap();
        assert!(same(&load(&file).unwrap(), &p));
        save(&p, &file).unwrap();
        assert!(dir.join("Viejo.antes-de-carpeta.json").exists());
        assert!(fs::read_to_string(&file).unwrap().contains(FORMAT_KEY));
        assert!(same(&load(&file).unwrap(), &p));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn versions_share_unchanged_sections_skip_identical_autosaves_and_restore_one_section() {
        let dir = temp("history");
        let file = dir.join("Corto.scriptpendra");
        let mut p = sample();
        save(&p, &file).unwrap();
        let v1 = snapshot(&p, &file, "Ana", "Primer corte", false, 1_000).unwrap().unwrap();
        // Idéntica a la última: una automática no se guarda.
        assert!(snapshot(&p, &file, "Ana", "", true, 1_100).unwrap().is_none());
        // Cambian el guion (Ana) y el arte (Luis): dos versiones distintas.
        p.paragraphs[1].text = "Ana cierra la puerta.".into();
        let v2 = snapshot(&p, &file, "Ana", "", true, 2_000).unwrap().unwrap();
        p.art_items[0].title = "Sillón".into();
        let v3 = snapshot(&p, &file, "Luis", "", true, 2_000).unwrap().unwrap();
        assert_ne!(v2.id, v3.id, "mismo segundo, distinto autor: no se pisan");
        let versions = list(&file);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[2].name, "Primer corte");
        // Lo que no cambió comparte el mismo contenido.
        assert_eq!(v1.sections["plantas"], v3.sections["plantas"]);
        // Qué es distinto de ahora.
        assert_eq!(differing_sections(&p, &v1).unwrap(), vec!["arte".to_string(), "guion".to_string()]);
        // Restaurar solo el guion: el arte de ahora se queda.
        let back = restore(&p, &file, &v1, Some(&["guion".to_string()])).unwrap();
        assert_eq!(back.paragraphs[1].text, "Ana abre la puerta.");
        assert_eq!(back.art_items[0].title, "Sillón");
        // Restaurar todo vuelve al primer corte.
        let all = restore(&p, &file, &v1, None).unwrap();
        assert!(all.art_items[0].title.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn old_automatic_versions_are_thinned_out_but_named_ones_and_recent_ones_stay() {
        let day = 86_400;
        let now = 100 * day;
        let v = |time: i64, name: &str, auto: bool| Version {
            id: String::new(),
            time,
            author: "Ana".into(),
            name: name.into(),
            auto,
            sections: BTreeMap::new(),
        };
        let versions = vec![
            v(now - 3600, "", true),          // del último día: queda
            v(now - 2 * day, "", true),       // la más nueva de su día: queda
            v(now - 2 * day - 3600, "", true), // otra del mismo día: se saca
            v(now - 3 * day, "Entrega", false), // con nombre: queda
            v(now - 50 * day, "", true),      // la primera de su semana: queda
            v(now - 50 * day - 3600, "", true), // de la misma semana: se saca
        ];
        assert_eq!(prune_plan(&versions, now), vec![2, 5]);
    }

    #[test]
    fn pruning_removes_the_contents_nobody_uses() {
        let dir = temp("prune");
        let file = dir.join("Corto.scriptpendra");
        let mut p = sample();
        save(&p, &file).unwrap();
        let old = 1_000;
        snapshot(&p, &file, "Ana", "", true, old).unwrap();
        p.paragraphs[1].text = "Otra cosa.".into();
        snapshot(&p, &file, "Ana", "", true, old + 10).unwrap();
        let blobs = |f: &Path| fs::read_dir(blobs_dir(f)).unwrap().count();
        let before = blobs(&file);
        // Dos versiones del mismo día, hace mucho: queda la más nueva y se va el guion que solo usaba la vieja.
        assert_eq!(prune(&file, old + 90 * 86_400).unwrap(), 1);
        let left = list(&file);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].time, old + 10);
        assert_eq!(blobs(&file), before - 1);
        let _ = fs::remove_dir_all(dir);
    }

    fn section_of_project(p: &Project, name: &str) -> Vec<u8> {
        split(p).unwrap().remove(name).unwrap()
    }

    #[test]
    fn two_copies_that_touch_different_things_merge_and_the_same_thing_is_a_conflict() {
        let mut base = sample();
        let rain = base.make_paragraph(ParagraphKind::Action, "Llueve.");
        base.paragraphs.push(rain);
        let (mut mine, mut theirs) = (base.clone(), base.clone());
        // Yo cambio el primer párrafo de acción y agrego uno; la otra persona cambia el último y agrega otro.
        mine.paragraphs[1].text = "Ana cierra la puerta.".into();
        let a = mine.make_paragraph(ParagraphKind::Action, "Mía.");
        mine.paragraphs.push(a);
        theirs.paragraphs[2].text = "Deja de llover.".into();
        let mut b = theirs.make_paragraph(ParagraphKind::Action, "De ella.");
        b.id += 1_000_000;
        theirs.paragraphs.push(b);
        let merged = merge_section(
            &section_of_project(&base, "guion"),
            &section_of_project(&mine, "guion"),
            &section_of_project(&theirs, "guion"),
        )
        .expect("sin conflictos");
        let value: Value = serde_json::from_slice(&merged).unwrap();
        let texts: Vec<&str> = value["paragraphs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["text"].as_str().unwrap())
            .collect();
        for expected in ["Ana cierra la puerta.", "Deja de llover.", "Mía.", "De ella."] {
            assert!(texts.contains(&expected), "{texts:?}");
        }
        assert_eq!(texts.len(), 5);
        // Los dos tocan el mismo párrafo, distinto: conflicto.
        theirs.paragraphs[1].text = "Ana se queda quieta.".into();
        let conflict = merge_section(
            &section_of_project(&base, "guion"),
            &section_of_project(&mine, "guion"),
            &section_of_project(&theirs, "guion"),
        );
        assert!(conflict.is_err());
        // Si solo cambió la otra persona, se toma lo suyo.
        let only = merge_section(
            &section_of_project(&base, "guion"),
            &section_of_project(&base, "guion"),
            &section_of_project(&theirs, "guion"),
        )
        .unwrap();
        assert_eq!(only, section_of_project(&theirs, "guion"));
    }

    #[test]
    fn a_thing_one_side_deleted_and_the_other_changed_is_a_conflict_but_an_untouched_one_is_deleted() {
        let base = sample();
        let (mut mine, mut theirs) = (base.clone(), base.clone());
        mine.paragraphs.remove(1);
        theirs.paragraphs[0].text = "EXT. CALLE - DÍA".into();
        let merged = merge_section(
            &section_of_project(&base, "guion"),
            &section_of_project(&mine, "guion"),
            &section_of_project(&theirs, "guion"),
        )
        .expect("lo que borré yo y ella no tocó se borra");
        let value: Value = serde_json::from_slice(&merged).unwrap();
        assert_eq!(value["paragraphs"].as_array().unwrap().len(), 1);
        assert_eq!(value["paragraphs"][0]["text"], "EXT. CALLE - DÍA");
        theirs.paragraphs[1].text = "Cambiado por ella.".into();
        assert!(merge_section(
            &section_of_project(&base, "guion"),
            &section_of_project(&mine, "guion"),
            &section_of_project(&theirs, "guion"),
        )
        .is_err());
    }

    #[test]
    fn a_change_is_described_with_the_content_that_changed() {
        let mut base = sample();
        let rain = base.make_paragraph(ParagraphKind::Action, "Llueve.");
        base.paragraphs.push(rain);
        let mut after = base.clone();
        after.paragraphs[1].text = "Ana cierra la puerta.".into();
        let new = after.make_paragraph(ParagraphKind::Action, "Entra Luis.");
        after.paragraphs.push(new);
        after.paragraphs.remove(2);
        let lines = describe_change(&section_of_project(&base, "guion"), &section_of_project(&after, "guion"), 6);
        assert!(lines.contains(&"Agregó párrafo «Entra Luis.»".to_string()), "{lines:?}");
        assert!(lines.contains(&"Editó párrafo «Ana cierra la puerta.»".to_string()), "{lines:?}");
        assert!(lines.contains(&"Borró un párrafo".to_string()), "{lines:?}");
        assert!(describe_change(&section_of_project(&base, "guion"), &section_of_project(&base, "guion"), 6).is_empty());
        assert_eq!(describe_change(b"no json", b"{}", 6), Vec::<String>::new());
    }
}
