//! Actualizaciones: busca la última versión publicada en GitHub (Releases) y la baja.
//!
//! Las dos variantes del programa (la de la facultad y la de GitHub) buscan acá, y las dos bajan el binario de GitHub, que no
//! lleva ninguna nube incorporada: quien venía de la variante de la facultad ya dejó guardada su nube en los ajustes
//! (`Settings::load`), así que la copia nueva sigue conectada.
//!
//! - Windows: el `.exe` nuevo se baja al lado del actual, se verifica su SHA-256 y reemplaza al que está corriendo (que se
//!   renombra a `.old`); la versión nueva se abre la próxima vez que se inicie el programa.
//! - Linux: el programa se reparte como Flatpak, que no se puede cambiar desde adentro. Se baja el `.flatpak` a la carpeta de
//!   descargas y se le pide al sistema que lo abra para instalarlo.
//!
//! Nunca se instala nada cuyo SHA-256 no coincida con el de `SHA256SUMS`, publicado en el mismo release.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use serde_json::Value;
use sha2::{Digest, Sha256};

pub const REPO: &str = "marcofmarino/Scriptpendra";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Cómo se llama en cada release el archivo de este sistema (y `SHA256SUMS`, con la huella de cada uno).
#[cfg(windows)]
const ASSET: &str = "Scriptpendra-windows-x86_64.exe";
#[cfg(not(windows))]
const ASSET: &str = "Scriptpendra-linux-x86_64.flatpak";
const SUMS: &str = "SHA256SUMS";

const MAX_DOWNLOAD: u64 = 300 * 1024 * 1024;
/// Cada cuántos días, como máximo, el programa busca solo (y sin que nadie lo pida).
const CHECK_EVERY_DAYS: u64 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    url: String,
    sums_url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading { done: u64, total: u64 },
    /// Windows: la versión nueva ya reemplazó a esta; se abre al reiniciar.
    #[cfg_attr(not(windows), allow(dead_code))]
    Installed,
    /// Linux: el instalador quedó en esta ruta.
    #[cfg_attr(windows, allow(dead_code))]
    Saved(PathBuf),
    Failed(String),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);
static CTX: Mutex<Option<eframe::egui::Context>> = Mutex::new(None);

pub fn state() -> State {
    STATE.lock().map(|s| s.clone()).unwrap_or(State::Idle)
}

fn set(state: State) {
    if let Ok(mut s) = STATE.lock() {
        *s = state;
    }
    if let Ok(ctx) = CTX.lock()
        && let Some(ctx) = ctx.as_ref()
    {
        ctx.request_repaint();
    }
}

/// Se llama una vez al iniciar: guarda el contexto para repintar cuando termine un hilo y borra lo que dejó la actualización
/// anterior.
pub fn init(ctx: &eframe::egui::Context) {
    if let Ok(mut c) = CTX.lock() {
        *c = Some(ctx.clone());
    }
    cleanup_old();
}

/// Busca solo, a lo sumo una vez por día. Devuelve `true` si cambió el día de la última búsqueda (hay que guardar los ajustes).
pub fn auto_check(last_check: &mut u64) -> bool {
    if cfg!(test) {
        return false;
    }
    let today = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400);
    if today < last_check.saturating_add(CHECK_EVERY_DAYS) {
        return false;
    }
    *last_check = today;
    check();
    true
}

/// Busca la última versión publicada (en un hilo aparte).
pub fn check() {
    if matches!(state(), State::Checking | State::Downloading { .. }) {
        return;
    }
    set(State::Checking);
    std::thread::spawn(|| {
        set(match latest() {
            Ok(Some(release)) => State::Available(release),
            Ok(None) => State::UpToDate,
            Err(why) => State::Failed(why),
        });
    });
}

/// Baja e instala la versión que se encontró (en un hilo aparte).
pub fn install() {
    let State::Available(release) = state() else { return };
    set(State::Downloading { done: 0, total: release.size });
    std::thread::spawn(move || {
        set(match download(&release) {
            Ok(done) => done,
            Err(why) => State::Failed(why),
        });
    });
}

// ------------------------------------------------------------------------------------------------
// Red
// ------------------------------------------------------------------------------------------------

