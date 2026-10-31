//! Hoja de llamado: lo que el equipo y el elenco tienen que saber de un día de rodaje. Casi todo sale del plan (escenas,
//! lugar, elenco, contactos, desglose) y lo manual (citaciones, almuerzo, pronóstico, notas) se guarda en el día.

use std::collections::BTreeMap;

use crate::{
    model::{CategorySource, Project, clean_character_name, fmt_eighths},
    schedule::DayView,
};

/// Una hora del día en minutos, escrita como «07:30».
pub fn fmt_clock(minutes: f64) -> String {
    let total = minutes.round() as i64;
    format!(
        "{:02}:{:02}",
        total.div_euclid(60).rem_euclid(24),
        total.rem_euclid(60)
    )
}

fn days_before_month(year: i32, month: u32) -> u32 {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][(month as usize - 1).min(11)]
        + u32::from(leap && month > 2)
}

/// La fecha escrita como AAAA-MM-DD: (año, mes, día).
pub fn parse_date(text: &str) -> Option<(i32, u32, u32)> {
    let mut parts = text.trim().split('-');
    let (year, month, day) = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    ((1..=12).contains(&month) && (1..=31).contains(&day) && parts.next().is_none())
        .then_some((year, month, day))
}

/// Salida y puesta del sol, en minutos desde la medianoche de la hora local, para una fecha y un lugar (latitud y longitud
/// en grados, el oeste y el sur negativos). Calculado sin internet con las fórmulas de la NOAA; da `None` donde el sol no
/// sale o no se pone ese día.
pub fn sun_times(
    date: &str,
    latitude: f64,
    longitude: f64,
    utc_offset_minutes: i32,
) -> Option<(f64, f64)> {
    let (year, month, day) = parse_date(date)?;
    let n = f64::from(days_before_month(year, month) + day);
    let gamma = 2.0 * std::f64::consts::PI / 365.0 * (n - 1.0);
    let equation = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let declination = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    let lat = latitude.to_radians();
    let cos_hour = (90.833f64.to_radians().cos() / (lat.cos() * declination.cos()))
        - lat.tan() * declination.tan();
    if !(-1.0..=1.0).contains(&cos_hour) {
        return None;
    }
    let hour_angle = cos_hour.acos().to_degrees();
    let local = f64::from(utc_offset_minutes);
    Some((
        720.0 - 4.0 * (longitude + hour_angle) - equation + local,
        720.0 - 4.0 * (longitude - hour_angle) - equation + local,
    ))
}

#[derive(Clone, Debug, Default)]
pub struct PlaceInfo {
    pub name: String,
    pub address: String,
    pub contact: Option<(String, String)>,
    pub parking: String,
    pub hospital: String,
    pub access_hours: String,
}

