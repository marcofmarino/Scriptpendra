//! Plan de rodaje: una lista ordenada de tiras (una por escena) separadas en días de rodaje. Las escenas que no están
//! en la lista quedan sin programar. Acá están los datos y lo que se calcula de ellos (octavos y tiempo del día, avisos,
//! calendario del elenco); la pantalla está en `plan.rs`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::{Acquisition, PermitStatus, Project, heading_parts};

/// Interior de día, exterior de día, interior de noche y exterior de noche: la convención habitual de las tiras.
pub const DEFAULT_STRIP_COLORS: [[u8; 3]; 4] = [
    [225, 225, 225],
    [240, 205, 70],
    [90, 140, 230],
    [90, 185, 110],
];

/// Un día de rodaje.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShootDay {
    pub id: u64,
    /// AAAA-MM-DD; vacío mientras no se decidió.
    pub date: String,
    /// Citación general y fin estimado (HH:MM).
    pub call_time: String,
    pub wrap_time: String,
    pub lunch: String,
    pub forecast: String,
    /// Cuándo se cita a cada personaje del elenco ese día (por id de personaje).
    pub cast_calls: std::collections::BTreeMap<u64, CastCall>,
    /// Citación y notas de cada área del equipo (por nombre de departamento).
    pub area_calls: std::collections::BTreeMap<String, String>,
    pub area_notes: std::collections::BTreeMap<String, String>,
    pub notes: String,
}

/// A qué hora se cita a una persona del elenco: general, maquillaje y llegada al set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CastCall {
    pub call: String,
    pub makeup: String,
    pub set: String,
}

/// Cada renglón del plan: una escena, el comienzo de un día o un cartel («Mudanza a la casa de Pedro»).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StripEntry {
    /// Una escena entera.
    Scene(u64),
    /// Un tramo de una escena: algunos de sus planos, en el orden en que se filman. Una escena puede tener varios tramos,
    /// en días distintos o en el mismo día entre tramos de otras escenas.
    Part(u64, Vec<u64>),
    DayBreak(u64),
    Banner(String),
}

impl StripEntry {
    /// La escena a la que pertenece el renglón, entera o en tramos.
    pub fn scene(&self) -> Option<u64> {
        match self {
            StripEntry::Scene(id) | StripEntry::Part(id, _) => Some(*id),
            _ => None,
        }
    }
}

/// Los segundos que se cuentan de un plano al que todavía no se le cargó la duración.
pub const DEFAULT_SHOT_SECONDS: f32 = 5.0;

/// Un día con las posiciones (en `Project::schedule`) de lo que se filma en él.
#[derive(Clone, Debug)]
pub struct DayView {
    pub id: u64,
    /// Número de jornada: 1, 2, 3…
    pub number: usize,
    /// Posición del separador del día.
    pub at: usize,
    /// Posiciones de las tiras y los carteles de este día.
    pub entries: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DayStats {
    /// Duración en pantalla de los planos del día (con `DEFAULT_SHOT_SECONDS` donde falta) y si alguna es una referencia.
    pub screen_seconds: f32,
    pub screen_estimated: bool,
    pub scenes: usize,
    pub eighths: u32,
    pub minutes: u32,
    /// Hay escenas sin tiempo propio: parte del total es un cálculo por octavos.
    pub estimated: bool,
    pub setups: usize,
}

/// Cómo está una persona del elenco en un día del plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastCode {
    /// Primer día de trabajo (SW).
    Start,
    /// Trabaja (W).
    Work,
    /// Último día de trabajo (WF).
    Finish,
    /// Trabaja un solo día (SWF).
    StartFinish,
    /// En espera: entre su primer y su último día, sin trabajar (H).
    Hold,
    Off,
}

impl CastCode {
    pub fn code(self) -> &'static str {
        match self {
            CastCode::Start => "SW",
            CastCode::Work => "W",
            CastCode::Finish => "WF",
            CastCode::StartFinish => "SWF",
            CastCode::Hold => "H",
            CastCode::Off => "",
        }
    }

    pub fn works(self) -> bool {
        matches!(
            self,
            CastCode::Start | CastCode::Work | CastCode::Finish | CastCode::StartFinish
        )
    }
}

#[derive(Clone, Debug)]
pub struct CastRow {
    pub number: Option<u32>,
    pub name: String,
    pub actor: Option<String>,
    pub codes: Vec<CastCode>,
    pub worked: usize,
    pub held: usize,
}

/// ¿Es una escena de noche? Lo dice el momento del encabezado.
pub fn is_night(time: &str) -> bool {
    let t = time.to_uppercase();
    ["NOCHE", "ANOCHECER", "MADRUGADA"]
        .iter()
        .any(|w| t.contains(w))
}

