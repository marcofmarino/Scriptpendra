//! La conexión con Supabase (ver `supabase/schema.sql` para lo que hace el servidor).
//!
//! Reglas de seguridad de este módulo:
//!   * Solo se habla por HTTPS, con la validación de certificados normal; no hay ninguna opción para saltearla ni se siguen
//!     redirecciones.
//!   * El programa lleva la URL y la clave pública (`anon`). La clave `service_role` no se usa nunca.
//!   * El token de acceso vive solo en memoria; lo único que se guarda es el de renovación, y en el llavero del sistema.
//!   * Nada que escriba la persona o que llegue del servidor se pega en una dirección: los ids, hashes y nombres de sección se
//!     validan con listas estrictas, y las consultas se arman con parámetros.
//!   * Lo que llega del servidor tiene tope de tamaño y nunca hace caer al programa; los mensajes de error no repiten lo que
//!     contesta el servidor (podría incluir datos) ni llevan tokens.

use std::time::Duration;

use serde_json::{Value, json};

/// La nube que viene incorporada al programa al compilarlo (variables de entorno `SCRIPTPENDRA_CLOUD_URL` y
/// `SCRIPTPENDRA_CLOUD_KEY`). Son la dirección y la clave pública (`anon`): no son secretas. Si no se definen, el programa pide
/// configurar la nube a mano.
pub const BUILT_IN_URL: &str = match option_env!("SCRIPTPENDRA_CLOUD_URL") {
    Some(url) => url,
    None => "",
};
pub const BUILT_IN_KEY: &str = match option_env!("SCRIPTPENDRA_CLOUD_KEY") {
    Some(key) => key,
    None => "",
};

/// ¿Esta versión del programa trae la nube incorporada?
pub fn has_built_in() -> bool {
    !BUILT_IN_URL.is_empty() && !BUILT_IN_KEY.is_empty()
}

/// Qué nube se usa: la incorporada, si la hay (y entonces los ajustes no la pueden cambiar), o la que se configuró a mano.
pub fn effective(settings: &crate::storage::Settings) -> (String, String) {
    if has_built_in() {
        (BUILT_IN_URL.trim().to_string(), BUILT_IN_KEY.trim().to_string())
    } else {
        (settings.cloud_url.trim().to_string(), settings.cloud_anon_key.trim().to_string())
    }
}

pub const MAX_SECTION_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 6 * 1024 * 1024;

// ------------------------------------------------------------------------------------------------
// Errores
// ------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudError {
    /// No hay conexión (o el servidor no contesta).
    Offline,
    NotSignedIn,
    BadLogin,
    Forbidden,
    /// Alguien escribió antes: hay que traer lo nuevo, combinar y reintentar.
    Conflict,
    /// El grupo llenó su cuota de imágenes.
    QuotaFull,
    TooLarge,
    TooMany,
    Invalid(String),
    Server(u16),
    /// El servidor contestó algo que no se entiende.
    Protocol,
}

impl CloudError {
    /// El texto para la persona. No incluye nada de lo que contestó el servidor.
    pub fn message(&self) -> String {
        match self {
            CloudError::Offline => "No hay conexión con la nube.".into(),
            CloudError::NotSignedIn => "Tenés que iniciar sesión.".into(),
            CloudError::BadLogin => "El mail o la contraseña no son correctos.".into(),
            CloudError::Forbidden => "No tenés permiso para hacer eso.".into(),
            CloudError::Conflict => "Otra persona guardó antes: hay que traer sus cambios.".into(),
            CloudError::QuotaFull => "El grupo llenó el espacio para imágenes. Quitá algunas que no uses.".into(),
            CloudError::TooLarge => "Es demasiado grande.".into(),
            CloudError::TooMany => "Demasiados intentos seguidos. Esperá un rato.".into(),
            CloudError::Invalid(why) => why.clone(),
            CloudError::Server(code) => format!("La nube tuvo un problema (código {code})."),
            CloudError::Protocol => "La nube contestó algo inesperado.".into(),
        }
    }
}

/// Qué error es, según lo que contestó el servidor. Se mira el código de estado y, para las funciones de la base, su código
/// SQL; el texto no se usa.
fn map_error(status: u16, body: &[u8]) -> CloudError {
    let value: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    // Al iniciar sesión con un mail sin confirmar, Supabase lo avisa con este código.
    if value.get("error_code").and_then(Value::as_str) == Some("email_not_confirmed") {
        return CloudError::Invalid("Falta confirmar el mail: abrí el mensaje que te mandamos y después iniciá sesión.".into());
    }
    let code = value.get("code").map(|c| c.to_string().trim_matches('"').to_string()).unwrap_or_default();
    match code.as_str() {
        "40001" => return CloudError::Conflict,
        "53100" => return CloudError::QuotaFull,
        "54000" => return CloudError::TooLarge,
        "53400" => return CloudError::TooMany,
        "42501" => return CloudError::Forbidden,
        "28000" => return CloudError::NotSignedIn,
        "22023" => return CloudError::Invalid("Los datos no son válidos.".into()),
        _ => {}
    }
    match status {
        400 | 422 => CloudError::BadLogin,
        401 => CloudError::NotSignedIn,
        403 => CloudError::Forbidden,
        409 => CloudError::Conflict,
        413 => CloudError::TooLarge,
        429 => CloudError::TooMany,
        500..=599 => CloudError::Server(status),
        _ => CloudError::Protocol,
    }
}

// ------------------------------------------------------------------------------------------------
// Validaciones estrictas
// ------------------------------------------------------------------------------------------------

/// La URL del proyecto de Supabase, normalizada. Solo `https://`, sin usuario ni contraseña, sin camino, sin parámetros y con un
/// nombre de servidor de letras, números, puntos y guiones.
pub fn parse_url(raw: &str) -> Result<String, String> {
    let text = raw.trim().trim_end_matches('/');
    let rest = match text.strip_prefix("https://") {
        Some(rest) => rest,
        // Las pruebas hablan con un servidor local.
        None if cfg!(test) && text.starts_with("http://127.0.0.1") => &text["http://".len()..],
        None => return Err("La dirección tiene que empezar con https://".into()),
    };
    let host_ok = !rest.is_empty()
        && rest.len() <= 253
        && rest.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':')
        })
        && rest.contains('.');
    if !host_ok {
        return Err("La dirección no es válida (tiene que ser algo como https://xxxx.supabase.co).".into());
    }
    Ok(format!("{}{rest}", if text.starts_with("https://") { "https://" } else { "http://" }))
}