#[derive(Clone, Debug, Default)]
pub struct SceneLine {
    pub number: String,
    pub heading: String,
    pub summary: String,
    pub eighths: String,
    pub cast: String,
    /// Los personajes que están en la escena.
    pub cast_ids: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct CastLine {
    pub character: u64,
    pub number: Option<u32>,
    pub name: String,
    pub actor: String,
    pub phone: String,
    pub call: String,
    pub makeup: String,
    pub set: String,
}

#[derive(Clone, Debug, Default)]
pub struct CrewArea {
    pub department: String,
    pub call: String,
    pub people: Vec<(String, String, String)>,
}

/// La hoja de llamado de un día, con todo lo que lleva ya resuelto.
#[derive(Clone, Debug, Default)]
pub struct CallSheet {
    pub project: String,
    pub day_number: usize,
    pub days_total: usize,
    pub date: String,
    pub call: String,
    pub lunch: String,
    pub wrap: String,
    pub forecast: String,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub places: Vec<PlaceInfo>,
    pub scenes: Vec<SceneLine>,
    pub cast: Vec<CastLine>,
    pub crew: Vec<CrewArea>,
    /// Lo que piden las escenas del día, por categoría.
    pub requirements: Vec<(String, Vec<String>)>,
    pub area_notes: Vec<(String, String)>,
    pub notes: String,
    /// La jornada siguiente: su número, su fecha y sus escenas.
    pub next: Option<(usize, String, Vec<SceneLine>)>,
}

fn scene_line(project: &Project, id: u64, labels: &BTreeMap<u64, String>) -> SceneLine {
    let heading = project
        .script_scenes()
        .into_iter()
        .find(|(s, _)| *s == id)
        .map(|(_, h)| h)
        .unwrap_or_default();
    let cast: Vec<String> = project
        .scene_cast_ids(id)
        .iter()
        .filter_map(|c| project.character(*c))
        .map(|c| {
            c.cast_number
                .map_or_else(|| clean_character_name(&c.name), |n| n.to_string())
        })
        .collect();
    SceneLine {
        number: labels.get(&id).cloned().unwrap_or_default(),
        heading: heading.trim().to_string(),
        summary: project
            .scenes
            .get(&id)
            .map(|m| m.summary.clone())
            .unwrap_or_default(),
        eighths: fmt_eighths(project.scene_eighths(id)),
        cast: cast.join(", "),
        cast_ids: project.scene_cast_ids(id),
    }
}

fn day_scene_ids(project: &Project, day: &DayView) -> Vec<u64> {
    // Una escena con varios tramos en el día figura una sola vez.
    let mut out: Vec<u64> = Vec::new();
    for id in day
        .entries
        .iter()
        .filter_map(|&i| project.schedule[i].scene())
    {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Arma la hoja de llamado de un día del plan.
pub fn build(project: &Project, day_id: u64) -> Option<CallSheet> {
    let days = project.days_view();
    let day = days.iter().find(|d| d.id == day_id)?;
    let shoot = project.shoot_days.iter().find(|d| d.id == day_id)?;
    let labels: BTreeMap<u64, String> = project.scene_labels().into_iter().collect();
    let scenes_ids = day_scene_ids(project, day);

    // Lugares del día, en el orden en que se filma.
    let mut place_ids: Vec<u64> = Vec::new();
    for id in &scenes_ids {
        if let Some(place) = project.scene_shoot_location(*id) {
            if !place_ids.contains(&place) {
                place_ids.push(place);
            }
        }
    }
    let places: Vec<PlaceInfo> = place_ids
        .iter()
        .filter_map(|id| project.shoot_locations.iter().find(|l| l.id == *id))
        .map(|l| PlaceInfo {
            name: l.name.clone(),
            address: l.address.clone(),
            contact: l
                .contact
                .and_then(|c| project.contacts.iter().find(|x| x.id == c))
                .map(|c| (c.name.clone(), c.phone.clone())),
            parking: l.parking.clone(),
            hospital: l.hospital.clone(),
            access_hours: l.access_hours.clone(),
        })
        .collect();

    // El sol, del primer lugar con coordenadas.
    let sun = project
        .shoot_locations
        .iter()
        .filter(|l| place_ids.contains(&l.id))
        .find_map(|l| l.latitude.zip(l.longitude))
        .and_then(|(lat, lon)| sun_times(&shoot.date, lat, lon, project.utc_offset_minutes));

    // Elenco: los personajes de las escenas del día, con sus citaciones.
    let mut character_ids: Vec<u64> = Vec::new();
    for id in &scenes_ids {
        for c in project.scene_cast_ids(*id) {
            if !character_ids.contains(&c) {
                character_ids.push(c);
            }
        }
    }
    let mut cast: Vec<CastLine> = character_ids
        .iter()
        .filter_map(|id| project.character(*id))
        .map(|c| {
            let actor = c
                .actor
                .and_then(|a| project.contacts.iter().find(|x| x.id == a));
            let call = shoot.cast_calls.get(&c.id).cloned().unwrap_or_default();
            CastLine {
                character: c.id,
                number: c.cast_number,
                name: clean_character_name(&c.name),
                actor: actor.map(|a| a.name.clone()).unwrap_or_default(),
                phone: actor.map(|a| a.phone.clone()).unwrap_or_default(),
                call: call.call,
                makeup: call.makeup,
                set: call.set,
            }
        })
        .collect();
    cast.sort_by(|a, b| {
        (a.number.is_none(), a.number, &a.name).cmp(&(b.number.is_none(), b.number, &b.name))
    });

    // Equipo técnico: los contactos que no son el elenco de hoy, por departamento.
    let acting: Vec<u64> = project.characters.iter().filter_map(|c| c.actor).collect();
    let mut crew: Vec<CrewArea> = Vec::new();
    for contact in project.contacts.iter().filter(|c| !acting.contains(&c.id)) {
        let department = if contact.department.trim().is_empty() {
            "Sin área".to_string()
        } else {
            contact.department.trim().to_string()
        };
        let at = crew
            .iter()
            .position(|a| a.department == department)
            .unwrap_or_else(|| {
                crew.push(CrewArea {
                    call: shoot
                        .area_calls
                        .get(&department)
                        .cloned()
                        .unwrap_or_default(),
                    department,
                    people: Vec::new(),
                });
                crew.len() - 1
            });
        crew[at].people.push((
            contact.name.clone(),
            contact.role.clone(),
            contact.phone.clone(),
        ));
    }
    crew.sort_by(|a, b| a.department.cmp(&b.department));

    // Requerimientos: lo que piden las escenas, por categoría (el elenco y las locaciones ya están arriba).
    let mut requirements: Vec<(String, Vec<String>)> = Vec::new();
    for cat in project.breakdown_categories.iter().filter(|c| {
        !matches!(
            c.source,
            CategorySource::Characters | CategorySource::Locations
        )
    }) {
        let mut names: Vec<String> = Vec::new();
        for id in &scenes_ids {
            for item in project
                .scene_sheet_items(*id)
                .into_iter()
                .filter(|i| i.category == cat.id)
            {
                let quantity = item
                    .scene_quantity
                    .or((item.quantity > 0).then_some(item.quantity));
                let text = quantity.map_or(item.name.clone(), |q| format!("{} (x{q})", item.name));
                if !names.contains(&text) {
                    names.push(text);
                }
            }
        }
        if !names.is_empty() {
            requirements.push((cat.name.clone(), names));
        }
    }

    // La jornada siguiente, para que el equipo se prepare.
    let next = days.iter().find(|d| d.number == day.number + 1).map(|d| {
        let date = project
            .shoot_days
            .iter()
            .find(|s| s.id == d.id)
            .map(|s| s.date.clone())
            .unwrap_or_default();
        (
            d.number,
            date,
            day_scene_ids(project, d)
                .iter()
                .map(|id| scene_line(project, *id, &labels))
                .collect(),
        )
    });

    Some(CallSheet {
        project: project.title.clone(),
        day_number: day.number,
        days_total: days.len(),
        date: shoot.date.clone(),
        call: shoot.call_time.clone(),
        lunch: shoot.lunch.clone(),
        wrap: shoot.wrap_time.clone(),
        forecast: shoot.forecast.clone(),
        sunrise: sun.map(|(r, _)| fmt_clock(r)),
        sunset: sun.map(|(_, s)| fmt_clock(s)),
        places,
        scenes: scenes_ids
            .iter()
            .map(|id| scene_line(project, *id, &labels))
            .collect(),
        cast,
        crew,
        requirements,
        area_notes: shoot
            .area_notes
            .iter()
            .filter(|(_, t)| !t.trim().is_empty())
            .map(|(d, t)| (d.clone(), t.clone()))
            .collect(),
        notes: shoot.notes.clone(),
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::ParagraphKind::*, schedule::CastCall};

    #[test]
    fn the_sun_rises_and_sets_where_it_should_in_buenos_aires() {
        let (lat, lon) = (-34.6037, -58.3816);
        // Solsticio de verano: días largos, sale a las 5:36 y se pone a las 20:04 (hora de Argentina, UTC-3).
        let (rise, set) = sun_times("2026-12-21", lat, lon, -180).unwrap();
        assert!(
            (rise - (5.0 * 60.0 + 36.0)).abs() < 5.0 && (set - (20.0 * 60.0 + 4.0)).abs() < 5.0,
            "{} {}",
            fmt_clock(rise),
            fmt_clock(set)
        );
        // Solsticio de invierno: sale cerca de las 8:00 y se pone cerca de las 17:48.
        let (rise, set) = sun_times("2026-06-21", lat, lon, -180).unwrap();
        assert!(
            (rise - (8.0 * 60.0)).abs() < 5.0 && (set - (17.0 * 60.0 + 48.0)).abs() < 6.0,
            "{} {}",
            fmt_clock(rise),
            fmt_clock(set)
        );
        // Más al sur, el verano es más largo; en el polo en verano el sol no se pone.
        let (_, tierra_del_fuego) = sun_times("2026-12-21", -54.8, -68.3, -180).unwrap();
        assert!(tierra_del_fuego > set + 60.0);
        assert!(sun_times("2026-12-21", -89.0, 0.0, 0).is_none());
        assert!(sun_times("no es una fecha", lat, lon, -180).is_none());
        assert_eq!(fmt_clock(405.0), "06:45");
        assert_eq!(fmt_clock(-30.0), "23:30");
        assert_eq!(parse_date("2026-02-29"), Some((2026, 2, 29)));
    }

    fn project() -> (Project, Vec<u64>, u64) {
        let mut p = Project::default();
        p.title = "El nido".into();
        p.paragraphs = [
            (SceneHeading, "INT. LIVING - DÍA"),
            (Action, "Ana toma la linterna."),
            (Character, "ANA"),
            (Dialogue, "Hola."),
            (SceneHeading, "EXT. CALLE - DÍA"),
            (Character, "JUAN"),
            (Dialogue, "Chau."),
            (SceneHeading, "INT. COCINA - DÍA"),
            (Action, "Hierve agua."),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        p.ensure_breakdown_defaults();
        p.auto_link_all();
        let ids: Vec<u64> = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        let day = p.add_shoot_day();
        p.place_scenes(&[ids[0], ids[1]], None);
        p.add_shoot_day();
        p.place_scenes(&[ids[2]], None);
        (p, ids, day)
    }

    #[test]
    fn the_call_sheet_collects_the_plan_the_places_the_cast_the_crew_and_what_the_scenes_need() {
        let (mut p, ids, day) = project();
        // Lugar con contacto y coordenadas.
        let pedro = p.add_contact("Pedro");
        p.contacts[0].phone = "11 4444".into();
        let house = p.add_shoot_location("Casa de Pedro");
        {
            let place = &mut p.shoot_locations[0];
            place.address = "Calle 1".into();
            place.contact = Some(pedro);
            place.hospital = "Hospital Italiano".into();
            place.latitude = Some(-34.6);
            place.longitude = Some(-58.4);
        }
        p.scenes.entry(ids[0]).or_default().shoot_location = Some(house);
        // Elenco con su intérprete y su citación.
        let luisa = p.add_contact("Luisa");
        p.contacts[1].phone = "11 5555".into();
        let ana = p.character_id_by_name("ANA").unwrap();
        p.characters.iter_mut().find(|c| c.id == ana).unwrap().actor = Some(luisa);
        p.characters
            .iter_mut()
            .find(|c| c.id == ana)
            .unwrap()
            .cast_number = Some(1);
        // Equipo.
        let sound = p.add_contact("Marta");
        p.contacts
            .iter_mut()
            .find(|c| c.id == sound)
            .unwrap()
            .department = "Sonido".into();
        p.contacts.iter_mut().find(|c| c.id == sound).unwrap().role = "Sonidista".into();
        // Lo que piden las escenas.
        let utileria = p
            .breakdown_categories
            .iter()
            .find(|c| c.name == "Utilería")
            .unwrap()
            .id;
        let para = p.paragraphs[1].id;
        p.add_breakdown_tag(para, "linterna", utileria, "Linterna", false);
        // Datos manuales del día.
        let shoot = p.shoot_days.iter_mut().find(|d| d.id == day).unwrap();
        shoot.date = "2026-12-21".into();
        shoot.call_time = "07:00".into();
        shoot.lunch = "13:00".into();
        shoot.cast_calls.insert(
            ana,
            CastCall {
                call: "08:00".into(),
                makeup: "07:30".into(),
                set: "09:00".into(),
            },
        );
        shoot.area_calls.insert("Sonido".into(), "06:30".into());
        shoot
            .area_notes
            .insert("Sonido".into(), "Llevar el boom".into());

        let sheet = build(&p, day).unwrap();
        assert_eq!(
            (sheet.day_number, sheet.days_total, sheet.date.as_str()),
            (1, 2, "2026-12-21")
        );
        assert_eq!(sheet.places.len(), 1);
        assert_eq!(
            sheet.places[0].contact,
            Some(("Pedro".to_string(), "11 4444".to_string()))
        );
        assert!(sheet.sunrise.is_some() && sheet.sunset.is_some());
        assert_eq!(
            sheet
                .scenes
                .iter()
                .map(|s| s.number.as_str())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert_eq!(sheet.cast.len(), 2);
        let first = &sheet.cast[0];
        assert_eq!(
            (
                first.name.as_str(),
                first.actor.as_str(),
                first.phone.as_str(),
                first.call.as_str(),
                first.makeup.as_str(),
                first.set.as_str()
            ),
            ("ANA", "Luisa", "11 5555", "08:00", "07:30", "09:00")
        );
        // Luisa actúa: no figura en el equipo; Pedro y Marta sí, cada uno en su área.
        let areas: Vec<(&str, &str, usize)> = sheet
            .crew
            .iter()
            .map(|a| (a.department.as_str(), a.call.as_str(), a.people.len()))
            .collect();
        assert_eq!(areas, [("Sin área", "", 1), ("Sonido", "06:30", 1)]);
        assert_eq!(
            sheet.requirements,
            vec![("Utilería".to_string(), vec!["Linterna".to_string()])]
        );
        assert_eq!(
            sheet.area_notes,
            vec![("Sonido".to_string(), "Llevar el boom".to_string())]
        );
        // La jornada siguiente.
        let next = sheet.next.unwrap();
        assert_eq!((next.0, next.2.len()), (2, 1));
        assert!(build(&p, 999_999).is_none());
    }

    #[test]
    fn a_day_without_coordinates_has_no_sun_times() {
        let (p, _, day) = project();
        let sheet = build(&p, day).unwrap();
        assert!(sheet.sunrise.is_none() && sheet.places.is_empty());
    }
}
