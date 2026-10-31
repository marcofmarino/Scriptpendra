//! Editor del tratamiento: un texto corrido sobre hojas, con los nombres vinculados al proyecto (con autocompletado),
//! comentarios y fragmentos que se ponen dentro de las escenas de la escaleta.
//!
//! A diferencia del guion, es un solo campo de texto, así que la selección, Ctrl+A y el deshacer funcionan de forma
//! natural. Los nombres no se pasan a mayúsculas; solo los de personajes se pueden poner en negrita.

use crate::ui::{kit, tokens};
use eframe::egui::{
    self, Align, Align2, FontId, Id, Key, Modifiers, RichText, Stroke, TextEdit, Ui,
    text::{CCursor, CCursorRange},
    text_edit::TextEditState,
};

use crate::{
    create::{self, NewEntity, create_entity, entity_name},
    editor::{
        Suggestion, comment_range, format_mention, highlight_range, mention_suggestions,
        scene_suggestions, styled_job,
    },
    icon,
    model::{
        CaseOptions, EntityRef, Link, Mention, Project, adjust_links, mention_spans, validate_links,
    },
    storage::Settings,
    theme::{self, Palette},
    widgets,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    /// La escaleta: las escenas armadas con fragmentos del tratamiento.
    #[default]
    Outline,
    Comments,
    /// Personajes, locaciones y elementos nombrados en el texto.
    Linked,
}

#[derive(Default)]
pub struct TreatmentState {
    pub tab: Tab,
    /// Pestaña del inspector que se pidió abrir (0 Escaleta, 1 Comentarios, 2 Vinculados).
    pub tab_request: Option<usize>,
    /// Cambio pedido a «personajes en negrita»; lo aplica la app.
    pub set_bold: Option<bool>,
    /// Dar el foco al texto y seleccionar este rango (en caracteres).
    focus: Option<(usize, usize)>,
    /// Última selección no vacía del texto: sobre ella se comenta y se arma la escaleta.
    sel: Option<(usize, usize)>,
    /// Posición del cursor la última vez que el texto tuvo el foco.
    cursor: Option<usize>,
    suggest_idx: usize,
    /// Texto para el que se descartó la sugerencia con Esc.
    dismissed: Option<String>,
    was_focused: bool,
    comment_draft: String,
    /// Teclas de deshacer/rehacer pedidas desde la barra; se entregan al texto en el próximo cuadro.
    pub keys: Vec<(Key, Modifiers)>,
    /// Encabezado que se está escribiendo para una escena nueva (sobre lo seleccionado).
    scene_draft: Option<SceneDraft>,
    /// El texto del cuadro anterior, para saber qué cambió.
    prev_text: String,
    entity: Option<NewEntity>,
    /// Cambio pedido al zoom de la vista, en pasos; lo aplica la app.
    pub zoom_delta: i32,
    zoom_acc: f32,
}

/// Encabezado de escena que se escribe directamente en el texto, en la línea que se abrió sobre lo seleccionado.
struct SceneDraft {
    /// Dónde empieza la línea del encabezado (carácter).
    start: usize,
    /// El tratamiento tal como estaba, para volver a él si se cancela.
    orig: String,
    /// Ya tuvo el foco: recién entonces perderlo cierra el encabezado.
    seen_focus: bool,
}

/// Dónde termina la línea que empieza en `start`.
fn heading_end(text: &str, start: usize) -> usize {
    let mut n = start;
    for c in text.chars().skip(start) {
        if c == '\n' {
            break;
        }
        n += 1;
    }
    n
}

fn char_slice(text: &str, a: usize, b: usize) -> String {
    text.chars().skip(a).take(b.saturating_sub(a)).collect()
}

/// Medidas de la hoja en píxeles para una letra de `pt` puntos y un zoom de vista de `zoom` %: la pulgada, el tamaño de
/// la letra y la altura de un renglón (Arial: 1,15 de interlínea). El zoom escala todo por igual; los puntos, solo la letra.
fn sheet_scale(pt: u32, zoom: u32) -> (f32, f32, f32) {
    let inch = 120.0 * zoom as f32 / 100.0;
    let size = pt as f32 * inch / 72.0;
    (inch, size, size * 1.15)
}

/// Pinta con el color de cada rango (en caracteres) lo que cae dentro de él, partiendo los tramos del texto donde haga falta.
fn recolor(job: &mut egui::text::LayoutJob, text: &str, ranges: &[(usize, usize, egui::Color32)]) {
    if ranges.is_empty() {
        return;
    }
    let byte = |c: usize| text.char_indices().nth(c).map_or(text.len(), |(b, _)| b);
    let ranges: Vec<(usize, usize, egui::Color32)> = ranges
        .iter()
        .map(|&(a, b, color)| (byte(a), byte(b), color))
        .collect();
    let mut out = Vec::with_capacity(job.sections.len());
    for section in std::mem::take(&mut job.sections) {
        let (start, end) = (section.byte_range.start.0, section.byte_range.end.0);
        let mut cuts = vec![start, end];
        for &(a, b, _) in &ranges {
            cuts.extend([a, b].into_iter().filter(|&x| x > start && x < end));
        }
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let mut piece = section.clone();
            piece.byte_range = egui::text::ByteIndex(pair[0])..egui::text::ByteIndex(pair[1]);
            if pair[0] != start {
                piece.leading_space = 0.0;
            }
            if let Some(&(_, _, color)) = ranges.iter().find(|r| pair[0] >= r.0 && pair[1] <= r.1) {
                piece.format.color = color;
            }
            out.push(piece);
        }
    }
    job.sections = out;
}

/// Hueco entre una hoja y la siguiente.
const SHEET_GAP: f32 = 28.0;

/// Corre los renglones de cada hoja hacia abajo para que el texto salte los márgenes de abajo y de arriba y el hueco
/// entre hojas: cada `rows_per_sheet` renglones empiezan una hoja nueva, `skip` más abajo que si fuera todo seguido.
fn spread_over_sheets(
    galley: std::sync::Arc<egui::Galley>,
    rows_per_sheet: usize,
    skip: f32,
) -> std::sync::Arc<egui::Galley> {
    if rows_per_sheet == 0 || galley.rows.len() <= rows_per_sheet {
        return galley;
    }
    let mut galley = std::sync::Arc::unwrap_or_clone(galley);
    for (n, row) in galley.rows.iter_mut().enumerate() {
        row.pos.y += (n / rows_per_sheet) as f32 * skip;
    }
    let extra = ((galley.rows.len() - 1) / rows_per_sheet) as f32 * skip;
    galley.rect.max.y += extra;
    galley.mesh_bounds.max.y += extra;
    std::sync::Arc::new(galley)
}

/// Reemplaza los caracteres `a..b` del tratamiento, y los vínculos acompañan.
fn replace_range(project: &mut Project, a: usize, b: usize, with: &str) {
    let old = std::mem::take(&mut project.treatment);
    project.treatment = char_slice(&old, 0, a) + with + &old.chars().skip(b).collect::<String>();
    adjust_links(&mut project.treatment_links, &old, &project.treatment);
}

