//! La sincronización entre la carpeta del proyecto y la nube (ver `cloud` y `folder`).
//!
//! La carpeta local sigue siendo la copia de trabajo: la nube solo es por dónde viajan las secciones. Por eso, si la nube no
//! contesta, nadie pierde nada, y todo lo de combinar cambios y resolver conflictos (que ya hace el programa con los archivos
//! de la carpeta) sirve igual.
//!
//!   * `pull` trae las secciones que cambiaron en la nube y las escribe en la carpeta. Si en la carpeta hay cambios guardados
//!     que todavía no se subieron, los combina.
//!   * `push` sube las secciones que cambiaron en la carpeta, diciéndole al servidor desde qué versión parten. Si alguien se
//!     adelantó, el servidor lo rechaza y se vuelve a empezar con un `pull`.
//!
//! Qué se sabe de la nube por proyecto vive en `Corto.datos/nube.enlace` (la versión de cada sección) y en `nube-base/` (cómo
//! estaba cada sección la última vez que se sincronizó: es la base para combinar).

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    cloud::{self, Client, CloudError, Transport},
    folder,
};

/// A qué proyecto de la nube está unida esta carpeta.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLink {
    pub url: String,
    pub group: String,
    pub project: String,
    /// La versión de cada sección en la nube la última vez que se sincronizó.
    pub versions: BTreeMap<String, i64>,
    /// Quién subió la última versión que se trajo de cada sección (para avisar «Ana cambió el guion»).
    #[serde(default)]
    pub authors: BTreeMap<String, String>,
}

fn link_path(file: &Path) -> PathBuf {
    folder::data_dir(file).join("nube.enlace")
}

fn base_dir(file: &Path) -> PathBuf {
    folder::data_dir(file).join("nube-base")
}

fn conflict_dir(file: &Path) -> PathBuf {
    folder::data_dir(file).join("nube-conflictos")
}

impl CloudLink {
    /// El enlace guardado de un proyecto, si tiene uno válido. Se vuelve a validar todo: es un archivo en una carpeta que otras
    /// personas pueden tocar.
    pub fn load(file: &Path) -> Option<CloudLink> {
        let link: CloudLink = serde_json::from_slice(&fs::read(link_path(file)).ok()?).ok()?;
        let valid = cloud::parse_url(&link.url).is_ok()
            && cloud::is_uuid(&link.group)
            && cloud::is_uuid(&link.project)
            && link.versions.keys().all(|k| cloud::is_section_name(k));
        valid.then_some(link)
    }

    /// Deja de sincronizar la carpeta con la nube (no borra nada de la nube ni de la carpeta).
    pub fn remove(file: &Path) {
        let _ = fs::remove_file(link_path(file));
        let _ = fs::remove_dir_all(base_dir(file));
    }

