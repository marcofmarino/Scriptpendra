//! Las pantallas del proyecto siguiendo el contrato `Screen` (reglas-interfaz.md §1.2). Cada una toma prestado su estado
//! de `ScriptApp` y le entrega al marco lo que va en cada zona; el marco (`ui::shell`) las dibuja todas igual.

use eframe::egui::Ui;

use crate::{
    calls, contacts, editor,
    model::EntityRef,
    pages, plan, scenes, shoot, synopsis, techscript, treatment,
    ui::{
        kit,
        screen::{Confirm, Effect, Nav, Screen, ScreenCx, ScreenKind, linked_entities},
    },
};

/// Lleva el pedido de la pantalla Escenas (tabla o inspector) a la navegación del marco.
fn scene_nav(out: &scenes::Out, cx: &mut ScreenCx) {
    if let Some(id) = out.goto {
        cx.navigate = Some(Nav::Script(id));
    }
    if let Some((link, scene)) = out.link {
        cx.navigate = Some(match link {
            scenes::SceneLink::Blocking => Nav::Plan(scene),
            scenes::SceneLink::Breakdown => Nav::Breakdown(scene),
            scenes::SceneLink::Shots => Nav::Shots(scene),
        });
    }
    if out.status.is_some() {
        cx.status = out.status.clone();
    }
}

// ------------------------------------------------------------------------------------------------
// ESCRITURA
// ------------------------------------------------------------------------------------------------

/// Información del proyecto · tipo E.
pub struct InfoScreen;

impl Screen for InfoScreen {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Información del proyecto".into()
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        kit::export_menu(ui, |ui| {
            if ui.button("Exportar…").clicked() {
                cx.effects.push(Effect::OpenExport);
                ui.close();
            }
        });
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= pages::info(ui, cx.project, cx.palette);
    }
}

/// Página de título · tipo B.
pub struct TitleScreen<'a> {
    pub state: &'a mut pages::TitleState,
}

impl Screen for TitleScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Preview
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Página de título".into()
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        if kit::export_button(ui, "Exportar la portada y el guion a PDF").clicked() {
            cx.effects.push(Effect::ExportPdf);
        }
        self.state.zoom_delta += kit::zoom_control(ui, cx.palette, cx.settings.view_zoom);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        pages::title_page(
            ui,
            cx.project,
            cx.palette,
            cx.settings.paper,
            cx.settings.view_zoom,
        );
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Datos"]
    }
    fn inspector(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        cx.dirty |= pages::title_inspector(ui, cx.project, cx.palette);
    }
}

/// Sinopsis · tipo A.
pub struct SynopsisScreen<'a> {
    pub state: &'a mut synopsis::SynopsisState,
}

impl Screen for SynopsisScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Sheet
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Sinopsis".into()
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        synopsis::toolbar(ui, self.state, cx.project, cx.palette);
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        synopsis::toolbar_end(ui, self.state, cx.palette, cx.settings);
    }
    fn can_undo(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn can_redo(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn undo(&mut self, _cx: &mut ScreenCx) {
        // El campo de texto recibe el pedido como si fuera del teclado.
        self.state
            .keys
            .push((eframe::egui::Key::Z, eframe::egui::Modifiers::COMMAND));
    }
    fn redo(&mut self, _cx: &mut ScreenCx) {
        self.state.keys.push((
            eframe::egui::Key::Z,
            eframe::egui::Modifiers::COMMAND | eframe::egui::Modifiers::SHIFT,
        ));
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= synopsis::show(ui, self.state, cx.project, cx.settings, cx.palette);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Vinculados"]
    }
    fn inspector(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        let (nav, create) = linked_entities(ui, cx.palette, cx.project, &cx.project.synopsis_links);
        if create {
            self.state.open_entity(cx.project);
        }
        if let Some(nav) = nav {
            cx.navigate = Some(nav);
        }
    }
}

/// Tratamiento · tipo A.
pub struct TreatmentScreen<'a> {
    pub state: &'a mut treatment::TreatmentState,
}