/// Los puntos de la curva dramática como texto para el tratamiento, en el orden de la historia: el nombre de cada punto y, debajo,
/// lo que se anotó para él.
pub fn drama_text(project: &Project) -> String {
    let mut points: Vec<&crate::model::DramaPoint> = project.drama.iter().collect();
    points.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut out = String::from("CURVA DRAMÁTICA\n");
    for (n, d) in points.iter().enumerate() {
        out.push_str(&format!("\n{}. {}\n", n + 1, d.label.trim()));
        if !d.text.trim().is_empty() {
            out.push_str(d.text.trim());
            out.push('\n');
        }
    }
    out
}

impl TreatmentState {
    /// Pone la curva dramática (los puntos y sus anotaciones) en el cursor, en líneas propias, para tenerla de referencia al
    /// escribir. Devuelve si había algo para importar.
    pub fn import_drama(&mut self, project: &mut Project) -> bool {
        project.ensure_drama();
        if project.drama.is_empty() {
            return false;
        }
        let len = project.treatment.chars().count();
        let at = self
            .sel
            .map(|(a, _)| a)
            .or(self.cursor)
            .unwrap_or(len)
            .min(len);
        let chars: Vec<char> = project.treatment.chars().collect();
        let mut block = String::new();
        if at > 0 && chars[at - 1] != '\n' {
            block.push('\n');
        }
        block.push_str(&drama_text(project));
        if at < len && chars[at] != '\n' {
            block.push('\n');
        }
        replace_range(project, at, at, &block);
        let end = at + block.chars().count();
        self.focus = Some((end, end));
        true
    }

    /// Abre una línea nueva sobre lo seleccionado (o sobre el cursor) para escribir ahí el encabezado de la escena.
    pub fn open_scene(&mut self, project: &mut Project) {
        if self.scene_draft.is_some() {
            return;
        }
        let len = project.treatment.chars().count();
        let at = self
            .sel
            .map(|(a, _)| a)
            .or(self.cursor)
            .unwrap_or(len)
            .min(len);
        let orig = project.treatment.clone();
        // El encabezado queda en su propia línea: si el corte cae en medio de un párrafo, se parte en dos.
        let chars: Vec<char> = orig.chars().collect();
        let mut from = at;
        while from > 0 && matches!(chars[from - 1], ' ' | '\t') {
            from -= 1;
        }
        let lead = usize::from(from > 0 && chars[from - 1] != '\n');
        replace_range(project, from, at, &format!("{}\n", "\n".repeat(lead)));
        let start = from + lead;
        self.sel = None;
        self.focus = Some((start, start));
        self.scene_draft = Some(SceneDraft {
            start,
            orig,
            seen_focus: false,
        });
    }

    /// Abre el formulario para crear algo del proyecto. Con un texto seleccionado, ese texto es el nombre y se
    /// convierte en lo que se cree.
    fn open_entity(&mut self, project: &Project) {
        let quote = selected_quote(self, project);
        let replace = self.sel.filter(|_| quote.is_some());
        let name = quote.map(|q| q.trim().to_string()).unwrap_or_default();
        self.entity = Some(NewEntity {
            fresh: true,
            name,
            replace,
            ..Default::default()
        });
    }

    /// Cierra el encabezado: con `create` se crea la escena en la escaleta; si no (o si quedó vacío), la línea se quita.
    fn finish_scene(&mut self, project: &mut Project, create: bool, refocus: bool) -> bool {
        let Some(d) = self.scene_draft.take() else {
            return false;
        };
        let end = heading_end(&project.treatment, d.start);
        let heading = char_slice(&project.treatment, d.start, end)
            .trim()
            .to_string();
        if !create || heading.is_empty() {
            let now = project.treatment.chars().count();
            replace_range(project, 0, now, &d.orig);
            if refocus {
                self.focus = Some((d.start, d.start));
            }
            return true;
        }
        project.add_outline_scene_at(&heading, d.start);
        // La locación del encabezado se crea o se referencia, y la escena queda numerada.
        project.link_treatment_heading(d.start, end);
        if refocus {
            self.focus = Some((end + 1, end + 1));
        }
        true
    }
}

fn text_id() -> Id {
    Id::unique("treatment_text")
}

fn set_selection(ctx: &egui::Context, from: usize, to: usize, len: usize) {
    let id = text_id();
    ctx.memory_mut(|m| m.request_focus(id));
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(from.min(len)),
        CCursor::new(to.min(len)),
    )));
    state.store(ctx, id);
}

/// Aplica una sugerencia al texto. La primera vez que un personaje aparece en el tratamiento se abre un paréntesis
/// con su edad y su descripción: si la ficha los tiene, ya vienen escritos; si no, el cursor queda adentro para
/// completarlos. Devuelve dónde queda el cursor.
fn apply_suggestion(project: &mut Project, pick: &Suggestion) -> usize {
    let first = pick.link.is_some_and(
        |l| matches!(l.target, EntityRef::Character(id) if !project.treatment_mentions(id)),
    );
    let old = std::mem::replace(&mut project.treatment, pick.text.clone());
    adjust_links(&mut project.treatment_links, &old, &project.treatment);
    let mut cursor = pick.cursor;
    let Some(link) = pick.link else { return cursor };
    project
        .treatment_links
        .retain(|l| !(l.start < link.end && l.end > link.start));
    project.treatment_links.push(link);
    project.treatment_links.sort_by_key(|l| l.start);
    let text = project.treatment.clone();
    validate_links(&mut project.treatment_links, &text);

    if let (true, EntityRef::Character(id)) = (first, link.target) {
        let intro = project.character_intro(id);
        let inserted = format!(" ({intro})");
        let at = link.end;
        let before = project.treatment.clone();
        let chars: Vec<char> = before.chars().collect();
        project.treatment = chars[..at].iter().collect::<String>()
            + &inserted
            + &chars[at..].iter().collect::<String>();
        adjust_links(&mut project.treatment_links, &before, &project.treatment);
        // Con la ficha completa el cursor sigue después del paréntesis; si no, queda dentro para escribir.
        cursor = if intro.is_empty() {
            at + 2
        } else {
            at + inserted.chars().count()
        };
    }
    cursor
}

/// Escribe en el tratamiento, en el cursor, el nombre de algo del proyecto y lo vincula. Devuelve el cursor nuevo.
fn insert_entity(project: &mut Project, cursor: usize, target: EntityRef, typed: &str) -> usize {
    // Se escribe en su forma natural, igual que al autocompletar (los nombres guardados pueden estar en mayúsculas).
    let stored = entity_name(project, target).unwrap_or_default();
    let mention = Mention {
        name: stored,
        character: matches!(target, EntityRef::Character(_)),
        target: Some(target),
        detail: String::new(),
    };
    let name = format_mention(&mention, typed, false);
    let chars: Vec<char> = project.treatment.chars().collect();
    let at = cursor.min(chars.len());
    let lead = if at > 0 && chars[at - 1].is_alphanumeric() {
        " "
    } else {
        ""
    };
    let start = at + lead.chars().count();
    let end = start + name.chars().count();
    let tail = if chars.get(at).is_some_and(|c| c.is_alphanumeric()) {
        " "
    } else {
        ""
    };
    let text: String = chars[..at].iter().collect::<String>()
        + lead
        + &name
        + tail
        + &chars[at..].iter().collect::<String>();
    let pick = Suggestion {
        label: name,
        text,
        stay: false,
        cursor: end,
        enter: false,
        link: Some(Link { start, end, target }),
    };
    apply_suggestion(project, &pick)
}

