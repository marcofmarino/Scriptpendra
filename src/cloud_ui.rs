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
    theme::Palette,
    ui::{kit, tokens},
};

/// Cada cuántos segundos se sincroniza un proyecto unido a la nube.
const SYNC_SECS: f64 = 6.0;

/// Cada cuántos segundos se avisa dónde estoy (el servidor da por ido a quien lleva más de 30 sin avisar).
const PRESENCE_SECS: f64 = 8.0;

enum Job {
    SignIn(String, String),
    SignUp(String, String),
    SignOut,
    Groups,
    CreateGroup(String),
    Invite(String),
    Join(String),
    Projects(String),
    Share { group: String, name: String, file: PathBuf },
    Open { group: String, project: String, file: PathBuf },
    Sync { file: PathBuf, link: CloudLink },
    Usage(String),
    Images(ImageWork),
    Presence { project: String, who: String, screen: String, detail: String },
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
        Job::Share { group, name, file } => cloudsync::share_project(client, &group, &name, &file)
            .map(Event::Shared)
            .unwrap_or_else(fail_sync),
        Job::Open { group, project, file } => cloudsync::download_project(client, &group, &project, &file)
            .map(|link| Event::Opened(file.clone(), link))
            .unwrap_or_else(fail_sync),
        Job::Sync { file, mut link } => match cloudsync::sync_once(client, &file, &mut link) {
            Ok(report) => Event::Synced(report, link),
            Err(e) => fail_sync(e),
        },
        Job::Images(work) => Event::Images(cloudimages::run(client, work)),
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
    last_sync: f64,
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
    pub fn tick(&mut self, ctx: &egui::Context, now: f64, settings: &Settings, file: Option<&Path>, link: Option<&CloudLink>) -> Out {
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
                    self.signed_as = Some(email);
                    self.password.clear();
                    self.note.clear();
                    self.send(Job::Groups);
                }
                Event::SignedOut => {
                    self.signed_as = None;
                    self.groups.clear();
                    self.projects.clear();
                }
                Event::NeedsConfirmation => {
                    self.note = "Te mandamos un mail para confirmar la cuenta. Abrilo y después iniciá sesión.".into();
                }
                Event::Groups(groups) => {
                    self.groups = groups;
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
                    out.message = Some("Proyecto compartido en la nube.".into());
                    out.link = Some(new_link);
                }
                Event::Opened(path, new_link) => {
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
            && now - self.last_sync > SYNC_SECS
        {
            self.last_sync = now;
            self.syncing = true;
            self.send(Job::Sync { file: file.to_path_buf(), link: link.clone() });
        }
        if self.tx.is_some() && file.is_some() && link.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(SYNC_SECS));
        }
        out
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

    /// La ventana de la nube. `file` y `title` son los del proyecto abierto (si está guardado en un archivo).
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
        let signed = self.signed_as.clone();
        let busy = self.busy;
        let mut chosen_group: Option<usize> = None;
        let mut open_dest: Option<(String, String)> = None;
        let mut sign_out = false;
        let mut unlink = false;
        let mut clean = false;
        let mut save_config = false;
        let (mut config_url, mut config_key) = (self.config_url.clone(), self.config_key.clone());
        if config_url.is_empty() && config_key.is_empty() {
            config_url = settings.cloud_url.clone();
        }
        let this = &mut *self;
        let closed = kit::dialog(
            ctx,
            p,
            "cloud",
            "Nube",
            kit::DialogWidth::Normal,
            |ui| {
                if !configured {
                    ui.label("Pegá la dirección y la clave pública de la nube que te pasaron. La clave pública es la «anon»; nunca pegues una clave «service_role».");
                    ui.add(egui::TextEdit::singleline(&mut config_url).hint_text("https://xxxx.supabase.co").desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(&mut config_key).hint_text("Clave pública (anon)").desired_width(f32::INFINITY));
                    let ok = cloud::parse_url(&config_url).is_ok() && !config_key.trim().is_empty();
                    if ui.add_enabled(ok, egui::Button::new("Guardar")).clicked() {
                        save_config = true;
                    }
                    if !config_url.is_empty()
                        && let Err(why) = cloud::parse_url(&config_url)
                    {
                        ui.label(RichText::new(why).color(p.warning));
                    }
                    return;
                }
                let Some(email) = &signed else {
                    ui.label("Iniciá sesión para compartir y sincronizar proyectos.");
                    ui.add(egui::TextEdit::singleline(&mut this.email).hint_text("Mail").desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(&mut this.password).password(true).hint_text("Contraseña (10 caracteres o más)").desired_width(f32::INFINITY));
                    ui.horizontal(|ui| {
                        let ready = !busy && !this.email.trim().is_empty() && !this.password.is_empty();
                        if ui.add_enabled(ready, egui::Button::new("Entrar")).clicked() {
                            job = Some(Job::SignIn(this.email.trim().to_string(), this.password.clone()));
                        }
                        if ui.add_enabled(ready, egui::Button::new("Crear cuenta")).clicked() {
                            job = Some(Job::SignUp(this.email.trim().to_string(), this.password.clone()));
                        }
                    });
                    if !this.note.is_empty() {
                        ui.label(RichText::new(&this.note).color(p.dim));
                    }
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Sesión: {email}")).color(p.dim));
                    if ui.button("Salir").clicked() {
                        sign_out = true;
                    }
                });
                if !this.note.is_empty() {
                    ui.label(RichText::new(&this.note).color(p.warning));
                }
                // Los grupos.
                kit::section(ui, p, "Grupos", None);
                if this.groups.is_empty() {
                    ui.label(RichText::new("Todavía no estás en ningún grupo. Creá uno o entrá con un código.").color(p.dim));
                }
                for (i, (_, name, _)) in this.groups.iter().enumerate() {
                    if ui.selectable_label(this.selected_group == Some(i), name).clicked() {
                        chosen_group = Some(i);
                    }
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut this.new_group).hint_text("Nombre del grupo nuevo").desired_width(200.0));
                    if ui.add_enabled(!busy && !this.new_group.trim().is_empty(), egui::Button::new("Crear grupo")).clicked() {
                        job = Some(Job::CreateGroup(this.new_group.trim().to_string()));
                        this.new_group.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut this.join_code).hint_text("Código de invitación").desired_width(200.0));
                    if ui.add_enabled(!busy && !this.join_code.trim().is_empty(), egui::Button::new("Entrar con el código")).clicked() {
                        job = Some(Job::Join(this.join_code.trim().to_string()));
                    }
                });
                let Some((group_id, group_name, _)) = this.selected_group.and_then(|i| this.groups.get(i)).cloned() else {
                    return;
                };
                // El grupo elegido.
                kit::section(ui, p, &format!("Grupo «{group_name}»"), None);
                if let Some((used, quota, count)) = this.usage {
                    let fraction = if quota > 0 { (used as f32 / quota as f32).clamp(0.0, 1.0) } else { 1.0 };
                    ui.label(RichText::new(format!("Imágenes: {} de {} ({} en total)", megabytes(used), megabytes(quota), count)).size(tokens::FONT_SMALL));
                    ui.add(egui::ProgressBar::new(fraction).fill(if fraction > 0.9 { p.warning } else { p.accent }));
                    if fraction > 0.8 {
                        ui.label(RichText::new("El espacio para imágenes está casi lleno: conviene quitar las que no se usan.").size(tokens::FONT_SMALL).color(p.warning));
                    }
                }
                if unused_images > 0 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{unused_images} imágenes de este proyecto ya no se usan.")).size(tokens::FONT_SMALL));
                        if ui.button("Quitarlas de la nube").clicked() {
                            clean = true;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Crear código de invitación")).clicked() {
                        job = Some(Job::Invite(group_id.clone()));
                    }
                });
                if let Some(code) = &this.invite {
                    ui.label(RichText::new("Pasale este código a quien quieras sumar. Vale 7 días y se muestra una sola vez:").size(tokens::FONT_SMALL).color(p.dim));
                    let mut shown = code.clone();
                    ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY));
                }
                // El proyecto abierto.
                kit::section(ui, p, "Este proyecto", None);
                match (file, link) {
                    (Some(_), Some(l)) if l.group == group_id => {
                        ui.label(RichText::new("Se sincroniza solo con la nube.").color(p.success));
                        if ui.button("Dejar de sincronizar").clicked() {
                            unlink = true;
                        }
                    }
                    (Some(_), Some(_)) => {
                        ui.label(RichText::new("Este proyecto está unido a otro grupo.").color(p.dim));
                    }
                    (Some(_), None) => {
                        ui.add(egui::TextEdit::singleline(&mut this.share_name).hint_text(title).desired_width(260.0));
                        if ui.add_enabled(!busy, egui::Button::new("Compartir este proyecto en el grupo")).clicked() {
                            let name = if this.share_name.trim().is_empty() { title.to_string() } else { this.share_name.trim().to_string() };
                            if let Some(file) = file {
                                job = Some(Job::Share { group: group_id.clone(), name, file: file.to_path_buf() });
                            }
                        }
                    }
                    (None, _) => {
                        ui.label(RichText::new("Guardá el proyecto en un archivo para poder compartirlo.").color(p.dim));
                    }
                }
                // Los proyectos del grupo.
                kit::section(ui, p, "Proyectos del grupo", None);
                if this.projects.is_empty() {
                    ui.label(RichText::new("Todavía no hay proyectos compartidos en este grupo.").color(p.dim));
                }
                for (id, name) in &this.projects {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        if ui.add_enabled(!busy, egui::Button::new("Abrir una copia")).clicked() {
                            open_dest = Some((id.clone(), name.clone()));
                        }
                    });
                }
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
