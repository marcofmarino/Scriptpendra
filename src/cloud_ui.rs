//! La ventana de la nube: iniciar sesión, grupos, compartir y abrir proyectos, y la sincronización en segundo plano.
//!
//! Los pedidos a la red se hacen en un hilo aparte (`worker`), así la ventana nunca se congela esperando una respuesta. El hilo
//! es el único que tiene el cliente (y con él el token de acceso); la ventana solo le manda trabajos y recibe resultados.

use std::{
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender, channel},
};

use eframe::egui::{self, RichText};

use crate::{
    cloud::{self, Client, HttpsTransport, SignUp},
    cloudimages,
    cloudsync::{self, CloudLink, Report},
    storage::Settings,
    theme::{self, Palette},
    ui::{kit, tokens},
};

/// Cada cuántos segundos se sincroniza un proyecto unido a la nube: rápido cuando hay otra persona trabajando o se está editando, y
/// despacio si no, para no gastar tráfico de más.
const SYNC_FAST_SECS: f64 = 2.0;
const SYNC_SLOW_SECS: f64 = 15.0;

/// Cada cuántos segundos se avisa dónde estoy (el servidor da por ido a quien lleva más de 30 sin avisar).
const PRESENCE_SECS: f64 = 8.0;

/// Cada cuántos segundos se vuelve a mirar qué proyectos hay en los grupos (para la pantalla de inicio).
const CATALOG_SECS: f64 = 30.0;

/// Cada cuántos segundos se prueba si ya se confirmó el mail de la cuenta nueva.
const CONFIRM_SECS: f64 = 4.0;

enum Job {
    SignIn(String, String),
    SignUp(String, String),
    /// Pide el código para recuperar la contraseña.
    Recover(String),
    /// Mail, código y contraseña nueva.
    ResetPassword(String, String, String),
    /// Mail y código para confirmar la cuenta nueva.
    ConfirmSignup(String, String),
    /// Prueba entrar con la cuenta nueva: funciona en cuanto se abre el enlace del mail.
    WaitConfirm(String, String),
    Leave(String),
    AllProjects,
    SignOut,
    Groups,
    CreateGroup(String),
    Invite(String),
    Join(String),
    Projects(String),
    Share { group: String, name: String, file: PathBuf, who: String },
    Open { group: String, project: String, file: PathBuf },
    Sync { file: PathBuf, link: CloudLink, who: String },
    Usage(String),
    Images(ImageWork),
    Presence { project: String, who: String, screen: String, detail: String },
    Profile,
    SetName(String),
}

/// Lo que hay que hacer con las imágenes de un proyecto.
pub struct ImageWork {
    pub group: String,
    pub project: String,
    /// La carpeta del proyecto (las imágenes viven en `recursos/`).
    pub base_dir: PathBuf,
    /// Rutas (relativas) de imágenes locales que todavía no están en la nube.
    pub upload: Vec<String>,
    /// Imágenes de la nube que faltan en esta carpeta: (ruta, huella, tipo).
    pub download: Vec<(String, String, String)>,
    /// Imágenes que ya no se usan y se sacan de la nube: (huella, tipo).
    pub remove: Vec<(String, String)>,
}

/// Lo que se hizo con las imágenes.
#[derive(Default)]
pub struct ImageResult {
    pub uploaded: Vec<(String, String, String)>,
    pub downloaded: usize,
    pub removed: Vec<String>,
    pub skipped: Vec<String>,
}

enum Event {
    SignedIn(String),
    SignedOut,
    NeedsConfirmation,
    /// Todavía no se confirmó el mail.
    StillWaiting,
    Left(String),
    AllProjects(Vec<(String, String, String)>),
    RecoverySent,
    Groups(Vec<(String, String, i64)>),
    Invite(String),
    Joined,
    Projects(Vec<(String, String)>),
    Shared(CloudLink),
    Opened(PathBuf, CloudLink),
    Synced(Report, CloudLink),
    Usage(i64, i64, i64),
    Images(ImageResult),
    Peers(Vec<cloud::Peer>),
    /// El nombre que tiene la persona en el servidor (`None`: todavía no eligió).
    Name(Option<String>),
    Error(String),
}

/// El hilo que habla con Supabase. Termina solo cuando se cierra el canal de trabajos.
fn worker(url: String, key: String, jobs: Receiver<Job>, events: Sender<Event>, ctx: egui::Context) {
    let mut client: Client<HttpsTransport> = match Client::connect(&url, &key) {
        Ok(c) => c,
        Err(e) => {
            let _ = events.send(Event::Error(e));
            return;
        }
    };
    // Si había una sesión guardada en el llavero, se intenta retomar.
    if client.ensure_session().is_ok()
        && let Some(email) = client.email()
    {
        let _ = events.send(Event::SignedIn(email.to_string()));
    }
    while let Ok(job) = jobs.recv() {
        let event = run(&mut client, job);
        if events.send(event).is_err() {
            break;
        }
        ctx.request_repaint();
    }
}