impl Screen for TreatmentScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Sheet
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Tratamiento".into()
    }
    fn enter(&mut self, cx: &mut ScreenCx) {
        cx.inspector_tab = self.state.tab_request.take().or(cx.inspector_tab);
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let mut open = false;
        cx.dirty |= treatment::toolbar(
            ui,
            self.state,
            cx.project,
            cx.palette,
            &mut open,
            cx.settings,
        );
        cx.open_inspector |= open;
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        kit::export_menu(ui, |ui| {
                if ui.button("Exportar…").clicked() {
                    cx.effects.push(Effect::OpenExport);
                    ui.close();
                }
            },
        );
        self.state.zoom_delta += kit::zoom_control(ui, cx.palette, cx.settings.view_zoom);
    }
    fn can_undo(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn can_redo(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn undo(&mut self, _cx: &mut ScreenCx) {
        self.state
            .keys
            .push((eframe::egui::Key::Z, eframe::egui::Modifiers::COMMAND));
    }
    fn redo(&mut self, _cx: &mut ScreenCx) {
        self.state.keys.push((
            eframe::egui::Key::Z,
            eframe::egui::Modifiers::COMMAND | eframe::egui::Modifiers::SHIFT,
        ));
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= treatment::show(ui, self.state, cx.project, cx.settings, cx.palette);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Escaleta", "Comentarios", "Vinculados"]
    }
    fn has_inspector_footer(&self, tab: usize, _cx: &ScreenCx) -> bool {
        tab == 0
    }
    fn inspector_footer(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        // «Importar al guion» avisa cuántas escenas nuevas va a traer.
        let pending = cx.project.pending_outline_scenes();
        let width = ui.available_width();
        let label = match pending {
            0 => "Importar al guion".to_string(),
            n => format!("Importar al guion ({n})"),
        };
        if ui
            .add_enabled(
                pending > 0,
                eframe::egui::Button::new(format!("{}  {label}", crate::icon::DOWNLOAD_SIMPLE))
                    .min_size(eframe::egui::vec2(width, 0.0)),
            )
            .clicked()
        {
            cx.effects.push(Effect::ImportOutline);
        }
        let note = match (cx.project.outline.is_empty(), pending) {
            (true, _) => "Armá la escaleta con fragmentos del tratamiento.".to_string(),
            (_, 0) => "Todas las escenas de la escaleta ya están en el guion.".to_string(),
            (_, 1) => "Trae 1 escena nueva, sin el texto.".to_string(),
            (_, n) => format!("Trae {n} escenas nuevas, sin el texto."),
        };
        ui.add(
            eframe::egui::Label::new(
                eframe::egui::RichText::new(note)
                    .size(crate::ui::tokens::FONT_SMALL)
                    .color(cx.palette.dim),
            )
            .wrap(),
        );
    }
    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        let (changed, nav) = treatment::right_panel(ui, self.state, cx.project, cx.palette, tab);
        cx.dirty |= changed;
        if nav.is_some() {
            cx.navigate = nav;
        }
    }
}

/// Guion · tipo A.
pub struct ScriptScreen<'a> {
    pub state: &'a mut editor::EditorState,
}

impl Screen for ScriptScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Sheet
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Guion".into()
    }
    fn enter(&mut self, cx: &mut ScreenCx) {
        // La escena actual es compartida: si otra pantalla la cambió, el guion va a ella.
        if *cx.current_scene != self.state.shared_scene {
            if let Some(scene) = *cx.current_scene {
                self.state.jump_to(scene);
            }
            self.state.shared_scene = *cx.current_scene;
        }
        cx.inspector_tab = self.state.tab_request.take().or(cx.inspector_tab);
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let mut open = false;
        cx.dirty |= editor::toolbar(
            ui,
            self.state,
            cx.project,
            cx.palette,
            &mut open,
            cx.settings,
        );
        cx.open_inspector |= open;
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        // Exportar el guion solo, sin la página de título; los demás formatos, desde el menú ☰.
        if kit::export_button(ui, "Exportar el guion a PDF, sin la página de título").clicked() {
            cx.effects.push(Effect::ExportScriptPdf);
        }
        self.state.zoom_delta += kit::zoom_control(ui, cx.palette, cx.settings.view_zoom);
    }
    fn can_undo(&self, _cx: &ScreenCx) -> bool {
        self.state.can_undo()
    }
    fn can_redo(&self, _cx: &ScreenCx) -> bool {
        self.state.can_redo()
    }
    fn undo(&mut self, cx: &mut ScreenCx) {
        cx.dirty |= self.state.undo(cx.project);
    }
    fn redo(&mut self, cx: &mut ScreenCx) {
        cx.dirty |= self.state.redo(cx.project);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= editor::show(ui, self.state, cx.project, cx.settings, cx.palette);
        // La escena donde está el cursor pasa a ser la actual.
        if let Some(scene) = self
            .state
            .current_id()
            .and_then(|id| cx.project.scene_of(id))
            && Some(scene) != self.state.shared_scene
        {
            *cx.current_scene = Some(scene);
            self.state.shared_scene = Some(scene);
        }
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Escenas", "Ficha", "Comentarios"]
    }
    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        cx.dirty |= editor::right_panel(ui, self.state, cx.project, cx.palette, tab);
        // Los enlaces de la ficha de la escena llevan a las plantas, el desglose o los planos.
        if let Some((link, scene)) = self.state.scene_link.take() {
            let out = scenes::Out {
                link: Some((link, scene)),
                ..Default::default()
            };
            scene_nav(&out, cx);
        }
    }
}

