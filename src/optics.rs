//! Cámara y lentes: el cuerpo de cámara (su sensor), el ángulo que da una lente, la lente equivalente en full frame, el
//! código de cada plano, cuánto lleva filmarlo y en qué orden conviene hacerlo.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    model::{BlockItem, BlockKind, Project},
    schedule::StripEntry,
};

/// El ancho de un sensor full frame (35 mm), contra el que se mide todo lo demás.
pub const FULL_FRAME_MM: f32 = 36.0;

/// Cámaras de uso común y su factor de recorte.
pub const BODY_PRESETS: [(&str, f32); 4] = [
    ("Full frame", 1.0),
    ("APS-C Canon", 1.6),
    ("APS-C Sony, Nikon o Fujifilm", 1.5),
    ("Micro Cuatro Tercios", 2.0),
];

/// Proporción del cuadro de la película.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aspect {
    #[default]
    Wide,
    Scope,
    Classic,
}

impl Aspect {
    pub const ALL: [Aspect; 3] = [Aspect::Wide, Aspect::Scope, Aspect::Classic];

    pub fn label(self) -> &'static str {
        match self {
            Aspect::Wide => "16:9",
            Aspect::Scope => "2,39:1",
            Aspect::Classic => "4:3",
        }
    }

    /// Ancho sobre alto.
    pub fn ratio(self) -> f32 {
        match self {
            Aspect::Wide => 16.0 / 9.0,
            Aspect::Scope => 2.39,
            Aspect::Classic => 4.0 / 3.0,
        }
    }
}

/// Un cuerpo de cámara: lo que importa de él es el ancho de su sensor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraBody {
    pub id: u64,
    pub name: String,
    /// Factor de recorte respecto de full frame (1,0; 1,5; 1,6; 2,0).
    pub crop_factor: f32,
    /// Ancho del sensor en mm, si se lo carga: manda sobre el factor de recorte.
    pub sensor_w_mm: Option<f32>,
    /// Recorte extra del modo de video (4K, estabilización digital): 1,0 si no recorta.
    pub video_crop: f32,
}

impl Default for CameraBody {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            crop_factor: 1.0,
            sensor_w_mm: None,
            video_crop: 1.0,
        }
    }
}

impl CameraBody {
    /// El ancho del sensor que realmente se usa, en mm.
    pub fn sensor_width(&self) -> f32 {
        let full = self
            .sensor_w_mm
            .filter(|w| *w > 0.0)
            .unwrap_or(FULL_FRAME_MM / self.crop_factor.max(0.1));
        full / self.video_crop.max(1.0)
    }
}

/// El ancho de sensor de una cámara sin cuerpo cargado: full frame.
fn sensor_width_of(body: Option<&CameraBody>) -> f32 {
    body.map_or(FULL_FRAME_MM, CameraBody::sensor_width)
}

/// Ángulo horizontal en grados de una lente de `lens_mm` en un sensor de `sensor_mm` de ancho.
pub fn horizontal_angle(sensor_mm: f32, lens_mm: f32) -> f32 {
    2.0 * (sensor_mm / (2.0 * lens_mm.max(1.0))).atan().to_degrees()
}

/// La lente que, en full frame, ve lo mismo que `lens_mm` en un sensor de `sensor_mm`.
pub fn equivalent_lens(sensor_mm: f32, lens_mm: f32) -> f32 {
    lens_mm * FULL_FRAME_MM / sensor_mm
}

/// «50 mm» o «50,5 mm».
pub fn fmt_mm(mm: f32) -> String {
    if (mm - mm.round()).abs() < 0.05 {
        format!("{} mm", mm.round() as i32)
    } else {
        format!("{} mm", format!("{mm:.1}").replace('.', ","))
    }
}

impl Project {
    pub fn camera_body(&self, id: u64) -> Option<&CameraBody> {
        self.camera_bodies.iter().find(|b| b.id == id)
    }

    /// Agrega un cuerpo de cámara al proyecto y devuelve su id.
    pub fn add_camera_body(&mut self, name: &str, crop_factor: f32) -> u64 {
        let id = self.alloc_id();
        self.camera_bodies.push(CameraBody {
            id,
            name: name.to_string(),
            crop_factor,
            ..Default::default()
        });
        id
    }

    /// Quita un cuerpo de cámara; las cámaras que lo usaban vuelven a full frame.
    pub fn remove_camera_body(&mut self, id: u64) {
        self.camera_bodies.retain(|b| b.id != id);
        for blocking in &mut self.blockings {
            for item in blocking.items.iter_mut().filter(|i| i.body == Some(id)) {
                item.body = None;
            }
        }
    }