fn run(client: &mut Client<HttpsTransport>, job: Job) -> Event {
    let fail = |e: cloud::CloudError| Event::Error(e.message());
    let fail_sync = |e: cloudsync::SyncError| Event::Error(e.message());
    match job {
        Job::SignIn(email, password) => match client.sign_in(&email, &password) {
            Ok(()) => Event::SignedIn(client.email().unwrap_or_default().to_string()),
            Err(e) => fail(e),
        },
        Job::SignUp(email, password) => match client.sign_up(&email, &password) {
            Ok(SignUp::SignedIn) => Event::SignedIn(client.email().unwrap_or_default().to_string()),
            Ok(SignUp::NeedsConfirmation) => Event::NeedsConfirmation,
            Err(e) => fail(e),
        },
        Job::Recover(email) => match client.request_recovery(&email) {
            Ok(()) => Event::RecoverySent,
            Err(e) => fail(e),
        },
        Job::ResetPassword(email, code, password) => match client.reset_password(&email, &code, &password) {
            Ok(()) => Event::SignedIn(client.email().unwrap_or_default().to_string()),
            Err(e) => fail(e),
        },
        Job::ConfirmSignup(email, code) => match client.confirm_signup(&email, &code) {
            Ok(()) => Event::SignedIn(client.email().unwrap_or_default().to_string()),
            Err(e) => fail(e),
        },
        Job::WaitConfirm(email, password) => match client.sign_in(&email, &password) {
            Ok(()) => Event::SignedIn(client.email().unwrap_or_default().to_string()),
            // Sin red o sin confirmar todavía: se sigue esperando sin mostrar un error.
            Err(cloud::CloudError::Unconfirmed | cloud::CloudError::Offline(_) | cloud::CloudError::TooMany) => Event::StillWaiting,
            Err(e) => fail(e),
        },
        Job::Leave(group) => match client.leave_group(&group) {
            Ok(()) => Event::Left(group),
            Err(e) => fail(e),
        },
        Job::AllProjects => client.all_projects().map(Event::AllProjects).unwrap_or_else(|_| Event::AllProjects(Vec::new())),
        Job::SignOut => {
            client.sign_out();
            Event::SignedOut
        }
        Job::Groups => client.groups().map(Event::Groups).unwrap_or_else(fail),
        Job::CreateGroup(name) => match client.create_group(&name) {
            Ok(_) => client.groups().map(Event::Groups).unwrap_or_else(fail),
            Err(e) => fail(e),
        },
        Job::Invite(group) => client.create_invite(&group).map(Event::Invite).unwrap_or_else(fail),
        Job::Join(code) => match client.join_group(&code) {
            Ok(_) => Event::Joined,
            Err(e) => fail(e),
        },
        Job::Projects(group) => client.projects(&group).map(Event::Projects).unwrap_or_else(fail),
        Job::Share { group, name, file, who } => cloudsync::share_project(client, &group, &name, &file, &who)
            .map(Event::Shared)
            .unwrap_or_else(fail_sync),
        Job::Open { group, project, file } => cloudsync::download_project(client, &group, &project, &file)
            .map(|link| Event::Opened(file.clone(), link))
            .unwrap_or_else(fail_sync),
        Job::Sync { file, mut link, who } => match cloudsync::sync_once(client, &file, &mut link, &who) {
            Ok(report) => Event::Synced(report, link),
            Err(e) => fail_sync(e),
        },
        Job::Images(work) => Event::Images(cloudimages::run(client, work)),
        Job::Profile => client.display_name().map(Event::Name).unwrap_or_else(fail),
        Job::SetName(name) => client.set_display_name(&name).map(|n| Event::Name(Some(n))).unwrap_or_else(fail),
        Job::Presence { project, who, screen, detail } => {
            // Avisar que estoy y ver quién más está: si falla no es importante, no se muestra como error.
            let _ = client.set_presence(&project, &who, &screen, &detail);
            Event::Peers(client.presence(&project).unwrap_or_default())
        }
        Job::Usage(group) => client
            .group_usage(&group)
            .map(|(used, quota, count)| Event::Usage(used, quota, count))
            .unwrap_or_else(fail),
    }
}

/// Un proyecto de un grupo (para la pantalla de inicio).
pub struct GroupProject {
    pub id: String,
    pub name: String,
    pub group: String,
}

/// Lo que la ventana le pide a la app después de cada cuadro.
#[derive(Default)]
pub struct Out {
    /// El proyecto abierto quedó unido a la nube (o cambió su enlace).
    pub link: Option<CloudLink>,
    /// Se dejó de sincronizar el proyecto abierto.
    pub unlink: bool,
    /// Se bajó un proyecto de la nube a esta carpeta: hay que abrirlo.
    pub open: Option<PathBuf>,
    /// La sincronización escribió secciones nuevas en la carpeta: hay que traerlas a la pantalla.
    pub disk_changed: bool,
    /// Hay algo para avisar en la barra de estado.
    pub message: Option<String>,
    /// El nombre de la persona según el servidor (cuando llegó o cambió).
    pub name: Option<String>,
    /// Quién más está en el proyecto (cuando llegó una actualización).
    pub peers: Option<Vec<cloud::Peer>>,
    /// Cambió la configuración (URL o clave).
    pub settings_changed: bool,
    /// Terminó una sincronización (sirve para revisar qué imágenes faltan).
    pub synced: bool,
    /// Terminó un trabajo de imágenes.
    pub images: Option<ImageResult>,
    /// Se pidió sacar de la nube las imágenes que ya no se usan.
    pub clean_images: bool,
}

#[derive(Default)]
pub struct CloudUi {
    pub open: bool,
    tx: Option<Sender<Job>>,
    rx: Option<Receiver<Event>>,
    /// Con qué configuración se creó el hilo (si cambia, se vuelve a crear).
    started_with: Option<(String, String)>,
    email: String,
    password: String,
    signed_as: Option<String>,
    groups: Vec<(String, String, i64)>,
    selected_group: Option<usize>,
    projects: Vec<(String, String)>,
    usage: Option<(i64, i64, i64)>,
    new_group: String,
    join_code: String,
    share_name: String,
    invite: Option<String>,
    note: String,
    busy: bool,
    syncing: bool,
    images_busy: bool,
    presence_busy: bool,
    last_presence: f64,
    /// Los proyectos de todos mis grupos: (id, nombre, id del grupo).
    catalog: Vec<(String, String, String)>,
    catalog_busy: bool,
    last_catalog: f64,
    /// El proyecto de grupo que se está bajando (su id).
    downloading: Option<String>,
    /// Cuenta nueva que espera la confirmación del mail: (mail, contraseña). Solo vive en memoria.
    awaiting: Option<(String, String)>,
    confirm_code: String,
    wait_busy: bool,
    last_wait: f64,
    /// El grupo del que se pidió salir y falta confirmar.
    leave_confirm: Option<String>,
    last_sync: f64,
    /// Entrar o crear cuenta (0 / 1) y si se ve la contraseña.
    login_mode: usize,
    show_password: bool,
    /// Recuperar la contraseña: se está en esa pantalla, ya se mandó el mail, y el código que escribe la persona.
    forgot: bool,
    code_sent: bool,
    code: String,
    /// El nombre que tiene la persona en el servidor, y lo que está escribiendo para elegirlo.
    cloud_name: Option<String>,
    name_loaded: bool,
    name_input: String,
    config_url: String,
    config_key: String,
}