    pub fn save(&self, file: &Path) -> io::Result<()> {
        fs::create_dir_all(folder::data_dir(file))?;
        write_atomic(&link_path(file), &serde_json::to_vec_pretty(self).map_err(io::Error::other)?)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

fn section_path(file: &Path, name: &str) -> PathBuf {
    folder::data_dir(file).join(format!("{name}.json"))
}

fn base_path(file: &Path, name: &str) -> PathBuf {
    base_dir(file).join(format!("{name}.json"))
}

fn write_base(file: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    fs::create_dir_all(base_dir(file))?;
    write_atomic(&base_path(file, name), bytes)
}

/// Qué pasó en una sincronización.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Secciones que se trajeron de la nube.
    pub pulled: Vec<String>,
    /// Secciones que se combinaron con lo guardado en la carpeta.
    pub merged: Vec<String>,
    /// Secciones que se subieron.
    pub pushed: Vec<String>,
    /// Secciones que se tocaron igual en la carpeta y en la nube: quedó lo de la nube en la carpeta y lo de la carpeta,
    /// respaldado en `nube-conflictos`.
    pub conflicts: Vec<String>,
    /// Alguien se adelantó al subir: hay que volver a empezar por `pull`.
    pub retry: bool,
}

#[derive(Debug)]
pub enum SyncError {
    Cloud(CloudError),
    Disk(io::Error),
}

impl From<CloudError> for SyncError {
    fn from(e: CloudError) -> Self {
        SyncError::Cloud(e)
    }
}

impl From<io::Error> for SyncError {
    fn from(e: io::Error) -> Self {
        SyncError::Disk(e)
    }
}

impl SyncError {
    pub fn message(&self) -> String {
        match self {
            SyncError::Cloud(e) => e.message(),
            SyncError::Disk(e) => format!("No se pudo escribir en la carpeta del proyecto ({}).", e.kind()),
        }
    }
}

impl SyncError {
    /// ¿Es que no hay conexión? No es un error de la persona: los cambios quedan en la carpeta y se suben cuando vuelva.
    pub fn is_offline(&self) -> bool {
        matches!(self, SyncError::Cloud(CloudError::Offline(_)))
    }
}

/// Cuántos cambios de la carpeta todavía no se subieron a la nube (lo que difiere de lo último sincronizado). Cada sección
/// cambiada cuenta como mínimo uno; se acota en 99.
pub fn pending_changes(file: &Path) -> usize {
    let Ok(sections) = folder::read_sections(file) else {
        return 0;
    };
    let mut total = 0;
    for (name, bytes) in sections {
        if !cloud::is_section_name(&name) || bytes.len() > cloud::MAX_SECTION_BYTES {
            continue;
        }
        let base = fs::read(base_path(file, &name)).ok();
        if base.as_deref() == Some(bytes.as_slice()) {
            continue;
        }
        let empty = b"{}".to_vec();
        total += folder::describe_change(base.as_ref().unwrap_or(&empty), &bytes, 99).len().max(1);
    }
    total.min(99)
}

/// Trae de la nube lo que cambió desde la última vez.
pub fn pull<T: Transport>(
    client: &mut Client<T>,
    file: &Path,
    link: &mut CloudLink,
    report: &mut Report,
) -> Result<(), SyncError> {
    pull_with(client, file, link, report, false)
}

/// Igual que `pull`; con `full` no se fía del número del proyecto y compara versión por versión (se usa tras un conflicto).
fn pull_with<T: Transport>(
    client: &mut Client<T>,
    file: &Path,
    link: &mut CloudLink,
    report: &mut Report,
    full: bool,
) -> Result<(), SyncError> {
    // El proyecto tiene que ser de esta misma nube: el archivo de enlace está en una carpeta que otros pueden tocar.
    if link.url != client.url() {
        return Err(CloudError::Invalid("El proyecto está unido a otra nube.".into()).into());
    }
    // Primero una pregunta mínima: si el número del proyecto es el que ya conocemos, no hay nada que traer y se evita bajar la lista.
    let known: i64 = link.versions.values().sum();
    if !full && !link.versions.is_empty() && client.project_stamp(&link.project)? == known {
        return Ok(());
    }
    let remote = client.section_versions(&link.project)?;
    let dir = folder::data_dir(file);
    fs::create_dir_all(&dir)?;
    for (name, version, who) in remote {
        if link.versions.get(&name) == Some(&version) {
            continue;
        }
        link.authors.insert(name.clone(), who);
        let (content, version) = client.section(&link.project, &name)?;
        let disk = fs::read(section_path(file, &name)).ok();
        let base = fs::read(base_path(file, &name)).ok();
        if disk == base {
            // Lo guardado en la carpeta es lo que ya se había sincronizado: se trae lo nuevo y listo.
            write_atomic(&section_path(file, &name), &content)?;
            report.pulled.push(name.clone());
        } else {
            // En la carpeta hay cambios guardados que todavía no se subieron: se combinan con lo de la nube.
            let empty = b"{}".to_vec();
            match folder::merge_section(base.as_ref().unwrap_or(&empty), disk.as_ref().unwrap_or(&empty), &content) {
                Ok(merged) => {
                    write_atomic(&section_path(file, &name), &merged)?;
                    report.merged.push(name.clone());
                }
                Err(_) => {
                    // Se tocó lo mismo en los dos lados: queda lo de la nube y lo local, a salvo.
                    if let Some(local) = &disk {
                        fs::create_dir_all(conflict_dir(file))?;
                        let stamp = crate::storage::now_unix();
                        write_atomic(&conflict_dir(file).join(format!("{name}.{stamp}.json")), local)?;
                    }
                    write_atomic(&section_path(file, &name), &content)?;
                    report.conflicts.push(name.clone());
                }
            }
        }
        write_base(file, &name, &content)?;
        link.versions.insert(name, version);
    }
    link.save(file)?;
    Ok(())
}

/// Sube a la nube las secciones de la carpeta que cambiaron desde la última sincronización.
pub fn push<T: Transport>(
    client: &mut Client<T>,
    file: &Path,
    link: &mut CloudLink,
    report: &mut Report,
    who: &str,
) -> Result<(), SyncError> {
    for (name, bytes) in folder::read_sections(file)? {
        if !cloud::is_section_name(&name) || bytes.len() > cloud::MAX_SECTION_BYTES {
            continue;
        }
        if fs::read(base_path(file, &name)).ok().as_deref() == Some(bytes.as_slice()) {
            continue;
        }
        let base_version = link.versions.get(&name).copied().unwrap_or(0);
        match client.push_section(&link.project, &name, base_version, &bytes, who) {
            Ok(version) => {
                write_base(file, &name, &bytes)?;
                link.versions.insert(name.clone(), version);
                report.pushed.push(name);
            }
            Err(CloudError::Conflict) => {
                report.retry = true;
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }
    link.save(file)?;
    Ok(())
}

/// Un ciclo completo: trae, y si no hubo conflicto, sube.
pub fn sync_once<T: Transport>(
    client: &mut Client<T>,
    file: &Path,
    link: &mut CloudLink,
    who: &str,
) -> Result<Report, SyncError> {
    let mut report = Report::default();
    pull(client, file, link, &mut report)?;
    push(client, file, link, &mut report, who)?;
    if report.retry {
        // Alguien se adelantó y el número del proyecto no alcanzó para notarlo: se reconcilian las versiones una por una.
        pull_with(client, file, link, &mut report, true)?;
    }
    Ok(report)
}

/// Comparte un proyecto en un grupo: crea el proyecto en la nube, sube todas sus secciones y deja el enlace.
pub fn share_project<T: Transport>(
    client: &mut Client<T>,
    group: &str,
    name: &str,
    file: &Path,
    who: &str,
) -> Result<CloudLink, SyncError> {
    let project = client.create_project(group, name)?;
    let mut link = CloudLink {
        url: client.url().to_string(),
        group: group.to_string(),
        project,
        versions: BTreeMap::new(),
        authors: BTreeMap::new(),
    };
    let mut report = Report::default();
    push(client, file, &mut link, &mut report, who)?;
    Ok(link)
}

/// Baja un proyecto de la nube a una carpeta nueva y deja el enlace. `file` es el archivo índice del proyecto nuevo.
pub fn download_project<T: Transport>(
    client: &mut Client<T>,
    group: &str,
    project: &str,
    file: &Path,
) -> Result<CloudLink, SyncError> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut link = CloudLink {
        url: client.url().to_string(),
        group: group.to_string(),
        project: project.to_string(),
        versions: BTreeMap::new(),
        authors: BTreeMap::new(),
    };
    let mut report = Report::default();
    pull(client, file, &mut link, &mut report)?;
    // Se arma el índice del proyecto a partir de lo bajado.
    let loaded = folder::join(&folder::read_sections(file)?)?;
    folder::save(&loaded, file)?;
    Ok(link)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cloud::{Method, Request, Response};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};

    const KEY: &str = "eyJhbGciOiJIUzI1NiJ9.eyJyb2xlIjoiYW5vbiJ9.firmafirmafirmafirma";
    const USER: &str = "11111111-1111-4111-8111-111111111111";
    const GROUP: &str = "22222222-2222-4222-8222-222222222222";
    const PROJECT: &str = "33333333-3333-4333-8333-333333333333";

    /// Un servidor de Supabase de juguete: las secciones de un proyecto con su versión, igual que `push_section` del SQL.
    #[derive(Default)]
    pub(crate) struct Server {
        pub(crate) sections: BTreeMap<String, (String, i64)>,
        who: BTreeMap<String, String>,
        /// Los caminos que se pidieron, para comprobar cuánto se habla con el servidor.
        calls: Vec<String>,
    }

    #[derive(Clone)]
    pub(crate) struct Fake(Arc<Mutex<Server>>);

    impl Transport for Fake {
        fn send(&self, r: &Request) -> Result<Response, CloudError> {
            let reply = |status: u16, v: Value| Ok(Response { status, body: v.to_string().into_bytes() });
            let mut server = self.0.lock().unwrap();
            server.calls.push(r.path.clone());
            match (r.method, r.path.as_str()) {
                (Method::Post, "/rest/v1/rpc/project_stamp") => reply(200, json!(server.sections.values().map(|(_, v)| *v).sum::<i64>())),
                (Method::Post, "/auth/v1/token") => reply(
                    200,
                    json!({"access_token": "t", "refresh_token": "r", "expires_in": 3600, "user": {"id": USER, "email": "a@b.co"}}),
                ),
                (Method::Get, "/rest/v1/sections") => {
                    let select = r.query.iter().find(|(k, _)| k == "select").map(|(_, v)| v.as_str()).unwrap_or("");
                    let wanted = r.query.iter().find(|(k, _)| k == "name").map(|(_, v)| v.trim_start_matches("eq.").to_string());
                    let server_who = server.who.clone();
                    let rows: Vec<Value> = server
                        .sections
                        .iter()
                        .filter(|(n, _)| wanted.as_ref().is_none_or(|w| w == *n))
                        .map(|(n, (c, v))| {
                            if select.contains("content") {
                                json!({"content": c, "version": v})
                            } else {
                                json!({"name": n, "version": v, "updated_who": server_who.get(n).cloned().unwrap_or_default()})
                            }
                        })
                        .collect();
                    reply(200, Value::Array(rows))
                }
                (Method::Post, "/rest/v1/rpc/push_section") => {
                    let body: Value = serde_json::from_slice(&r.body).unwrap();
                    let (name, base, content) = (
                        body["section"].as_str().unwrap().to_string(),
                        body["base_version"].as_i64().unwrap(),
                        body["new_content"].as_str().unwrap().to_string(),
                    );
                    let current = server.sections.get(&name).map_or(0, |(_, v)| *v);
                    if current != base {
                        return reply(400, json!({"code": "40001", "message": "conflicto"}));
                    }
                    server.who.insert(name.clone(), body["p_who"].as_str().unwrap_or("").to_string());
                    server.sections.insert(name, (content, current + 1));
                    reply(200, json!(current + 1))
                }
                (Method::Post, "/rest/v1/rpc/create_project") => reply(200, json!(PROJECT)),
                _ => reply(404, json!({})),
            }
        }
    }

    pub(crate) fn person(server: &Arc<Mutex<Server>>) -> Client<Fake> {
        let mut client = Client::new("https://abc.supabase.co".into(), KEY, Fake(server.clone())).unwrap();
        client.sign_in("a@b.co", "una-clave-larga").unwrap();
        client
    }

    pub(crate) fn folder_for(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("scriptpendra-cloud-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("Corto.scriptpendra")
    }

    pub(crate) fn link() -> CloudLink {
        CloudLink {
            url: "https://abc.supabase.co".into(),
            group: GROUP.into(),
            project: PROJECT.into(),
            versions: BTreeMap::new(),
            authors: BTreeMap::new(),
        }
    }

    fn put(file: &Path, name: &str, text: &str) {
        fs::create_dir_all(folder::data_dir(file)).unwrap();
        fs::write(section_path(file, name), text).unwrap();
    }

    fn get(file: &Path, name: &str) -> String {
        fs::read_to_string(section_path(file, name)).unwrap()
    }

    #[test]
    fn what_one_person_pushes_the_other_pulls() {
        let server = Arc::new(Mutex::new(Server::default()));
        let (mut ana, mut luis) = (person(&server), person(&server));
        let (ana_dir, luis_dir) = (folder_for("ana"), folder_for("luis"));
        let (mut ana_link, mut luis_link) = (link(), link());
        put(&ana_dir, "guion", r#"{"paragraphs": []}"#);
        let r = sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        assert_eq!(r.pushed, vec!["guion".to_string()]);
        let r = sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        assert_eq!(r.pulled, vec!["guion".to_string()]);
        assert_eq!(get(&luis_dir, "guion"), r#"{"paragraphs": []}"#);
        // Sin cambios, no pasa nada de nuevo.
        let r = sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        assert_eq!(r, Report::default());
        // El enlace se recuerda entre sesiones.
        assert_eq!(CloudLink::load(&luis_dir), Some(luis_link));
    }

    #[test]
    fn the_name_of_whoever_pushed_a_section_travels_with_it() {
        let server = Arc::new(Mutex::new(Server::default()));
        let (mut ana, mut luis) = (person(&server), person(&server));
        let (ana_dir, luis_dir) = (folder_for("ana5"), folder_for("luis5"));
        let (mut ana_link, mut luis_link) = (link(), link());
        put(&ana_dir, "guion", r#"{"a":1}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        assert_eq!(luis_link.authors.get("guion").map(String::as_str), Some("Ana"));
    }

    #[test]
    fn a_round_with_nothing_new_only_asks_for_the_project_number() {
        let server = Arc::new(Mutex::new(Server::default()));
        let (mut ana, mut luis) = (person(&server), person(&server));
        let (ana_dir, luis_dir) = (folder_for("ana6"), folder_for("luis6"));
        let (mut ana_link, mut luis_link) = (link(), link());
        put(&ana_dir, "guion", r#"{"a":1}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        server.lock().unwrap().calls.clear();
        let r = sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        assert_eq!(r, Report::default());
        assert_eq!(server.lock().unwrap().calls, vec!["/rest/v1/rpc/project_stamp".to_string()]);
        // Si Ana sube algo, el número cambia y Luis lo trae.
        put(&ana_dir, "guion", r#"{"a":2}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        let r = sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        assert_eq!(r.pulled, vec!["guion".to_string()]);
    }

    #[test]
    fn someone_getting_ahead_is_a_retry_and_the_next_round_merges_both() {
        let server = Arc::new(Mutex::new(Server::default()));
        let (mut ana, mut luis) = (person(&server), person(&server));
        let (ana_dir, luis_dir) = (folder_for("ana2"), folder_for("luis2"));
        let (mut ana_link, mut luis_link) = (link(), link());
        let base = r#"{"paragraphs":[{"id":1,"text":"uno"},{"id":2,"text":"dos"}]}"#;
        put(&ana_dir, "guion", base);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        // Los dos editan párrafos distintos y guardan en su carpeta.
        put(&ana_dir, "guion", r#"{"paragraphs":[{"id":1,"text":"UNO"},{"id":2,"text":"dos"}]}"#);
        put(&luis_dir, "guion", r#"{"paragraphs":[{"id":1,"text":"uno"},{"id":2,"text":"DOS"}]}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        // Luis se adelantó?: no, Ana subió primero. Al subir, a Luis la nube le trae lo de Ana, lo combina y lo sube.
        let r = sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        assert_eq!(r.merged, vec!["guion".to_string()]);
        assert_eq!(r.pushed, vec!["guion".to_string()]);
        let merged: Value = serde_json::from_str(&get(&luis_dir, "guion")).unwrap();
        assert_eq!(merged["paragraphs"][0]["text"], "UNO");
        assert_eq!(merged["paragraphs"][1]["text"], "DOS");
        // Ana lo recibe.
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&get(&ana_dir, "guion")).unwrap(), merged);
    }

    #[test]
    fn a_push_rejected_for_a_stale_version_asks_to_retry_without_losing_anything() {
        let server = Arc::new(Mutex::new(Server::default()));
        let (mut ana, mut luis) = (person(&server), person(&server));
        let (ana_dir, luis_dir) = (folder_for("ana3"), folder_for("luis3"));
        let (mut ana_link, mut luis_link) = (link(), link());
        put(&ana_dir, "guion", r#"{"a":1}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        // Ana sube una versión nueva entre el pull y el push de Luis.
        put(&luis_dir, "guion", r#"{"a":2}"#);
        put(&ana_dir, "guion", r#"{"a":3}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        let mut report = Report::default();
        push(&mut luis, &luis_dir, &mut luis_link, &mut report, "Luis").unwrap();
        assert!(report.retry && report.pushed.is_empty());
        assert_eq!(get(&luis_dir, "guion"), r#"{"a":2}"#, "lo local sigue intacto");
    }

    #[test]
    fn the_same_thing_changed_on_both_sides_keeps_the_cloud_copy_and_backs_up_the_local_one() {
        let server = Arc::new(Mutex::new(Server::default()));
        let (mut ana, mut luis) = (person(&server), person(&server));
        let (ana_dir, luis_dir) = (folder_for("ana4"), folder_for("luis4"));
        let (mut ana_link, mut luis_link) = (link(), link());
        put(&ana_dir, "guion", r#"{"paragraphs":[{"id":1,"text":"uno"}]}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        put(&ana_dir, "guion", r#"{"paragraphs":[{"id":1,"text":"de Ana"}]}"#);
        put(&luis_dir, "guion", r#"{"paragraphs":[{"id":1,"text":"de Luis"}]}"#);
        sync_once(&mut ana, &ana_dir, &mut ana_link, "Ana").unwrap();
        let r = sync_once(&mut luis, &luis_dir, &mut luis_link, "Luis").unwrap();
        assert_eq!(r.conflicts, vec!["guion".to_string()]);
        assert!(get(&luis_dir, "guion").contains("de Ana"));
        let backups: Vec<_> = fs::read_dir(conflict_dir(&luis_dir)).unwrap().flatten().collect();
        assert_eq!(backups.len(), 1);
        assert!(fs::read_to_string(backups[0].path()).unwrap().contains("de Luis"));
    }

    #[test]
    fn changes_not_uploaded_yet_are_counted_until_the_next_sync() {
        let server = Arc::new(Mutex::new(Server::default()));
        let mut ana = person(&server);
        let dir = folder_for("pending");
        let mut ana_link = link();
        put(&dir, "guion", r#"{"paragraphs":[{"id":1,"text":"uno"}]}"#);
        assert!(pending_changes(&dir) >= 1, "una sección nueva todavía no subida");
        sync_once(&mut ana, &dir, &mut ana_link, "Ana").unwrap();
        assert_eq!(pending_changes(&dir), 0);
        put(&dir, "guion", r#"{"paragraphs":[{"id":1,"text":"uno"},{"id":2,"text":"dos"}]}"#);
        assert_eq!(pending_changes(&dir), 1);
        sync_once(&mut ana, &dir, &mut ana_link, "Ana").unwrap();
        assert_eq!(pending_changes(&dir), 0);
    }

    #[test]
    fn a_tampered_link_file_is_ignored() {
        let file = folder_for("enlace");
        put(&file, "guion", "{}");
        for bad in [
            r#"{"url":"http://x.co","group":"22222222-2222-4222-8222-222222222222","project":"33333333-3333-4333-8333-333333333333","versions":{}}"#,
            r#"{"url":"https://abc.supabase.co","group":"../..","project":"33333333-3333-4333-8333-333333333333","versions":{}}"#,
            r#"{"url":"https://abc.supabase.co","group":"22222222-2222-4222-8222-222222222222","project":"33333333-3333-4333-8333-333333333333","versions":{"../x":1}}"#,
            "no es json",
        ] {
            fs::write(link_path(&file), bad).unwrap();
            assert_eq!(CloudLink::load(&file), None, "{bad}");
        }
        link().save(&file).unwrap();
        assert!(CloudLink::load(&file).is_some());
    }

    #[test]
    fn sharing_a_project_uploads_its_sections_and_downloading_rebuilds_it() {
        use crate::model::{ParagraphKind, Project};
        let server = Arc::new(Mutex::new(Server::default()));
        let mut ana = person(&server);
        let src = folder_for("share-src");
        let mut project = Project::default();
        project.title = "Corto".into();
        let heading = project.make_paragraph(ParagraphKind::SceneHeading, "INT. CASA - DÍA");
        project.paragraphs = vec![heading];
        folder::save(&project, &src).unwrap();
        let shared = share_project(&mut ana, GROUP, "Corto", &src, "Ana").unwrap();
        assert_eq!(shared.project, PROJECT);
        assert!(server.lock().unwrap().sections.contains_key("guion"));
        let dst = folder_for("share-dst");
        let mut luis = person(&server);
        download_project(&mut luis, GROUP, PROJECT, &dst).unwrap();
        let opened = folder::load(&dst).unwrap();
        assert_eq!(opened.paragraphs[0].text, "INT. CASA - DÍA");
        assert_eq!(opened.title, "Corto");
    }
}