pub fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
        })
}

pub fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

pub fn is_section_name(s: &str) -> bool {
    (3..=20).contains(&s.len()) && s.chars().all(|c| c.is_ascii_lowercase())
}

pub fn is_image_ext(s: &str) -> bool {
    matches!(s, "jpg" | "png" | "webp")
}

/// Decodifica base64url sin relleno (lo que usan las partes de un JWT). `None` si no es válido.
fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0u32, 0u32);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Comprueba que lo que se pegó es una clave PÚBLICA de Supabase y no una secreta. Devuelve si es del formato nuevo
/// (`sb_publishable_…`). Se rechazan las claves secretas (`sb_secret_…`, o un JWT con rol `service_role`): una clave así dentro
/// del programa daría acceso total a la base a cualquiera que lo tenga.
pub fn check_public_key(key: &str) -> Result<bool, String> {
    let secret = "Esa es una clave secreta: no la pegues en el programa. Usá la clave pública («anon» o «publishable»).";
    if key.starts_with("sb_secret_") {
        return Err(secret.into());
    }
    if let Some(rest) = key.strip_prefix("sb_publishable_") {
        let ok = (16..=200).contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
        return if ok { Ok(true) } else { Err("La clave pública no tiene el formato esperado.".into()) };
    }
    let shape_ok = key.len() >= 20
        && key.len() <= 4096
        && key.split('.').count() == 3
        && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '='));
    if !shape_ok {
        return Err("La clave pública no tiene el formato esperado.".into());
    }
    // Un JWT: se mira el rol que declara. Solo vale `anon`.
    let payload = key.split('.').nth(1).and_then(base64url_decode).and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    match payload.as_ref().and_then(|p| p.get("role")).and_then(Value::as_str) {
        Some("anon") => Ok(false),
        Some("service_role") => Err(secret.into()),
        _ => Err("Esa clave no es la clave pública (anon).".into()),
    }
}

/// Un mail razonable: lo justo para no mandar basura al servidor (el servidor lo valida de verdad).
pub fn is_email_shape(s: &str) -> bool {
    let s = s.trim();
    s.len() <= 254
        && s.matches('@').count() == 1
        && !s.starts_with('@')
        && !s.ends_with('@')
        && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// El camino de una imagen en Storage: «grupo/proyecto/hash.ext». Todo validado: no puede salirse de su carpeta.
pub fn image_path(group: &str, project: &str, hash: &str, ext: &str) -> Result<String, CloudError> {
    if is_uuid(group) && is_uuid(project) && is_hash(hash) && is_image_ext(ext) {
        Ok(format!("{group}/{project}/{hash}.{ext}"))
    } else {
        Err(CloudError::Invalid("La imagen no es válida.".into()))
    }
}

// ------------------------------------------------------------------------------------------------
// Transporte
// ------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

// Sin `Debug`: lleva el token de acceso en las cabeceras y no tiene que poder imprimirse por error.
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Lo máximo que se acepta de respuesta.
    pub max_response: usize,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Quien hace el pedido de verdad. Es un tipo aparte para poder probar el cliente con un servidor simulado.
pub trait Transport: Send {
    fn send(&self, request: &Request) -> Result<Response, CloudError>;
}

/// El transporte real: HTTPS con rustls y los certificados raíz de Mozilla, sin redirecciones, con plazos.
pub struct HttpsTransport {
    agent: ureq::Agent,
    base: String,
}

impl HttpsTransport {
    pub fn new(base_url: &str) -> Result<Self, String> {
        let base = parse_url(base_url)?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(!cfg!(test))
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .user_agent("Scriptpendra")
            .build()
            .into();
        Ok(Self { agent, base })
    }
}

impl Transport for HttpsTransport {
    fn send(&self, request: &Request) -> Result<Response, CloudError> {
        let url = format!("{}{}", self.base, request.path);
        let result = match request.method {
            Method::Get => {
                let mut r = self.agent.get(&url);
                for (k, v) in &request.query {
                    r = r.query(k, v);
                }
                for (k, v) in &request.headers {
                    r = r.header(k, v);
                }
                r.call()
            }
            Method::Delete => {
                let mut r = self.agent.delete(&url);
                for (k, v) in &request.query {
                    r = r.query(k, v);
                }
                for (k, v) in &request.headers {
                    r = r.header(k, v);
                }
                r.call()
            }
            Method::Post => {
                let mut r = self.agent.post(&url);
                for (k, v) in &request.query {
                    r = r.query(k, v);
                }
                for (k, v) in &request.headers {
                    r = r.header(k, v);
                }
                r.send(&request.body[..])
            }
        };
        let mut response = result.map_err(|_| CloudError::Offline)?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(request.max_response as u64)
            .read_to_vec()
            .map_err(|_| CloudError::Protocol)?;
        Ok(Response { status, body })
    }
}

// ------------------------------------------------------------------------------------------------
// Sesión
// ------------------------------------------------------------------------------------------------

// Sin `Debug`: lleva los tokens y no tiene que poder imprimirse por error.
#[derive(Clone)]
pub struct Session {
    access_token: String,
    refresh_token: String,
    /// Cuándo vence el token de acceso (segundos desde 1970).
    expires_at: i64,
    pub user_id: String,
    pub email: String,
}

fn now() -> i64 {
    crate::storage::now_unix()
}

const KEYRING_SERVICE: &str = "scriptpendra-nube";

/// Lo único que se guarda de la sesión: el token de renovación, en el llavero del sistema (no en un archivo).
pub fn store_session(url: &str, session: &Session) {
    // Las pruebas no tocan el llavero real de quien las corre.
    if cfg!(test) {
        return;
    }
    let value = json!({"refresh": session.refresh_token, "user": session.user_id, "email": session.email}).to_string();
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, url) {
        let _ = entry.set_password(&value);
    }
}

pub fn forget_session(url: &str) {
    if cfg!(test) {
        return;
    }
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, url) {
        let _ = entry.delete_credential();
    }
}