fn megabytes(bytes: i64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0).replace('.', ",")
}

impl CloudUi {
    pub fn configured(settings: &Settings) -> bool {
        let (url, key) = cloud::effective(settings);
        cloud::parse_url(&url).is_ok() && !key.is_empty()
    }

    /// El nombre que la persona eligió en el servidor. Una vez elegido no se puede cambiar (ver `supabase/schema.sql`).
    pub fn locked_name(&self) -> Option<&str> {
        self.cloud_name.as_deref()
    }

    /// El mail de la cuenta con la sesión iniciada, si hay una.
    pub fn account(&self) -> Option<&str> {
        self.signed_as.as_deref()
    }

    fn send(&mut self, job: Job) {
        if let Some(tx) = &self.tx
            && tx.send(job).is_ok()
        {
            self.busy = true;
        }
    }

    /// Crea el hilo si hace falta (o si cambió la configuración).
    fn ensure_worker(&mut self, ctx: &egui::Context, settings: &Settings) {
        let wanted = cloud::effective(settings);
        if self.started_with.as_ref() == Some(&wanted) {
            return;
        }
        self.tx = None;
        self.rx = None;
        self.signed_as = None;
        self.groups.clear();
        if !Self::configured(settings) {
            self.started_with = None;
            return;
        }
        let (job_tx, job_rx) = channel();
        let (event_tx, event_rx) = channel();
        let (url, key, ctx) = (wanted.0.clone(), wanted.1.clone(), ctx.clone());
        std::thread::spawn(move || worker(url, key, job_rx, event_tx, ctx));
        self.tx = Some(job_tx);
        self.rx = Some(event_rx);
        self.started_with = Some(wanted);
    }