impl Project {
    /// Saca del plan lo que ya no existe (escenas borradas, repetidas, días sin separador) y se asegura de que lo primero
    /// sea un día. Sin escenas ni días no hace nada.
    pub fn prune_schedule(&mut self) {
        let scenes: HashSet<u64> = self.script_scenes().into_iter().map(|(id, _)| id).collect();
        let mut seen_scenes: HashSet<u64> = HashSet::new();
        let mut seen_days: HashSet<u64> = HashSet::new();
        let days: HashSet<u64> = self.shoot_days.iter().map(|d| d.id).collect();
        self.schedule.retain(|e| match e {
            StripEntry::Scene(id) => scenes.contains(id) && seen_scenes.insert(*id),
            StripEntry::Part(id, _) => scenes.contains(id),
            StripEntry::DayBreak(id) => days.contains(id) && seen_days.insert(*id),
            StripEntry::Banner(_) => true,
        });
        // Los tramos: solo planos que existen y son de su escena, cada plano en un solo tramo, y nada si la escena está
        // entera en el plan.
        let mut used: HashSet<u64> = HashSet::new();
        let shots: Vec<(u64, u64)> = self.shots.iter().map(|s| (s.id, s.scene)).collect();
        for entry in &mut self.schedule {
            if let StripEntry::Part(scene, list) = entry {
                list.retain(|shot| shots.contains(&(*shot, *scene)) && used.insert(*shot));
            }
        }
        self.schedule.retain(|e| match e {
            StripEntry::Part(scene, list) => !list.is_empty() && !seen_scenes.contains(scene),
            _ => true,
        });
        // Los días sin separador se borran, y lo que queda antes del primer día pasa a un día nuevo.
        self.shoot_days.retain(|d| seen_days.contains(&d.id));
        if !self.schedule.is_empty() && !matches!(self.schedule[0], StripEntry::DayBreak(_)) {
            let id = self.new_day_id();
            self.shoot_days.push(ShootDay {
                id,
                ..Default::default()
            });
            self.schedule.insert(0, StripEntry::DayBreak(id));
        }
    }

    fn new_day_id(&mut self) -> u64 {
        self.alloc_id()
    }

    /// Agrega un día de rodaje al final del plan y devuelve su id.
    pub fn add_shoot_day(&mut self) -> u64 {
        let id = self.new_day_id();
        self.shoot_days.push(ShootDay {
            id,
            ..Default::default()
        });
        self.schedule.push(StripEntry::DayBreak(id));
        id
    }

    /// Quita un día: sus tiras pasan al día anterior; si era el primero, vuelven a quedar sin programar.
    pub fn remove_shoot_day(&mut self, id: u64) {
        let Some(at) = self
            .schedule
            .iter()
            .position(|e| *e == StripEntry::DayBreak(id))
        else {
            self.shoot_days.retain(|d| d.id != id);
            return;
        };
        let first = at == 0;
        self.schedule.remove(at);
        if first {
            while self
                .schedule
                .get(at)
                .is_some_and(|e| !matches!(e, StripEntry::DayBreak(_)))
            {
                self.schedule.remove(at);
            }
        }
        self.shoot_days.retain(|d| d.id != id);
    }

    /// Los días del plan, con las posiciones de lo que se filma en cada uno.
    pub fn days_view(&self) -> Vec<DayView> {
        let mut out: Vec<DayView> = Vec::new();
        for (i, entry) in self.schedule.iter().enumerate() {
            match entry {
                StripEntry::DayBreak(id) => out.push(DayView {
                    id: *id,
                    number: out.len() + 1,
                    at: i,
                    entries: Vec::new(),
                }),
                _ => {
                    if let Some(day) = out.last_mut() {
                        day.entries.push(i);
                    }
                }
            }
        }
        out
    }