fn agent(global_secs: u64) -> ureq::Agent {
    let builder = ureq::Agent::config_builder();
    // Como en la nube: en Windows se confía en los certificados del sistema.
    #[cfg(windows)]
    let builder = builder.tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build());
    builder
        .https_only(true)
        .max_redirects(5)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(global_secs)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .user_agent("Scriptpendra")
        .build()
        .into()
}

fn why_failed(error: &ureq::Error) -> String {
    match error {
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Timeout(_) => {
            "No se pudo conectar con GitHub: revisá la conexión.".into()
        }
        _ => "No se pudo hablar con GitHub.".into(),
    }
}

/// Pide una dirección. `None` si el servidor contestó 404 (no hay releases).
fn fetch(agent: &ureq::Agent, url: &str) -> Result<Option<ureq::http::Response<ureq::Body>>, String> {
    let response = agent
        .get(url)
        .header("Accept", "application/vnd.github+json, application/octet-stream")
        .call()
        .map_err(|e| why_failed(&e))?;
    match response.status().as_u16() {
        200 => Ok(Some(response)),
        404 => Ok(None),
        403 | 429 => Err("GitHub limitó las consultas: probá de nuevo más tarde.".into()),
        code => Err(format!("GitHub contestó con un error ({code}).")),
    }
}

fn fetch_text(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Option<String>, String> {
    let Some(mut response) = fetch(agent, url)? else { return Ok(None) };
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map(Some)
        .map_err(|_| "La respuesta de GitHub llegó incompleta.".to_string())
}

fn latest() -> Result<Option<Release>, String> {
    let agent = agent(30);
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    match fetch_text(&agent, &url, 2 * 1024 * 1024)? {
        Some(text) => parse_release(&text, CURRENT),
        None => Ok(None),
    }
}

// ------------------------------------------------------------------------------------------------
// Qué dice el release
// ------------------------------------------------------------------------------------------------

/// Los números de una versión («0.2.10» → [0, 2, 10]); lo que sigue a un guion («-beta») no cuenta.
fn numbers(version: &str) -> Vec<u64> {
    let version = version.trim().trim_start_matches('v');
    let version = version.split(['-', '+']).next().unwrap_or("");
    version.split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

fn newer(candidate: &str, current: &str) -> bool {
    let (mut a, mut b) = (numbers(candidate), numbers(current));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

/// Solo se baja de los releases de este repositorio en github.com: lo que diga la respuesta sobre otra dirección no se sigue.
fn trusted(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/releases/download/"))
}

fn parse_release(text: &str, current: &str) -> Result<Option<Release>, String> {
    let json: Value = serde_json::from_str(text).map_err(|_| "La respuesta de GitHub no se entiende.".to_string())?;
    let version = json
        .get("tag_name")
        .and_then(Value::as_str)
        .map(|t| t.trim().trim_start_matches('v').to_string())
        .filter(|t| !t.is_empty())
        .ok_or("El release no trae versión.")?;
    if !newer(&version, current) {
        return Ok(None);
    }
    let assets = json.get("assets").and_then(Value::as_array).cloned().unwrap_or_default();
    let find = |name: &str| {
        assets.iter().find(|a| a.get("name").and_then(Value::as_str) == Some(name)).and_then(|a| {
            let url = a.get("browser_download_url")?.as_str()?.to_string();
            let size = a.get("size").and_then(Value::as_u64).unwrap_or(0);
            trusted(&url).then_some((url, size))
        })
    };
    let (url, size) = find(ASSET).ok_or_else(|| format!("La versión {version} no trae el archivo para este sistema."))?;
    let (sums_url, _) = find(SUMS).ok_or_else(|| format!("La versión {version} no trae las huellas para verificarla."))?;
    Ok(Some(Release { version, url, sums_url, size }))
}

/// La huella de un archivo según un `SHA256SUMS` (el formato de `sha256sum`: «huella  nombre»).
fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        let valid = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
        (file == name && valid).then(|| hash.to_ascii_lowercase())
    })
}

// ------------------------------------------------------------------------------------------------
// Descarga e instalación
// ------------------------------------------------------------------------------------------------

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Dónde queda el archivo bajado.
#[cfg(windows)]
fn destination(_version: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|_| "No se sabe dónde está el programa.".to_string())?;
    Ok(with_suffix(&exe, ".nuevo"))
}