    /// Recibe lo que terminó el hilo, y cada tanto manda a sincronizar el proyecto abierto. Se llama en cada cuadro.
    pub fn tick(&mut self, ctx: &egui::Context, now: f64, settings: &Settings, file: Option<&Path>, link: Option<&CloudLink>, fast: bool) -> Out {
        self.ensure_worker(ctx, settings);
        let mut out = Out::default();
        let mut events = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            self.busy = false;
            match event {
                Event::SignedIn(email) => {
                    self.forgot = false;
                    self.code_sent = false;
                    self.code.clear();
                    self.signed_as = Some(email);
                    self.password.clear();
                    self.awaiting = None;
                    self.confirm_code.clear();
                    self.wait_busy = false;
                    self.note.clear();
                    self.name_loaded = false;
                    self.send(Job::Groups);
                    self.send(Job::Profile);
                }
                Event::Name(name) => {
                    self.name_loaded = true;
                    if let Some(n) = &name {
                        out.name = Some(n.clone());
                        self.note.clear();
                    } else {
                        // Todavía no eligió: la ventana se abre sola para que lo haga.
                        self.open = true;
                    }
                    self.cloud_name = name;
                }
                Event::SignedOut => {
                    self.cloud_name = None;
                    self.name_loaded = false;
                    self.signed_as = None;
                    self.groups.clear();
                    self.projects.clear();
                    self.catalog.clear();
                }
                Event::RecoverySent => {
                    self.code_sent = true;
                    self.note = "Si el mail tiene cuenta, te mandamos un código. Puede tardar un minuto.".into();
                }
                Event::NeedsConfirmation => {
                    self.note = "Te mandamos un mail para confirmar la cuenta.".into();
                    self.awaiting = Some((self.email.trim().to_string(), std::mem::take(&mut self.password)));
                    self.confirm_code.clear();
                    self.last_wait = f64::NEG_INFINITY;
                }
                Event::StillWaiting => self.wait_busy = false,
                Event::Left(group) => {
                    self.leave_confirm = None;
                    self.selected_group = None;
                    self.invite = None;
                    self.projects.clear();
                    self.note.clear();
                    if link.is_some_and(|l| l.group == group) {
                        out.unlink = true;
                    }
                    out.message = Some("Saliste del grupo.".into());
                    self.send(Job::Groups);
                }
                Event::AllProjects(list) => {
                    self.catalog_busy = false;
                    self.catalog = list;
                }
                Event::Groups(groups) => {
                    self.groups = groups;
                    self.request_catalog();
                    if self.selected_group.is_none_or(|i| i >= self.groups.len()) {
                        self.selected_group = (!self.groups.is_empty()).then_some(0);
                    }
                    self.refresh_group();
                }
                Event::Invite(code) => self.invite = Some(code),
                Event::Joined => {
                    self.join_code.clear();
                    self.send(Job::Groups);
                }
                Event::Projects(list) => self.projects = list,
                Event::Shared(new_link) => {
                    self.request_catalog();
                    out.message = Some("Proyecto compartido en la nube.".into());
                    out.link = Some(new_link);
                }
                Event::Opened(path, new_link) => {
                    self.downloading = None;
                    out.link = Some(new_link);
                    out.open = Some(path);
                }
                Event::Synced(report, new_link) => {
                    self.syncing = false;
                    out.synced = true;
                    out.disk_changed = !report.pulled.is_empty() || !report.merged.is_empty() || !report.conflicts.is_empty();
                    if !report.conflicts.is_empty() {
                        out.message = Some("Alguien cambió lo mismo que vos: se guardó tu versión aparte en la carpeta del proyecto.".into());
                    }
                    // Si alguien se adelantó al subir, se vuelve a empezar enseguida.
                    if report.retry {
                        self.last_sync = 0.0;
                    }
                    out.link = Some(new_link);
                }
                Event::Peers(peers) => {
                    self.presence_busy = false;
                    out.peers = Some(peers);
                }
                Event::Usage(used, quota, count) => self.usage = Some((used, quota, count)),
                Event::Images(result) => {
                    self.images_busy = false;
                    // El uso del grupo cambió.
                    if let Some(group) = self.selected_group.and_then(|i| self.groups.get(i)).map(|g| g.0.clone()) {
                        self.send(Job::Usage(group));
                    }
                    out.images = Some(result);
                }
                Event::Error(message) => {
                    self.syncing = false;
                    self.downloading = None;
                    self.wait_busy = false;
                    self.note = message.clone();
                    if self.open || self.signed_as.is_some() {
                        out.message = Some(message);
                    }
                }
            }
        }
        // La sincronización periódica del proyecto abierto.
        if let (Some(file), Some(link), Some(_)) = (file, link, &self.signed_as)
            && !self.syncing
            && now - self.last_sync > if fast { SYNC_FAST_SECS } else { SYNC_SLOW_SECS }
        {
            self.last_sync = now;
            self.syncing = true;
            self.send(Job::Sync { file: file.to_path_buf(), link: link.clone(), who: settings.author() });
        }
        // La lista de proyectos de los grupos, para la pantalla de inicio.
        if self.signed_as.is_some() && self.cloud_name.is_some() && now - self.last_catalog > CATALOG_SECS {
            self.request_catalog();
            self.last_catalog = now;
        }
        // Cuenta nueva sin confirmar: se prueba entrar cada tanto, así basta con abrir el mail.
        if let Some((email, password)) = &self.awaiting
            && self.signed_as.is_none()
            && !self.wait_busy
            && now - self.last_wait > CONFIRM_SECS
            && let Some(tx) = &self.tx
        {
            self.last_wait = now;
            self.wait_busy = tx.send(Job::WaitConfirm(email.clone(), password.clone())).is_ok();
        }
        if self.tx.is_some() && (self.awaiting.is_some() || self.signed_as.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(if self.awaiting.is_some() { CONFIRM_SECS } else { CATALOG_SECS }));
        }
        if self.tx.is_some() && file.is_some() && link.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(if fast { SYNC_FAST_SECS } else { SYNC_SLOW_SECS }));
        }
        out
    }

    /// Pide la lista de proyectos de todos los grupos (sin marcar la ventana como ocupada).
    fn request_catalog(&mut self) {
        if self.catalog_busy {
            return;
        }
        if let Some(tx) = &self.tx
            && tx.send(Job::AllProjects).is_ok()
        {
            self.catalog_busy = true;
        }
    }

    /// Los proyectos de mis grupos, con el nombre de su grupo.
    pub fn group_projects(&self) -> Vec<GroupProject> {
        self.catalog
            .iter()
            .map(|(id, name, group)| GroupProject {
                id: id.clone(),
                name: name.clone(),
                group: self.groups.iter().find(|g| &g.0 == group).map(|g| g.1.clone()).unwrap_or_default(),
            })
            .collect()
    }

    /// El nombre de un grupo mío.
    pub fn group_name(&self, id: &str) -> Option<&str> {
        self.groups.iter().find(|g| g.0 == id).map(|g| g.1.as_str())
    }

    /// El proyecto de grupo que se está bajando, si hay uno.
    pub fn downloading(&self) -> Option<&str> {
        self.downloading.as_deref()
    }

    /// Baja un proyecto de un grupo: la persona elige dónde dejar la copia y se abre sola al terminar.
    pub fn download(&mut self, project: &str) {
        if self.downloading.is_some() {
            return;
        }
        let Some((id, name, group)) = self.catalog.iter().find(|c| c.0 == project).cloned() else {
            return;
        };
        let Some(chosen) = rfd::FileDialog::new().set_file_name(format!("{name}.{}", crate::storage::EXTENSION)).save_file() else {
            return;
        };
        let file = crate::assets::project_file_for(&chosen);
        if let Some(tx) = &self.tx
            && tx.send(Job::Open { group, project: id.clone(), file }).is_ok()
        {
            self.downloading = Some(id);
        }
    }

    /// Manda un trabajo de imágenes (si no hay otro en curso).
    pub fn send_images(&mut self, work: ImageWork) {
        if self.images_busy {
            return;
        }
        if let Some(tx) = &self.tx
            && tx.send(Job::Images(work)).is_ok()
        {
            self.images_busy = true;
        }
    }

    /// Cada pocos segundos avisa dónde estoy y trae quién más está. `screen` y `detail` los arma la app.
    pub fn presence(&mut self, now: f64, link: Option<&CloudLink>, who: &str, screen: &str, detail: &str) {
        let (Some(link), Some(_)) = (link, &self.signed_as) else {
            return;
        };
        if self.presence_busy || now - self.last_presence < PRESENCE_SECS {
            return;
        }
        self.last_presence = now;
        if let Some(tx) = &self.tx
            && tx
                .send(Job::Presence {
                    project: link.project.clone(),
                    who: who.to_string(),
                    screen: screen.to_string(),
                    detail: detail.to_string(),
                })
                .is_ok()
        {
            self.presence_busy = true;
        }
    }

    fn refresh_group(&mut self) {
        self.projects.clear();
        self.usage = None;
        self.invite = None;
        if let Some(group) = self.selected_group.and_then(|i| self.groups.get(i)).map(|g| g.0.clone()) {
            self.send(Job::Projects(group.clone()));
            self.send(Job::Usage(group));
        }
    }

    /// La ventana de la cuenta. `file` y `title` son los del proyecto abierto (si está guardado en un archivo).
    pub fn dialog(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        settings: &mut Settings,
        file: Option<&Path>,
        title: &str,
        link: Option<&CloudLink>,
        unused_images: usize,
    ) -> Out {
        let mut out = Out::default();
        if !self.open {
            return out;
        }
        self.ensure_worker(ctx, settings);
        let mut close = false;
        let mut job: Option<Job> = None;
        let configured = Self::configured(settings);
        let built_in = cloud::has_built_in();
        let signed = self.signed_as.clone();
        let busy = self.busy;
        let mut chosen_group: Option<usize> = None;
        let mut open_dest: Option<(String, String)> = None;
        let mut sign_out = false;
        let mut unlink = false;
        let mut clean = false;
        let mut save_config = false;
        let author = settings.author();
        let (mut config_url, mut config_key) = (self.config_url.clone(), self.config_key.clone());
        if config_url.is_empty() && config_key.is_empty() {
            config_url = settings.cloud_url.clone();
        }
        let this = &mut *self;
        let closed = kit::dialog(
            ctx,
            p,
            "cloud",
            "Tu cuenta",
            kit::DialogWidth::Normal,
            |ui| {
                if !configured && !built_in {
                    setup_view(ui, p, &mut config_url, &mut config_key, &mut save_config);
                    return;
                }
                let Some(email) = signed.clone() else {
                    login_view(ui, p, this, busy, &mut job);
                    return;
                };
                account_header(ui, p, &email, this.cloud_name.as_deref(), &mut sign_out);
                if !this.note.is_empty() {
                    message_line(ui, p, &this.note, true);
                }
                // Primero el nombre: sin él no se puede seguir.
                if this.cloud_name.is_none() {
                    if this.name_loaded {
                        name_prompt(ui, p, this, busy, &mut job);
                    } else {
                        ui.add_space(tokens::SP_L);
                        ui.label(RichText::new("Cargando tu cuenta…").color(p.dim));
                    }
                    return;
                }
                groups_view(ui, p, this, busy, &mut job, &mut chosen_group);
                let Some((group_id, group_name, _)) = this.selected_group.and_then(|i| this.groups.get(i)).cloned() else {
                    return;
                };
                group_view(
                    ui, p, this, busy, &group_id, &group_name, file, title, link, unused_images, &author,
                    &mut job, &mut unlink, &mut clean, &mut open_dest,
                );
            },
            |ui| {
                if ui.button("Cerrar").clicked() {
                    close = true;
                }
            },
        );
        self.config_url = config_url.clone();
        self.config_key = config_key.clone();
        if save_config {
            settings.cloud_url = cloud::parse_url(&config_url).unwrap_or_default();
            settings.cloud_anon_key = config_key.trim().to_string();
            self.config_key.clear();
            out.settings_changed = true;
        }
        if let Some(i) = chosen_group {
            self.leave_confirm = None;
            self.selected_group = Some(i);
            self.refresh_group();
        }
        if let Some((project, name)) = open_dest {
            // La copia va a una carpeta nueva que elige la persona.
            if let Some(chosen) = rfd::FileDialog::new().set_file_name(format!("{name}.{}", crate::storage::EXTENSION)).save_file() {
                let dest = crate::assets::project_file_for(&chosen);
                if let Some(group) = self.selected_group.and_then(|i| self.groups.get(i)).map(|g| g.0.clone()) {
                    job = Some(Job::Open { group, project, file: dest });
                }
            }
        }
        if sign_out {
            job = Some(Job::SignOut);
        }
        if let Some(job) = job {
            self.send(job);
        }
        out.unlink = unlink;
        out.clean_images = clean;
        if close || closed {
            self.open = false;
        }
        out
    }
}