/// Herramientas del tratamiento en la barra de herramientas: «Ver ▾» y, en un grupo, comentar, crear escena y vincular
/// (interfaz.md §3.4). Deshacer y rehacer los dibuja el marco; el zoom y Exportar van a la derecha (`toolbar_end`).
pub fn toolbar(
    ui: &mut Ui,
    state: &mut TreatmentState,
    project: &mut Project,
    p: &Palette,
    show_right: &mut bool,
    settings: &Settings,
) -> bool {
    let (mut comment, mut scene, mut entity, mut drama) = (false, false, false, false);
    crate::ui::kit::toolbar_groups(ui, "treatment_tools", &[1, 3], |ui, group| match group {
        0 => {
            // Nombres en negrita y atajos: lo que cambia cómo se ve el tratamiento.
            ui.menu_button(format!("{}  Ver  {}", icon::EYE, icon::CARET_DOWN), |ui| {
                ui.set_max_width(crate::ui::tokens::DIALOG_MAX);
                format_tab(ui, state, p, settings.treatment_bold);
            });
        }
        _ => {
            comment |= widgets::icon_button(
                ui,
                p,
                icon::CHAT_TEXT,
                "Comentar lo seleccionado (Ctrl+Mayús+C)",
                true,
            )
            .clicked();
            scene |= widgets::icon_button(
                ui,
                p,
                icon::FILM_SLATE,
                "Crear escena con lo seleccionado (Ctrl+Mayús+E)",
                true,
            )
            .clicked();
            entity |= widgets::icon_button(
                ui,
                p,
                icon::USER_PLUS,
                "Vincular a una ficha: crear personaje, locación, objeto… (Ctrl+Mayús+N)",
                true,
            )
            .clicked();
            drama |= widgets::icon_button(
                ui,
                p,
                icon::CHART_LINE_UP,
                "Importar la curva dramática (los puntos y sus notas) donde está el cursor",
                true,
            )
            .clicked();
        }
    });
    if comment {
        state.tab_request = Some(1);
        *show_right = true;
    }
    if scene {
        state.open_scene(project);
    }
    if entity {
        state.open_entity(project);
    }
    drama && state.import_drama(project)
}