/// El desglose como tabla (PDF o CSV): solo los ítems con datos y las categorías que tienen alguno.
fn export_breakdown(
    project: &crate::model::Project,
    paper: crate::storage::Paper,
    csv: bool,
) -> Option<String> {
    let lines = crate::reports::breakdown_lines(project);
    let title = &project.title;
    let (name, filter, bytes) = if csv {
        (
            format!("{title} - desglose.csv"),
            ("CSV", "csv"),
            crate::reports::breakdown_csv(&lines).into_bytes(),
        )
    } else {
        let sheet = paper.sheet();
        let pages = crate::reports::breakdown_pages(project, &lines, paper);
        let bytes = match crate::pdf::render(
            &pages,
            sheet.width * 72.0,
            sheet.height * 72.0,
            &format!("{title} — desglose"),
            &project.author,
        ) {
            Ok(bytes) => bytes,
            Err(e) => return Some(format!("Error al exportar: {e}")),
        };
        (format!("{title} - desglose.pdf"), ("PDF", "pdf"), bytes)
    };
    let path = rfd::FileDialog::new()
        .add_filter(filter.0, &[filter.1])
        .set_file_name(name)
        .save_file()?;
    Some(match std::fs::write(&path, bytes) {
        Ok(()) => "Desglose exportado".to_string(),
        Err(e) => format!("Error al exportar: {e}"),
    })
}

/// Estadísticas · tipo F.
pub struct StatsScreen;

impl Screen for StatsScreen {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Grid
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Estadísticas".into()
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        kit::export_menu(ui, |ui| {
            if ui.button("Estadísticas (CSV)").clicked() {
                cx.status = export_stats_csv(cx.project);
                ui.close();
            }
        });
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        pages::statistics(ui, cx.project, cx.palette);
    }
}

/// Las estadísticas como CSV: los totales y la participación de cada personaje.
fn export_stats_csv(project: &crate::model::Project) -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name(format!("{} - estadísticas.csv", project.title))
        .save_file()?;
    let scenes = project.scenes();
    let seconds: f32 = scenes.iter().map(|s| s.seconds).sum();
    let mut csv = String::from("Dato,Valor\n");
    csv.push_str(&format!(
        "Escenas,{}\nDuración estimada,\"{}\"\n\nPersonaje,Intervenciones,Palabras,Escenas\n",
        scenes.len(),
        pages::fmt_duration(seconds)
    ));
    for c in project.character_activity() {
        csv.push_str(&format!(
            "\"{}\",{},{},{}\n",
            c.name.replace('"', "\"\""),
            c.dialogues,
            c.words,
            c.scenes
        ));
    }
    Some(match std::fs::write(&path, csv) {
        Ok(()) => "Estadísticas exportadas".to_string(),
        Err(e) => format!("Error al exportar: {e}"),
    })
}

// ------------------------------------------------------------------------------------------------
// DISEÑO
// ------------------------------------------------------------------------------------------------

/// Guion técnico · tipo C con vista de grilla.
pub struct TechScreen<'a> {
    pub state: &'a mut techscript::TechState,
}

impl TechScreen<'_> {
    fn apply(&mut self, out: &techscript::Out, cx: &mut ScreenCx) {
        cx.dirty |= out.changed;
        cx.open_inspector |= out.open_inspector;
        if out.status.is_some() {
            cx.status = out.status.clone();
        }
        if let Some((blocking, camera)) = out.open_blocking {
            cx.navigate = Some(Nav::Camera(blocking, camera));
        }
        if let Some(id) = out.delete {
            match cx.project.shots.iter().find(|s| s.id == id) {
                Some(shot)
                    if !shot.description.trim().is_empty()
                        || !shot.dialogue.trim().is_empty()
                        || !shot.sound.trim().is_empty()
                        || shot.image.is_some() =>
                {
                    cx.confirm = Some(Confirm::DeleteShot(id));
                }
                _ => {
                    cx.project.remove_shot(id);
                    cx.dirty = true;
                }
            }
        }
        if let Some(id) = out.delete_column {
            let count = cx.project.column_values(id);
            if count > 0 {
                let name = cx
                    .project
                    .shot_columns
                    .iter()
                    .find(|c| c.id == id)
                    .map_or(String::new(), |c| c.name.clone());
                cx.confirm = Some(Confirm::DeleteShotColumn(id, name, count));
            } else {
                cx.project.remove_shot_column(id);
                cx.dirty = true;
            }
        }
    }
}