// ------------------------------------------------------------------------------------------------
// Las partes de la ventana
// ------------------------------------------------------------------------------------------------

/// Un mensaje con su color: rojo si es un problema, el tono normal si es información.
fn message_line(ui: &mut egui::Ui, p: &Palette, text: &str, problem: bool) {
    egui::Frame::new()
        .fill(theme::mix(p.container, if problem { p.warning } else { p.accent }, 0.14))
        .corner_radius(tokens::R_CONTROL)
        .inner_margin(egui::Margin::symmetric(tokens::SP_M as i8, tokens::SP_S as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).size(tokens::FONT_SMALL).color(p.text)).wrap());
        });
}

/// Un círculo con una letra o un icono (el «avatar» de la cuenta).
fn badge(ui: &mut egui::Ui, text: &str, color: egui::Color32, size: f32, font: egui::FontId, ink: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font, ink);
}

/// Un campo con su rótulo arriba, a todo el ancho.
fn labeled(ui: &mut egui::Ui, p: &Palette, label: &str, add: impl FnOnce(&mut egui::Ui) -> egui::Response) -> egui::Response {
    ui.label(RichText::new(label).size(tokens::FONT_SMALL).color(p.dim));
    ui.add_space(2.0);
    let resp = add(ui);
    ui.add_space(tokens::SP_S);
    resp
}

/// Cuando la nube todavía no está configurada (la versión sin la nube incorporada).
fn setup_view(ui: &mut egui::Ui, p: &Palette, url: &mut String, key: &mut String, save: &mut bool) {
    ui.vertical_centered(|ui| {
        badge(ui, theme_icon(), p.container_hi, 44.0, egui::FontId::proportional(22.0), p.accent);
        ui.add_space(tokens::SP_S);
        ui.label(RichText::new("Conectar con la nube").font(egui::FontId::new(tokens::FONT_CARD_TITLE, theme::bold_family())).color(p.text));
        ui.label(RichText::new("Pegá la dirección y la clave pública que te pasaron.").color(p.dim));
    });
    ui.add_space(tokens::SP_M);
    labeled(ui, p, "Dirección", |ui| ui.add(egui::TextEdit::singleline(url).hint_text("https://xxxx.supabase.co").desired_width(f32::INFINITY)));
    labeled(ui, p, "Clave pública", |ui| ui.add(egui::TextEdit::singleline(key).hint_text("anon o publishable").desired_width(f32::INFINITY)));
    ui.label(RichText::new("Nunca pegues una clave secreta: el programa la rechaza.").size(tokens::FONT_SMALL).color(p.dim));
    if !url.is_empty()
        && let Err(why) = cloud::parse_url(url)
    {
        message_line(ui, p, &why, true);
    }
    ui.add_space(tokens::SP_S);
    let ok = cloud::parse_url(url).is_ok() && !key.trim().is_empty();
    if kit::primary_button(ui, p, "Guardar", (!ok).then_some("Falta la dirección o la clave")).clicked() {
        *save = true;
    }
}

fn theme_icon() -> &'static str {
    crate::icon::CLOUD
}