    /// Las escenas que están enteras en el plan o cuyos planos ya están todos repartidos en tramos.
    fn planned_scenes(&self) -> HashSet<u64> {
        let mut planned: HashSet<u64> = self
            .schedule
            .iter()
            .filter_map(|e| {
                if let StripEntry::Scene(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        for (scene, _) in self.script_scenes() {
            if planned.contains(&scene) {
                continue;
            }
            let own: Vec<u64> = self
                .shots
                .iter()
                .filter(|s| s.scene == scene)
                .map(|s| s.id)
                .collect();
            if !own.is_empty() && own.iter().all(|s| self.shot_in_a_part(*s)) {
                planned.insert(scene);
            }
        }
        planned
    }

    fn shot_in_a_part(&self, shot: u64) -> bool {
        self.schedule
            .iter()
            .any(|e| matches!(e, StripEntry::Part(_, list) if list.contains(&shot)))
    }

    /// Las escenas del guion que todavía no están (enteras) en el plan, en orden. Una escena con algunos planos ya
    /// repartidos en tramos sigue acá hasta que no le quede ninguno suelto.
    pub fn unscheduled_scenes(&self) -> Vec<u64> {
        let planned = self.planned_scenes();
        self.script_scenes()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| !planned.contains(id))
            .collect()
    }

    /// Los planos de una escena que no están en ningún tramo (todos, si la escena no está entera en el plan).
    pub fn unscheduled_shots(&self, scene: u64) -> Vec<u64> {
        if self.schedule.iter().any(|e| *e == StripEntry::Scene(scene)) {
            return Vec::new();
        }
        self.shots
            .iter()
            .filter(|s| s.scene == scene && !self.shot_in_a_part(s.id))
            .map(|s| s.id)
            .collect()
    }

    /// Los segundos de unos planos en pantalla y si alguno no tiene duración cargada (se cuentan `DEFAULT_SHOT_SECONDS`).
    pub fn shots_seconds(&self, shots: &[u64]) -> (f32, bool) {
        let mut total = 0.0;
        let mut estimated = false;
        for id in shots {
            let Some(shot) = self.shots.iter().find(|s| s.id == *id) else {
                continue;
            };
            match crate::techscript::parse_seconds(&shot.duration) {
                Some(seconds) => total += seconds,
                None => {
                    total += DEFAULT_SHOT_SECONDS;
                    estimated = true;
                }
            }
        }
        (total, estimated)
    }

    /// Cuánto se tarda en filmar unos planos de una escena (suma de lo que lleva cada uno).
    pub fn part_minutes(&self, shots: &[u64]) -> u32 {
        let minutes = self.shot_minutes();
        shots.iter().filter_map(|s| minutes.get(s)).sum()
    }

    /// Los octavos de un tramo: los de la escena repartidos según cuántos planos tiene el tramo.
    pub fn part_eighths(&self, scene: u64, shots: &[u64]) -> u32 {
        let total = self
            .shots
            .iter()
            .filter(|s| s.scene == scene)
            .count()
            .max(1) as u32;
        let whole = u32::from(self.scene_eighths(scene));
        (whole * shots.len() as u32).div_ceil(total).min(whole)
    }

    /// Parte una escena entera del plan en un tramo con todos sus planos, para poder repartirlos. Devuelve si lo hizo.
    pub fn split_scene_into_part(&mut self, scene: u64) -> bool {
        let shots: Vec<u64> = self
            .shots
            .iter()
            .filter(|s| s.scene == scene)
            .map(|s| s.id)
            .collect();
        let Some(at) = self
            .schedule
            .iter()
            .position(|e| *e == StripEntry::Scene(scene))
        else {
            return false;
        };
        if shots.is_empty() {
            return false;
        }
        self.schedule[at] = StripEntry::Part(scene, shots);
        true
    }

    /// Pone planos de una escena en un tramo nuevo justo antes de la posición `at` (o al final del último día). Salen de
    /// los tramos donde estaban; si la escena estaba entera en el plan, se parte primero. Devuelve dónde quedó.
    pub fn place_shots(&mut self, scene: u64, shots: &[u64], at: Option<usize>) -> usize {
        if shots.is_empty() {
            return at.unwrap_or(self.schedule.len());
        }
        if !self
            .schedule
            .iter()
            .any(|e| matches!(e, StripEntry::DayBreak(_)))
        {
            self.add_shoot_day();
        }
        self.split_scene_into_part(scene);
        let mut target = at.unwrap_or(self.schedule.len()).min(self.schedule.len());
        let mut emptied_before = 0;
        let mut kept = Vec::with_capacity(self.schedule.len());
        for (pos, entry) in std::mem::take(&mut self.schedule).into_iter().enumerate() {
            match entry {
                StripEntry::Part(s, mut list) => {
                    list.retain(|x| !shots.contains(x));
                    if list.is_empty() {
                        if pos < target {
                            emptied_before += 1;
                        }
                    } else {
                        kept.push(StripEntry::Part(s, list));
                    }
                }
                other => kept.push(other),
            }
        }
        self.schedule = kept;
        target = (target - emptied_before).clamp(1, self.schedule.len());
        self.schedule
            .insert(target, StripEntry::Part(scene, shots.to_vec()));
        target
    }

    /// Saca unos planos de sus tramos (vuelven a la bandeja); un tramo que queda vacío desaparece.
    pub fn unschedule_shots(&mut self, shots: &[u64]) {
        for entry in &mut self.schedule {
            if let StripEntry::Part(_, list) = entry {
                list.retain(|x| !shots.contains(x));
            }
        }
        self.schedule
            .retain(|e| !matches!(e, StripEntry::Part(_, list) if list.is_empty()));
    }

    /// Sube o baja un plano dentro de su tramo (el orden en que se filman). Devuelve si se movió.
    pub fn nudge_part_shot(&mut self, at: usize, shot: u64, delta: i32) -> bool {
        let Some(StripEntry::Part(_, list)) = self.schedule.get_mut(at) else {
            return false;
        };
        let Some(from) = list.iter().position(|s| *s == shot) else {
            return false;
        };
        let to = from as i32 + delta;
        if to < 0 || to as usize >= list.len() {
            return false;
        }
        list.swap(from, to as usize);
        true
    }

    /// Pone escenas en el plan (o las mueve si ya estaban) justo antes de la posición `at`; sin posición, al final del
    /// último día (que se crea si no hay ninguno). Devuelve dónde quedó la primera.
    pub fn place_scenes(&mut self, scenes: &[u64], at: Option<usize>) -> usize {
        if self.schedule.is_empty()
            || !self
                .schedule
                .iter()
                .any(|e| matches!(e, StripEntry::DayBreak(_)))
        {
            self.add_shoot_day();
        }
        let mut target = at.unwrap_or(self.schedule.len()).min(self.schedule.len());
        // Al sacar las que ya estaban, lo que cae antes de la posición la corre.
        let moving: HashSet<u64> = scenes.iter().copied().collect();
        // Una escena que se pone entera deja de tener tramos.
        let goes = |e: &StripEntry| matches!(e, StripEntry::Scene(id) | StripEntry::Part(id, _) if moving.contains(id));
        let before = self.schedule[..target].iter().filter(|e| goes(e)).count();
        self.schedule.retain(|e| !goes(e));
        target -= before;
        // Nada va antes del primer día.
        target = target.max(1);
        for (n, id) in scenes.iter().enumerate() {
            self.schedule.insert(target + n, StripEntry::Scene(*id));
        }
        target
    }

    /// Saca escenas del plan: vuelven a la bandeja.
    pub fn unschedule_scenes(&mut self, scenes: &[u64]) {
        self.schedule.retain(
            |e| !matches!(e, StripEntry::Scene(id) | StripEntry::Part(id, _) if scenes.contains(id)),
        );
    }

    /// Mueve el renglón `at` una posición (−1 arriba, 1 abajo). Pasar sobre el separador de un día lo cambia de día.
    /// Devuelve si se movió.
    pub fn nudge_entry(&mut self, at: usize, delta: i32) -> bool {
        let to = at as i32 + delta;
        if at >= self.schedule.len()
            || to < 1
            || to as usize >= self.schedule.len()
            || matches!(self.schedule[at], StripEntry::DayBreak(_))
        {
            return false;
        }
        self.schedule.swap(at, to as usize);
        true
    }

    /// Un cartel nuevo antes de la posición `at` (o al final).
    pub fn add_banner(&mut self, at: Option<usize>, text: &str) {
        if self.schedule.is_empty() {
            self.add_shoot_day();
        }
        let at = at
            .unwrap_or(self.schedule.len())
            .clamp(1, self.schedule.len());
        self.schedule
            .insert(at, StripEntry::Banner(text.to_string()));
    }

    /// Ordena las escenas de un día para no mudar al equipo: las de un mismo lugar de rodaje juntas (en el orden en que
    /// aparece cada lugar), y dentro de cada lugar, primero las de día y después las de noche.
    pub fn sort_day_by_location(&mut self, day: u64) {
        let Some(view) = self.days_view().into_iter().find(|d| d.id == day) else {
            return;
        };
        let entries: Vec<(usize, StripEntry)> = view
            .entries
            .iter()
            .filter(|&&i| self.schedule[i].scene().is_some())
            .map(|&i| (i, self.schedule[i].clone()))
            .collect();
        let scene_of = |e: &StripEntry| e.scene().unwrap_or_default();
        let mut order: Vec<Option<u64>> = Vec::new();
        for (_, e) in &entries {
            let place = self.scene_shoot_location(scene_of(e));
            if !order.contains(&place) {
                order.push(place);
            }
        }
        let night = |p: &Project, id: u64| {
            is_night(
                &heading_parts(
                    &p.script_scenes()
                        .iter()
                        .find(|(s, _)| *s == id)
                        .map_or(String::new(), |(_, h)| h.clone()),
                )
                .2,
            )
        };
        let keys: Vec<(usize, bool)> = entries
            .iter()
            .map(|(_, e)| {
                let id = scene_of(e);
                (
                    order
                        .iter()
                        .position(|o| *o == self.scene_shoot_location(id))
                        .unwrap_or(0),
                    night(self, id),
                )
            })
            .collect();
        let mut indexed: Vec<usize> = (0..entries.len()).collect();
        indexed.sort_by_key(|&n| keys[n]);
        for (&(pos, _), &n) in entries.iter().zip(&indexed) {
            self.schedule[pos] = entries[n].1.clone();
        }
    }

    /// Ordena las escenas de un día por emplazamiento: primero las que se filman antes según el orden de rodaje del guion
    /// técnico; las que todavía no lo tienen, al final y como estaban.
    pub fn sort_day_by_setup(&mut self, day: u64) {
        let Some(view) = self.days_view().into_iter().find(|d| d.id == day) else {
            return;
        };
        let entries: Vec<(usize, StripEntry)> = view
            .entries
            .iter()
            .filter(|&&i| self.schedule[i].scene().is_some())
            .map(|&i| (i, self.schedule[i].clone()))
            .collect();
        let key = |p: &Project, e: &StripEntry| {
            let scene = e.scene().unwrap_or_default();
            p.shots
                .iter()
                .filter(|s| s.scene == scene)
                .filter(|s| match e {
                    StripEntry::Part(_, list) => list.contains(&s.id),
                    _ => true,
                })
                .filter_map(|s| s.shoot_order)
                .min()
                .unwrap_or(u32::MAX)
        };
        let mut sorted: Vec<StripEntry> = entries.iter().map(|(_, e)| e.clone()).collect();
        sorted.sort_by_key(|e| key(self, e));
        for ((pos, _), entry) in entries.iter().zip(sorted) {
            self.schedule[*pos] = entry;
        }
    }

    /// Cuántos emplazamientos tiene una escena: cada par (bloqueo, cámara) con planos del guion técnico es uno.
    pub fn scene_setups(&self, scene: u64) -> usize {
        let pairs: HashSet<(u64, u64)> = self
            .shots
            .iter()
            .filter(|s| s.scene == scene)
            .filter_map(|s| s.blocking.zip(s.camera))
            .collect();
        pairs.len()
    }

    /// El tiempo que se calcula para rodar una escena, en minutos, y si es un cálculo (por octavos) o un dato.
    pub fn scene_minutes(&self, scene: u64) -> (u32, bool) {
        match (
            self.scenes.get(&scene).and_then(|m| m.est_minutes),
            self.scene_shots_minutes(scene),
        ) {
            (Some(m), _) => (m, false),
            // Con planos cargados, lo que se calcula de ellos; si no, por octavos de página.
            (None, Some(m)) => (m, true),
            (None, None) => (
                u32::from(self.scene_eighths(scene)) * self.minutes_per_eighth,
                true,
            ),
        }
    }

    /// Lo que suma un día del plan.
    pub fn day_stats(&self, day: &DayView) -> DayStats {
        let mut stats = DayStats::default();
        let mut scenes: HashSet<u64> = HashSet::new();
        let mut counted_whole: HashSet<u64> = HashSet::new();
        for &i in &day.entries {
            match &self.schedule[i] {
                StripEntry::Scene(id) => {
                    let (minutes, estimated) = self.scene_minutes(*id);
                    scenes.insert(*id);
                    counted_whole.insert(*id);
                    stats.eighths += u32::from(self.scene_eighths(*id));
                    stats.minutes += minutes;
                    stats.estimated |= estimated;
                    stats.setups += self.scene_setups(*id);
                    let own: Vec<u64> = self
                        .shots
                        .iter()
                        .filter(|s| s.scene == *id)
                        .map(|s| s.id)
                        .collect();
                    let (seconds, guessed) = self.shots_seconds(&own);
                    stats.screen_seconds += seconds;
                    stats.screen_estimated |= guessed;
                }
                StripEntry::Part(id, shots) => {
                    scenes.insert(*id);
                    stats.eighths += self.part_eighths(*id, shots);
                    stats.minutes += self.part_minutes(shots);
                    stats.estimated = true;
                    let pairs: HashSet<(u64, u64)> = self
                        .shots
                        .iter()
                        .filter(|s| shots.contains(&s.id))
                        .filter_map(|s| s.blocking.zip(s.camera))
                        .collect();
                    stats.setups += pairs.len();
                    let (seconds, guessed) = self.shots_seconds(shots);
                    stats.screen_seconds += seconds;
                    stats.screen_estimated |= guessed;
                }
                _ => {}
            }
        }
        stats.scenes = scenes.len();
        let _ = counted_whole;
        stats
    }

    /// El día de rodaje en que cae una escena, si está en el plan.
    pub fn scene_day(&self, scene: u64) -> Option<DayView> {
        self.days_view().into_iter().find(|d| {
            d.entries
                .iter()
                .any(|&i| self.schedule[i].scene() == Some(scene))
        })
    }

    /// Qué hay para tener en cuenta en un día: se pasa del tiempo, mezcla día y noche, alguien que no puede ese día o un
    /// lugar sin permiso aprobado.
    pub fn day_warnings(&self, day: &DayView) -> Vec<String> {
        let mut out = Vec::new();
        let stats = self.day_stats(day);
        if stats.minutes > self.day_minutes {
            out.push(format!(
                "Pasa de la jornada: {} contra {}",
                fmt_minutes(stats.minutes),
                fmt_minutes(self.day_minutes)
            ));
        }
        let headings = self.script_scenes();
        let mut scenes: Vec<u64> = day
            .entries
            .iter()
            .filter_map(|&i| self.schedule[i].scene())
            .collect();
        scenes.dedup();
        let nights: Vec<bool> = scenes
            .iter()
            .map(|id| {
                is_night(
                    &headings
                        .iter()
                        .find(|(s, _)| s == id)
                        .map_or(String::new(), |(_, h)| heading_parts(h).2),
                )
            })
            .collect();
        if nights.contains(&true) && nights.contains(&false) {
            out.push("Mezcla escenas de día y de noche".to_string());
        }
        let date = self
            .shoot_days
            .iter()
            .find(|d| d.id == day.id)
            .map(|d| d.date.clone())
            .unwrap_or_default();
        let mut warned: HashSet<u64> = HashSet::new();
        for id in &scenes {
            if !date.is_empty() {
                for c in self.scene_cast_ids(*id) {
                    let Some(character) = self.character(c) else {
                        continue;
                    };
                    let Some(actor) = character
                        .actor
                        .and_then(|a| self.contacts.iter().find(|x| x.id == a))
                    else {
                        continue;
                    };
                    if actor.unavailable.iter().any(|r| r.contains(&date))
                        && warned.insert(actor.id)
                    {
                        out.push(format!("{} no está disponible el {date}", actor.name));
                    }
                }
            }
            if let Some(place) = self
                .scene_shoot_location(*id)
                .and_then(|p| self.shoot_locations.iter().find(|l| l.id == p))
            {
                if place.permit != PermitStatus::Approved && warned.insert(place.id) {
                    out.push(format!(
                        "{}: el permiso está {}",
                        place.name,
                        place.permit.label().to_lowercase()
                    ));
                }
            }
        }
        out
    }

    /// El calendario del elenco: las columnas son los días del plan y cada fila un personaje que trabaja en alguno.
    pub fn cast_calendar(&self) -> (Vec<String>, Vec<CastRow>) {
        let days = self.days_view();
        let labels: Vec<String> = days
            .iter()
            .map(|d| {
                self.shoot_days
                    .iter()
                    .find(|s| s.id == d.id)
                    .filter(|s| !s.date.is_empty())
                    .map_or(format!("D{}", d.number), |s| s.date.clone())
            })
            .collect();
        let mut works: Vec<(u64, Vec<bool>)> = Vec::new();
        for (n, day) in days.iter().enumerate() {
            for &i in &day.entries {
                let Some(scene) = self.schedule[i].scene() else {
                    continue;
                };
                for c in self.scene_cast_ids(scene) {
                    let at = works
                        .iter()
                        .position(|(id, _)| *id == c)
                        .unwrap_or_else(|| {
                            works.push((c, vec![false; days.len()]));
                            works.len() - 1
                        });
                    works[at].1[n] = true;
                }
            }
        }
        let mut rows: Vec<CastRow> = works
            .into_iter()
            .filter_map(|(id, working)| {
                let character = self.character(id)?;
                let first = working.iter().position(|w| *w)?;
                let last = working.iter().rposition(|w| *w)?;
                let codes: Vec<CastCode> = working
                    .iter()
                    .enumerate()
                    .map(|(n, w)| match (*w, n == first, n == last) {
                        (true, true, true) => CastCode::StartFinish,
                        (true, true, false) => CastCode::Start,
                        (true, false, true) => CastCode::Finish,
                        (true, false, false) => CastCode::Work,
                        (false, _, _) if n > first && n < last => CastCode::Hold,
                        _ => CastCode::Off,
                    })
                    .collect();
                let worked = codes.iter().filter(|c| c.works()).count();
                let held = codes.iter().filter(|c| **c == CastCode::Hold).count();
                let actor = character
                    .actor
                    .and_then(|a| self.contacts.iter().find(|x| x.id == a))
                    .map(|x| x.name.clone());
                Some(CastRow {
                    number: character.cast_number,
                    name: crate::model::clean_character_name(&character.name),
                    actor,
                    codes,
                    worked,
                    held,
                })
            })
            .collect();
        // Primero los que tienen número de elenco, en orden; después el resto, por nombre.
        rows.sort_by(|a, b| {
            (a.number.is_none(), a.number, &a.name).cmp(&(b.number.is_none(), b.number, &b.name))
        });
        (labels, rows)
    }

    /// Una pista para la tira: qué color le toca según interior/exterior y día/noche.
    pub fn strip_color(&self, scene: u64) -> [u8; 3] {
        let heading = self
            .script_scenes()
            .into_iter()
            .find(|(s, _)| *s == scene)
            .map(|(_, h)| h)
            .unwrap_or_default();
        let (kind, _, time) = heading_parts(&heading);
        let exterior = kind.starts_with("EXT");
        let index = match (exterior, is_night(&time)) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => 3,
        };
        self.strip_colors[index]
    }

    /// Cuántas escenas del guion hay sin programar y cuántos octavos suman.
    pub fn unscheduled_eighths(&self) -> u32 {
        self.unscheduled_scenes()
            .iter()
            .map(|id| u32::from(self.scene_eighths(*id)))
            .sum()
    }

    /// Lo que todavía falta conseguir para las escenas de un día (elementos pendientes de comprar, alquilar o fabricar).
    pub fn day_pending_items(&self, day: &DayView) -> usize {
        let scenes: HashSet<u64> = day
            .entries
            .iter()
            .filter_map(|&i| self.schedule[i].scene())
            .collect();
        let mut names: HashSet<u64> = HashSet::new();
        for scene in scenes {
            for item in self.scene_sheet_items(scene) {
                if let Some(id) = item
                    .item_id
                    .filter(|_| !matches!(item.status, Acquisition::Have))
                {
                    names.insert(id);
                }
            }
        }
        names.len()
    }
}

/// Minutos como horas: «8 h 30 min».
pub fn fmt_minutes(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DateRange, ParagraphKind::*};