#[cfg(not(windows))]
fn destination(version: &str) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("No se encontró tu carpeta personal.")?;
    let folder = ["Descargas", "Downloads"].iter().map(|n| home.join(n)).find(|p| p.is_dir()).unwrap_or(home);
    Ok(folder.join(format!("Scriptpendra-{version}-linux-x86_64.flatpak")))
}

fn download(release: &Release) -> Result<State, String> {
    let agent = agent(900);
    let sums = fetch_text(&agent, &release.sums_url, 1024 * 1024)?.ok_or("No se encontraron las huellas de la versión.")?;
    let expected = checksum_for(&sums, ASSET).ok_or("Las huellas de la versión no incluyen este archivo.")?;
    let dest = destination(&release.version)?;
    let partial = with_suffix(&dest, ".part");
    let result = download_to(&agent, release, &partial, &expected);
    if let Err(why) = result {
        let _ = fs::remove_file(&partial);
        return Err(why);
    }
    fs::rename(&partial, &dest).map_err(|e| format!("No se pudo guardar el archivo: {e}"))?;
    finish(&dest)
}

fn download_to(agent: &ureq::Agent, release: &Release, partial: &Path, expected: &str) -> Result<(), String> {
    let mut response = fetch(agent, &release.url)?.ok_or("No se encontró el archivo de la versión.")?;
    let total = response.body().content_length().unwrap_or(release.size);
    if total > MAX_DOWNLOAD {
        return Err("El archivo es más grande de lo esperado: no se bajó.".into());
    }
    let mut reader = response.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut file = fs::File::create(partial).map_err(|e| format!("No se pudo escribir en {}: {e}", partial.display()))?;
    let (mut hasher, mut done, mut buf) = (Sha256::new(), 0u64, vec![0u8; 64 * 1024]);
    loop {
        let n = reader.read(&mut buf).map_err(|_| "Se cortó la descarga.".to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| format!("No se pudo escribir el archivo: {e}"))?;
        done += n as u64;
        set(State::Downloading { done, total });
    }
    file.flush().map_err(|e| format!("No se pudo escribir el archivo: {e}"))?;
    if done == 0 || hex(&hasher.finalize()) != expected {
        return Err("El archivo bajado no coincide con su huella: no se instaló.".into());
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(windows)]
fn finish(downloaded: &Path) -> Result<State, String> {
    let exe = std::env::current_exe().map_err(|_| "No se sabe dónde está el programa.".to_string())?;
    swap(&exe, downloaded).map_err(|why| {
        let _ = fs::remove_file(downloaded);
        why
    })?;
    Ok(State::Installed)
}

#[cfg(not(windows))]
fn finish(downloaded: &Path) -> Result<State, String> {
    // Se le pide al sistema que lo abra (en un Flatpak, por el portal); si no puede, queda en la carpeta.
    let _ = std::process::Command::new("xdg-open").arg(downloaded).spawn();
    Ok(State::Saved(downloaded.to_path_buf()))
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Pone `new` en el lugar de `current`, que queda como `current.old`. Si algo falla, todo vuelve a como estaba.
fn swap(current: &Path, new: &Path) -> Result<(), String> {
    let old = with_suffix(current, ".old");
    let _ = fs::remove_file(&old);
    fs::rename(current, &old).map_err(|_| {
        "No se puede escribir en la carpeta donde está el programa: bajá la versión nueva desde GitHub a mano.".to_string()
    })?;
    if let Err(e) = fs::rename(new, current) {
        let _ = fs::rename(&old, current);
        return Err(format!("No se pudo instalar la versión nueva: {e}"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(current, fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// Borra lo que dejó una actualización anterior (el programa viejo, que no se podía borrar mientras corría).
fn cleanup_old() {
    if cfg!(test) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = fs::remove_file(with_suffix(&exe, ".old"));
        let _ = fs::remove_file(with_suffix(&exe, ".nuevo"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json(tag: &str, assets: &[(&str, &str)]) -> String {
        let assets: Vec<Value> = assets
            .iter()
            .map(|(name, url)| serde_json::json!({"name": name, "browser_download_url": url, "size": 1234}))
            .collect();
        serde_json::json!({"tag_name": tag, "assets": assets}).to_string()
    }

    fn url(name: &str) -> String {
        format!("https://github.com/{REPO}/releases/download/v9.9.9/{name}")
    }

    /// Habla con GitHub de verdad: `cargo test asks_github -- --ignored`.
    #[test]
    #[ignore]
    fn asks_github_for_the_latest_release() {
        let found = latest();
        println!("{found:?}");
        assert!(found.is_ok());
    }

    #[test]
    fn versions_are_compared_number_by_number() {
        assert!(newer("0.2.0", "0.1.0"));
        assert!(newer("v0.10.0", "0.9.9"), "10 es más que 9");
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1", "0.1.0"), "0.1 y 0.1.0 son la misma");
        assert!(!newer("0.0.9", "0.1.0"));
        assert!(!newer("0.1.0-beta", "0.1.0"));
    }

    #[test]
    fn a_newer_release_carries_the_file_for_this_system_and_its_checksums() {
        let json = release_json("v9.9.9", &[(ASSET, &url(ASSET)), (SUMS, &url(SUMS)), ("otro.txt", &url("otro.txt"))]);
        let release = parse_release(&json, "0.1.0").unwrap().expect("hay una versión nueva");
        assert_eq!((release.version.as_str(), release.size), ("9.9.9", 1234));
        assert_eq!(release.url, url(ASSET));
        assert_eq!(release.sums_url, url(SUMS));
    }

    #[test]
    fn the_same_or_an_older_release_is_not_an_update() {
        let json = release_json("v0.1.0", &[(ASSET, &url(ASSET)), (SUMS, &url(SUMS))]);
        assert_eq!(parse_release(&json, "0.1.0").unwrap(), None);
    }

    #[test]
    fn a_release_without_the_file_or_the_checksums_is_an_error() {
        let no_file = release_json("v9.9.9", &[(SUMS, &url(SUMS))]);
        assert!(parse_release(&no_file, "0.1.0").is_err());
        let no_sums = release_json("v9.9.9", &[(ASSET, &url(ASSET))]);
        assert!(parse_release(&no_sums, "0.1.0").is_err());
        assert!(parse_release("no es json", "0.1.0").is_err());
    }

    #[test]
    fn files_that_are_not_from_this_repository_are_never_followed() {
        let elsewhere = |name: &str| format!("https://github.com/otro/repo/releases/download/v9.9.9/{name}");
        let json = release_json("v9.9.9", &[(ASSET, &elsewhere(ASSET)), (SUMS, &url(SUMS))]);
        assert!(parse_release(&json, "0.1.0").is_err());
        let plain = release_json("v9.9.9", &[(ASSET, &url(ASSET).replace("https", "http")), (SUMS, &url(SUMS))]);
        assert!(parse_release(&plain, "0.1.0").is_err());
    }

    #[test]
    fn the_checksum_of_a_file_is_read_from_the_sha256sum_format() {
        let hash = "a".repeat(64);
        let other = "b".repeat(64);
        let sums = format!("{other}  Otro.exe\n{hash} *{ASSET}\n");
        assert_eq!(checksum_for(&sums, ASSET), Some(hash));
        assert_eq!(checksum_for(&sums, "falta"), None);
        assert_eq!(checksum_for(&format!("corta  {ASSET}"), ASSET), None, "una huella que no mide 64 no vale");
    }

    #[test]
    fn the_hash_of_known_bytes_is_the_expected_one() {
        let digest = Sha256::digest(b"abc");
        assert_eq!(hex(&digest), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn swapping_keeps_the_old_program_aside_and_undoes_itself_if_it_cannot_finish() {
        let dir = std::env::temp_dir().join(format!("scriptpendra-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (current, new) = (dir.join("Scriptpendra"), dir.join("Scriptpendra.nuevo"));
        fs::write(&current, "viejo").unwrap();
        fs::write(&new, "nuevo").unwrap();
        swap(&current, &new).unwrap();
        assert_eq!(fs::read_to_string(&current).unwrap(), "nuevo");
        assert_eq!(fs::read_to_string(with_suffix(&current, ".old")).unwrap(), "viejo");
        assert!(!new.exists());
        // Si falta el archivo nuevo, el programa queda como estaba.
        assert!(swap(&current, &dir.join("no-existe")).is_err());
        assert_eq!(fs::read_to_string(&current).unwrap(), "nuevo");
        let _ = fs::remove_dir_all(&dir);
    }
}