/// Dibuja el tratamiento y devuelve si el proyecto cambió.
pub fn show(
    ui: &mut Ui,
    state: &mut TreatmentState,
    project: &mut Project,
    settings: &Settings,
    p: &Palette,
) -> bool {
    let ctx = ui.ctx().clone();
    let id = text_id();
    let mut changed = false;
    project.sort_outline_by_text();

    // Atajos de la barra.
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::E)) {
        state.open_scene(project);
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::N)) {
        state.open_entity(project);
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::C)) {
        state.tab_request = Some(1);
    }
    // Deshacer y rehacer pedidos desde la barra: el campo de texto los recibe como si fueran del teclado.
    if !state.keys.is_empty() {
        ctx.memory_mut(|m| m.request_focus(id));
        for (key, modifiers) in state.keys.drain(..) {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers,
                })
            });
        }
    }

    let mentions = project.mention_names();
    // El tratamiento muestra los nombres como están escritos: nada de mayúsculas.
    let case = CaseOptions::as_written();
    // Medidas reales: 120 píxeles por pulgada al 100 % de zoom y la letra en puntos (Arial/Liberation: 1,15 de interlínea).
    let (inch, size, line_h) = sheet_scale(crate::storage::TREATMENT_PT, settings.view_zoom);
    let font = FontId::new(size, theme::sans_family());
    let bold_font = FontId::new(size, theme::sans_bold_family());
    let (text_color, bold) = (p.text, settings.treatment_bold);
    let links_for_layout = project.treatment_links.clone();
    let len = project.treatment.chars().count();

    let focus_req = state.focus.take();
    if let Some((a, b)) = focus_req {
        set_selection(&ctx, a, b, len);
    }
    let has_focus = ctx.memory(|m| m.has_focus(id));
    let cursor: usize = TextEditState::load(&ctx, id)
        .and_then(|s| s.cursor.char_range())
        .map_or_else(|| state.cursor.unwrap_or(len), |r| r.primary.index.into());

    // Autocompletado: nombres en el texto o, mientras se escribe un encabezado de escena, INT./EXT., locación y momento.
    let draft_line = state
        .scene_draft
        .as_ref()
        .map(|d| (d.start, heading_end(&project.treatment, d.start)));
    let in_draft = draft_line.is_some();
    let sugg: Vec<Suggestion> = match draft_line {
        Some((a, b)) if has_focus && (a..=b).contains(&cursor) => {
            let locations: Vec<Mention> = mentions
                .iter()
                .filter(|m| matches!(m.target, Some(EntityRef::Location(_))))
                .cloned()
                .collect();
            scene_suggestions(
                &locations,
                &char_slice(&project.treatment, a, b).to_uppercase(),
                false,
            )
        }
        Some(_) => Vec::new(),
        None if has_focus && state.dismissed.as_deref() != Some(project.treatment.as_str()) => {
            mention_suggestions(
                &mentions,
                &project.treatment,
                cursor,
                &case,
                &project.treatment_links,
            )
        }
        None => Vec::new(),
    };
    let mut pick: Option<Suggestion> = None;
    let mut finish: Option<(bool, bool)> = None; // (crear, devolver el foco al texto)
    if has_focus {
        let key = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if in_draft && key(Key::Escape) {
            finish = Some((false, true));
        } else if sugg.is_empty() {
            key(Key::Tab); // sin sugerencia, Tab no mete una tabulación en el texto
            if in_draft && key(Key::Enter) {
                finish = Some((true, true));
            }
        } else if !in_draft && key(Key::Escape) {
            state.dismissed = Some(project.treatment.clone());
        } else if key(Key::ArrowDown) {
            state.suggest_idx = (state.suggest_idx + 1) % sugg.len();
        } else if key(Key::ArrowUp) {
            state.suggest_idx = (state.suggest_idx + sugg.len() - 1) % sugg.len();
        } else if key(Key::Tab)
            || (in_draft && sugg[state.suggest_idx.min(sugg.len() - 1)].enter && key(Key::Enter))
        {
            pick = Some(sugg[state.suggest_idx.min(sugg.len() - 1)].clone());
        } else if in_draft && key(Key::Enter) {
            finish = Some((true, true));
        }
    }
    if let Some(pick) = pick {
        let at = match draft_line {
            Some((a, b)) => {
                replace_range(project, a, b, &pick.text);
                a + pick.cursor
            }
            None => apply_suggestion(project, &pick),
        };
        state.focus = Some((at, at));
        changed = true;
    }

    // La hoja: 8,5 × 11 pulgadas como en el guion (1,5" a la izquierda, 1" a la derecha).
    let avail = ui.available_width();
    let sheet_dims = settings.paper.sheet();
    let nominal = sheet_dims.width * inch;
    let text_w = sheet_dims.text_w() * inch;
    let (margin_left, margin_right) = if avail >= nominal {
        (sheet_dims.left * inch, sheet_dims.right * inch)
    } else {
        let t = ((nominal - avail) / (2.0 * inch)).clamp(0.0, 1.0);
        let lerp = |from: f32| from + (0.3 * inch - from) * t;
        (lerp(sheet_dims.left * inch), lerp(sheet_dims.right * inch))
    };
    let sheet_w = margin_left + text_w + margin_right;
    let lines_per_page = ((sheet_dims.text_h() * inch) / line_h).floor().max(1.0);
    let page_h = lines_per_page * line_h;

    // Entre el último renglón de una hoja y el primero de la siguiente: margen de abajo, hueco entre hojas y margen de arriba.
    let margins_v = sheet_dims.margins_v() * inch;
    let page_skip = margins_v + SHEET_GAP;
    // Los encabezados de las escenas de la escaleta van en el color pleno de su escena.
    let heading_colors: Vec<(usize, usize, egui::Color32)> = project
        .outline
        .iter()
        .zip(project.outline_blocks())
        .map(|(scene, (from, _))| {
            (
                from,
                heading_end(&project.treatment, from),
                theme::character_color(&scene.id.to_string()),
            )
        })
        .collect();
    let layout = |ui: &Ui, text: &str, wrap: f32| -> std::sync::Arc<egui::Galley> {
        let spans = mention_spans(text, &links_for_layout, &mentions, &case);
        let mut job = styled_job(
            text,
            &font,
            &bold_font,
            text_color,
            wrap,
            line_h,
            &spans,
            bold,
            false,
            Align::LEFT,
        );
        recolor(&mut job, text, &heading_colors);
        spread_over_sheets(
            ui.fonts_mut(|f| f.layout_job(job)),
            lines_per_page as usize,
            page_skip,
        )
    };
    let rows = layout(ui, &project.treatment, text_w).rows.len().max(1);
    let pages = ((rows as f32 / lines_per_page).ceil() as usize).max(1);
    let text_h = pages as f32 * page_h + (pages - 1) as f32 * page_skip;
    let one_sheet_h = page_h + margins_v;
    let sheet_h = pages as f32 * one_sheet_h + (pages - 1) as f32 * SHEET_GAP;
    const TOP_PAD: f32 = 56.0;

    // Ctrl + rueda cambia el zoom de la vista, igual que en el guion.
    let zoom = ctx.input(|i| i.zoom_delta());
    if zoom != 1.0 && ui.rect_contains_pointer(ui.max_rect()) {
        state.zoom_acc += zoom - 1.0;
        if state.zoom_acc >= 0.06 {
            state.zoom_delta += 1;
            state.zoom_acc = 0.0;
        } else if state.zoom_acc <= -0.06 {
            state.zoom_delta -= 1;
            state.zoom_acc = 0.0;
        }
    }

    let before = project.treatment.clone();
    let content_w = avail.max(sheet_w);
    let mut scroll_target: Option<usize> = None;
    if focus_req.is_some() {
        scroll_target = focus_req.map(|(a, _)| a);
    }

    let out = egui::ScrollArea::both()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let (content, _) = ui.allocate_exact_size(
                egui::vec2(content_w, TOP_PAD + sheet_h + 120.0),
                egui::Sense::hover(),
            );
            let sheet = egui::Rect::from_min_size(
                egui::pos2(
                    content.min.x + (content_w - sheet_w) / 2.0,
                    content.min.y + TOP_PAD,
                ),
                egui::vec2(sheet_w, sheet_h),
            );
            let painter = ui.painter().clone();
            // Cada hoja es una página aparte, con su margen de arriba y de abajo y un hueco hasta la siguiente.
            for k in 0..pages {
                let top = sheet.min.y + k as f32 * (one_sheet_h + SHEET_GAP);
                let page = egui::Rect::from_min_size(
                    egui::pos2(sheet.min.x, top),
                    egui::vec2(sheet_w, one_sheet_h),
                );
                painter.rect(
                    page,
                    2,
                    p.panel,
                    Stroke::new(1.0, theme::mix(p.panel, p.border, 0.5)),
                    egui::StrokeKind::Inside,
                );
                if k > 0 {
                    painter.text(
                        egui::pos2(sheet.min.x + margin_left + text_w, top + inch * 0.5),
                        Align2::RIGHT_CENTER,
                        format!("{}.", k + 1),
                        FontId::proportional(size * 0.8),
                        p.dim,
                    );
                }
            }
            let text_rect = egui::Rect::from_min_size(
                egui::pos2(
                    sheet.min.x + margin_left,
                    sheet.min.y + sheet_dims.top * inch,
                ),
                egui::vec2(text_w, text_h),
            );
            let mut layouter =
                |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| layout(ui, buf.as_str(), wrap);
            let output = ui
                .scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
                    TextEdit::multiline(&mut project.treatment)
                        .id(id)
                        .layouter(&mut layouter)
                        .desired_width(text_w)
                        .min_size(egui::vec2(text_w, text_h))
                        .margin(egui::Margin::ZERO)
                        .frame(egui::Frame::NONE)
                        .lock_focus(true)
                        .hint_text(RichText::new("Escribí el tratamiento…").color(p.dim))
                        .show(ui)
                })
                .inner;

            // El clic que apretó el botón de la barra cuenta como «clic afuera» para el texto, que soltaría el foco recién
            // pedido: se vuelve a pedir.
            if focus_req.is_some() {
                ui.memory_mut(|m| m.request_focus(id));
            }

            // Lo que ya está en escenas de la escaleta, lo comentado y lo que se va a pasar a una escena, resaltado.
            let text = &project.treatment;
            // Cada escena abarca desde su encabezado hasta el siguiente: todo ese texto lleva el fondo tenue de su color, y el
            // encabezado va en el color pleno (ver `recolor`).
            for (scene, (from, to)) in project.outline.iter().zip(project.outline_blocks()) {
                let color = theme::character_color(&scene.id.to_string());
                highlight_range(
                    &painter,
                    &output.galley,
                    output.galley_pos,
                    from,
                    to,
                    color.linear_multiply(0.12),
                    None,
                );
            }
            for c in project
                .comments
                .iter()
                .filter(|c| c.in_treatment && !c.resolved)
            {
                if let Some((a, b)) = comment_range(text, c.quote.as_deref()) {
                    highlight_range(
                        &painter,
                        &output.galley,
                        output.galley_pos,
                        a,
                        b,
                        p.accent.linear_multiply(0.22),
                        None,
                    );
                }
            }
            // Número de cada escena de la escaleta, junto a su encabezado.
            let positions = project.outline_positions();
            let mut line_start = 0;
            for line in text.split('\n') {
                let n = line.chars().count();
                if let Some(si) = positions.iter().position(|pos| *pos == Some(line_start)) {
                    let top = output
                        .galley
                        .pos_from_cursor(CCursor::new(line_start))
                        .top()
                        + output.galley_pos.y;
                    painter.text(
                        egui::pos2(text_rect.min.x - 10.0, top),
                        Align2::RIGHT_TOP,
                        format!("{}.", si + 1),
                        bold_font.clone(),
                        p.accent,
                    );
                }
                line_start += n + 1;
            }
            if let Some(at) = scroll_target {
                let r = output
                    .galley
                    .pos_from_cursor(CCursor::new(at))
                    .translate(output.galley_pos.to_vec2());
                ui.scroll_to_rect(r, Some(Align::Center));
            }
            output
        })
        .inner;

    if out.response.changed() {
        changed = true;
        adjust_links(&mut project.treatment_links, &before, &project.treatment);
    }
    let focused_now = out.response.has_focus();
    if focused_now {
        let range = out
            .cursor_range
            .map(|r| (r.primary.index.into(), r.secondary.index.into()));
        state.cursor = range.map(|(a, _): (usize, usize)| a);
        state.sel =
            range.and_then(|(a, b): (usize, usize)| (a != b).then_some((a.min(b), a.max(b))));
    }

    // Al soltar el foco: se vinculan los nombres y lo que se escribió entre paréntesis pasa a la ficha.
    if state.was_focused && !focused_now {
        let links_before = project.treatment_links.clone();
        project.auto_link_treatment();
        changed |= links_before != project.treatment_links;
        changed |= project.learn_character_intros();
    }
    state.was_focused = focused_now;

    // Lista de sugerencias junto al cursor.
    // La lista se dibuja mientras el campo tenía el foco al empezar el cuadro: así sigue ahí al apretar sobre ella.
    if !sugg.is_empty() {
        state.suggest_idx = state.suggest_idx.min(sugg.len() - 1);
        let pos = out.galley_pos
            + out
                .galley
                .pos_from_cursor(CCursor::new(cursor))
                .left_bottom()
                .to_vec2();
        let mut clicked: Option<Suggestion> = None;
        egui::Area::new(Id::unique("treatment_suggestions"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.set_min_width(180.0);
                    for (n, s) in sugg.iter().enumerate() {
                        if widgets::suggestion_item(ui, n == state.suggest_idx, &s.label) {
                            clicked = Some(s.clone());
                        }
                    }
                });
            });
        if let Some(pick) = clicked {
            // Igual que con Tab: mientras se escribe un encabezado, la sugerencia reemplaza solo esa línea.
            let at = match draft_line {
                Some((a, b)) => {
                    replace_range(project, a, b, &pick.text);
                    a + pick.cursor
                }
                None => apply_suggestion(project, &pick),
            };
            state.focus = Some((at, at));
            changed = true;
        }
    } else {
        state.suggest_idx = 0;
    }

    // El encabezado de la escena se escribe en mayúsculas, y se cierra al salir de su línea.
    if let Some(d) = state.scene_draft.as_mut() {
        let end = heading_end(&project.treatment, d.start);
        if out.response.changed() {
            let line = char_slice(&project.treatment, d.start, end);
            let up = line.to_uppercase();
            if up != line && up.chars().count() == line.chars().count() {
                replace_range(project, d.start, end, &up);
            }
        }
        let here = focused_now
            && out
                .cursor_range
                .is_some_and(|r| (d.start..=end).contains(&r.primary.index.into()));
        d.seen_focus |= here;
        if finish.is_none() && d.seen_focus && !here {
            finish = Some((true, false));
        }
    }
    if let Some((create, refocus)) = finish {
        changed |= state.finish_scene(project, create, refocus);
    }

    // Si la edición cayó dentro de un encabezado de escena, la escena sigue a su encabezado.
    if state.prev_text != project.treatment {
        project.follow_heading_edits(&state.prev_text, &project.treatment.clone());
        state.prev_text = project.treatment.clone();
    }

    // Crear personajes, locaciones, objetos… sin salir del texto.
    if let Some(form) = create::dialog(&ctx, &mut state.entity, project, p) {
        if let Some(at) = commit_entity(project, state.cursor, &form) {
            state.focus = Some((at, at));
            changed = true;
        }
    }
    changed
}