impl Screen for TechScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Table
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        match self.state.view {
            techscript::View::Table => "Guion técnico".into(),
            techscript::View::Storyboard => "Storyboard".into(),
        }
    }
    fn enter(&mut self, cx: &mut ScreenCx) {
        cx.inspector_tab = self.state.tab_request.take().or(cx.inspector_tab);
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = techscript::toolbar(ui, self.state, cx.project, cx.palette);
        self.apply(&out, cx);
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = techscript::toolbar_end(ui, cx.project, cx.settings.paper);
        self.apply(&out, cx);
    }
    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn context_bar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = techscript::context_bar(ui, self.state, cx.project, cx.palette);
        self.apply(&out, cx);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = techscript::show(ui, self.state, cx.project, cx.palette);
        self.apply(&out, cx);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Plano", "Columnas", "Formato"]
    }
    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        let out = techscript::right_panel(
            ui,
            self.state,
            cx.project,
            cx.palette,
            cx.settings.paper,
            tab,
        );
        self.apply(&out, cx);
    }
    fn has_inspector_footer(&self, tab: usize, cx: &ScreenCx) -> bool {
        tab == 0 && techscript::has_footer(self.state, cx.project)
    }
    fn inspector_footer(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        let out = techscript::footer(ui, self.state, cx.project, cx.palette);
        self.apply(&out, cx);
    }
    fn status(&self, cx: &ScreenCx) -> Vec<String> {
        let n = cx.project.shots.len();
        vec![format!("{n} {}", if n == 1 { "plano" } else { "planos" })]
    }
}

// ------------------------------------------------------------------------------------------------
// PRODUCCIÓN
// ------------------------------------------------------------------------------------------------

/// Escenas · tipo C.
pub struct ScenesScreen<'a> {
    pub state: &'a mut scenes::ScenesState,
}

impl Screen for ScenesScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Table
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Escenas".into()
    }
    fn enter(&mut self, cx: &mut ScreenCx) {
        // La escena actual es compartida: la tabla la marca como elegida.
        if let Some(scene) = *cx.current_scene
            && self.state.selected != Some(scene)
        {
            self.state.selected = Some(scene);
        }
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = scenes::toolbar(ui, cx.project, cx.palette);
        cx.dirty |= out.changed;
        scene_nav(&out, cx);
    }
    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn context_bar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        scenes::context_bar(ui, self.state, cx.project, cx.palette);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = scenes::show(ui, self.state, cx.project, cx.palette);
        cx.dirty |= out.changed;
        scene_nav(&out, cx);
        *cx.current_scene = self.state.selected.or(*cx.current_scene);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Escena"]
    }
    fn inspector(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        let out = scenes::inspector(ui, self.state, cx.project, cx.palette);
        cx.dirty |= out.changed;
        scene_nav(&out, cx);
    }
}

/// Desglose · tipo C.
pub struct BreakdownScreen<'a> {
    pub state: &'a mut pages::BreakdownState,
}

impl Screen for BreakdownScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Table
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Desglose".into()
    }
    fn enter(&mut self, cx: &mut ScreenCx) {
        // La hoja por escena sigue a la escena actual cuando cambió desde otra pantalla.
        if let Some(scene) = *cx.current_scene
            && self.state.view == pages::BreakdownView::ByScene
        {
            self.state.show_scene(scene);
        }
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        pages::breakdown_toolbar(ui, self.state, cx.palette);
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        kit::export_menu(ui, |ui| {
                for (label, csv) in [("Desglose (PDF)", false), ("Desglose (CSV)", true)] {
                    if ui.button(label).clicked() {
                        cx.status = export_breakdown(cx.project, cx.settings.paper, csv);
                        ui.close();
                    }
                }
            },
        );
    }
    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        self.state.view == pages::BreakdownView::ByCategory
    }
    fn context_bar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= pages::breakdown_context_bar(ui, cx.project, self.state, cx.palette);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Ítem", "Categorías"]
    }
    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        cx.dirty |= pages::breakdown_inspector(ui, cx.project, self.state, cx.palette, tab);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = pages::breakdown(ui, cx.project, self.state, cx.palette, cx.settings.paper);
        cx.dirty |= out.changed;
        if out.status.is_some() {
            cx.status = out.status;
        }
        cx.navigate = match out.open {
            Some(pages::BreakdownTarget::Character(id)) => {
                Some(Nav::Page(format!("character:{id}")))
            }
            Some(pages::BreakdownTarget::Location(id)) => Some(Nav::Page(format!("location:{id}"))),
            Some(pages::BreakdownTarget::Object(id)) => Some(Nav::Page(format!("object:{id}"))),
            Some(pages::BreakdownTarget::Scene(id)) => Some(Nav::Script(id)),
            None => cx.navigate.take(),
        };
    }
}