/// Iniciar sesión o crear una cuenta.
fn login_view(ui: &mut egui::Ui, p: &Palette, this: &mut CloudUi, busy: bool, job: &mut Option<Job>) {
    if this.forgot {
        recovery_view(ui, p, this, busy, job);
        return;
    }
    if this.awaiting.is_some() {
        confirm_view(ui, p, this, busy, job);
        return;
    }
    ui.vertical_centered(|ui| {
        badge(ui, theme_icon(), theme::mix(p.container, p.accent, 0.25), 52.0, egui::FontId::proportional(26.0), p.accent);
        ui.add_space(tokens::SP_S);
        ui.label(RichText::new("Tu cuenta").font(egui::FontId::new(tokens::FONT_TITLE, theme::bold_family())).color(p.text));
        ui.label(RichText::new("Entrá para compartir proyectos y trabajar en equipo.").color(p.dim));
    });
    ui.add_space(tokens::SP_M);
    if let Some(i) = kit::segmented(ui, p, &["Entrar", "Crear cuenta"], this.login_mode) {
        this.login_mode = i;
        this.note.clear();
    }
    ui.add_space(tokens::SP_M);
    let signup = this.login_mode == 1;
    let email = labeled(ui, p, "Mail", |ui| {
        ui.add(egui::TextEdit::singleline(&mut this.email).hint_text("nombre@ejemplo.com").desired_width(f32::INFINITY))
    });
    let mut enter = email.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.label(RichText::new(if signup { "Contraseña (10 caracteres o más)" } else { "Contraseña" }).size(tokens::FONT_SMALL).color(p.dim));
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let width = ui.available_width() - tokens::ICON_BTN - tokens::SP_S;
        let resp = ui.add(egui::TextEdit::singleline(&mut this.password).password(!this.show_password).desired_width(width));
        enter |= resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let eye = if this.show_password { crate::icon::EYE_SLASH } else { crate::icon::EYE };
        if kit::icon_button(ui, p, eye, if this.show_password { "Ocultar la contraseña" } else { "Mostrar la contraseña" }, true).clicked() {
            this.show_password = !this.show_password;
        }
    });
    if !signup {
        ui.add_space(tokens::SP_XS);
        if ui.link("Olvidé mi contraseña").clicked() {
            this.forgot = true;
            this.code_sent = false;
            this.code.clear();
            this.password.clear();
            this.note.clear();
        }
    }
    ui.add_space(tokens::SP_M);
    let missing = if this.email.trim().is_empty() {
        Some("Falta el mail")
    } else if this.password.is_empty() {
        Some("Falta la contraseña")
    } else if signup && this.password.chars().count() < 10 {
        Some("La contraseña tiene que tener al menos 10 caracteres")
    } else if busy {
        Some("Un momento…")
    } else {
        None
    };
    let label = match (busy, signup) {
        (true, _) => "Un momento…",
        (false, false) => "Entrar",
        (false, true) => "Crear cuenta",
    };
    ui.add_sized([ui.available_width(), tokens::CONTROL_H + 4.0], |ui: &mut egui::Ui| {
        let clicked = kit::primary_button(ui, p, label, missing).clicked();
        if (clicked || (enter && missing.is_none())) && missing.is_none() {
            let (email, password) = (this.email.trim().to_string(), this.password.clone());
            *job = Some(if signup { Job::SignUp(email, password) } else { Job::SignIn(email, password) });
        }
        ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
    });
    if !this.note.is_empty() {
        ui.add_space(tokens::SP_S);
        message_line(ui, p, &this.note, !this.note.contains("Te mandamos"));
    }
    if signup {
        ui.add_space(tokens::SP_S);
        ui.label(RichText::new("Te vamos a mandar un mail para confirmar la cuenta.").size(tokens::FONT_SMALL).color(p.dim));
    }
}

/// Cuenta nueva: se espera a que abra el mail. Entra sola en cuanto lo confirma, o escribiendo el código que llega.
fn confirm_view(ui: &mut egui::Ui, p: &Palette, this: &mut CloudUi, busy: bool, job: &mut Option<Job>) {
    let email = this.awaiting.as_ref().map(|a| a.0.clone()).unwrap_or_default();
    ui.vertical_centered(|ui| {
        badge(ui, crate::icon::CLOUD, theme::mix(p.container, p.accent, 0.25), 52.0, egui::FontId::proportional(26.0), p.accent);
        ui.add_space(tokens::SP_S);
        ui.label(RichText::new("Revisá tu mail").font(egui::FontId::new(tokens::FONT_TITLE, theme::bold_family())).color(p.text));
        ui.label(RichText::new(format!("Te mandamos un mensaje a {email}.")).color(p.dim));
    });
    ui.add_space(tokens::SP_M);
    message_line(ui, p, "Abrí el enlace del mail y esta ventana entra sola, o escribí acá el código que viene en el mensaje.", false);
    ui.add_space(tokens::SP_M);
    let resp = labeled(ui, p, "Código del mail", |ui| {
        ui.add(egui::TextEdit::singleline(&mut this.confirm_code).hint_text("123456").desired_width(f32::INFINITY))
    });
    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let missing = if !cloud::is_recovery_code(&this.confirm_code) {
        Some("Falta el código")
    } else if busy {
        Some("Un momento…")
    } else {
        None
    };
    if (kit::primary_button(ui, p, "Confirmar y entrar", missing).clicked() || (enter && missing.is_none())) && missing.is_none() {
        *job = Some(Job::ConfirmSignup(email, this.confirm_code.trim().to_string()));
    }
    if !this.note.is_empty() && !this.note.contains("Te mandamos") {
        ui.add_space(tokens::SP_S);
        message_line(ui, p, &this.note, true);
    }
    ui.add_space(tokens::SP_S);
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(14.0));
        ui.label(RichText::new("Esperando que confirmes el mail…").size(tokens::FONT_SMALL).color(p.dim));
    });
    if ui.link("Volver").clicked() {
        this.awaiting = None;
        this.confirm_code.clear();
        this.login_mode = 0;
        this.note.clear();
    }
}