fn stored_session(url: &str) -> Option<Session> {
    if cfg!(test) {
        return None;
    }
    let text = keyring::Entry::new(KEYRING_SERVICE, url).ok()?.get_password().ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    Some(Session {
        access_token: String::new(),
        refresh_token: v.get("refresh")?.as_str()?.to_string(),
        expires_at: 0,
        user_id: v.get("user")?.as_str()?.to_string(),
        email: v.get("email")?.as_str()?.to_string(),
    })
}

// ------------------------------------------------------------------------------------------------
// Cliente
// ------------------------------------------------------------------------------------------------

/// Otra persona que está en el proyecto: quién, en qué pantalla y un detalle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub who: String,
    pub screen: String,
    pub detail: String,
}

/// Qué pasó al crear una cuenta.
#[derive(Debug, PartialEq, Eq)]
pub enum SignUp {
    SignedIn,
    /// Falta confirmar el mail.
    NeedsConfirmation,
}

pub struct Client<T: Transport> {
    anon_key: String,
    /// La clave es del formato nuevo (`sb_publishable_…`), que no es un JWT y no va en la cabecera `Authorization`.
    publishable: bool,
    url: String,
    transport: T,
    session: Option<Session>,
}

impl Client<HttpsTransport> {
    /// Un cliente para la nube configurada. Si había una sesión guardada en el llavero, queda lista para renovarse.
    pub fn connect(url: &str, anon_key: &str) -> Result<Self, String> {
        let transport = HttpsTransport::new(url)?;
        let mut client = Client::new(parse_url(url)?, anon_key, transport)?;
        client.session = stored_session(&client.url);
        Ok(client)
    }
}

impl<T: Transport> Client<T> {
    pub fn new(url: String, anon_key: &str, transport: T) -> Result<Self, String> {
        let key = anon_key.trim();
        let publishable = check_public_key(key)?;
        Ok(Self { anon_key: key.to_string(), publishable, url, transport, session: None })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn email(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.email.as_str())
    }

    fn request(&self, method: Method, path: &str, query: Vec<(String, String)>, body: Vec<u8>, max: usize) -> Request {
        let token = self.session.as_ref().filter(|s| !s.access_token.is_empty()).map(|s| s.access_token.as_str());
        let mut headers = vec![("apikey".to_string(), self.anon_key.clone())];
        // Con la sesión iniciada va el token de la persona. Sin sesión, la clave pública de formato JWT (la nueva, `sb_publishable_`,
        // no es un JWT y viaja solo en `apikey`).
        match (token, self.publishable) {
            (Some(t), _) => headers.push(("Authorization".into(), format!("Bearer {t}"))),
            (None, false) => headers.push(("Authorization".into(), format!("Bearer {}", self.anon_key))),
            (None, true) => {}
        }
        Request { method, path: path.to_string(), query, headers, body, max_response: max }
    }