/// Plan de rodaje · tablero (variante del tipo C).
pub struct PlanScreen<'a> {
    pub state: &'a mut plan::PlanState,
}

impl PlanScreen<'_> {
    fn apply(out: &plan::Out, cx: &mut ScreenCx) {
        cx.dirty |= out.changed;
        if out.status.is_some() {
            cx.status = out.status.clone();
        }
        if out.inspector_tab.is_some() {
            cx.inspector_tab = out.inspector_tab;
            cx.open_inspector = true;
        }
        if let Some(id) = out.open_scene {
            cx.navigate = Some(Nav::Script(id));
        }
        if let Some((link, scene)) = out.link {
            scene_nav(
                &scenes::Out {
                    link: Some((link, scene)),
                    ..Default::default()
                },
                cx,
            );
        }
    }
}

impl Screen for PlanScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Table
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Plan de rodaje".into()
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = plan::toolbar(
            ui,
            self.state,
            cx.project,
            cx.palette,
            cx.settings.paper,
            false,
        );
        Self::apply(&out, cx);
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = plan::toolbar(
            ui,
            self.state,
            cx.project,
            cx.palette,
            cx.settings.paper,
            true,
        );
        Self::apply(&out, cx);
    }
    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        self.state.is_board()
    }
    fn context_bar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = plan::context_bar(ui, self.state, cx.project, cx.palette);
        Self::apply(&out, cx);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = plan::show(ui, self.state, cx.project, cx.palette);
        Self::apply(&out, cx);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Día", "Escena", "Ajustes"]
    }
    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        let out = plan::inspector(ui, self.state, cx.project, cx.palette, tab);
        Self::apply(&out, cx);
    }
}

/// Hojas de llamado · tipo B.
pub struct CallsScreen<'a> {
    pub state: &'a mut calls::CallsState,
}

impl CallsScreen<'_> {
    fn apply(out: &calls::Out, cx: &mut ScreenCx) {
        cx.dirty |= out.changed;
        if out.status.is_some() {
            cx.status = out.status.clone();
        }
    }
}

impl Screen for CallsScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Preview
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Hojas de llamado".into()
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = calls::toolbar(ui, self.state, cx.project, cx.settings.paper);
        Self::apply(&out, cx);
    }
    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn context_bar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        calls::context_bar(ui, self.state, cx.project, cx.palette);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = calls::show(ui, self.state, cx.project, cx.palette);
        Self::apply(&out, cx);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Horarios", "Elenco", "Notas"]
    }
    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        let out = calls::inspector(
            ui,
            self.state,
            cx.project,
            cx.palette,
            cx.settings.paper,
            tab,
        );
        Self::apply(&out, cx);
    }
}

/// Locaciones de rodaje · tipo C.
pub struct ShootScreen<'a> {
    pub state: &'a mut shoot::ShootState,
}

impl ShootScreen<'_> {
    fn apply(&mut self, out: &shoot::Out, cx: &mut ScreenCx) {
        cx.dirty |= out.changed;
        if let Some(id) = out.open_page {
            cx.navigate = Some(Nav::Page(format!("shootplace:{id}")));
        }
        if let Some(id) = out.delete {
            let name = cx
                .project
                .shoot_locations
                .iter()
                .find(|l| l.id == id)
                .map_or(String::new(), |l| l.name.clone());
            let scenes = cx.project.shoot_location_scenes(id).len();
            cx.confirm = Some(Confirm::DeleteShootLocation(id, name, scenes));
        }
    }
}

impl Screen for ShootScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Table
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Locaciones de rodaje".into()
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = shoot::toolbar(ui, self.state, cx.project, cx.palette);
        self.apply(&out, cx);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = shoot::show(ui, self.state, cx.project, cx.palette);
        self.apply(&out, cx);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Ficha"]
    }
    fn inspector(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        let out = shoot::inspector(ui, self.state, cx.project, cx.palette);
        self.apply(&out, cx);
    }
}

/// La ficha de un lugar de rodaje abierta como página · tipo E.
pub struct ShootPlaceScreen {
    pub id: u64,
}