    fn project() -> (Project, Vec<u64>) {
        let mut p = Project::default();
        p.paragraphs = [
            (SceneHeading, "INT. LIVING - DÍA"),
            (Action, "Ana espera."),
            (Character, "ANA"),
            (Dialogue, "Hola."),
            (SceneHeading, "EXT. CALLE - NOCHE"),
            (Action, "Llueve."),
            (SceneHeading, "INT. COCINA - DÍA"),
            (Character, "JUAN"),
            (Dialogue, "Chau."),
            (SceneHeading, "EXT. PLAZA - DÍA"),
            (Action, "Sol."),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p.auto_link_all();
        let ids = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        (p, ids)
    }

    fn order(p: &Project) -> Vec<String> {
        p.schedule
            .iter()
            .map(|e| match e {
                StripEntry::DayBreak(id) => format!(
                    "D{}",
                    p.days_view()
                        .iter()
                        .find(|d| d.id == *id)
                        .map_or(0, |d| d.number)
                ),
                StripEntry::Scene(id) => p
                    .script_scenes()
                    .iter()
                    .position(|(s, _)| s == id)
                    .map_or("?".into(), |n| (n + 1).to_string()),
                StripEntry::Part(id, shots) => format!(
                    "{}[{}]",
                    p.script_scenes()
                        .iter()
                        .position(|(s, _)| s == id)
                        .map_or("?".into(), |n| (n + 1).to_string()),
                    shots.len()
                ),
                StripEntry::Banner(t) => format!("[{t}]"),
            })
            .collect()
    }

    #[test]
    fn scenes_are_scheduled_moved_and_unscheduled_with_a_day_always_first() {
        let (mut p, ids) = project();
        assert_eq!(p.unscheduled_scenes(), ids);
        p.place_scenes(&[ids[0], ids[2]], None);
        assert_eq!(
            order(&p),
            ["D1", "1", "3"],
            "al agregar la primera escena se crea el día 1"
        );
        let second = p.add_shoot_day();
        p.place_scenes(&[ids[1]], None);
        assert_eq!(order(&p), ["D1", "1", "3", "D2", "2"]);
        // Mover varias a la vez, a otro día y antes de otra tira.
        p.place_scenes(&[ids[0], ids[2]], Some(4));
        assert_eq!(
            order(&p),
            ["D1", "D2", "1", "3", "2"],
            "las que se mueven se acomodan antes de la 2"
        );
        // Nada va antes del primer día.
        p.place_scenes(&[ids[3]], Some(0));
        assert_eq!(order(&p)[0], "D1");
        p.unschedule_scenes(&[ids[3], ids[1]]);
        assert_eq!(p.unscheduled_scenes(), vec![ids[1], ids[3]]);
        // Subir una tira sobre el separador la pasa al día anterior.
        assert!(p.nudge_entry(2, -1));
        assert_eq!(order(&p), ["D1", "1", "D2", "3"]);
        assert!(!p.nudge_entry(0, 1), "los separadores no se mueven");
        // Quitar un día pasa sus tiras al anterior.
        p.remove_shoot_day(second);
        assert!(
            p.days_view().len() == 1 && p.days_view()[0].entries.len() == 2,
            "{:?}",
            order(&p)
        );
    }

    #[test]
    fn removing_the_first_day_sends_its_scenes_back_to_the_tray_and_the_plan_cleans_itself() {
        let (mut p, ids) = project();
        let first = p.add_shoot_day();
        p.place_scenes(&[ids[0], ids[1]], None);
        p.add_shoot_day();
        p.place_scenes(&[ids[2]], None);
        p.remove_shoot_day(first);
        assert_eq!(order(&p), ["D1", "3"]);
        assert!(
            p.unscheduled_scenes().contains(&ids[0]) && p.unscheduled_scenes().contains(&ids[1])
        );
        // Una escena borrada sale del plan; los días sin separador y las repetidas también.
        p.paragraphs.retain(|q| q.id != ids[2]);
        p.schedule.push(StripEntry::Scene(ids[3]));
        p.schedule.push(StripEntry::Scene(ids[3]));
        p.schedule.push(StripEntry::DayBreak(999_999));
        p.prune_schedule();
        assert_eq!(p.schedule.len(), 2);
        assert_eq!(p.schedule[1], StripEntry::Scene(ids[3]));
        // Con tiras sueltas antes de cualquier día, aparece un día para ellas.
        let (mut q, qids) = project();
        q.schedule = vec![StripEntry::Scene(qids[0])];
        q.prune_schedule();
        assert_eq!(order(&q), ["D1", "1"]);
    }

    #[test]
    fn a_day_adds_up_eighths_time_and_setups_and_warns_when_it_runs_over() {
        let (mut p, ids) = project();
        p.place_scenes(&[ids[0], ids[1]], None);
        p.scenes.entry(ids[0]).or_default().est_minutes = Some(300);
        let day = p.days_view().remove(0);
        let stats = p.day_stats(&day);
        assert_eq!((stats.scenes, stats.estimated), (2, true));
        assert_eq!(stats.minutes, 300 + p.scene_eighths(ids[1]) as u32 * 10);
        // La escena 1 es de día y la 2 de noche: se avisa. El tiempo, no se pasa de 10 h.
        let warnings = p.day_warnings(&day);
        assert_eq!(warnings, ["Mezcla escenas de día y de noche"]);
        p.day_minutes = 200;
        assert!(
            p.day_warnings(&day)
                .iter()
                .any(|w| w.starts_with("Pasa de la jornada: 5 h 10 min contra 3 h 20 min")),
            "{:?}",
            p.day_warnings(&day)
        );
        // Los emplazamientos salen de las cámaras de los planos.
        let blocking = p.add_blocking(ids[0]);
        let camera = p
            .add_block_item(blocking, crate::model::BlockKind::Camera, 0.0, 0.0)
            .unwrap();
        for _ in 0..2 {
            let shot = p.add_shot(ids[0], None);
            let s = p.shots.iter_mut().find(|s| s.id == shot).unwrap();
            (s.blocking, s.camera) = (Some(blocking), Some(camera));
        }
        assert_eq!(
            p.scene_setups(ids[0]),
            1,
            "dos planos con la misma cámara son un emplazamiento"
        );
        assert_eq!(p.day_stats(&day).setups, 1);
        assert_eq!(fmt_minutes(90), "1 h 30 min");
        assert_eq!(fmt_minutes(45), "45 min");
    }

    #[test]
    fn the_day_warns_about_unavailable_actors_and_places_without_a_permit() {
        let (mut p, ids) = project();
        let actor = p.add_contact("Luisa");
        p.contacts[0].unavailable = vec![DateRange {
            from: "2026-11-03".into(),
            to: "2026-11-05".into(),
        }];
        let ana = p.character_id_by_name("ANA").unwrap();
        p.characters.iter_mut().find(|c| c.id == ana).unwrap().actor = Some(actor);
        let house = p.add_shoot_location("Casa de Pedro");
        p.scenes.entry(ids[0]).or_default().shoot_location = Some(house);
        let day_id = p.add_shoot_day();
        p.place_scenes(&[ids[0]], None);
        fn view(p: &Project) -> DayView {
            p.days_view().remove(0)
        }
        // Sin fecha no se puede saber si está disponible; el permiso pendiente se avisa siempre.
        assert_eq!(
            p.day_warnings(&view(&p)),
            ["Casa de Pedro: el permiso está pendiente"]
        );
        p.shoot_days
            .iter_mut()
            .find(|d| d.id == day_id)
            .unwrap()
            .date = "2026-11-04".into();
        assert!(
            p.day_warnings(&view(&p))
                .contains(&"Luisa no está disponible el 2026-11-04".to_string())
        );
        p.shoot_days[0].date = "2026-11-10".into();
        assert!(
            !p.day_warnings(&view(&p))
                .iter()
                .any(|w| w.contains("disponible"))
        );
        p.shoot_locations[0].permit = PermitStatus::Approved;
        assert!(p.day_warnings(&view(&p)).is_empty());
    }

    #[test]
    fn a_day_can_be_sorted_by_place_with_the_day_scenes_before_the_night_ones() {
        let (mut p, ids) = project();
        let house = p.add_shoot_location("Casa");
        let park = p.add_shoot_location("Parque");
        p.scenes.entry(ids[0]).or_default().shoot_location = Some(house);
        p.scenes.entry(ids[1]).or_default().shoot_location = Some(park); // noche
        p.scenes.entry(ids[2]).or_default().shoot_location = Some(house);
        p.scenes.entry(ids[3]).or_default().shoot_location = Some(park);
        let day = p.add_shoot_day();
        p.place_scenes(&[ids[0], ids[1], ids[2], ids[3]], None);
        p.sort_day_by_location(day);
        // Primero la casa (1 y 3), después el parque con la de día (4) antes que la de noche (2).
        assert_eq!(order(&p), ["D1", "1", "3", "4", "2"]);
    }

    #[test]
    fn the_cast_calendar_marks_start_work_finish_and_hold_days() {
        let (mut p, ids) = project();
        // Ana está en las escenas 1 y 2; Juan, en la 3.
        let ana = p.character_id_by_name("ANA").unwrap();
        let characters = p.characters.iter_mut().find(|c| c.id == ana).unwrap();
        characters.cast_number = Some(1);
        // Escena 2 con Ana: se la nombra en la acción.
        let pid = p
            .paragraphs
            .iter()
            .position(|q| q.text == "Llueve.")
            .unwrap();
        p.paragraphs[pid].text = "Llueve sobre Ana.".into();
        p.auto_link_all();
        for scenes in [&[ids[0]][..], &[ids[2]][..], &[ids[1]][..]] {
            p.add_shoot_day();
            p.place_scenes(scenes, None);
        }
        p.shoot_days[0].date = "2026-11-02".into();
        let (labels, rows) = p.cast_calendar();
        assert_eq!(labels, ["2026-11-02", "D2", "D3"]);
        let ana_row = rows.iter().find(|r| r.name == "ANA").unwrap();
        assert_eq!(
            ana_row.codes.iter().map(|c| c.code()).collect::<Vec<_>>(),
            ["SW", "H", "WF"]
        );
        assert_eq!((ana_row.worked, ana_row.held), (2, 1));
        let juan = rows.iter().find(|r| r.name == "JUAN").unwrap();
        assert_eq!(
            juan.codes.iter().map(|c| c.code()).collect::<Vec<_>>(),
            ["", "SWF", ""]
        );
        // Con número de elenco primero.
        assert_eq!(rows[0].name, "ANA");
    }

    #[test]
    fn strips_take_their_color_from_interior_exterior_and_day_night() {
        let (mut p, ids) = project();
        assert_eq!(p.strip_color(ids[0]), DEFAULT_STRIP_COLORS[0]);
        assert_eq!(p.strip_color(ids[1]), DEFAULT_STRIP_COLORS[3]);
        assert_eq!(p.strip_color(ids[3]), DEFAULT_STRIP_COLORS[1]);
        p.strip_colors[2] = [1, 2, 3];
        assert_eq!(p.strip_color(ids[2]), DEFAULT_STRIP_COLORS[0]);
        assert!(is_night("NOCHE") && is_night("anochecer") && !is_night("DÍA"));
    }

    #[test]
    fn a_scene_can_be_shot_in_several_days_and_two_scenes_can_share_a_day_by_shots() {
        let (mut p, ids) = project();
        let a = p.add_shot(ids[0], None);
        let b = p.add_shot(ids[0], None);
        let c = p.add_shot(ids[0], None);
        let d = p.add_shot(ids[2], None);
        p.add_shoot_day();
        let second = p.add_shoot_day();
        // Dos planos de la escena 1 el primer día, junto con un plano de la 3; el resto el segundo día.
        let day1_at = p.place_shots(ids[0], &[a, b], Some(1));
        assert_eq!(day1_at, 1);
        p.place_shots(ids[2], &[d], Some(2));
        let second_at = p
            .schedule
            .iter()
            .position(|e| *e == StripEntry::DayBreak(second))
            .unwrap();
        p.place_shots(ids[0], &[c], Some(second_at + 1));
        assert_eq!(order(&p), ["D1", "1[2]", "3[1]", "D2", "1[1]"]);
        // La escena 1 ya está repartida entera; la 3 tiene un plano solo y como es el único, también.
        assert!(!p.unscheduled_scenes().contains(&ids[0]));
        assert!(!p.unscheduled_scenes().contains(&ids[2]));
        assert!(p.unscheduled_scenes().contains(&ids[1]));
        // La escena figura en los dos días.
        let days = p.days_view();
        let stats: Vec<_> = days.iter().map(|d| p.day_stats(d)).collect();
        assert_eq!(stats[0].scenes, 2);
        assert_eq!(stats[1].scenes, 1);
        // Los planos sin duración cuentan 5 s de referencia y se marcan.
        assert!((stats[0].screen_seconds - 15.0).abs() < 0.01);
        assert!(stats[0].screen_estimated);
        p.shots.iter_mut().find(|s| s.id == a).unwrap().duration = "10 s".into();
        let (seconds, guessed) = p.shots_seconds(&[a, b]);
        assert_eq!((seconds, guessed), (15.0, true));
        // Sacar un plano del tramo lo devuelve a la bandeja; el orden de rodaje lo decide el tramo.
        assert!(p.nudge_part_shot(1, a, 1));
        assert_eq!(p.schedule[1], StripEntry::Part(ids[0], vec![b, a]));
        p.unschedule_shots(&[b]);
        assert_eq!(p.schedule[1], StripEntry::Part(ids[0], vec![a]));
        assert_eq!(p.unscheduled_shots(ids[0]), vec![b]);
        assert!(p.unscheduled_scenes().contains(&ids[0]));
        // Una escena sin planos se programa entera como siempre, y partir una entera la convierte en un tramo.
        p.place_scenes(&[ids[1]], None);
        assert!(p.unscheduled_shots(ids[1]).is_empty());
        p.place_scenes(&[ids[3]], None);
        let shot = p.add_shot(ids[3], None);
        assert!(p.split_scene_into_part(ids[3]));
        assert!(p.schedule.contains(&StripEntry::Part(ids[3], vec![shot])));
        // Poner la escena entera de nuevo saca sus tramos.
        p.place_scenes(&[ids[0]], None);
        assert!(
            !p.schedule
                .iter()
                .any(|e| matches!(e, StripEntry::Part(id, _) if *id == ids[0]))
        );
        p.prune_schedule();
    }
}