    fn send_json(&self, request: Request) -> Result<Value, CloudError> {
        let response = self.transport.send(&request)?;
        if !(200..300).contains(&response.status) {
            return Err(map_error(response.status, &response.body));
        }
        if response.body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&response.body).map_err(|_| CloudError::Protocol)
    }

    fn session_from(&self, v: &Value) -> Result<Session, CloudError> {
        let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let access = text(v, "access_token").ok_or(CloudError::Protocol)?;
        let refresh = text(v, "refresh_token").ok_or(CloudError::Protocol)?;
        let user = v.get("user").ok_or(CloudError::Protocol)?;
        let user_id = text(user, "id").filter(|id| is_uuid(id)).ok_or(CloudError::Protocol)?;
        Ok(Session {
            access_token: access,
            refresh_token: refresh,
            expires_at: now() + v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600),
            user_id,
            email: text(user, "email").unwrap_or_default().chars().take(254).collect(),
        })
    }

    pub fn sign_in(&mut self, email: &str, password: &str) -> Result<(), CloudError> {
        if !is_email_shape(email) || password.is_empty() || password.len() > 256 {
            return Err(CloudError::BadLogin);
        }
        let body = json!({"email": email.trim(), "password": password}).to_string().into_bytes();
        let request = self.request(
            Method::Post,
            "/auth/v1/token",
            vec![("grant_type".into(), "password".into())],
            body,
            MAX_JSON_BYTES,
        );
        let value = self.send_json(request)?;
        let session = self.session_from(&value)?;
        store_session(&self.url, &session);
        self.session = Some(session);
        Ok(())
    }

    pub fn sign_up(&mut self, email: &str, password: &str) -> Result<SignUp, CloudError> {
        if !is_email_shape(email) {
            return Err(CloudError::Invalid("El mail no parece válido.".into()));
        }
        if password.chars().count() < 10 || password.len() > 256 {
            return Err(CloudError::Invalid("La contraseña tiene que tener al menos 10 caracteres.".into()));
        }
        let body = json!({"email": email.trim(), "password": password}).to_string().into_bytes();
        let request = self.request(Method::Post, "/auth/v1/signup", Vec::new(), body, MAX_JSON_BYTES);
        let value = self.send_json(request)?;
        if value.get("access_token").is_some() {
            let session = self.session_from(&value)?;
            store_session(&self.url, &session);
            self.session = Some(session);
            Ok(SignUp::SignedIn)
        } else {
            Ok(SignUp::NeedsConfirmation)
        }
    }

    /// Renueva el token de acceso si venció o está por vencer. Sin sesión da `NotSignedIn`.
    pub fn ensure_session(&mut self) -> Result<(), CloudError> {
        let Some(session) = &self.session else {
            return Err(CloudError::NotSignedIn);
        };
        if !session.access_token.is_empty() && session.expires_at > now() + 60 {
            return Ok(());
        }
        let body = json!({"refresh_token": session.refresh_token}).to_string().into_bytes();
        let request = self.request(
            Method::Post,
            "/auth/v1/token",
            vec![("grant_type".into(), "refresh_token".into())],
            body,
            MAX_JSON_BYTES,
        );
        match self.send_json(request).and_then(|v| self.session_from(&v)) {
            Ok(fresh) => {
                store_session(&self.url, &fresh);
                self.session = Some(fresh);
                Ok(())
            }
            // El token de renovación ya no sirve: hay que volver a entrar.
            Err(CloudError::BadLogin | CloudError::NotSignedIn | CloudError::Forbidden) => {
                forget_session(&self.url);
                self.session = None;
                Err(CloudError::NotSignedIn)
            }
            Err(e) => Err(e),
        }
    }

    pub fn sign_out(&mut self) {
        if self.session.is_some() {
            let request = self.request(Method::Post, "/auth/v1/logout", Vec::new(), Vec::new(), 1024);
            let _ = self.transport.send(&request);
        }
        forget_session(&self.url);
        self.session = None;
    }

    // --- Datos -------------------------------------------------------------------------------------------------

    fn rpc(&mut self, name: &str, args: Value) -> Result<Value, CloudError> {
        self.ensure_session()?;
        let request = self.request(
            Method::Post,
            &format!("/rest/v1/rpc/{name}"),
            Vec::new(),
            args.to_string().into_bytes(),
            MAX_JSON_BYTES,
        );
        let mut request = request;
        request.headers.push(("Content-Type".into(), "application/json".into()));
        self.send_json(request)
    }

    fn select(&mut self, table: &str, query: Vec<(&str, String)>, max: usize) -> Result<Value, CloudError> {
        self.ensure_session()?;
        let query = query.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let request = self.request(Method::Get, &format!("/rest/v1/{table}"), query, Vec::new(), max);
        self.send_json(request)
    }

    fn uuid_arg(value: &str) -> Result<(), CloudError> {
        if is_uuid(value) { Ok(()) } else { Err(CloudError::Invalid("Identificador inválido.".into())) }
    }

    pub fn create_group(&mut self, name: &str) -> Result<String, CloudError> {
        let v = self.rpc("create_group", json!({"group_name": name}))?;
        v.as_str().filter(|s| is_uuid(s)).map(str::to_string).ok_or(CloudError::Protocol)
    }

    pub fn create_invite(&mut self, group: &str) -> Result<String, CloudError> {
        Self::uuid_arg(group)?;
        let v = self.rpc("create_invite", json!({"gid": group}))?;
        v.as_str().map(str::to_string).ok_or(CloudError::Protocol)
    }

    pub fn join_group(&mut self, code: &str) -> Result<String, CloudError> {
        let code = code.trim();
        if code.len() < 20 || code.len() > 80 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(CloudError::Invalid("El código no es válido.".into()));
        }
        let v = self.rpc("join_group", json!({"code": code}))?;
        v.as_str().filter(|s| is_uuid(s)).map(str::to_string).ok_or(CloudError::Protocol)
    }

    pub fn create_project(&mut self, group: &str, name: &str) -> Result<String, CloudError> {
        Self::uuid_arg(group)?;
        let v = self.rpc("create_project", json!({"gid": group, "project_name": name}))?;
        v.as_str().filter(|s| is_uuid(s)).map(str::to_string).ok_or(CloudError::Protocol)
    }

    /// Los grupos de la persona: (id, nombre, cuota de imágenes).
    pub fn groups(&mut self) -> Result<Vec<(String, String, i64)>, CloudError> {
        let v = self.select("groups", vec![("select", "id,name,image_quota_bytes".into()), ("order", "name.asc".into())], MAX_JSON_BYTES)?;
        let rows = v.as_array().ok_or(CloudError::Protocol)?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let id = r.get("id")?.as_str().filter(|s| is_uuid(s))?.to_string();
                let name = r.get("name")?.as_str()?.chars().take(80).collect();
                Some((id, name, r.get("image_quota_bytes")?.as_i64()?))
            })
            .collect())
    }

    /// Los proyectos de un grupo: (id, nombre).
    pub fn projects(&mut self, group: &str) -> Result<Vec<(String, String)>, CloudError> {
        Self::uuid_arg(group)?;
        let v = self.select(
            "projects",
            vec![("select", "id,name".into()), ("group_id", format!("eq.{group}")), ("order", "name.asc".into())],
            MAX_JSON_BYTES,
        )?;
        let rows = v.as_array().ok_or(CloudError::Protocol)?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let id = r.get("id")?.as_str().filter(|s| is_uuid(s))?.to_string();
                Some((id, r.get("name")?.as_str()?.chars().take(120).collect()))
            })
            .collect())
    }

    /// Las secciones de un proyecto con su versión: (nombre, versión).
    pub fn section_versions(&mut self, project: &str) -> Result<Vec<(String, i64)>, CloudError> {
        Self::uuid_arg(project)?;
        let v = self.select(
            "sections",
            vec![("select", "name,version".into()), ("project_id", format!("eq.{project}"))],
            MAX_JSON_BYTES,
        )?;
        let rows = v.as_array().ok_or(CloudError::Protocol)?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let name = r.get("name")?.as_str().filter(|n| is_section_name(n))?.to_string();
                Some((name, r.get("version")?.as_i64()?))
            })
            .collect())
    }

    /// El contenido de una sección y su versión.
    pub fn section(&mut self, project: &str, name: &str) -> Result<(Vec<u8>, i64), CloudError> {
        Self::uuid_arg(project)?;
        if !is_section_name(name) {
            return Err(CloudError::Invalid("Sección inválida.".into()));
        }
        let v = self.select(
            "sections",
            vec![
                ("select", "content,version".into()),
                ("project_id", format!("eq.{project}")),
                ("name", format!("eq.{name}")),
            ],
            MAX_SECTION_BYTES * 2,
        )?;
        let row = v.as_array().and_then(|a| a.first()).ok_or(CloudError::Protocol)?;
        let content = row.get("content").and_then(Value::as_str).ok_or(CloudError::Protocol)?;
        if content.len() > MAX_SECTION_BYTES {
            return Err(CloudError::TooLarge);
        }
        Ok((content.as_bytes().to_vec(), row.get("version").and_then(Value::as_i64).ok_or(CloudError::Protocol)?))
    }

    /// Sube una sección si nadie escribió desde `base_version` (0 si es nueva). Devuelve la versión nueva.
    pub fn push_section(&mut self, project: &str, name: &str, base_version: i64, content: &[u8]) -> Result<i64, CloudError> {
        Self::uuid_arg(project)?;
        if !is_section_name(name) {
            return Err(CloudError::Invalid("Sección inválida.".into()));
        }
        if content.len() > MAX_SECTION_BYTES {
            return Err(CloudError::TooLarge);
        }
        let text = std::str::from_utf8(content).map_err(|_| CloudError::Invalid("La sección no es texto.".into()))?;
        let v = self.rpc(
            "push_section",
            json!({"pid": project, "section": name, "base_version": base_version, "new_content": text}),
        )?;
        v.as_i64().ok_or(CloudError::Protocol)
    }

    // --- Presencia ---------------------------------------------------------------------------------------------

    /// Avisa que estoy en el proyecto: mi nombre, la pantalla y un detalle (por ejemplo «escena 3»).
    pub fn set_presence(&mut self, project: &str, who: &str, screen: &str, detail: &str) -> Result<(), CloudError> {
        Self::uuid_arg(project)?;
        let cut = |s: &str, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
        self.rpc(
            "set_presence",
            json!({"pid": project, "p_who": cut(who, 60), "p_screen": cut(screen, 40), "p_detail": cut(detail, 120)}),
        )
        .map(|_| ())
    }

    /// Quién más está en el proyecto ahora.
    pub fn presence(&mut self, project: &str) -> Result<Vec<Peer>, CloudError> {
        Self::uuid_arg(project)?;
        let v = self.rpc("project_presence", json!({"pid": project}))?;
        let rows = v.as_array().ok_or(CloudError::Protocol)?;
        // Lo que llega es texto de otra persona: se limpia y se acota antes de mostrarlo.
        let clean = |r: &Value, k: &str, n: usize| {
            r.get(k).and_then(Value::as_str).unwrap_or("").chars().filter(|c| !c.is_control()).take(n).collect::<String>()
        };
        Ok(rows
            .iter()
            .take(50)
            .map(|r| Peer { who: clean(r, "who", 60), screen: clean(r, "screen", 40), detail: clean(r, "detail", 120) })
            .collect())
    }

    // --- Imágenes ----------------------------------------------------------------------------------------------

    /// Reserva lugar y sube la imagen. `Ok(true)` si se subió el archivo, `Ok(false)` si ya estaba en la nube.
    ///
    /// Si la reserva ya existía (por ejemplo, otra computadora reservó y se apagó antes de subir el archivo), igual se intenta
    /// subirlo: si ya está, Storage dice que es un duplicado y listo; si faltaba, queda completo.
    pub fn upload_image(&mut self, group: &str, project: &str, image: &PreparedImage) -> Result<bool, CloudError> {
        let path = image_path(group, project, &image.hash, image.ext)?;
        let reserved = self.rpc(
            "reserve_image",
            json!({"pid": project, "image_hash": image.hash, "image_ext": image.ext, "image_size": image.bytes.len()}),
        )?;
        let was_reserved = reserved.as_str() == Some("existe");
        let mut request = self.request(
            Method::Post,
            &format!("/storage/v1/object/images/{path}"),
            Vec::new(),
            image.bytes.clone(),
            MAX_JSON_BYTES,
        );
        request.headers.push(("Content-Type".into(), image.mime().into()));
        request.headers.push(("x-upsert".into(), "false".into()));
        let response = match self.transport.send(&request) {
            Ok(r) => r,
            Err(e) => {
                if !was_reserved {
                    let _ = self.rpc("release_image", json!({"pid": project, "image_hash": image.hash}));
                }
                return Err(e);
            }
        };
        if (200..300).contains(&response.status) {
            return Ok(true);
        }
        // El archivo ya estaba subido: Storage lo avisa como duplicado.
        let body = String::from_utf8_lossy(&response.body);
        if response.status == 409 || body.contains("Duplicate") || body.contains("already exists") {
            return Ok(false);
        }
        if !was_reserved {
            // La reserva quedó sin archivo: se libera para no gastar cuota.
            let _ = self.rpc("release_image", json!({"pid": project, "image_hash": image.hash}));
        }
        Err(map_error(response.status, &response.body))
    }

    pub fn download_image(&mut self, group: &str, project: &str, hash: &str, ext: &str) -> Result<Vec<u8>, CloudError> {
        let path = image_path(group, project, hash, ext)?;
        self.ensure_session()?;
        let request = self.request(
            Method::Get,
            &format!("/storage/v1/object/authenticated/images/{path}"),
            Vec::new(),
            Vec::new(),
            MAX_IMAGE_BYTES + 1,
        );
        let response = self.transport.send(&request)?;
        if !(200..300).contains(&response.status) {
            return Err(map_error(response.status, &response.body));
        }
        // Lo que baja se vuelve a comprobar: tiene que ser una imagen y su huella tiene que ser la del nombre.
        if response.body.len() > MAX_IMAGE_BYTES || sha256_hex(&response.body) != hash {
            return Err(CloudError::Protocol);
        }
        Ok(response.body)
    }

    /// Quita una imagen del proyecto: libera su lugar en la cuota y borra el archivo.
    pub fn delete_image(&mut self, group: &str, project: &str, hash: &str, ext: &str) -> Result<(), CloudError> {
        let path = image_path(group, project, hash, ext)?;
        self.rpc("release_image", json!({"pid": project, "image_hash": hash}))?;
        let request = self.request(Method::Delete, &format!("/storage/v1/object/images/{path}"), Vec::new(), Vec::new(), 4096);
        let response = self.transport.send(&request)?;
        if (200..300).contains(&response.status) || response.status == 404 {
            Ok(())
        } else {
            Err(map_error(response.status, &response.body))
        }
    }

    /// Las imágenes de un proyecto: (hash, extensión, tamaño).
    #[allow(dead_code)] // para una pantalla de administración de imágenes
    pub fn images(&mut self, project: &str) -> Result<Vec<(String, String, i64)>, CloudError> {
        Self::uuid_arg(project)?;
        let v = self.select(
            "images",
            vec![("select", "hash,ext,size".into()), ("project_id", format!("eq.{project}")), ("order", "created_at.desc".into())],
            MAX_JSON_BYTES,
        )?;
        let rows = v.as_array().ok_or(CloudError::Protocol)?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let hash = r.get("hash")?.as_str().filter(|h| is_hash(h))?.to_string();
                let ext = r.get("ext")?.as_str().filter(|e| is_image_ext(e))?.to_string();
                Some((hash, ext, r.get("size")?.as_i64()?))
            })
            .collect())
    }

    /// Cuánto ocupa el grupo en imágenes: (usado, cuota, cantidad).
    pub fn group_usage(&mut self, group: &str) -> Result<(i64, i64, i64), CloudError> {
        Self::uuid_arg(group)?;
        let v = self.rpc("group_usage", json!({"gid": group}))?;
        let row = v.as_array().and_then(|a| a.first()).ok_or(CloudError::Protocol)?;
        let n = |k: &str| row.get(k).and_then(Value::as_i64).ok_or(CloudError::Protocol);
        Ok((n("used_bytes")?, n("quota_bytes")?, n("image_count")?))
    }

    /// Para quien administra: el uso de cada grupo (id, nombre, miembros, usado, cuota).
    #[allow(dead_code)] // por ahora la administración se hace con SQL (ver `supabase/schema.sql`)
    pub fn admin_usage(&mut self) -> Result<Vec<(String, String, i64, i64, i64)>, CloudError> {
        let v = self.rpc("admin_usage", json!({}))?;
        let rows = v.as_array().ok_or(CloudError::Protocol)?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                Some((
                    r.get("group_id")?.as_str().filter(|s| is_uuid(s))?.to_string(),
                    r.get("group_name")?.as_str()?.chars().take(80).collect(),
                    r.get("members")?.as_i64()?,
                    r.get("used_bytes")?.as_i64()?,
                    r.get("quota_bytes")?.as_i64()?,
                ))
            })
            .collect())
    }

    #[allow(dead_code)]
    pub fn admin_set_quota(&mut self, group: &str, bytes: i64) -> Result<(), CloudError> {
        Self::uuid_arg(group)?;
        self.rpc("admin_set_quota", json!({"gid": group, "bytes": bytes})).map(|_| ())
    }
}