impl Screen for ShootPlaceScreen {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, cx: &ScreenCx) -> String {
        cx.project
            .shoot_locations
            .iter()
            .find(|l| l.id == self.id)
            .map_or("(sin nombre)".to_string(), |l| {
                if l.name.trim().is_empty() {
                    "(sin nombre)".into()
                } else {
                    l.name.clone()
                }
            })
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        if crate::ui::kit::icon_button(
            ui,
            cx.palette,
            crate::icon::TRASH,
            "Eliminar esta locación de rodaje",
            true,
        )
        .clicked()
        {
            let name = cx
                .project
                .shoot_locations
                .iter()
                .find(|l| l.id == self.id)
                .map_or(String::new(), |l| l.name.clone());
            let scenes = cx.project.shoot_location_scenes(self.id).len();
            cx.confirm = Some(Confirm::DeleteShootLocation(self.id, name, scenes));
        }
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = shoot::page(ui, cx.project, self.id, cx.palette);
        cx.dirty |= out.changed;
    }
}

/// Contactos · tipo C.
pub struct ContactsScreen<'a> {
    pub state: &'a mut contacts::ContactsState,
}

impl ContactsScreen<'_> {
    fn apply(out: &contacts::Out, cx: &mut ScreenCx) {
        cx.dirty |= out.changed;
        if out.status.is_some() {
            cx.status = out.status.clone();
        }
    }
}

impl Screen for ContactsScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Table
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Contactos".into()
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = contacts::toolbar(ui, self.state, cx.project, cx.palette);
        Self::apply(&out, cx);
    }
    fn has_context_bar(&self, _cx: &ScreenCx) -> bool {
        true
    }
    fn context_bar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        contacts::context_bar(ui, self.state, cx.palette);
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = contacts::show(ui, self.state, cx.project, cx.palette);
        Self::apply(&out, cx);
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Contacto"]
    }
    fn inspector(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        let out = contacts::inspector(ui, self.state, cx.project, cx.palette);
        Self::apply(&out, cx);
    }
}

// ------------------------------------------------------------------------------------------------
// Fichas y documentos · tipo E (los documentos de texto son tipo A; el mapa mental, D; la galería, F)
// ------------------------------------------------------------------------------------------------

fn named(name: &str) -> String {
    if name.trim().is_empty() {
        "(sin nombre)".to_string()
    } else {
        name.to_string()
    }
}

/// Dónde aparece algo: las escenas donde está nombrado, con un enlace a cada una (pestaña APARICIONES).
fn appearances(ui: &mut Ui, cx: &mut ScreenCx, target: EntityRef) {
    use crate::ui::kit;
    let scenes = cx.project.script_scenes();
    let numbers: std::collections::HashMap<u64, String> =
        cx.project.scene_labels().into_iter().collect();
    let mut found: Vec<(u64, String)> = Vec::new();
    for (id, heading) in scenes {
        let here = match target {
            EntityRef::Character(c) => cx.project.scene_cast_ids(id).contains(&c),
            EntityRef::Location(l) => cx.project.scenes_in_location(l).contains(&id),
            _ => false,
        };
        if here {
            found.push((
                id,
                format!(
                    "{}. {}",
                    numbers.get(&id).cloned().unwrap_or_default(),
                    heading.trim()
                ),
            ));
        }
    }
    if found.is_empty() {
        kit::empty_state(
            ui,
            cx.palette,
            crate::icon::LIST_NUMBERS,
            "Todavía no aparece en ninguna escena.",
            None,
        );
        return;
    }
    for (id, label) in found {
        if kit::tree_row(
            ui,
            cx.palette,
            kit::Row {
                depth: 0,
                caret: None,
                icon: Some(crate::icon::SCROLL),
                label: &label,
                selected: false,
            },
        )
        .clicked()
        {
            cx.navigate = Some(Nav::Script(id));
        }
    }
}

/// Ficha de personaje.
pub struct CharacterScreen<'a> {
    pub id: u64,
    pub export_sel: &'a mut crate::export::Selection,
}

impl Screen for CharacterScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, cx: &ScreenCx) -> String {
        named(&cx.project.label_for(EntityRef::Character(self.id)))
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = pages::character(ui, cx.project, self.id, cx.palette);
        cx.dirty |= out.changed;
        if out.changed {
            cx.effects.push(Effect::AfterRename);
        }
        let name = cx
            .project
            .character(self.id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        if out.reset {
            cx.confirm = Some(Confirm::ResetCharacter(self.id, name.clone()));
        }
        if out.delete {
            let cues = cx.project.character_cues(self.id);
            if cues == 0 {
                cx.effects.push(Effect::DeleteCharacter(self.id));
            } else {
                cx.confirm = Some(Confirm::DeleteCharacter(self.id, name, cues));
            }
        }
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Apariciones", "Exportar"]
    }
    fn inspector(&mut self, ui: &mut Ui, tab: usize, cx: &mut ScreenCx) {
        if tab == 0 {
            appearances(ui, cx, EntityRef::Character(self.id));
        } else if pages::character_export_panel(
            ui,
            cx.project,
            self.id,
            self.export_sel,
            cx.palette,
        ) {
            cx.effects.push(Effect::ExportCharacter(self.id));
        }
    }
}