    /// El ángulo de visión horizontal de una cámara del bloqueo: con lente cargado sale del sensor; sin él, el ángulo
    /// guardado.
    pub fn camera_fov(&self, item: &BlockItem) -> f32 {
        if item.kind == BlockKind::Camera && item.lens_mm > 0.0 {
            horizontal_angle(
                sensor_width_of(item.body.and_then(|b| self.camera_body(b))),
                item.lens_mm,
            )
        } else {
            item.fov
        }
    }

    /// La lente de un plano: la suya o, si no tiene, la de su cámara; con la lente equivalente en full frame.
    pub fn shot_lens(&self, shot: u64) -> Option<(f32, f32)> {
        let shot = self.shots.iter().find(|s| s.id == shot)?;
        let camera = shot.blocking.zip(shot.camera).and_then(|(b, c)| {
            self.blockings
                .iter()
                .find(|x| x.id == b)?
                .items
                .iter()
                .find(|i| i.id == c)
        });
        let lens = shot
            .lens_mm
            .or_else(|| camera.map(|c| c.lens_mm).filter(|l| *l > 0.0))?;
        let sensor = sensor_width_of(
            camera
                .and_then(|c| c.body)
                .and_then(|b| self.camera_body(b)),
        );
        Some((lens, equivalent_lens(sensor, lens)))
    }

    /// El código de cada plano es su número: 1, 2, 3… de corrido en todo el guion, o desde 1 en cada escena si se marcó
    /// «Reiniciar en cada escena». Nunca lleva letras.
    pub fn shot_codes(&self) -> HashMap<u64, String> {
        self.shot_numbers()
            .into_iter()
            .map(|(id, n)| (id, n.to_string()))
            .collect()
    }

    #[cfg(test)]
    pub fn shot_code(&self, shot: u64) -> String {
        self.shot_codes().remove(&shot).unwrap_or_default()
    }

    /// Cuánto lleva cada plano: los minutos de un plano más, en el primero de cada emplazamiento, los de armarlo.
    pub fn shot_minutes(&self) -> HashMap<u64, u32> {
        let mut seen: Vec<(u64, u64, u64)> = Vec::new();
        let mut out = HashMap::new();
        for shot in &self.shots {
            let mut minutes = self.minutes_per_shot;
            if let Some((blocking, camera)) = shot.blocking.zip(shot.camera) {
                let key = (shot.scene, blocking, camera);
                if !seen.contains(&key) {
                    seen.push(key);
                    minutes += self.minutes_per_setup;
                }
            }
            out.insert(shot.id, minutes);
        }
        out
    }

    /// Lo que se calcula para filmar una escena a partir de sus planos, si los tiene.
    pub fn scene_shots_minutes(&self, scene: u64) -> Option<u32> {
        let minutes = self.shot_minutes();
        let mine: Vec<u32> = self
            .shots
            .iter()
            .filter(|s| s.scene == scene)
            .filter_map(|s| minutes.get(&s.id).copied())
            .collect();
        (!mine.is_empty()).then(|| mine.iter().sum())
    }