// ------------------------------------------------------------------------------------------------
// Imágenes: se achican y se vuelven a codificar antes de subir
// ------------------------------------------------------------------------------------------------

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Una imagen lista para subir: achicada, sin metadatos y con su huella.
#[derive(Debug, Clone)]
pub struct PreparedImage {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
    pub hash: String,
}

impl PreparedImage {
    pub fn mime(&self) -> &'static str {
        match self.ext {
            "png" => "image/png",
            "webp" => "image/webp",
            _ => "image/jpeg",
        }
    }
}

/// Lo más grande que se acepta de origen (para no gastar memoria con un archivo enorme) y los lados máximos.
const MAX_SOURCE_BYTES: usize = 40 * 1024 * 1024;
const MAX_SOURCE_PIXELS: u64 = 60_000_000;
const MAX_SIDE: u32 = 1600;

/// Decodifica la imagen, la achica a 1600 px de lado como máximo y la vuelve a codificar (JPEG, o PNG si tiene transparencia)
/// de modo que pese menos de 2 MB. Al volver a codificar se pierden los metadatos (ubicación, cámara…).
pub fn prepare_image(source: &[u8]) -> Result<PreparedImage, String> {
    use image::{GenericImageView, ImageReader, codecs::jpeg::JpegEncoder, codecs::png::PngEncoder, ImageEncoder};
    if source.len() > MAX_SOURCE_BYTES {
        return Err("La imagen es demasiado pesada.".into());
    }
    let reader = ImageReader::new(std::io::Cursor::new(source))
        .with_guessed_format()
        .map_err(|_| "No se pudo leer la imagen.".to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    let mut reader = reader;
    reader.limits(limits);
    let (w, h) = reader.into_dimensions().map_err(|_| "No se pudo leer la imagen.".to_string())?;
    if u64::from(w) * u64::from(h) > MAX_SOURCE_PIXELS {
        return Err("La imagen es demasiado grande.".into());
    }
    let mut decoded = ImageReader::new(std::io::Cursor::new(source))
        .with_guessed_format()
        .map_err(|_| "No se pudo leer la imagen.".to_string())?
        .decode()
        .map_err(|_| "No se pudo leer la imagen.".to_string())?;
    let has_alpha = decoded.color().has_alpha();
    let mut side = MAX_SIDE;
    loop {
        if decoded.width().max(decoded.height()) > side {
            decoded = decoded.resize(side, side, image::imageops::FilterType::Lanczos3);
        }
        let mut out = Vec::new();
        if has_alpha {
            let rgba = decoded.to_rgba8();
            PngEncoder::new(&mut out)
                .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
                .map_err(|_| "No se pudo preparar la imagen.".to_string())?;
        } else {
            let rgb = decoded.to_rgb8();
            JpegEncoder::new_with_quality(&mut out, 82)
                .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
                .map_err(|_| "No se pudo preparar la imagen.".to_string())?;
        }
        if out.len() <= MAX_IMAGE_BYTES {
            return Ok(PreparedImage { hash: sha256_hex(&out), ext: if has_alpha { "png" } else { "jpg" }, bytes: out });
        }
        // Todavía pesa de más: se achica un poco más.
        side = side * 3 / 4;
        if side < 200 {
            return Err("No se pudo achicar la imagen lo suficiente.".into());
        }
        let _ = decoded.dimensions();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const KEY: &str = "eyJhbGciOiJIUzI1NiJ9.eyJyb2xlIjoiYW5vbiJ9.firmafirmafirmafirma";
    const UUID_A: &str = "11111111-1111-4111-8111-111111111111";
    const UUID_B: &str = "22222222-2222-4222-8222-222222222222";

    /// Un servidor simulado: guarda los pedidos y contesta lo que se le cargó.
    #[derive(Clone, Default)]
    struct Fake {
        seen: Arc<Mutex<Vec<Request>>>,
        replies: Arc<Mutex<Vec<(u16, String)>>>,
    }

    impl Fake {
        fn reply(&self, status: u16, body: &str) {
            self.replies.lock().unwrap().push((status, body.to_string()));
        }
    }

    impl Transport for Fake {
        fn send(&self, request: &Request) -> Result<Response, CloudError> {
            self.seen.lock().unwrap().push(request.clone());
            let (status, body) = self.replies.lock().unwrap().remove(0);
            Ok(Response { status, body: body.into_bytes() })
        }
    }

    fn client(fake: &Fake) -> Client<Fake> {
        Client::new("https://abc.supabase.co".into(), KEY, fake.clone()).unwrap()
    }

    fn login(fake: &Fake, c: &mut Client<Fake>) {
        fake.reply(
            200,
            &format!(r#"{{"access_token":"tok","refresh_token":"ref","expires_in":3600,"user":{{"id":"{UUID_A}","email":"a@b.co"}}}}"#),
        );
        c.sign_in("a@b.co", "una-clave-larga").unwrap();
        fake.seen.lock().unwrap().clear();
    }

    #[test]
    fn only_https_addresses_with_a_plain_host_are_accepted() {
        assert_eq!(parse_url("https://abc.supabase.co/").unwrap(), "https://abc.supabase.co");
        for bad in [
            "http://abc.supabase.co",
            "ftp://abc.supabase.co",
            "https://",
            "https://abc",
            "https://user:pass@abc.supabase.co",
            "https://abc.supabase.co/rest",
            "https://abc.supabase.co?x=1",
            "https://abc .co",
            "https://-abc.co",
            "https://abc..co",
            "//abc.supabase.co",
            "",
        ] {
            assert!(parse_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ids_hashes_and_names_are_checked_against_strict_lists() {
        assert!(is_uuid(UUID_A) && !is_uuid("../x") && !is_uuid("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA") && !is_uuid(&format!("{UUID_A}x")));
        assert!(is_hash(&"a".repeat(64)) && !is_hash(&"g".repeat(64)) && !is_hash(&"a".repeat(63)));
        assert!(is_section_name("guion") && !is_section_name("Guion") && !is_section_name("gu/ion") && !is_section_name("ab"));
        assert!(image_path(UUID_A, UUID_B, &"a".repeat(64), "png").is_ok());
        for (g, p, h, e) in [
            ("../x", UUID_B, "a".repeat(64), "png"),
            (UUID_A, "..", "a".repeat(64), "png"),
            (UUID_A, UUID_B, "../../etc".to_string(), "png"),
            (UUID_A, UUID_B, "a".repeat(64), "exe"),
        ] {
            assert!(image_path(g, p, &h, e).is_err());
        }
    }

    #[test]
    fn the_public_key_must_look_like_a_jwt() {
        assert!(Client::new("https://a.b.co".into(), "no es una clave", Fake::default()).is_err());
        assert!(Client::new("https://a.b.co".into(), "a.b", Fake::default()).is_err());
        assert!(Client::new("https://a.b.co".into(), KEY, Fake::default()).is_ok());
    }

    #[test]
    fn a_secret_key_is_refused_and_the_new_publishable_format_travels_only_in_apikey() {
        // JWT con rol service_role: {"role":"service_role"}
        let service = "eyJhbGciOiJIUzI1NiJ9.eyJyb2xlIjoic2VydmljZV9yb2xlIn0.firmafirmafirmafirma";
        assert!(check_public_key(service).unwrap_err().contains("secreta"));
        assert!(check_public_key("sb_secret_abcdefghijklmnopqrstuvwxyz").unwrap_err().contains("secreta"));
        // Otro rol cualquiera tampoco.
        assert!(check_public_key("eyJhbGciOiJIUzI1NiJ9.eyJyb2xlIjoiYWRtaW4ifQ.firmafirmafirmafirma").is_err());
        assert_eq!(check_public_key(KEY), Ok(false));
        let publishable = "sb_publishable_AbCdEfGhIjKlMnOpQrStUv_123";
        assert_eq!(check_public_key(publishable), Ok(true));
        assert!(check_public_key("sb_publishable_corta").is_err());
        let fake = Fake::default();
        let mut c = Client::new("https://abc.supabase.co".into(), publishable, fake.clone()).unwrap();
        fake.reply(200, r#"{"x":1}"#);
        let _ = c.sign_up("a@b.co", "una-clave-larga");
        let seen = fake.seen.lock().unwrap().clone();
        assert!(seen[0].headers.iter().any(|(k, v)| k == "apikey" && v == publishable));
        assert!(!seen[0].headers.iter().any(|(k, _)| k == "Authorization"), "la clave nueva no va como Authorization");
    }

    #[test]
    fn requests_carry_the_public_key_and_after_login_the_user_token() {
        let fake = Fake::default();
        let mut c = client(&fake);
        fake.reply(
            200,
            &format!(r#"{{"access_token":"tok","refresh_token":"ref","expires_in":3600,"user":{{"id":"{UUID_A}","email":"a@b.co"}}}}"#),
        );
        c.sign_in("a@b.co", "una-clave-larga").unwrap();
        let seen = fake.seen.lock().unwrap().clone();
        let header = |r: &Request, k: &str| r.headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(header(&seen[0], "apikey").as_deref(), Some(KEY));
        assert_eq!(header(&seen[0], "Authorization").unwrap(), format!("Bearer {KEY}"), "antes de entrar va la clave pública");
        assert_eq!(seen[0].query, vec![("grant_type".to_string(), "password".to_string())]);
        assert_eq!(c.email(), Some("a@b.co"));
        fake.reply(200, &format!(r#""{UUID_B}""#));
        c.create_group("Grupo").unwrap();
        let seen = fake.seen.lock().unwrap().clone();
        assert_eq!(header(&seen[1], "Authorization").as_deref(), Some("Bearer tok"));
        assert_eq!(seen[1].path, "/rest/v1/rpc/create_group");
        c.sign_out_without_keyring();
    }

    #[test]
    fn what_the_server_says_becomes_a_clear_error_without_repeating_its_text() {
        assert_eq!(map_error(400, br#"{"code":"40001","message":"conflicto"}"#), CloudError::Conflict);
        assert_eq!(map_error(400, br#"{"code":"53100","message":"x"}"#), CloudError::QuotaFull);
        assert_eq!(map_error(400, br#"{"code":"54000"}"#), CloudError::TooLarge);
        assert_eq!(map_error(403, br#"{"code":"42501"}"#), CloudError::Forbidden);
        assert_eq!(map_error(401, b"{}"), CloudError::NotSignedIn);
        assert_eq!(map_error(400, b"{}"), CloudError::BadLogin);
        assert!(matches!(
            map_error(400, br#"{"error_code":"email_not_confirmed","msg":"Email not confirmed"}"#),
            CloudError::Invalid(m) if m.contains("confirmar el mail")
        ));
        assert_eq!(map_error(503, b"cualquier cosa"), CloudError::Server(503));
        assert_eq!(map_error(200, b"no es json"), CloudError::Protocol);
        assert!(!CloudError::Server(500).message().contains("select"));
    }

    #[test]
    fn user_input_never_reaches_an_address_unvalidated() {
        let fake = Fake::default();
        let mut c = client(&fake);
        login(&fake, &mut c);
        assert!(matches!(c.projects("../../auth/v1/admin"), Err(CloudError::Invalid(_))));
        assert!(matches!(c.section(UUID_A, "../x"), Err(CloudError::Invalid(_))));
        assert!(matches!(c.push_section("x", "guion", 0, b"{}"), Err(CloudError::Invalid(_))));
        assert!(matches!(c.create_invite("not-a-uuid"), Err(CloudError::Invalid(_))));
        assert!(matches!(c.join_group("../../etc/passwd"), Err(CloudError::Invalid(_))));
        assert!(matches!(c.delete_image(UUID_A, UUID_B, "..", "png"), Err(CloudError::Invalid(_))));
        assert!(fake.seen.lock().unwrap().is_empty(), "no salió ningún pedido");
        // El filtro de una consulta va como parámetro, nunca pegado al camino.
        fake.reply(200, "[]");
        c.projects(UUID_A).unwrap();
        let seen = fake.seen.lock().unwrap().clone();
        assert_eq!(seen[0].path, "/rest/v1/projects");
        assert!(seen[0].query.contains(&("group_id".to_string(), format!("eq.{UUID_A}"))));
        c.sign_out_without_keyring();
    }

    #[test]
    fn pushing_a_section_sends_the_expected_version_and_a_conflict_is_reported() {
        let fake = Fake::default();
        let mut c = client(&fake);
        login(&fake, &mut c);
        fake.reply(200, "2");
        assert_eq!(c.push_section(UUID_B, "guion", 1, br#"{"a":1}"#).unwrap(), 2);
        let body: Value = serde_json::from_slice(&fake.seen.lock().unwrap()[0].body).unwrap();
        assert_eq!(body["base_version"], 1);
        assert_eq!(body["section"], "guion");
        fake.reply(400, r#"{"code":"40001","message":"conflicto"}"#);
        assert_eq!(c.push_section(UUID_B, "guion", 1, b"{}"), Err(CloudError::Conflict));
        let too_big = vec![b'x'; MAX_SECTION_BYTES + 1];
        assert_eq!(c.push_section(UUID_B, "guion", 1, &too_big), Err(CloudError::TooLarge));
        c.sign_out_without_keyring();
    }

    #[test]
    fn a_downloaded_image_must_match_its_hash() {
        let fake = Fake::default();
        let mut c = client(&fake);
        login(&fake, &mut c);
        let data = b"imagen de mentira";
        let hash = sha256_hex(data);
        fake.reply(200, std::str::from_utf8(data).unwrap());
        assert_eq!(c.download_image(UUID_A, UUID_B, &hash, "png").unwrap(), data.to_vec());
        fake.reply(200, "otra cosa");
        assert_eq!(c.download_image(UUID_A, UUID_B, &hash, "png"), Err(CloudError::Protocol));
        c.sign_out_without_keyring();
    }

    #[test]
    fn images_are_shrunk_reencoded_and_refused_when_they_are_not_images() {
        let mut img = image::RgbImage::new(3000, 2000);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let prepared = prepare_image(&png).unwrap();
        assert!(prepared.bytes.len() <= MAX_IMAGE_BYTES);
        assert_eq!(prepared.ext, "jpg");
        let decoded = image::load_from_memory(&prepared.bytes).unwrap();
        assert!(decoded.width().max(decoded.height()) <= MAX_SIDE);
        assert!(is_hash(&prepared.hash));
        assert!(prepare_image(b"esto no es una imagen").is_err());
        assert!(prepare_image(&[]).is_err());
    }

    #[test]
    fn an_image_whose_upload_was_interrupted_is_completed_by_the_next_try_and_a_duplicate_is_not_an_error() {
        let fake = Fake::default();
        let mut c = client(&fake);
        login(&fake, &mut c);
        let image = PreparedImage { bytes: b"datos".to_vec(), ext: "png", hash: sha256_hex(b"datos") };
        // La reserva ya existía (otra computadora se apagó antes de subir) y el archivo faltaba: se sube y queda completo.
        fake.reply(200, r#""existe""#);
        fake.reply(200, "{}");
        assert_eq!(c.upload_image(UUID_A, UUID_B, &image), Ok(true));
        // La reserva existía y el archivo también: Storage avisa duplicado y no es un error.
        fake.reply(200, r#""existe""#);
        fake.reply(400, r#"{"statusCode":"409","error":"Duplicate","message":"The resource already exists"}"#);
        assert_eq!(c.upload_image(UUID_A, UUID_B, &image), Ok(false));
        // Reserva nueva y la subida falla: se libera la reserva (no gasta cuota).
        fake.reply(200, r#""ok""#);
        fake.reply(500, "{}");
        fake.reply(200, "null");
        assert_eq!(c.upload_image(UUID_A, UUID_B, &image), Err(CloudError::Server(500)));
        let seen = fake.seen.lock().unwrap().clone();
        assert!(seen.last().unwrap().path.ends_with("/rpc/release_image"));
        c.sign_out_without_keyring();
    }

    #[test]
    fn presence_is_sent_trimmed_and_what_others_send_is_cleaned_before_showing_it() {
        let fake = Fake::default();
        let mut c = client(&fake);
        login(&fake, &mut c);
        fake.reply(200, "null");
        c.set_presence(UUID_B, &"x".repeat(200), "Guion\n", "escena 3").unwrap();
        let body: Value = serde_json::from_slice(&fake.seen.lock().unwrap()[0].body).unwrap();
        assert_eq!(body["p_who"].as_str().unwrap().len(), 60);
        assert_eq!(body["p_screen"], "Guion");
        fake.reply(200, r#"[{"who":"Ana\u0007","screen":"Guion","detail":"escena 3"}]"#);
        let peers = c.presence(UUID_B).unwrap();
        assert_eq!(peers, vec![Peer { who: "Ana".into(), screen: "Guion".into(), detail: "escena 3".into() }]);
        assert!(matches!(c.presence("../x"), Err(CloudError::Invalid(_))));
        c.sign_out_without_keyring();
    }

    impl<T: Transport> Client<T> {
        /// Las pruebas no tocan el llavero del sistema.
        fn sign_out_without_keyring(&mut self) {
            self.session = None;
        }
    }
}