/// Recuperar la contraseña: primero el mail, después el código que llega y la contraseña nueva.
fn recovery_view(ui: &mut egui::Ui, p: &Palette, this: &mut CloudUi, busy: bool, job: &mut Option<Job>) {
    ui.vertical_centered(|ui| {
        badge(ui, crate::icon::LOCK_SIMPLE, theme::mix(p.container, p.accent, 0.25), 52.0, egui::FontId::proportional(26.0), p.accent);
        ui.add_space(tokens::SP_S);
        ui.label(RichText::new("Recuperar la contraseña").font(egui::FontId::new(tokens::FONT_TITLE, theme::bold_family())).color(p.text));
        ui.label(RichText::new("Te mandamos un código por mail para elegir una nueva.").color(p.dim));
    });
    ui.add_space(tokens::SP_M);
    labeled(ui, p, "Mail", |ui| {
        ui.add_enabled(!this.code_sent, egui::TextEdit::singleline(&mut this.email).hint_text("nombre@ejemplo.com").desired_width(f32::INFINITY))
    });
    if !this.code_sent {
        let missing = if !cloud::is_email_shape(&this.email) {
            Some("Escribí tu mail")
        } else if busy {
            Some("Un momento…")
        } else {
            None
        };
        if kit::primary_button(ui, p, "Mandar el código", missing).clicked() && missing.is_none() {
            *job = Some(Job::Recover(this.email.trim().to_string()));
        }
    } else {
        labeled(ui, p, "Código del mail", |ui| {
            ui.add(egui::TextEdit::singleline(&mut this.code).hint_text("123456").desired_width(f32::INFINITY))
        });
        ui.label(RichText::new("Contraseña nueva (10 caracteres o más)").size(tokens::FONT_SMALL).color(p.dim));
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let width = ui.available_width() - tokens::ICON_BTN - tokens::SP_S;
            ui.add(egui::TextEdit::singleline(&mut this.password).password(!this.show_password).desired_width(width));
            let eye = if this.show_password { crate::icon::EYE_SLASH } else { crate::icon::EYE };
            if kit::icon_button(ui, p, eye, if this.show_password { "Ocultar la contraseña" } else { "Mostrar la contraseña" }, true).clicked() {
                this.show_password = !this.show_password;
            }
        });
        ui.add_space(tokens::SP_M);
        let missing = if !cloud::is_recovery_code(&this.code) {
            Some("Falta el código")
        } else if this.password.chars().count() < 10 {
            Some("La contraseña tiene que tener al menos 10 caracteres")
        } else if busy {
            Some("Un momento…")
        } else {
            None
        };
        if kit::primary_button(ui, p, "Cambiar la contraseña y entrar", missing).clicked() && missing.is_none() {
            *job = Some(Job::ResetPassword(this.email.trim().to_string(), this.code.trim().to_string(), this.password.clone()));
        }
        if ui.link("Pedir otro código").clicked() {
            this.code_sent = false;
            this.code.clear();
            this.note.clear();
        }
    }
    if !this.note.is_empty() {
        ui.add_space(tokens::SP_S);
        message_line(ui, p, &this.note, !this.note.contains("mandamos un código"));
    }
    ui.add_space(tokens::SP_S);
    if ui.link("Volver").clicked() {
        this.forgot = false;
        this.code_sent = false;
        this.code.clear();
        this.password.clear();
        this.note.clear();
    }
}

/// El nombre se elige una vez, y lo ven los demás.
fn name_prompt(ui: &mut egui::Ui, p: &Palette, this: &mut CloudUi, busy: bool, job: &mut Option<Job>) {
    ui.add_space(tokens::SP_M);
    kit::card(ui, p, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Elegí tu nombre").font(egui::FontId::new(tokens::FONT_CARD_TITLE, theme::bold_family())).color(p.text));
        ui.label(RichText::new("Es el que ven los demás en los avisos y en quién está conectado.").color(p.dim));
        ui.add_space(tokens::SP_S);
        let resp = ui.add(egui::TextEdit::singleline(&mut this.name_input).hint_text("Por ejemplo, Ana López").desired_width(f32::INFINITY));
        let valid = cloud::valid_display_name(&this.name_input);
        if !this.name_input.trim().is_empty() && !valid {
            ui.label(RichText::new("De 3 a 30 letras, números, espacios, puntos o guiones.").size(tokens::FONT_SMALL).color(p.warning));
        }
        ui.add_space(tokens::SP_S);
        message_line(ui, p, "Una vez elegido no se puede cambiar.", false);
        ui.add_space(tokens::SP_S);
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let why = if !valid { Some("Escribí un nombre válido") } else if busy { Some("Un momento…") } else { None };
        if (kit::primary_button(ui, p, "Guardar mi nombre", why).clicked() || (enter && why.is_none())) && why.is_none() {
            *job = Some(Job::SetName(this.name_input.trim().to_string()));
        }
    });
}

/// Quién está dentro: su círculo con la inicial (del color de su nombre), el nombre y el mail.
fn account_header(ui: &mut egui::Ui, p: &Palette, email: &str, name: Option<&str>, sign_out: &mut bool) {
    kit::card(ui, p, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let shown = name.unwrap_or(email);
            let color = theme::character_color(shown);
            let initial = shown.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            badge(ui, &initial, color, 40.0, egui::FontId::new(18.0, theme::bold_family()), theme::ink_on(color, p));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name.unwrap_or("Sin nombre todavía")).font(egui::FontId::new(tokens::FONT_BODY, theme::bold_family())).color(p.text));
                    if name.is_some() {
                        ui.label(RichText::new(crate::icon::LOCK_SIMPLE).size(tokens::ICON_S).color(p.dim))
                            .on_hover_text("El nombre no se puede cambiar");
                    }
                });
                ui.label(RichText::new(email).size(tokens::FONT_SMALL).color(p.dim));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{}  Salir", crate::icon::SIGN_OUT)).clicked() {
                    *sign_out = true;
                }
            });
        });
    });
}