    /// Numera los planos en el orden en que conviene filmarlos: las escenas como están en el plan de rodaje (las que
    /// no están, al final en el orden del guion) y, dentro de cada una, juntos los de una misma cámara.
    pub fn assign_shoot_order(&mut self) {
        let mut scenes: Vec<u64> = self.schedule.iter().filter_map(StripEntry::scene).collect();
        let mut seen = std::collections::HashSet::new();
        scenes.retain(|id| seen.insert(*id));
        for (id, _) in self.script_scenes() {
            if !scenes.contains(&id) {
                scenes.push(id);
            }
        }
        for shot in &mut self.shots {
            shot.shoot_order = None;
        }
        let mut next = 1;
        for scene in scenes {
            // Los emplazamientos de la escena, en el orden en que aparecen sus planos.
            let mut keys: Vec<Option<(u64, u64)>> = Vec::new();
            for shot in self.shots.iter().filter(|s| s.scene == scene) {
                let key = shot.blocking.zip(shot.camera);
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
            for key in keys {
                for shot in self
                    .shots
                    .iter_mut()
                    .filter(|s| s.scene == scene && s.blocking.zip(s.camera) == key)
                {
                    shot.shoot_order = Some(next);
                    next += 1;
                }
            }
        }
    }

    /// Los planos en orden de rodaje (los que no se ordenaron, al final).
    pub fn shots_in_shoot_order(&self) -> Vec<u64> {
        let mut shots: Vec<(Option<u32>, usize, u64)> = self
            .shots
            .iter()
            .enumerate()
            .map(|(i, s)| (s.shoot_order, i, s.id))
            .collect();
        shots.sort_by_key(|(order, i, _)| (order.is_none(), *order, *i));
        shots.into_iter().map(|(_, _, id)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParagraphKind::*;

    #[test]
    fn the_lens_angle_and_its_full_frame_equivalent_follow_the_sensor() {
        // Un 50 mm en full frame abre unos 39,6°; en un APS-C Canon (22,5 mm de ancho) abre unos 25° y equivale a un 80 mm.
        assert!((horizontal_angle(36.0, 50.0) - 39.6).abs() < 0.1);
        let canon = CameraBody {
            crop_factor: 1.6,
            ..Default::default()
        };
        assert!((canon.sensor_width() - 22.5).abs() < 1e-4);
        assert!((horizontal_angle(canon.sensor_width(), 50.0) - 25.4).abs() < 0.2);
        assert!((equivalent_lens(canon.sensor_width(), 50.0) - 80.0).abs() < 1e-3);
        // El ancho de sensor cargado manda sobre el factor; el recorte del modo de video lo achica.
        let manual = CameraBody {
            crop_factor: 1.6,
            sensor_w_mm: Some(24.0),
            ..Default::default()
        };
        assert_eq!(manual.sensor_width(), 24.0);
        let cropped = CameraBody {
            video_crop: 1.2,
            ..Default::default()
        };
        assert!((cropped.sensor_width() - 30.0).abs() < 1e-4);
        assert_eq!(
            (fmt_mm(50.0), fmt_mm(80.0001), fmt_mm(17.5)),
            (
                "50 mm".to_string(),
                "80 mm".to_string(),
                "17,5 mm".to_string()
            )
        );
        for (name, crop) in BODY_PRESETS {
            assert!(!name.is_empty() && crop >= 1.0);
        }
    }

    fn project() -> (Project, Vec<u64>) {
        let mut p = Project::default();
        p.paragraphs = [
            (SceneHeading, "INT. A - DÍA"),
            (Action, "uno"),
            (SceneHeading, "EXT. B - DÍA"),
            (Action, "dos"),
        ]
        .iter()
        .map(|(k, t)| p.make_paragraph(*k, t))
        .collect();
        let ids = p.script_scenes().into_iter().map(|(id, _)| id).collect();
        (p, ids)
    }

    #[test]
    fn a_camera_takes_its_angle_from_its_lens_and_body_and_a_shot_shows_its_equivalent() {
        let (mut p, ids) = project();
        let blocking = p.add_blocking(ids[0]);
        let camera = p
            .add_block_item(blocking, BlockKind::Camera, 0.0, 0.0)
            .unwrap();
        let item = |p: &Project| {
            p.blockings[0]
                .items
                .iter()
                .find(|i| i.id == camera)
                .unwrap()
                .clone()
        };
        assert_eq!(
            item(&p).lens_mm,
            35.0,
            "las cámaras nuevas arrancan con una lente de 35 mm"
        );
        assert!((p.camera_fov(&item(&p)) - horizontal_angle(36.0, 35.0)).abs() < 1e-4);
        let canon = p.add_camera_body("Canon", 1.6);
        p.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .body = Some(canon);
        p.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .lens_mm = 50.0;
        assert!((p.camera_fov(&item(&p)) - 25.4).abs() < 0.2);
        // Un plano toma la lente de su cámara; con una propia, usa esa.
        let shot = p.add_shot(ids[0], None);
        assert_eq!(
            p.shot_lens(shot),
            None,
            "sin cámara ni lente propia no hay dato"
        );
        let s = p.shots.iter_mut().find(|s| s.id == shot).unwrap();
        (s.blocking, s.camera) = (Some(blocking), Some(camera));
        let (lens, eq) = p.shot_lens(shot).unwrap();
        assert_eq!((lens, eq.round()), (50.0, 80.0));
        p.shots.iter_mut().find(|s| s.id == shot).unwrap().lens_mm = Some(35.0);
        assert_eq!(p.shot_lens(shot).unwrap().0, 35.0);
        // Una cámara vieja, sin lente, sigue con su ángulo.
        p.blockings[0]
            .items
            .iter_mut()
            .find(|i| i.id == camera)
            .unwrap()
            .lens_mm = 0.0;
        assert_eq!(p.camera_fov(&item(&p)), 40.0);
        // Quitar el cuerpo devuelve las cámaras a full frame.
        p.remove_camera_body(canon);
        assert_eq!(item(&p).body, None);
    }

    #[test]
    fn shots_get_a_code_from_their_scene_and_a_time_estimate_and_an_order_by_setup() {
        let (mut p, ids) = project();
        let a = p.add_shot(ids[0], None);
        let b = p.add_shot(ids[0], None);
        let c = p.add_shot(ids[0], None);
        let d = p.add_shot(ids[1], None);
        assert_eq!(
            [
                p.shot_code(a),
                p.shot_code(b),
                p.shot_code(c),
                p.shot_code(d)
            ],
            ["1", "2", "3", "4"]
        );
        // El código del plano es solo su número, de corrido: una escena 1A no le pone letras.
        p.lock_scene_numbers();
        let extra = p.make_paragraph(SceneHeading, "INT. N - DÍA");
        p.paragraphs.insert(1, extra);
        p.sync_scene_numbers();
        assert_eq!(p.scene_labels()[1].1, "1A");
        let e = p.add_shot(p.script_scenes()[1].0, None);
        assert_eq!(p.shot_code(e), "4");

        // Dos cámaras: A (planos a y c) y B (plano b). Un emplazamiento por cámara.
        let blocking = p.add_blocking(ids[0]);
        let cam_a = p
            .add_block_item(blocking, BlockKind::Camera, 0.0, 0.0)
            .unwrap();
        let cam_b = p
            .add_block_item(blocking, BlockKind::Camera, 2.0, 0.0)
            .unwrap();
        for (shot, cam) in [(a, cam_a), (b, cam_b), (c, cam_a)] {
            let s = p.shots.iter_mut().find(|s| s.id == shot).unwrap();
            (s.blocking, s.camera) = (Some(blocking), Some(cam));
        }
        let minutes = p.shot_minutes();
        // a: plano + armar la cámara A; b: plano + armar la B; c: solo el plano (misma cámara que a).
        assert_eq!(
            (minutes[&a], minutes[&b], minutes[&c], minutes[&d]),
            (25, 25, 5, 5)
        );
        assert_eq!(p.scene_shots_minutes(ids[0]), Some(55));
        assert_eq!(
            p.scene_shots_minutes(999_999),
            None,
            "una escena sin planos no se estima"
        );

        // Ordenar por emplazamiento junta a y c, y después b; las demás escenas siguen.
        p.assign_shoot_order();
        let order = |shot: u64| p.shots.iter().find(|s| s.id == shot).unwrap().shoot_order;
        assert_eq!((order(a), order(c), order(b)), (Some(1), Some(2), Some(3)));
        assert!(order(d).unwrap() > 3);
        assert_eq!(p.shots_in_shoot_order()[..3], [a, c, b]);
    }

    #[test]
    fn the_aspect_ratios_are_the_usual_ones() {
        assert_eq!(Aspect::ALL.map(Aspect::label), ["16:9", "2,39:1", "4:3"]);
        assert!(
            (Aspect::Wide.ratio() - 1.7778).abs() < 1e-3
                && Aspect::Scope.ratio() > Aspect::Wide.ratio()
        );
    }

    #[test]
    fn the_time_to_shoot_a_scene_comes_from_its_shots_when_it_has_no_time_of_its_own() {
        let (mut p, ids) = project();
        let eighths = u32::from(p.scene_eighths(ids[0]));
        assert_eq!(
            p.scene_minutes(ids[0]),
            (eighths * p.minutes_per_eighth, true)
        );
        let blocking = p.add_blocking(ids[0]);
        let camera = p
            .add_block_item(blocking, BlockKind::Camera, 0.0, 0.0)
            .unwrap();
        for _ in 0..2 {
            let shot = p.add_shot(ids[0], None);
            let s = p.shots.iter_mut().find(|s| s.id == shot).unwrap();
            (s.blocking, s.camera) = (Some(blocking), Some(camera));
        }
        // Un emplazamiento (20) y dos planos (5 + 5).
        assert_eq!(p.scene_minutes(ids[0]), (30, true));
        p.minutes_per_setup = 30;
        assert_eq!(p.scene_minutes(ids[0]).0, 40);
        // Lo que se carga a mano manda.
        p.scenes.entry(ids[0]).or_default().est_minutes = Some(90);
        assert_eq!(p.scene_minutes(ids[0]), (90, false));
    }
}