/// Ficha de una locación del guion.
pub struct LocationScreen {
    pub id: u64,
}

impl Screen for LocationScreen {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, cx: &ScreenCx) -> String {
        named(&cx.project.label_for(EntityRef::Location(self.id)))
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = pages::location(ui, cx.project, self.id, cx.palette);
        cx.dirty |= out.changed;
        if out.changed {
            cx.effects.push(Effect::AfterRename);
        }
        if out.delete {
            cx.project.remove_location(self.id);
            cx.dirty = true;
            cx.effects.push(Effect::GoInfo);
        }
    }
    fn inspector_tabs(&self, _cx: &ScreenCx) -> &'static [&'static str] {
        &["Apariciones"]
    }
    fn inspector(&mut self, ui: &mut Ui, _tab: usize, cx: &mut ScreenCx) {
        appearances(ui, cx, EntityRef::Location(self.id));
    }
}

/// Ficha de un objeto clave.
pub struct ObjectScreen {
    pub id: u64,
}

impl Screen for ObjectScreen {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, cx: &ScreenCx) -> String {
        named(&cx.project.label_for(EntityRef::Object(self.id)))
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = pages::object(ui, cx.project, self.id, cx.palette);
        cx.dirty |= out.changed;
        if out.changed {
            cx.effects.push(Effect::AfterRename);
        }
        if out.delete {
            cx.project.remove_object(self.id);
            cx.dirty = true;
            cx.effects.push(Effect::GoInfo);
        }
    }
}

/// Ficha de un elemento de una categoría propia.
pub struct ElementScreen {
    pub id: u64,
}

impl Screen for ElementScreen {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, cx: &ScreenCx) -> String {
        named(
            cx.project
                .elements
                .iter()
                .find(|e| e.id == self.id)
                .map_or("", |e| e.name.as_str()),
        )
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = pages::element(ui, cx.project, self.id, cx.palette);
        cx.dirty |= out.changed;
        if out.changed {
            cx.effects.push(Effect::AfterRename);
        }
        if out.delete {
            let category = cx
                .project
                .elements
                .iter()
                .find(|e| e.id == self.id)
                .map(|e| e.category);
            cx.project.elements.retain(|e| e.id != self.id);
            cx.project.drop_links_to(EntityRef::Element(self.id));
            cx.dirty = true;
            match category {
                Some(c) => cx.effects.push(Effect::GoPage(format!("category:{c}"))),
                None => cx.effects.push(Effect::GoInfo),
            }
        }
    }
}

/// Editor de la plantilla de una categoría propia y la lista de sus elementos.
pub struct CategoryScreen<'a> {
    pub id: u64,
    pub editing: &'a mut Option<u64>,
}

impl Screen for CategoryScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, cx: &ScreenCx) -> String {
        named(
            cx.project
                .element_categories
                .iter()
                .find(|c| c.id == self.id)
                .map_or("", |c| c.name.as_str()),
        )
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = pages::element_category(ui, cx.project, self.id, self.editing, cx.palette);
        cx.dirty |= out.changed;
        if out.add
            && let Some(new) = cx.project.add_element(self.id, "Nuevo elemento")
        {
            cx.dirty = true;
            cx.navigate = Some(Nav::Page(format!("element:{new}")));
        }
        if let Some(open) = out.open {
            cx.navigate = Some(Nav::Page(format!("element:{open}")));
        }
        if let Some((section, field, count, label)) = out.confirm_part {
            cx.confirm = Some(Confirm::DeleteTemplatePart {
                category: self.id,
                section,
                field,
                label,
                count,
            });
        }
        if out.delete {
            let count = cx
                .project
                .elements
                .iter()
                .filter(|e| e.category == self.id)
                .count();
            let name = cx
                .project
                .element_categories
                .iter()
                .find(|c| c.id == self.id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            if count == 0 {
                cx.project.remove_element_category(self.id);
                cx.dirty = true;
                cx.effects.push(Effect::GoInfo);
            } else {
                cx.confirm = Some(Confirm::DeleteElementCategory(self.id, name, count));
            }
        }
    }
}

/// Documento: carpeta, texto, mundo, mapa mental o galería.
pub struct DocumentScreen<'a> {
    pub id: u64,
    pub doc_kind: Option<crate::model::DocKind>,
    pub selected: &'a mut Option<u64>,
    pub story: &'a mut crate::story::StoryState,
}

