mod app;
mod assets;
mod blocking;
mod breakdown_views;
mod calls;
mod callsheet;
mod cloud;
mod cloud_ui;
mod cloudimages;
mod cloudsync;
mod contacts;
mod create;
mod docx;
mod editor;
mod export;
mod export_dialog;
mod figures;
mod folder;
mod fountain;
mod icon;
mod model;
mod motion;
mod optics;
mod pages;
mod pdf;
mod placement;
mod plan;
mod plan_checks;
mod profile;
mod reports;
mod roles;
mod scenes;
mod schedule;
mod screens;
mod shoot;
mod sketch;
mod storage;
mod story;
mod synopsis;
mod techscript;
mod theme;
mod treatment;
mod ui;
mod vcard;
mod widgets;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size(ui::tokens::WINDOW_MIN),
        ..Default::default()
    };
    eframe::run_native(
        "Scriptpendra",
        options,
        Box::new(|cc| Ok(Box::new(app::ScriptApp::new(&cc.egui_ctx)))),
    )
}