/// Crea lo que pide el formulario y lo escribe en el texto: en lugar del texto seleccionado, si lo había, o en el
/// cursor. Devuelve dónde queda el cursor.
fn commit_entity(project: &mut Project, cursor: Option<usize>, form: &NewEntity) -> Option<usize> {
    let target = create_entity(project, form)?;
    let (at, typed) = match form.replace {
        Some((a, b)) => {
            let typed = char_slice(&project.treatment, a, b).trim().to_string();
            replace_range(project, a, b, "");
            (a, typed)
        }
        None => (cursor.unwrap_or(usize::MAX), "A".to_string()),
    };
    Some(insert_entity(project, at, target, &typed))
}

// ------------------------------------------------------------------------------------------------
// Panel derecho
// ------------------------------------------------------------------------------------------------

fn section_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(p.dim));
    ui.add_space(tokens::SP_S);
}

/// El texto de la última selección, si hay.
fn selected_quote(state: &TreatmentState, project: &Project) -> Option<String> {
    let (a, b) = state.sel?;
    let quote: String = project.treatment.chars().skip(a).take(b - a).collect();
    (!quote.trim().is_empty()).then_some(quote)
}

/// Panel derecho del tratamiento: formato, comentarios y escaleta. Devuelve si cambió el proyecto.
pub fn right_panel(
    ui: &mut Ui,
    state: &mut TreatmentState,
    project: &mut Project,
    p: &Palette,
    tab: usize,
) -> (bool, Option<crate::ui::screen::Nav>) {
    let tab = [Tab::Outline, Tab::Comments, Tab::Linked][tab.min(2)];
    let mut nav = None;
    state.tab = tab;

    let before = (project.comments.clone(), project.outline.clone());
    let text_before = project.treatment.clone();
    (|ui: &mut Ui| match tab {
        Tab::Comments => comments_tab(ui, state, project, p),
        Tab::Outline => outline_tab(ui, state, project, p),
        Tab::Linked => {
            let (found, create) =
                crate::ui::screen::linked_entities(ui, p, project, &project.treatment_links);
            nav = found;
            if create {
                state.open_entity(project);
            }
        }
    })(ui);
    (
        before != (project.comments.clone(), project.outline.clone())
            || text_before != project.treatment,
        nav,
    )
}