impl Screen for DocumentScreen<'_> {
    fn kind(&self) -> ScreenKind {
        use crate::model::DocKind;
        match self.doc_kind {
            Some(DocKind::MindMap | DocKind::Relations | DocKind::Cards) => ScreenKind::Canvas,
            Some(DocKind::Gallery) => ScreenKind::Grid,
            Some(DocKind::Text) => ScreenKind::Sheet,
            _ => ScreenKind::Card,
        }
    }
    fn title(&self, cx: &ScreenCx) -> String {
        named(
            cx.project
                .documents
                .iter()
                .find(|d| d.id == self.id)
                .map_or("", |d| d.name.as_str()),
        )
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let out = pages::document(ui, cx.project, self.id, self.selected, self.story, cx.palette);
        cx.dirty |= out.changed;
        if out.changed {
            cx.effects.push(Effect::AfterRename);
        }
        if out.delete {
            cx.project.delete_document(self.id);
            cx.dirty = true;
            cx.effects.push(Effect::GoInfo);
        }
        if let Some(open) = out.open {
            cx.navigate = Some(Nav::Page(format!("document:{open}")));
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Inicio y Ajustes
// ------------------------------------------------------------------------------------------------

/// Inicio · tipo F, sin el marco del proyecto.
pub struct HomeScreen<'a> {
    pub cards: &'a [crate::storage::RecentCard],
    pub action: &'a mut Option<pages::HomeAction>,
}

impl Screen for HomeScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Grid
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Inicio".into()
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        *self.action = pages::home(ui, self.cards, cx.palette);
    }
}

/// Ajustes: secciones a la izquierda y el contenido en una columna de ancho máximo 640.
pub struct SettingsScreen<'a> {
    pub section: &'a mut usize,
    /// Se abrió desde Inicio (sin el marco): el botón de volver va dentro del contenido.
    pub bare: bool,
}

impl Screen for SettingsScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Ajustes".into()
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        if crate::ui::kit::icon_button(ui, cx.palette, crate::icon::X, "Cerrar los ajustes", true)
            .clicked()
        {
            cx.effects.push(Effect::CloseSettings);
        }
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        if self.bare
            && crate::ui::kit::icon_button(ui, cx.palette, crate::icon::X, "Cerrar los ajustes", true)
                .clicked()
        {
            cx.effects.push(Effect::CloseSettings);
        }
        let mut copy = cx.settings.clone();
        let (changed, project_changed) =
            pages::settings(ui, self.section, &mut copy, cx.project, cx.palette);
        if changed {
            cx.new_settings = Some(copy);
        }
        cx.dirty |= project_changed;
    }
}

// ------------------------------------------------------------------------------------------------
// ROLES
// ------------------------------------------------------------------------------------------------

/// Dirección de sonido (Guion sonoro y Equipo de sonido) y Dirección de arte · tipo C: la lista de pedidos de cada rol, por
/// escena.
pub struct RoleScreen<'a> {
    pub role: crate::model::Role,
    pub state: &'a mut crate::roles::RoleState,
}

impl Screen for RoleScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Table
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        match self.role {
            crate::model::Role::Sound => "Guion sonoro".into(),
            crate::model::Role::SoundGear => "Equipo de sonido".into(),
            crate::model::Role::PhotoGear => "Equipos de fotografía".into(),
            crate::model::Role::Art => "Elementos de dirección de arte".into(),
        }
    }
    fn toolbar(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let role = self.role;
        if kit::icon_button(ui, cx.palette, crate::icon::CARET_RIGHT, "Plegar todas las escenas", true)
            .clicked()
        {
            self.state.set_all(cx.project, role, true);
        }
        if kit::icon_button(ui, cx.palette, crate::icon::CARET_DOWN, "Desplegar todas las escenas", true)
            .clicked()
        {
            self.state.set_all(cx.project, role, false);
        }
    }
    fn toolbar_end(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let role = self.role;
        kit::export_menu(ui, |ui| {
            if ui.button("Tabla (CSV)").clicked() {
                cx.status = crate::roles::export_csv(cx.project, role);
                ui.close();
            }
        });
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        let role = self.role;
        cx.dirty |= crate::roles::show(ui, cx.project, cx.palette, role, self.state);
    }
}

/// Curva dramática · tipo E.
pub struct DramaScreen<'a> {
    pub state: &'a mut crate::story::StoryState,
}

impl Screen for DramaScreen<'_> {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Card
    }
    fn title(&self, _cx: &ScreenCx) -> String {
        "Curva dramática".into()
    }
    fn content(&mut self, ui: &mut Ui, cx: &mut ScreenCx) {
        cx.dirty |= crate::story::drama(ui, cx.project, self.state, cx.palette);
    }
}