/// Los grupos de la persona, para elegir uno, y crear uno o entrar con un código.
fn groups_view(ui: &mut egui::Ui, p: &Palette, this: &mut CloudUi, busy: bool, job: &mut Option<Job>, chosen: &mut Option<usize>) {
    kit::section(ui, p, "Grupos", None);
    if this.groups.is_empty() {
        ui.label(RichText::new("Todavía no estás en ningún grupo. Creá uno o entrá con un código.").color(p.dim));
    }
    ui.horizontal_wrapped(|ui| {
        for (i, (_, name, _)) in this.groups.iter().enumerate() {
            let on = this.selected_group == Some(i);
            let text = RichText::new(name).color(if on { p.on_accent } else { p.text });
            let button = egui::Button::new(text).fill(if on { p.accent } else { p.container });
            if ui.add(button).clicked() {
                *chosen = Some(i);
            }
        }
    });
    ui.add_space(tokens::SP_S);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut this.new_group).hint_text("Nombre del grupo nuevo").desired_width(ui.available_width() - 110.0));
        if ui.add_enabled(!busy && !this.new_group.trim().is_empty(), egui::Button::new(format!("{}  Crear", crate::icon::PLUS))).clicked() {
            *job = Some(Job::CreateGroup(this.new_group.trim().to_string()));
            this.new_group.clear();
        }
    });
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut this.join_code).hint_text("Código de invitación").desired_width(ui.available_width() - 110.0));
        if ui.add_enabled(!busy && !this.join_code.trim().is_empty(), egui::Button::new("Entrar")).clicked() {
            *job = Some(Job::Join(this.join_code.trim().to_string()));
        }
    });
}

/// El grupo elegido: su espacio de imágenes, invitaciones, el proyecto abierto y los proyectos compartidos.
#[allow(clippy::too_many_arguments)]
fn group_view(
    ui: &mut egui::Ui,
    p: &Palette,
    this: &mut CloudUi,
    busy: bool,
    group_id: &str,
    group_name: &str,
    file: Option<&Path>,
    title: &str,
    link: Option<&CloudLink>,
    unused_images: usize,
    author: &str,
    job: &mut Option<Job>,
    unlink: &mut bool,
    clean: &mut bool,
    open_dest: &mut Option<(String, String)>,
) {
    kit::section(ui, p, &format!("Grupo «{group_name}»"), None);
    kit::card(ui, p, |ui| {
        ui.set_width(ui.available_width());
        if let Some((used, quota, count)) = this.usage {
            let fraction = if quota > 0 { (used as f32 / quota as f32).clamp(0.0, 1.0) } else { 1.0 };
            ui.label(RichText::new(format!("Imágenes: {} de {} ({} en total)", megabytes(used), megabytes(quota), count)).size(tokens::FONT_SMALL).color(p.dim));
            ui.add(egui::ProgressBar::new(fraction).fill(if fraction > 0.9 { p.warning } else { p.accent }));
            if fraction > 0.8 {
                ui.label(RichText::new("El espacio para imágenes está casi lleno: conviene quitar las que no se usan.").size(tokens::FONT_SMALL).color(p.warning));
            }
            ui.add_space(tokens::SP_S);
        }
        if unused_images > 0 {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{unused_images} imágenes de este proyecto ya no se usan.")).size(tokens::FONT_SMALL));
                if ui.button("Quitarlas de la nube").clicked() {
                    *clean = true;
                }
            });
        }
        if ui.add_enabled(!busy, egui::Button::new(format!("{}  Código de invitación", crate::icon::LINK_SIMPLE))).clicked() {
            *job = Some(Job::Invite(group_id.to_string()));
        }
        if let Some(code) = &this.invite {
            ui.label(RichText::new("Pasale este código a quien quieras sumar. Vale 7 días y se muestra una sola vez:").size(tokens::FONT_SMALL).color(p.dim));
            let mut shown = code.clone();
            ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY));
        }
        ui.add_space(tokens::SP_S);
        if this.leave_confirm.as_deref() == Some(group_id) {
            message_line(
                ui,
                p,
                "Si sos la última persona del grupo, el grupo y todos sus proyectos se borran de la nube. Las copias que tenés en esta computadora no se tocan.",
                true,
            );
            ui.add_space(tokens::SP_XS);
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new(format!("{}  Sí, salir del grupo", crate::icon::SIGN_OUT))).clicked() {
                    *job = Some(Job::Leave(group_id.to_string()));
                }
                if ui.button("Cancelar").clicked() {
                    this.leave_confirm = None;
                }
            });
        } else if ui.add_enabled(!busy, egui::Button::new(format!("{}  Salir del grupo", crate::icon::SIGN_OUT))).clicked() {
            this.leave_confirm = Some(group_id.to_string());
        }
    });
    // El proyecto abierto (desde la pantalla de inicio no hay ninguno).
    if !title.is_empty() {
        kit::section(ui, p, "Este proyecto", None);
        kit::card(ui, p, |ui| {
            ui.set_width(ui.available_width());
            match (file, link) {
                (Some(_), Some(l)) if l.group == group_id => {
                    ui.label(RichText::new(format!("{}  Se sincroniza solo con la nube.", crate::icon::CHECK_CIRCLE)).color(p.success));
                    if ui.button("Dejar de sincronizar").clicked() {
                        *unlink = true;
                    }
                }
                (Some(_), Some(_)) => {
                    ui.label(RichText::new("Este proyecto está unido a otro grupo.").color(p.dim));
                }
                (Some(f), None) => {
                    ui.add(egui::TextEdit::singleline(&mut this.share_name).hint_text(title).desired_width(f32::INFINITY));
                    ui.add_space(tokens::SP_XS);
                    if ui.add_enabled(!busy, egui::Button::new("Compartir este proyecto en el grupo")).clicked() {
                        let name = if this.share_name.trim().is_empty() { title.to_string() } else { this.share_name.trim().to_string() };
                        *job = Some(Job::Share { group: group_id.to_string(), name, file: f.to_path_buf(), who: author.to_string() });
                    }
                }
                (None, _) => {
                    ui.label(RichText::new("Guardá el proyecto en un archivo para poder compartirlo.").color(p.dim));
                }
            }
        });
    }
    kit::section(ui, p, "Proyectos del grupo", None);
    if this.projects.is_empty() {
        ui.label(RichText::new("Todavía no hay proyectos compartidos en este grupo.").color(p.dim));
    }
    for (id, name) in &this.projects {
        kit::card(ui, p, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Abrir una copia")).clicked() {
                        *open_dest = Some((id.clone(), name.clone()));
                    }
                });
            });
        });
    }
}