fn format_tab(ui: &mut Ui, state: &mut TreatmentState, p: &Palette, bold: bool) {
    section_label(ui, p, "NOMBRES");
    let mut on = bold;
    if ui.checkbox(&mut on, "Personajes en negrita").changed() {
        state.set_bold = Some(on);
    }

    ui.add_space(tokens::SP_L);
    section_label(ui, p, "ATAJOS");
    egui::Grid::new("treatment_shortcuts")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            for (key, what) in [
                ("Tab", "Acepta la sugerencia de nombre"),
                ("↑ ↓", "Elige entre las sugerencias"),
                ("Esc", "Descarta la sugerencia"),
                ("Ctrl+Mayús+N", "Crea un personaje, locación… y lo inserta"),
                ("Ctrl+Mayús+E", "Crea una escena con lo seleccionado"),
                ("Ctrl+Mayús+C", "Comenta lo seleccionado"),
                ("Ctrl+A", "Selecciona todo el texto"),
                ("Ctrl+Z / Y", "Deshacer / rehacer"),
            ] {
                ui.label(RichText::new(key).size(12.0).color(p.accent));
                ui.add(egui::Label::new(RichText::new(what).size(12.0).color(p.text)).wrap());
                ui.end_row();
            }
        });
}

fn comments_tab(ui: &mut Ui, state: &mut TreatmentState, project: &mut Project, p: &Palette) {
    let quote = selected_quote(state, project);
    section_label(ui, p, "NUEVO COMENTARIO");
    match &quote {
        Some(q) => {
            let shown: String = q.chars().take(60).collect();
            ui.label(
                RichText::new(format!("Sobre la selección: «{shown}»"))
                    .size(12.0)
                    .color(p.dim),
            );
        }
        None => {
            ui.label(
                RichText::new("Seleccioná un texto del tratamiento para comentarlo.")
                    .size(12.0)
                    .color(p.dim),
            );
        }
    }
    ui.add_space(tokens::SP_S);
    egui::Frame::new()
        .fill(p.field)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(4)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.add(
                TextEdit::multiline(&mut state.comment_draft)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE)
                    .hint_text("Escribí un comentario…"),
            );
        });
    ui.add_space(tokens::SP_S);
    ui.add_enabled_ui(
        quote.is_some() && !state.comment_draft.trim().is_empty(),
        |ui| {
            if ui.button("Añadir comentario").clicked() {
                project.add_treatment_comment(state.comment_draft.trim(), quote.clone());
                state.comment_draft.clear();
            }
        },
    );

    ui.add_space(tokens::SP_L);
    section_label(ui, p, "COMENTARIOS");
    let text = project.treatment.clone();
    let mut order: Vec<usize> = (0..project.comments.len())
        .filter(|&i| project.comments[i].in_treatment)
        .collect();
    order.sort_by_key(|&i| {
        project.comments[i]
            .quote
            .as_deref()
            .and_then(|q| comment_range(&text, Some(q)))
            .map_or(usize::MAX, |(a, _)| a)
    });
    if order.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::CHAT_TEXT,
            "Todavía no hay comentarios. Seleccioná texto y apretá Comentar.",
            None,
        );
    }
    let mut jump: Option<(usize, usize)> = None;
    let mut remove = None;
    for ci in order {
        let c = &mut project.comments[ci];
        let range = comment_range(&text, c.quote.as_deref());
        egui::Frame::new()
            .fill(p.container)
            .stroke(Stroke::new(1.0, theme::mix(p.container, p.border, 0.6)))
            .corner_radius(4)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let cited = c.quote.clone().unwrap_or_default();
                let resp = ui.add(
                    egui::Label::new(RichText::new(format!("«{cited}»")).size(12.0).color(p.dim))
                        .truncate()
                        .sense(egui::Sense::click()),
                );
                let body = if c.resolved {
                    RichText::new(&c.text).strikethrough().color(p.dim)
                } else {
                    RichText::new(&c.text)
                };
                let body_resp = ui.add(egui::Label::new(body).sense(egui::Sense::click()));
                // Un clic en el comentario lleva al texto y lo selecciona.
                if resp.clicked() || body_resp.clicked() {
                    jump = range;
                }
                crate::ui::kit::sides(
                    ui,
                    |_ui| {},
                    |ui| {
                        let tip = if c.resolved {
                            "Reabrir"
                        } else {
                            "Marcar como resuelto"
                        };
                        if widgets::icon_button(ui, p, icon::CHECK, tip, true).clicked() {
                            c.resolved = !c.resolved;
                        }
                        if widgets::icon_button(ui, p, icon::TRASH, "Eliminar", true).clicked() {
                            remove = Some(ci);
                        }
                    },
                );
            });
        ui.add_space(tokens::SP_S);
    }
    if let Some(range) = jump {
        state.focus = Some(range);
    }
    if let Some(ci) = remove {
        project.comments.remove(ci);
    }
}

/// Dónde empieza (en caracteres) la línea que es exactamente `heading`.
fn project_text_pos(text: &str, heading: &str) -> Option<(usize, usize)> {
    let mut start = 0;
    for line in text.split('\n') {
        let n = line.chars().count();
        if line == heading {
            return Some((start, start + n));
        }
        start += n + 1;
    }
    None
}

fn outline_tab(ui: &mut Ui, state: &mut TreatmentState, project: &mut Project, p: &Palette) {
    project.sort_outline_by_text();
    section_label(ui, p, "ESCENAS");
    if widgets::wide_button(
        ui,
        p,
        icon::FILM_SLATE,
        "Crear escena",
        ui.available_width(),
    )
    .clicked()
    {
        state.open_scene(project);
    }
    ui.add_space(tokens::SP_XS);
    ui.add_space(tokens::SP_M);
    if project.outline.is_empty() {
        kit::empty_state(
            ui,
            p,
            icon::FILM_SLATE,
            "Todavía no hay escenas en la escaleta. Seleccioná un fragmento del texto y apretá Crear escena.",
            None,
        );
        return;
    }
    let count = project.outline.len();
    let mut move_scene: Option<(usize, isize)> = None;
    let mut jump: Option<(usize, usize)> = None;
    let text = project.treatment.clone();
    for (si, scene) in project.outline.iter_mut().enumerate() {
        let color = theme::character_color(&scene.id.to_string());
        egui::Frame::new()
            .fill(p.container)
            .stroke(Stroke::new(1.0, theme::mix(p.container, p.border, 0.6)))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                // Cada fila ocupa solo el ancho disponible: el panel no cambia de tamaño al agregar escenas.
                let width = ui.available_width();
                ui.set_width(width);
                // El número y el encabezado son datos distintos: cada uno con su widget.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{}.", si + 1))
                            .font(FontId::new(tokens::FONT_BODY, theme::bold_family()))
                            .color(color),
                    );
                    ui.add(egui::Label::new(RichText::new(&scene.heading).strong()).truncate());
                });
                crate::ui::kit::sides(
                    ui,
                    |ui| {
                        if ui.small_button("Ir al texto").clicked() {
                            jump = project_text_pos(&text, &scene.heading);
                        }
                    },
                    |ui| {
                        if widgets::icon_button(ui, p, icon::DOWN_ARROW, "Bajar", si + 1 < count)
                            .clicked()
                        {
                            move_scene = Some((si, 1));
                        }
                        if widgets::icon_button(ui, p, icon::UP_ARROW, "Subir", si > 0).clicked() {
                            move_scene = Some((si, -1));
                        }
                    },
                );
            });
        ui.add_space(tokens::SP_S);
    }
    if let Some(range) = jump {
        state.focus = Some(range);
    }
    if let Some((si, d)) = move_scene {
        state.sel = None;
        project.move_outline_scene(si, d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create::Kind, model::Link};

    fn frame(
        ctx: &egui::Context,
        state: &mut TreatmentState,
        project: &mut Project,
        events: Vec<egui::Event>,
        t: f64,
    ) -> bool {
        let settings = Settings::default();
        let p = settings.scheme.palette();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 800.0),
            )),
            time: Some(t),
            focused: true,
            events,
            ..Default::default()
        };
        let mut changed = false;
        let mut out = ctx.run_ui(input, |ui| {
            changed = show(ui, state, project, &settings, &p)
        });
        out.textures_delta.clear();
        changed
    }

    #[test]
    fn accepting_a_new_character_opens_parentheses_with_the_sheet_data() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        let id = project.new_character("Ana");
        project.characters[0].age = "32".into();
        project.characters[0].one_liner = "bibliotecaria".into();
        let _ = id;
        let mut state = TreatmentState::default();
        project.treatment = "Entra Ana".into();
        let pick = Suggestion {
            label: "Ana".into(),
            text: "Entra Ana".into(),
            cursor: 9,
            stay: false,
            enter: false,
            link: Some(Link {
                start: 6,
                end: 9,
                target: EntityRef::Character(id),
            }),
        };
        let at = apply_suggestion(&mut project, &pick);
        assert_eq!(project.treatment, "Entra Ana (32 años, bibliotecaria)");
        assert_eq!(at, project.treatment.chars().count());
        // La segunda vez no se repite el paréntesis.
        project.treatment.push_str(" y luego ");
        frame(&ctx, &mut state, &mut project, vec![], 0.1);
    }

    #[test]
    fn an_empty_sheet_leaves_the_cursor_inside_the_parentheses() {
        let mut project = Project::default();
        let id = project.new_character("Ana");
        project.treatment = "Entra Ana".into();
        let pick = Suggestion {
            label: "Ana".into(),
            text: "Entra Ana".into(),
            cursor: 9,
            stay: false,
            enter: false,
            link: Some(Link {
                start: 6,
                end: 9,
                target: EntityRef::Character(id),
            }),
        };
        let at = apply_suggestion(&mut project, &pick);
        assert_eq!(project.treatment, "Entra Ana ()");
        assert_eq!(at, 11);
    }

    #[test]
    fn a_new_character_is_created_and_inserted_with_its_data() {
        let mut project = Project::default();
        project.treatment = "Entra ".into();
        let form = NewEntity {
            kind: Kind::Character,
            name: "Ana".into(),
            age: "32".into(),
            description: "bibliotecaria".into(),
            ..Default::default()
        };
        let target = create_entity(&mut project, &form).unwrap();
        let at = insert_entity(&mut project, 6, target, "A");
        assert_eq!(project.treatment, "Entra Ana (32 años, bibliotecaria)");
        assert_eq!(at, project.treatment.chars().count());
        assert_eq!(project.treatment_links.len(), 1);
        // Una locación se inserta pegada a la palabra anterior con un espacio.
        let place = create_entity(
            &mut project,
            &NewEntity {
                kind: Kind::Location,
                name: "Faro".into(),
                ..Default::default()
            },
        )
        .unwrap();
        project.treatment = "Va al".into();
        project.treatment_links.clear();
        insert_entity(&mut project, 5, place, "A");
        assert_eq!(project.treatment, "Va al Faro");
    }

    #[test]
    fn the_toolbar_undo_reverts_typing_and_the_scene_field_creates_a_scene() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        let mut state = TreatmentState::default();
        state.focus = Some((0, 0));
        frame(&ctx, &mut state, &mut project, vec![], 0.1);
        frame(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::Text("Ana llega al faro".into())],
            0.2,
        );
        frame(&ctx, &mut state, &mut project, vec![], 2.0);
        assert_eq!(project.treatment, "Ana llega al faro");

        // Seleccionar «llega al faro» y crear la escena: el encabezado se escribe en el texto, arriba.
        state.sel = Some((4, 17));
        state.open_scene(&mut project);
        assert_eq!(project.treatment, "Ana\n\nllega al faro");
        frame(&ctx, &mut state, &mut project, vec![], 2.1);
        frame(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::Text("int. faro - noche".into())],
            2.2,
        );
        assert_eq!(project.treatment, "Ana\nINT. FARO - NOCHE\nllega al faro");
        let enter = egui::Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut state, &mut project, vec![enter], 2.3);
        frame(&ctx, &mut state, &mut project, vec![], 2.4);
        assert_eq!(project.outline.len(), 1);
        assert_eq!(project.outline[0].heading, "INT. FARO - NOCHE");
        assert!(state.scene_draft.is_none());
        // La locación del encabezado se creó y quedó vinculada.
        assert_eq!(project.location_profiles.len(), 1);
        assert_eq!(project.location_profiles[0].name, "FARO");
        assert_eq!(project.treatment_links.len(), 1);

        // Escape cancela y quita la línea.
        state.sel = None;
        state.cursor = Some(0);
        state.open_scene(&mut project);
        let esc = egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut state, &mut project, vec![], 2.5);
        frame(&ctx, &mut state, &mut project, vec![esc], 2.6);
        assert_eq!(project.treatment, "Ana\nINT. FARO - NOCHE\nllega al faro");
        assert_eq!(project.outline.len(), 1);

        // Deshacer desde la barra.
        state.keys.push((Key::Z, Modifiers::COMMAND));
        frame(&ctx, &mut state, &mut project, vec![], 5.0);
        frame(&ctx, &mut state, &mut project, vec![], 5.1);
        assert_ne!(project.treatment, "Ana\nINT. FARO - NOCHE\nllega al faro");
    }

    #[test]
    fn a_selected_word_becomes_a_location() {
        let mut project = Project::default();
        project.treatment = "Ana entra en el baño y se mira.".into();
        let mut state = TreatmentState::default();
        state.sel = Some((16, 20));
        state.open_entity(&project);
        let mut form = state.entity.take().unwrap();
        assert_eq!(form.name, "baño");
        assert_eq!(form.replace, Some((16, 20)));
        form.kind = Kind::Location;
        let at = commit_entity(&mut project, None, &form).unwrap();
        assert_eq!(project.treatment, "Ana entra en el baño y se mira.");
        assert_eq!(at, 20);
        assert_eq!(project.location_profiles[0].name, "BAÑO");
        assert_eq!(project.treatment_links.len(), 1);
        assert_eq!(
            (
                project.treatment_links[0].start,
                project.treatment_links[0].end
            ),
            (16, 20)
        );
    }

    #[test]
    fn a_suggestion_can_be_chosen_with_a_click() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.new_character("Juan");
        project.treatment = "Entra Jua".into();
        let mut state = TreatmentState::default();
        state.focus = Some((9, 9));
        for n in 0..4 {
            frame(&ctx, &mut state, &mut project, vec![], 0.1 * n as f64);
        }
        let list = ctx
            .memory(|m| m.area_rect(Id::unique("treatment_suggestions")))
            .expect("se ofrece «Juan»");
        let at = list.min + egui::vec2(20.0, 12.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::PointerMoved(at)],
            1.0,
        );
        frame(&ctx, &mut state, &mut project, vec![button(true)], 1.1);
        frame(&ctx, &mut state, &mut project, vec![button(false)], 1.2);
        frame(&ctx, &mut state, &mut project, vec![], 1.3);
        assert!(
            project.treatment.starts_with("Entra Juan"),
            "{:?}",
            project.treatment
        );
    }

    #[test]
    fn rows_jump_the_margins_and_the_gap_between_sheets() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut checked = false;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let text = (0..25)
                .map(|n| format!("renglón {n}"))
                .collect::<Vec<_>>()
                .join("\n");
            let plain = ui.painter().layout(
                text,
                FontId::proportional(14.0),
                egui::Color32::WHITE,
                400.0,
            );
            let spread = spread_over_sheets(plain.clone(), 10, 50.0);
            let y = |g: &egui::Galley, n: usize| g.rows[n].pos.y;
            assert_eq!(y(&spread, 9), y(&plain, 9), "la primera hoja queda igual");
            assert!(
                (y(&spread, 10) - y(&plain, 10) - 50.0).abs() < 1e-3,
                "la segunda baja lo que pide el salto"
            );
            assert!(
                (y(&spread, 24) - y(&plain, 24) - 100.0).abs() < 1e-3,
                "la tercera, dos saltos"
            );
            assert!((spread.rect.height() - plain.rect.height() - 100.0).abs() < 1e-3);
            // Lo que cabe en una sola hoja no se toca.
            assert_eq!(spread_over_sheets(plain.clone(), 30, 50.0).rect, plain.rect);
            checked = true;
        });
        out.textures_delta.clear();
        assert!(checked);
    }

    #[test]
    fn ctrl_wheel_changes_the_text_size_and_long_text_draws_on_separate_sheets() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.treatment = (0..300)
            .map(|n| format!("Párrafo {n} de prueba."))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut state = TreatmentState::default();
        frame(&ctx, &mut state, &mut project, vec![], 0.1);
        frame(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::PointerMoved(egui::pos2(500.0, 400.0))],
            0.2,
        );
        for n in 0..4 {
            frame(
                &ctx,
                &mut state,
                &mut project,
                vec![egui::Event::Zoom(1.08)],
                0.3 + n as f64 * 0.1,
            );
        }
        assert!(state.zoom_delta > 0, "{}", state.zoom_delta);
    }

    #[test]
    fn the_view_zoom_scales_the_sheet_and_the_letters_alike_without_changing_how_much_fits() {
        let lines = |zoom: u32, paper: crate::storage::Paper| {
            let (inch, _, line_h) = sheet_scale(crate::storage::TREATMENT_PT, zoom);
            ((paper.sheet().text_h() * inch) / line_h).floor() as u32
        };
        // Arial 11: 51 renglones en Carta y 55 en A4, a cualquier zoom.
        for zoom in [50, 100, 250] {
            assert_eq!(lines(zoom, crate::storage::Paper::Letter), 51);
            assert_eq!(lines(zoom, crate::storage::Paper::A4), 55);
        }
        let (a, b) = (sheet_scale(11, 100), sheet_scale(11, 200));
        assert!((b.0 / a.0 - 2.0).abs() < 1e-4 && (b.1 / a.1 - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_scene_heading_is_painted_in_the_scene_color_and_the_rest_keeps_its_own() {
        let text = "Intro\nINT. CASA - DÍA\nElla entra.";
        let mut job = egui::text::LayoutJob::single_section(
            text.to_string(),
            egui::TextFormat {
                color: egui::Color32::WHITE,
                ..Default::default()
            },
        );
        let red = egui::Color32::RED;
        recolor(&mut job, text, &[(6, 21, red)]);
        let painted: Vec<(&str, egui::Color32)> = job
            .sections
            .iter()
            .map(|s| {
                (
                    &text[s.byte_range.start.0..s.byte_range.end.0],
                    s.format.color,
                )
            })
            .collect();
        assert_eq!(
            painted,
            [
                ("Intro\n", egui::Color32::WHITE),
                ("INT. CASA - DÍA", red),
                ("\nElla entra.", egui::Color32::WHITE),
            ]
        );
    }

    #[test]
    fn choosing_a_heading_suggestion_with_the_mouse_replaces_only_the_heading_line() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut project = Project::default();
        project.treatment = "INT. FARO - DÍA\nAna llega al faro".into();
        project.add_outline_scene("INT. FARO - DÍA");
        let mut state = TreatmentState::default();
        state.cursor = Some(project.treatment.chars().count());
        frame(&ctx, &mut state, &mut project, vec![], 0.1);
        let before = project.treatment.clone();
        state.open_scene(&mut project);
        frame(&ctx, &mut state, &mut project, vec![], 0.2);
        frame(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::Text("i".into())],
            0.3,
        );
        frame(&ctx, &mut state, &mut project, vec![], 0.4);
        let list = ctx
            .memory(|m| m.area_rect(Id::unique("treatment_suggestions")))
            .expect("hay sugerencias");
        let at = list.min + egui::vec2(20.0, 12.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut state,
            &mut project,
            vec![egui::Event::PointerMoved(at)],
            1.0,
        );
        frame(&ctx, &mut state, &mut project, vec![button(true)], 1.1);
        frame(&ctx, &mut state, &mut project, vec![button(false)], 1.2);
        frame(&ctx, &mut state, &mut project, vec![], 1.3);
        assert!(
            project.treatment.contains("Ana llega al faro"),
            "el texto sigue: {:?}",
            project.treatment
        );
        assert!(
            project.treatment.starts_with(&before),
            "{:?}",
            project.treatment
        );
        assert!(
            project.treatment.contains("\nINT./EXT."),
            "{:?}",
            project.treatment
        );
        assert_eq!(project.outline.len(), 1, "las escenas no se reinician");
    }

    #[test]
    fn the_drama_curve_is_imported_into_the_treatment_at_the_cursor_in_story_order() {
        let mut project = Project::default();
        project.treatment = "Antes.\nDespués.".into();
        project.ensure_drama();
        let climax = project.drama.iter().position(|d| d.kind == crate::model::DramaKind::Climax).unwrap();
        project.drama[climax].text = "Ana enfrenta al faro.".into();
        let mut state = TreatmentState::default();
        state.cursor = Some(7);
        assert!(state.import_drama(&mut project));
        let t = &project.treatment;
        assert!(t.starts_with("Antes.\nCURVA DRAMÁTICA\n"), "{t}");
        assert!(t.ends_with("Después."), "{t}");
        assert!(t.contains("Clímax\nAna enfrenta al faro."), "{t}");
        let (inicio, climax_at) = (t.find("1. Inicio").unwrap(), t.find("Clímax").unwrap());
        assert!(inicio < climax_at, "en el orden de la historia");
    }
}
