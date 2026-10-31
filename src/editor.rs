//! Editor del guion: una hoja con las medidas de un guion estándar (Courier a 10 caracteres por
//! pulgada, márgenes de 1,5" / 1") donde cada párrafo tiene su tipo y su sangría.

use crate::ui::tokens;
use std::sync::Arc;

use eframe::egui::{
    self, Align, Color32, CursorIcon, FontId, Id, Key, Modifiers, RichText, Stroke, TextEdit, Ui,
    text::{CCursor, CCursorRange, CharIndex, LayoutJob, TextFormat},
    text_edit::TextEditState,
};

use crate::{
    icon,
    model::{
        CaseOptions, EntityRef, Link, Mention, MentionSpan, Paragraph, ParagraphKind, Project,
        adjust_links, apply_case, clean_character_name, mention_spans, shifted_links, split_links,
        validate_links,
    },
    storage::Settings,
    theme::{self, Palette},
    widgets,
};

#[derive(Clone, Copy)]
enum Pos {
    Start,
    End,
    At(usize),
    /// Selecciona el rango `(inicio, fin)` en caracteres.
    Sel(usize, usize),
}

/// Cambios estructurales detectados durante el frame; se aplican al terminar de dibujar.
enum Edit {
    /// Parte el párrafo `i` en el carácter `at` (Enter).
    Split {
        i: usize,
        at: usize,
    },
    /// Inserta un párrafo vacío después de `i` (Tab sobre un personaje con nombre).
    InsertAfter {
        i: usize,
        kind: ParagraphKind,
    },
    SetKind {
        i: usize,
        kind: ParagraphKind,
    },
    /// Backspace al inicio: une el párrafo con el anterior.
    MergeWithPrev {
        i: usize,
    },
    /// Delete al final: absorbe el párrafo siguiente.
    MergeWithNext {
        i: usize,
    },
    Goto {
        i: usize,
        pos: Pos,
    },
    /// Mayús + flecha en el borde de un párrafo: la selección sigue en el vecino (pasa a ser de varios párrafos).
    Extend {
        anchor: (u64, usize),
        head: (u64, usize),
    },
}

impl Edit {
    /// ¿Cambia el texto del guion? (Mover el cursor o la selección no entra en el historial de deshacer.)
    fn changes_text(&self) -> bool {
        !matches!(self, Edit::Goto { .. } | Edit::Extend { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RightTab {
    /// El navegador del guion.
    #[default]
    Scenes,
    /// La ficha de producción de la escena del cursor y sus marcas de desglose.
    Card,
    Comments,
}

#[derive(Default)]
pub struct EditorState {
    focus: Option<(u64, Pos)>,
    scroll_to: Option<u64>,
    /// Párrafo con el cursor y su fila actual / total de filas (para saltar entre párrafos con ↑↓).
    caret: Option<(u64, usize, usize)>,
    suggest_idx: usize,
    pub right_tab: RightTab,
    /// Pestaña del inspector que se pidió abrir (0 Escenas, 1 Ficha, 2 Comentarios).
    pub tab_request: Option<usize>,
    /// La escena actual compartida la última vez que se sincronizó con el cursor.
    pub shared_scene: Option<u64>,
    /// Lo escrito en la ficha de la escena (pestaña FICHA) mientras no se confirma.
    scene_drafts: std::collections::HashMap<(u64, u8), String>,
    /// Enlace de la ficha de la escena que la app tiene que atender (planta, desglose o planos).
    pub scene_link: Option<(crate::scenes::SceneLink, u64)>,
    /// Cambio pedido al zoom de la vista, en pasos (Ctrl + rueda o los botones del panel); lo aplica la app.
    pub zoom_delta: i32,
    /// Se bloqueó o desbloqueó la numeración de escenas; el proyecto cambió.
    numbering_changed: bool,
    /// Nombre escrito para la próxima revisión.
    /// Cambio pedido a «nombres en negrita» desde el panel Formato; lo aplica la app.
    pub set_bold: Option<bool>,
    /// Cambio pedido a «mostrar la línea de tiempo»; lo aplica la app.
    pub set_timeline: Option<bool>,
    /// Desglose: categoría elegida, nombre del elemento y última selección vista.
    bd_category: Option<u64>,
    bd_name: String,
    bd_quote: String,
    bd_all: bool,
    zoom_acc: f32,
    /// Última selección en un párrafo: `(id, inicio, fin)` en caracteres. Es sobre lo que se comenta.
    sel: Option<(u64, usize, usize)>,
    /// Formulario abierto para crear (o convertir la selección en) un personaje, locación, objeto…
    entity: Option<crate::create::NewEntity>,
    /// Selección de varios párrafos (Ctrl+A o arrastrando el mouse de uno a otro).
    multi: Option<MultiSel>,
    /// Dónde quedó cada párrafo en el último cuadro: sirve para saber qué hay bajo el mouse y para mover la selección.
    last_geoms: Vec<Geom>,
    /// Dónde se dibujó cada párrafo en el último cuadro (para simular el mouse en las pruebas).
    #[cfg(test)]
    rects: Vec<(u64, egui::Rect)>,
    /// Arrastre en curso que empezó dentro de un párrafo: `(id, carácter)` donde se apretó el mouse.
    drag: Option<(u64, usize)>,
    /// Párrafo que tenía el foco en el frame anterior: al perderlo se vinculan sus nombres.
    prev_focus: Option<u64>,
    /// Sugerencia descartada con Esc para ese texto: no vuelve a aparecer hasta que cambie.
    dismissed: Option<(u64, String)>,
    comment_draft: String,
    undo: Vec<Vec<Paragraph>>,
    redo: Vec<Vec<Paragraph>>,
    /// Estado del guion tras el último cambio (es el «antes» del próximo).
    snapshot: Option<Vec<Paragraph>>,
    last_edit: f64,
}

const MAX_UNDO: usize = 300;
/// Pausa que separa dos grupos de tipeo en el historial de deshacer.
const GROUP_GAP: f64 = 0.8;

impl EditorState {
    /// Va a un párrafo y selecciona el texto comentado, si lo hay.
    pub fn jump_to_selection(&mut self, project: &Project, id: u64, quote: Option<&str>) {
        self.scroll_to = Some(id);
        let range = project
            .paragraphs
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| comment_range(&p.text, quote).filter(|_| quote.is_some()));
        self.focus = Some((id, range.map_or(Pos::End, |(a, b)| Pos::Sel(a, b))));
    }

    pub fn jump_to(&mut self, id: u64) {
        self.scroll_to = Some(id);
        self.focus = Some((id, Pos::End));
    }

    pub fn current_id(&self) -> Option<u64> {
        self.caret.map(|(id, ..)| id)
    }

    /// Abre el formulario para crear algo del proyecto. Con un texto seleccionado dentro de un párrafo, ese texto
    /// es el nombre y se convierte en lo que se cree.
    pub fn open_entity(&mut self, project: &Project) {
        let selection = self.sel.and_then(|(pid, a, b)| {
            let text = &project.paragraphs.iter().find(|q| q.id == pid)?.text;
            let quote: String = text.chars().skip(a).take(b.saturating_sub(a)).collect();
            (!quote.trim().is_empty()).then_some((pid, a, b, quote))
        });
        let mut form = crate::create::NewEntity {
            fresh: true,
            ..Default::default()
        };
        if let Some((pid, a, b, quote)) = selection {
            form.name = quote.trim().to_string();
            form.replace = Some((a, b));
            form.para = Some(pid);
        }
        self.entity = Some(form);
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo
            .push(std::mem::replace(&mut project.paragraphs, prev));
        self.restore(project);
        true
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo
            .push(std::mem::replace(&mut project.paragraphs, next));
        self.restore(project);
        true
    }

    /// Tras volver a un estado del historial se reescriben los nombres vinculados con los actuales, así un
    /// personaje renombrado conserva su nombre nuevo aunque el estado guardado tuviera el viejo.
    fn restore(&mut self, project: &mut Project) {
        project.sync_links();
        self.snapshot = Some(project.paragraphs.clone());
        let keep = self
            .current_id()
            .filter(|id| project.paragraphs.iter().any(|p| p.id == *id))
            .or_else(|| project.paragraphs.first().map(|p| p.id));
        if let Some(id) = keep {
            self.focus = Some((id, Pos::End));
        }
    }

    /// Tras un cambio hecho fuera del editor (por ejemplo, renombrar un personaje) la copia «actual» pasa a ser el
    /// guion tal como quedó; el historial anterior sigue válido porque se sincroniza al volver a él.
    pub fn resync_snapshot(&mut self, project: &Project) {
        self.snapshot = Some(project.paragraphs.clone());
    }

    /// Los pasos viejos del historial no tienen los vínculos que se crearon después (al soltar el foco, o al añadir
    /// algo al proyecto). Se les pasan, para que al deshacer sigan el renombrado en vez de volver con el nombre viejo.
    pub fn adopt_links(&mut self, project: &Project) {
        let states = self
            .undo
            .iter_mut()
            .chain(self.redo.iter_mut())
            .chain(self.snapshot.iter_mut());
        for state in states {
            for q in state.iter_mut().filter(|q| q.links.is_empty()) {
                if let Some(now) = project
                    .paragraphs
                    .iter()
                    .find(|n| n.id == q.id && n.text == q.text && !n.links.is_empty())
                {
                    q.links = now.links.clone();
                }
            }
        }
    }

    /// Olvida el historial: se usa cuando el guion cambia desde fuera del editor y no se puede conciliar
    /// (por ejemplo, al eliminar un personaje con sus intervenciones).
    pub fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.snapshot = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Registra que el guion cambió. Los cambios de tipeo seguidos se agrupan en un solo paso.
    fn commit(&mut self, project: &Project, structural: bool, now: f64) {
        let before = self.snapshot.take().unwrap_or_default();
        if structural || now - self.last_edit > GROUP_GAP {
            self.undo.push(before);
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.snapshot = Some(project.paragraphs.clone());
        self.last_edit = now;
    }
}

fn byte_index(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(b, _)| b)
}

/// Medidas de la hoja a partir del tamaño de letra. Courier mide 0,6 em por carácter, así que
/// 10 caracteres son una pulgada.
#[derive(Clone, Copy)]
struct Metrics {
    size: f32,
    inch: f32,
    char_w: f32,
    /// La hoja (papel y márgenes, en pulgadas).
    page: crate::storage::Sheet,
}

impl Metrics {
    /// `pt` es el tamaño de la letra en la hoja y `zoom` el de la vista (en %): una pulgada mide 120 píxeles al 100 %.
    fn new(pt: f32, zoom: u32, paper: crate::storage::Paper) -> Self {
        let inch = 120.0 * zoom as f32 / 100.0;
        let size = pt * inch / 72.0;
        Self {
            size,
            inch,
            char_w: size * 0.6,
            page: paper.sheet(),
        }
    }

    /// Sangría desde el margen izquierdo del texto y ancho, según el tipo de párrafo.
    fn geometry(self, kind: ParagraphKind, text: &str) -> (f32, f32) {
        let i = self.inch;
        match kind {
            ParagraphKind::SceneHeading
            | ParagraphKind::Action
            | ParagraphKind::Note
            | ParagraphKind::Act
            | ParagraphKind::Sequence => (0.0, self.page.text_w() * i),
            // El borde derecho de una transición está en 7,1 in de la hoja: 5,6 in desde el margen de texto.
            ParagraphKind::Transition => (0.0, (5.6 * i).min(self.page.text_w() * i)),
            // El personaje va de 3,7 in a 7,5 in de la hoja, sin pasarse del margen derecho.
            ParagraphKind::Character => (2.2 * i, (3.8 * i).min((self.page.text_w() - 2.2) * i)),
            ParagraphKind::Dialogue | ParagraphKind::Lyrics => (1.0 * i, 3.5 * i),
            ParagraphKind::Parenthetical => {
                // El ancho se ajusta al contenido: el paréntesis de cierre queda pegado al texto.
                let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                let cols = longest.clamp(1, 22) + 1;
                (1.6 * i + self.char_w, cols as f32 * self.char_w)
            }
        }
    }

    /// Espacio en blanco antes del párrafo: una línea, dos antes de un encabezado, un acto o una secuencia, y ninguna
    /// dentro de un diálogo ni entre los renglones de una letra.
    fn gap_before(self, kind: ParagraphKind, prev: Option<ParagraphKind>) -> f32 {
        match kind {
            ParagraphKind::Parenthetical | ParagraphKind::Dialogue => 0.0,
            ParagraphKind::Lyrics if prev == Some(ParagraphKind::Lyrics) => 0.0,
            ParagraphKind::SceneHeading | ParagraphKind::Act | ParagraphKind::Sequence => {
                2.0 * self.size
            }
            _ => self.size,
        }
    }
}

/// Selección que cruza párrafos: desde `anchor` hasta `head` (id de párrafo, carácter). El `TextEdit` de cada
/// párrafo solo sabe seleccionar dentro del suyo, así que esta selección la lleva el editor.
#[derive(Clone, Copy, Debug)]
struct MultiSel {
    anchor: (u64, usize),
    head: (u64, usize),
}

/// Dónde quedó dibujado un párrafo, para saber sobre cuál está el mouse.
struct Geom {
    pid: u64,
    rect: egui::Rect,
    galley: Arc<egui::Galley>,
    pos: egui::Pos2,
    len: usize,
}

/// Párrafo y carácter bajo el mouse. Por encima del primero es el comienzo del guion; entre dos párrafos, el final
/// del de arriba; por debajo del último, su final.
fn hit(geoms: &[Geom], pos: egui::Pos2) -> Option<(usize, usize)> {
    for (i, g) in geoms.iter().enumerate() {
        if pos.y < g.rect.top() {
            return Some(if i == 0 {
                (0, 0)
            } else {
                (i - 1, geoms[i - 1].len)
            });
        }
        if pos.y <= g.rect.bottom() {
            let at: usize = g.galley.cursor_from_pos(pos - g.pos).index.into();
            return Some((i, at.min(g.len)));
        }
    }
    geoms
        .len()
        .checked_sub(1)
        .map(|last| (last, geoms[last].len))
}

/// Un párrafo a repartir entre las hojas.
#[derive(Clone, Debug, Default)]
pub struct PageItem {
    pub height: f32,
    /// Espacio en blanco que lleva antes (no se pone al comienzo de una hoja).
    pub gap: f32,
    /// No puede quedar solo al pie de una hoja: encabezados de escena, personajes y acotaciones van con lo que sigue.
    pub keep_with_next: bool,
    /// Alto de un renglón, si el párrafo se puede partir entre dos hojas (0 = no se parte).
    pub row_h: f32,
    /// Un diálogo que se parte lleva `(MÁS)` en el último renglón de la hoja y el nombre con `(CONT.)` en el primero
    /// de la siguiente: cada corte gasta un renglón en cada hoja.
    pub dialogue: bool,
    /// Qué renglones terminan una oración (si se sabe): el corte prefiere caer después de uno.
    pub sentence_ends: Vec<bool>,
}

/// Dónde cae un párrafo: la hoja y la distancia desde el borde superior del área de texto de esa hoja.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub page: usize,
    pub y: f32,
    /// Si el párrafo se parte entre hojas: cuántos renglones quedan en esta; el resto sigue en las hojas siguientes.
    pub cut: Option<usize>,
}

/// Cuántos renglones de un párrafo largo quedan en la hoja si empieza a `start` de su borde superior, o `None` si
/// entra entero o no conviene partirlo (que no quede un solo renglón suelto arriba ni abajo). Si puede, el corte cae al
/// final de una oración.
fn split_fit(item: &PageItem, start: f32, page_height: f32) -> Option<usize> {
    if item.row_h <= 0.0 || start + item.height <= page_height + 0.01 {
        return None;
    }
    let rows = (item.height / item.row_h).round() as usize;
    let reserved = usize::from(item.dialogue);
    let fit =
        (((page_height - start) / item.row_h).floor().max(0.0) as usize).saturating_sub(reserved);
    if fit < 2 || rows < fit + 2 {
        return None;
    }
    // Hasta cinco renglones de hoja sin llenar valen un corte al final de una oración.
    let sentence = (fit.saturating_sub(5).max(2)..=fit)
        .rev()
        .find(|&c| rows >= c + 2 && item.sentence_ends.get(c - 1) == Some(&true));
    Some(sentence.unwrap_or(fit))
}

/// Cuántos renglones del párrafo van en cada hoja cuando se parte en `cut` en la primera: en las siguientes, un
/// diálogo gasta un renglón arriba (el nombre con `(CONT.)`) y, si todavía sigue, otro abajo (`(MÁS)`).
pub(crate) fn split_plan(rows: usize, cut: usize, per_page: usize, dialogue: bool) -> Vec<usize> {
    let (top, bottom) = if dialogue { (1, 1) } else { (0, 0) };
    let mut plan = vec![cut];
    let mut rest = rows.saturating_sub(cut);
    loop {
        if rest + top <= per_page || per_page <= top + bottom + 1 {
            plan.push(rest);
            return plan;
        }
        let take = per_page - top - bottom;
        plan.push(take);
        rest -= take;
    }
}

/// Reparte los párrafos entre hojas cuya área de texto mide `page_height`. Un párrafo pasa a la hoja siguiente si no
/// entra en lo que queda; los que deben ir con el siguiente se llevan consigo a esa cadena entera. Un párrafo largo
/// (con `row_h`) se parte entre hojas por renglones en vez de dejar la hoja a medio llenar.
pub fn paginate(items: &[PageItem], page_height: f32) -> Vec<Placement> {
    let mut out = Vec::with_capacity(items.len());
    let (mut page, mut y) = (0usize, 0.0f32);
    for (i, item) in items.iter().enumerate() {
        let mut gap = if y == 0.0 { 0.0 } else { item.gap };
        // Lo que ocupa este párrafo junto con los que tienen que ir pegados a él.
        let mut need = gap + item.height;
        let mut j = i;
        while items[j].keep_with_next && j + 1 < items.len() {
            j += 1;
            need += items[j].gap + items[j].height;
        }
        let fits_whole_chain = need <= page_height;
        let chain = if fits_whole_chain {
            need
        } else {
            gap + item.height
        };
        if y > 0.0 && y + chain > page_height && split_fit(item, y + gap, page_height).is_none() {
            page += 1;
            y = 0.0;
            gap = 0.0;
        }
        let start = y + gap;
        match split_fit(item, start, page_height) {
            Some(fit) => {
                out.push(Placement {
                    page,
                    y: start,
                    cut: Some(fit),
                });
                let rows = (item.height / item.row_h).round() as usize;
                let per_page = ((page_height / item.row_h).floor() as usize).max(1);
                let plan = split_plan(rows, fit, per_page, item.dialogue);
                page += plan.len() - 1;
                y = (plan[plan.len() - 1] + usize::from(item.dialogue)) as f32 * item.row_h;
            }
            None => {
                out.push(Placement {
                    page,
                    y: start,
                    cut: None,
                });
                y = start + item.height;
            }
        }
    }
    out
}

/// Cuántas hojas ocupa todo: la última, contando las que ocupa el final de un párrafo partido.
fn sheet_count(items: &[PageItem], placements: &[Placement], page_height: f32) -> usize {
    let (Some(last), Some(item)) = (placements.last(), items.last()) else {
        return 1;
    };
    let more = match last.cut {
        Some(cut) => {
            let rows = (item.height / item.row_h).round() as usize;
            let per_page = ((page_height / item.row_h).floor() as usize).max(1);
            split_plan(rows, cut, per_page, item.dialogue).len() - 1
        }
        None => 0,
    };
    last.page + more + 1
}

/// Parte un párrafo entre hojas: los renglones desde `cut` pasan a la hoja siguiente (y a las que hagan falta), con los
/// márgenes y el hueco entre hojas (`skip`) de por medio. `y0` es dónde empieza el párrafo en su hoja.
fn split_galley(
    galley: Arc<egui::Galley>,
    cut: usize,
    y0: f32,
    page_height: f32,
    skip: f32,
    row_h: f32,
    dialogue: bool,
) -> Arc<egui::Galley> {
    if galley.rows.len() <= cut || row_h <= 0.0 {
        return galley;
    }
    let per_page = ((page_height / row_h).floor() as usize).max(1);
    let plan = split_plan(galley.rows.len(), cut, per_page, dialogue);
    let top = usize::from(dialogue);
    let mut galley = Arc::unwrap_or_clone(galley);
    let mut r = cut;
    for (k, rows_here) in plan.iter().enumerate().skip(1) {
        for n in 0..*rows_here {
            if let Some(row) = galley.rows.get_mut(r) {
                row.pos.y = k as f32 * (page_height + skip) + (top + n) as f32 * row_h - y0;
            }
            r += 1;
        }
    }
    let bottom = galley.rows.last().map_or(0.0, |r| r.pos.y + row_h);
    galley.rect.max.y = galley.rect.max.y.max(bottom);
    galley.mesh_bounds.max.y = galley.mesh_bounds.max.y.max(bottom);
    Arc::new(galley)
}

/// ¿Termina el renglón una oración?
pub(crate) fn ends_sentence(row: &str) -> bool {
    let trimmed = row.trim_end().trim_end_matches(['"', '»', '”', ')', '\'']);
    trimmed.ends_with(['.', '?', '!', '…', ':'])
}

/// Pide el foco para `id` y coloca el cursor.
fn apply_focus(ctx: &egui::Context, id: Id, text: &str, pos: Pos) {
    let n = text.chars().count();
    let range = match pos {
        Pos::Start => CCursorRange::one(CCursor::new(0)),
        Pos::End => CCursorRange::one(CCursor::new(n)),
        Pos::At(c) => CCursorRange::one(CCursor::new(c.min(n))),
        Pos::Sel(a, b) => CCursorRange::two(CCursor::new(a.min(n)), CCursor::new(b.min(n))),
    };
    ctx.memory_mut(|m| m.request_focus(id));
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(range));
    state.store(ctx, id);
}

/// Una opción del autocompletado: `text` reemplaza al párrafo entero. Si `stay` es falso, al aceptarla con
/// Enter se pasa al párrafo siguiente (un personaje sigue con su diálogo).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Suggestion {
    pub label: String,
    pub text: String,
    pub stay: bool,
    /// Posición del cursor (en caracteres) tras aceptarla.
    pub cursor: usize,
    /// Si Enter también la acepta. Las sugerencias de menciones en una acción solo se aceptan con Tab
    /// para no estorbar al escribir.
    pub enter: bool,
    /// A qué cosa del proyecto apunta el tramo que se inserta (por ejemplo, un personaje).
    pub link: Option<Link>,
}

const SCENE_PREFIXES: [&str; 3] = ["INT./EXT.", "INT.", "EXT."];
const SCENE_TIMES: [&str; 10] = [
    "DÍA",
    "NOCHE",
    "TARDE",
    "MAÑANA",
    "AMANECER",
    "ATARDECER",
    "ANOCHECER",
    "CONTINUO",
    "MÁS TARDE",
    "MOMENTOS DESPUÉS",
];

/// Autocompletado según el tipo de párrafo: nombres de personaje, o las tres partes del encabezado
/// de escena (INT./EXT., locación y momento del día).
fn suggestions(
    locations: &[Mention],
    mentions: &[Mention],
    case: &CaseOptions,
    kind: ParagraphKind,
    text: &str,
    cursor: usize,
    links: &[Link],
) -> Vec<Suggestion> {
    match kind {
        ParagraphKind::Character => character_suggestions(mentions, text, !links.is_empty()),
        ParagraphKind::SceneHeading => {
            scene_suggestions(locations, &text.to_uppercase(), !links.is_empty())
        }
        ParagraphKind::Action | ParagraphKind::Dialogue => {
            mention_suggestions(mentions, text, cursor, case, links)
        }
        _ => Vec::new(),
    }
}

/// Personajes que empiezan con lo escrito en un encabezado de personaje. Si dos se llaman igual, se ofrecen los dos
/// (con lo que los distingue) para elegir a cuál se refiere; si el encabezado ya está vinculado, no hace falta.
fn character_suggestions(mentions: &[Mention], text: &str, linked: bool) -> Vec<Suggestion> {
    let typed = clean_character_name(text);
    if typed.is_empty() || text.contains('(') {
        return Vec::new();
    }
    let characters: Vec<&Mention> = mentions
        .iter()
        .filter(|m| matches!(m.target, Some(EntityRef::Character(_))))
        .collect();
    let twins = |name: &str| {
        characters
            .iter()
            .filter(|m| clean_character_name(&m.name) == name)
            .count()
    };
    characters
        .iter()
        .filter(|m| {
            let n = clean_character_name(&m.name);
            n.starts_with(&typed) && (n != typed || (!linked && twins(&n) >= 2))
        })
        .take(6)
        .filter_map(|m| {
            let name = clean_character_name(&m.name);
            let len = name.chars().count();
            let label = if twins(&name) >= 2 && !m.detail.is_empty() {
                format!("{name} — {}", m.detail)
            } else {
                name.clone()
            };
            Some(Suggestion {
                label,
                text: name,
                stay: false,
                cursor: len,
                enter: true,
                link: Some(Link {
                    start: 0,
                    end: len,
                    target: m.target?,
                }),
            })
        })
        .collect()
}

pub(crate) fn scene_suggestions(locations: &[Mention], t: &str, linked: bool) -> Vec<Suggestion> {
    let make = |label: &str, text: String| Suggestion {
        label: label.to_string(),
        cursor: text.chars().count(),
        text,
        stay: true,
        enter: true,
        link: None,
    };

    // 1) Todavía sin prefijo: INT. / EXT. / INT./EXT.
    let Some(prefix) = SCENE_PREFIXES
        .iter()
        .find(|p| t.starts_with(&format!("{p} ")))
    else {
        return SCENE_PREFIXES
            .iter()
            .filter(|p| p.starts_with(t) && **p != t)
            .map(|p| make(p, format!("{p} ")))
            .collect();
    };
    let rest = &t[prefix.len() + 1..];

    // 3) Después de « - »: momento del día.
    let (location, typed_time) = match rest.find(" - ") {
        Some(i) => (&rest[..i], Some(&rest[i + 3..])),
        None if rest.ends_with(" -") => (&rest[..rest.len() - 2], Some("")),
        None => (rest, None),
    };
    if let Some(typed_time) = typed_time {
        return SCENE_TIMES
            .iter()
            .filter(|time| time.starts_with(typed_time) && **time != typed_time)
            .map(|time| make(time, format!("{prefix} {location} - {time}")))
            .collect();
    }

    // 2) Locación: las que ya existen en el proyecto. Si dos se llaman igual y todavía no se eligió, se ofrecen las dos.
    let twins = |name: &str| {
        locations
            .iter()
            .filter(|m| m.name.eq_ignore_ascii_case(name))
            .count()
    };
    let start = prefix.chars().count() + 1;
    locations
        .iter()
        .filter(|m| {
            let name = m.name.to_uppercase();
            name.starts_with(location) && (name != location || (!linked && twins(&m.name) >= 2))
        })
        .take(6)
        .map(|m| {
            let name = m.name.to_uppercase();
            let len = name.chars().count();
            let text = format!("{prefix} {name} - ");
            let label = if twins(&m.name) >= 2 && !m.detail.is_empty() {
                format!("{name} — {}", m.detail)
            } else {
                name.clone()
            };
            Suggestion {
                label,
                cursor: text.chars().count(),
                text,
                stay: true,
                enter: true,
                link: m.target.map(|target| Link {
                    start,
                    end: start + len,
                    target,
                }),
            }
        })
        .collect()
}

/// Sugerencias de nombres guardados (personajes, locaciones, objetos, mundos) mientras se escribe una acción:
/// se busca, entre las últimas palabras antes del cursor, el fragmento más largo que empiece algún nombre.
pub(crate) fn mention_suggestions(
    mentions: &[Mention],
    text: &str,
    cursor: usize,
    case: &CaseOptions,
    links: &[Link],
) -> Vec<Suggestion> {
    let chars: Vec<char> = text.chars().collect();
    let cur = cursor.min(chars.len());
    // Solo se sugiere con el cursor al final de una palabra.
    if chars.get(cur).is_some_and(|c| c.is_alphanumeric()) {
        return Vec::new();
    }
    let before = &chars[..cur];
    let mut starts: Vec<usize> = (0..cur)
        .filter(|&i| before[i].is_alphanumeric() && (i == 0 || !before[i - 1].is_alphanumeric()))
        .collect();
    if starts.len() > 4 {
        starts.drain(..starts.len() - 4);
    }
    for start in starts {
        let fragment: String = before[start..].iter().collect();
        let single_word = !fragment.contains(' ');
        if fragment.chars().count() < if single_word { 3 } else { 4 } {
            continue;
        }
        let lower = fragment.to_lowercase();
        let mut out: Vec<Suggestion> = Vec::new();
        // ¿Ya está vinculado este tramo? Entonces no hace falta volver a elegir.
        let already_linked = links.iter().any(|l| l.start < cur && l.end > start);
        for mention in mentions {
            let name_lower = mention.name.to_lowercase();
            let twins = mentions
                .iter()
                .filter(|m| m.name.eq_ignore_ascii_case(&mention.name))
                .count()
                >= 2;
            // Una palabra completa solo se ofrece si hay dos con ese nombre y todavía no se eligió cuál.
            if !name_lower.starts_with(&lower)
                || (name_lower == lower && !(twins && !already_linked))
            {
                continue;
            }
            // Se guarda en su forma natural; la opción de mayúsculas solo cambia cómo se muestra.
            let stored = format_mention(mention, &fragment, false);
            let shown = format_mention(mention, &fragment, case.is_upper(mention.target));
            // Dos cosas con el mismo nombre se distinguen en la lista por lo que las diferencia.
            let label = if twins && !mention.detail.is_empty() {
                format!("{shown} — {}", mention.detail)
            } else {
                shown
            };
            if out
                .iter()
                .any(|s| s.label == label && s.link.map(|l| l.target) == mention.target)
            {
                continue;
            }
            let head: String = chars[..start].iter().collect();
            let tail: String = chars[cur..].iter().collect();
            let len = stored.chars().count();
            out.push(Suggestion {
                cursor: start + len,
                text: format!("{head}{stored}{tail}"),
                label,
                stay: true,
                enter: false,
                link: mention.target.map(|target| Link {
                    start,
                    end: start + len,
                    target,
                }),
            });
        }
        if !out.is_empty() {
            out.truncate(6);
            return out;
        }
    }
    Vec::new()
}

/// Cómo se escribe un nombre al autocompletarlo en una acción. Con mayúsculas activadas va todo en mayúsculas.
/// Si no, un personaje lleva mayúscula inicial en cada palabra importante («Juan») y cualquier otro nombre
/// se escribe como se empezó a tipear: «cas» → «casa de ana», «Cas» → «Casa de ana».
pub(crate) fn format_mention(mention: &Mention, typed: &str, uppercase: bool) -> String {
    if uppercase {
        return mention.name.to_uppercase();
    }
    let lower = mention.name.to_lowercase();
    if mention.character {
        const SMALL: [&str; 8] = ["de", "del", "la", "las", "los", "el", "y", "e"];
        return lower
            .split(' ')
            .enumerate()
            .map(|(i, w)| {
                if i > 0 && SMALL.contains(&w) {
                    w.to_string()
                } else {
                    capitalize(w)
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    if typed.chars().next().is_some_and(char::is_uppercase) {
        capitalize(&lower)
    } else {
        lower
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Rango en caracteres que cubre un comentario: el texto citado o, si no hay, el párrafo entero.
pub(crate) fn comment_range(text: &str, quote: Option<&str>) -> Option<(usize, usize)> {
    let total = text.chars().count();
    match quote.filter(|q| !q.is_empty()) {
        Some(q) => text.find(q).map(|b| {
            let start = text[..b].chars().count();
            (start, start + q.chars().count())
        }),
        None => Some((0, total)),
    }
}

/// Texto del párrafo tal como se muestra: los nombres guardados (personajes, locaciones, objetos…) se pasan a
/// mayúsculas o a mayúscula inicial según la opción, y los de personaje van en negrita si se pidió. El texto
/// guardado no cambia, así que alternar la opción cambia también lo que ya estaba escrito.
pub(crate) fn styled_job(
    text: &str,
    font: &FontId,
    bold_font: &FontId,
    color: Color32,
    wrap: f32,
    line_h: f32,
    spans: &[MentionSpan],
    bold_characters: bool,
    all_bold: bool,
    halign: Align,
) -> LayoutJob {
    let chars: Vec<char> = text.chars().collect();
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    job.halign = halign;
    let format = |font: &FontId| TextFormat {
        font_id: font.clone(),
        color,
        line_height: Some(line_h),
        ..Default::default()
    };
    let slice = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    if all_bold {
        job.append(&slice(0, chars.len()), 0.0, format(bold_font));
        return job;
    }
    let mut at = 0;
    for span in spans {
        if span.start > at {
            job.append(&slice(at, span.start), 0.0, format(font));
        }
        let shown = match span.rule {
            Some(rule) => apply_case(&chars[span.start..span.end], rule),
            None => slice(span.start, span.end),
        };
        let used = if bold_characters && span.character {
            bold_font
        } else {
            font
        };
        job.append(&shown, 0.0, format(used));
        at = span.end;
    }
    if at < chars.len() || chars.is_empty() {
        job.append(&slice(at, chars.len()), 0.0, format(font));
    }
    job
}

/// Pinta un fondo translúcido detrás de los caracteres `[start, end)` de un galley.
pub(crate) fn highlight_range(
    painter: &egui::Painter,
    galley: &egui::Galley,
    origin: egui::Pos2,
    start: usize,
    end: usize,
    color: Color32,
    underline: Option<Color32>,
) {
    let mut row_start = 0;
    for placed in &galley.rows {
        let visible = placed.row.char_count_excluding_newline().0;
        let (a, b) = (start.max(row_start), end.min(row_start + visible));
        if a < b {
            let x0 = placed.row.x_offset(CharIndex(a - row_start));
            let x1 = placed.row.x_offset(CharIndex(b - row_start));
            let r = egui::Rect::from_min_max(
                origin + egui::vec2(placed.pos.x + x0, placed.pos.y),
                origin + egui::vec2(placed.pos.x + x1, placed.pos.y + placed.row.size.y),
            );
            painter.rect_filled(r.expand2(egui::vec2(0.0, 1.0)), 2, color);
            if let Some(line) = underline {
                painter.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(2.0, line));
            }
        }
        row_start += placed.char_count_including_newline().0;
    }
}

#[derive(Clone, Copy)]
enum Step {
    Left,
    Right,
    Up,
    Down,
}

/// Mueve el extremo móvil de una selección un paso: un carácter a los lados (pasando al párrafo vecino al llegar al
/// borde) o una línea arriba o abajo, a la misma distancia del margen.
fn move_head(project: &Project, geoms: &[Geom], head: (u64, usize), step: Step) -> (u64, usize) {
    let Some(i) = project.paragraphs.iter().position(|p| p.id == head.0) else {
        return head;
    };
    let len = |j: usize| project.paragraphs[j].text.chars().count();
    let at = head.1.min(len(i));
    match step {
        Step::Left => {
            if at > 0 {
                (head.0, at - 1)
            } else if i > 0 {
                (project.paragraphs[i - 1].id, len(i - 1))
            } else {
                head
            }
        }
        Step::Right => {
            if at < len(i) {
                (head.0, at + 1)
            } else if i + 1 < project.paragraphs.len() {
                (project.paragraphs[i + 1].id, 0)
            } else {
                head
            }
        }
        Step::Up | Step::Down => {
            let Some(g) = geoms.iter().find(|g| g.pid == head.0) else {
                return head;
            };
            let caret = g.galley.pos_from_cursor(CCursor::new(at));
            let dy = if matches!(step, Step::Up) {
                -caret.height()
            } else {
                caret.height()
            };
            let target = g.pos + egui::vec2(caret.center().x, caret.center().y + dy);
            match hit(geoms, target) {
                Some((idx, to)) if (geoms[idx].pid, to) != (head.0, at) => (geoms[idx].pid, to),
                // Ya no hay otra línea hacia ese lado: la selección llega al comienzo o al final del guion.
                _ if matches!(step, Step::Up) => (project.paragraphs[0].id, 0),
                _ => {
                    let last = project.paragraphs.len() - 1;
                    (project.paragraphs[last].id, len(last))
                }
            }
        }
    }
}

/// Teclas y portapapeles sobre una selección de varios párrafos. Devuelve si el guion cambió.
fn handle_multi(
    ctx: &egui::Context,
    state: &mut EditorState,
    project: &mut Project,
    now: f64,
) -> bool {
    let focused = state
        .caret
        .map(|(pid, ..)| pid)
        .filter(|pid| ctx.memory(|m| m.has_focus(Id::unique(("paragraph", *pid)))));

    // Apretar el mouse suelta la selección (el párrafo clicado toma el foco por su cuenta). Con Mayús, en cambio,
    // la extiende desde donde estaba el cursor hasta donde se hizo clic.
    if ctx.input(|i| i.pointer.primary_pressed()) {
        let shift = ctx.input(|i| i.modifiers.shift);
        let clicked = ctx
            .input(|i| i.pointer.interact_pos())
            .and_then(|pos| hit(&state.last_geoms, pos));
        let anchor = state.multi.map(|m| m.anchor).or_else(|| {
            let pid = focused?;
            let at = TextEditState::load(ctx, Id::unique(("paragraph", pid)))?
                .cursor
                .char_range()?
                .secondary
                .index
                .into();
            Some((pid, at))
        });
        match (shift, clicked, anchor) {
            // Mayús + clic dentro del mismo párrafo lo resuelve su propio campo de texto.
            (true, Some((idx, at)), Some(anchor))
                if state.multi.is_some() || state.last_geoms[idx].pid != anchor.0 =>
            {
                state.multi = Some(MultiSel {
                    anchor,
                    head: (state.last_geoms[idx].pid, at),
                });
            }
            (true, ..) => {}
            _ => state.multi = None,
        }
    }

    // Ctrl+A selecciona todo el guion cuando se está escribiendo en él.
    if (focused.is_some() || (state.multi.is_some() && !ctx.egui_wants_keyboard_input()))
        && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A))
    {
        if let (Some(first), Some(last)) = (project.paragraphs.first(), project.paragraphs.last()) {
            state.multi = Some(MultiSel {
                anchor: (first.id, 0),
                head: (last.id, last.text.chars().count()),
            });
            if let Some(pid) = focused {
                ctx.memory_mut(|m| m.surrender_focus(Id::unique(("paragraph", pid))));
            }
        }
    }

    // Pegar texto con saltos de línea en un párrafo: cada línea pasa a ser un párrafo (con lo seleccionado reemplazado).
    if let (None, Some(pid)) = (state.multi, focused) {
        let pasted = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(t) if t.contains('\n') => Some(t.replace("\r\n", "\n")),
                _ => None,
            })
        });
        if let Some(text) = pasted {
            ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
            let range = TextEditState::load(ctx, Id::unique(("paragraph", pid)))
                .and_then(|s| s.cursor.char_range());
            let (a, b): (usize, usize) = range.map_or((0, 0), |r| {
                (r.primary.index.into(), r.secondary.index.into())
            });
            let at = project.delete_range((pid, a.min(b)), (pid, a.max(b)));
            let at = project.paste_text_at(at.0, at.1, &text);
            state.focus = Some((at.0, Pos::At(at.1)));
            state.commit(project, true, now);
            return true;
        }
    }

    let Some(sel) = state.multi else { return false };
    // Si otro campo (los comentarios, por ejemplo) tiene el teclado, no se le quita.
    if ctx.egui_wants_keyboard_input() {
        return false;
    }

    use egui::Event;
    let (mut copy, mut cut, mut paste, mut typed) = (false, false, None::<String>, String::new());
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            Event::Copy => {
                copy = true;
                false
            }
            Event::Cut => {
                cut = true;
                false
            }
            Event::Paste(text) => {
                paste = Some(text.clone());
                false
            }
            Event::Text(text) => {
                typed.push_str(text);
                false
            }
            _ => true,
        });
    });
    // Mayús + flechas mueven el extremo móvil de la selección (primero, antes que las flechas sin Mayús).
    let shifted = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, k));
    let moves = [
        (shifted(Key::ArrowLeft), Step::Left),
        (shifted(Key::ArrowRight), Step::Right),
        (shifted(Key::ArrowUp), Step::Up),
        (shifted(Key::ArrowDown), Step::Down),
    ];
    if let Some((_, step)) = moves.iter().find(|(pressed, _)| *pressed) {
        let head = move_head(project, &state.last_geoms, sel.head, *step);
        state.multi = Some(MultiSel {
            anchor: sel.anchor,
            head,
        });
        return false;
    }

    let key = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
    let (backspace, delete, enter) = (key(Key::Backspace), key(Key::Delete), key(Key::Enter));
    let escape = key(Key::Escape);
    let (left, up, right, down) = (
        key(Key::ArrowLeft),
        key(Key::ArrowUp),
        key(Key::ArrowRight),
        key(Key::ArrowDown),
    );

    let Some((start, end)) = project.order_positions(sel.anchor, sel.head) else {
        state.multi = None;
        return false;
    };
    let (start, end) = (
        (project.paragraphs[start.0].id, start.1),
        (project.paragraphs[end.0].id, end.1),
    );

    if copy || cut {
        ctx.copy_text(project.range_text(start, end));
    }
    if escape {
        state.multi = None;
        return false;
    }
    if left || up || right || down {
        let (pid, at) = if left || up { start } else { end };
        state.multi = None;
        state.focus = Some((pid, Pos::At(at)));
        return false;
    }
    let inserted = paste.filter(|t| !t.is_empty()).unwrap_or(typed);
    if !(cut || backspace || delete || enter || !inserted.is_empty()) {
        return false;
    }

    // Se reemplaza lo seleccionado por lo que se escribió o pegó (o por nada, o por un salto de línea).
    let mut at = project.delete_range(start, end);
    if !inserted.is_empty() {
        at = if inserted.contains('\n') {
            project.paste_text_at(at.0, at.1, &inserted)
        } else {
            project.insert_text_at(at.0, at.1, &inserted)
        };
    }
    if enter {
        at = project.insert_text_at(at.0, at.1, "\n");
    }
    state.multi = None;
    state.focus = Some((at.0, Pos::At(at.1)));
    state.commit(project, true, now);
    true
}

/// Dibuja el editor y devuelve si el proyecto cambió.
pub fn show(
    ui: &mut Ui,
    state: &mut EditorState,
    project: &mut Project,
    settings: &Settings,
    p: &Palette,
) -> bool {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    if state.snapshot.is_none() {
        state.snapshot = Some(project.paragraphs.clone());
    }

    // Deshacer / rehacer globales (se consumen antes de que los vea el TextEdit).
    let mut changed = false;
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z))
        || ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Y))
    {
        changed |= state.redo(project);
    } else if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
        changed |= state.undo(project);
    }

    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::N)) {
        state.open_entity(project);
    }
    changed |= handle_multi(&ctx, state, project, now);

    let mentions = project.mention_names();
    let case = project.case_options();
    // En un diálogo los nombres nunca van todo en mayúsculas, aunque la opción esté activada.
    let spoken_case = CaseOptions::uniform(false);
    let locations: Vec<Mention> = mentions
        .iter()
        .filter(|m| matches!(m.target, Some(EntityRef::Location(_))))
        .cloned()
        .collect();
    let mut new_sel = None;
    let mut edit: Option<Edit> = None;
    let mut text_changed = false;
    let focus_req = state.focus.take();
    let scroll_to = state.scroll_to.take();
    let mut new_caret = None;
    let mut popup: Option<(egui::Pos2, Vec<Suggestion>, usize)> = None;
    let mut suggestion_pick: Option<(usize, Suggestion)> = None;

    // El tamaño de la letra manda: nunca se achica para que entre. Si falta ancho, primero se
    // reducen los márgenes de la hoja y, si aun así no entra, aparece el desplazamiento horizontal.
    let m = Metrics::new(
        crate::storage::SCRIPT_PT as f32,
        settings.view_zoom,
        settings.paper,
    );
    let font = FontId::monospace(m.size);
    let bold_font = FontId::new(m.size, theme::script_bold_family());
    let text_w = m.page.text_w() * m.inch;
    let avail = ui.available_width();
    let nominal = m.page.width * m.inch;
    let (margin_left, margin_right) = if avail >= nominal {
        (m.page.left * m.inch, m.page.right * m.inch)
    } else {
        let t = ((nominal - avail) / (2.0 * m.inch)).clamp(0.0, 1.0);
        let lerp = |from: f32| from + (0.3 * m.inch - from) * t;
        (lerp(m.page.left * m.inch), lerp(m.page.right * m.inch))
    };
    let sheet_w = margin_left + text_w + margin_right;

    // Ctrl + rueda cambia el zoom de la vista (no el tamaño de la letra en la hoja).
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

    // Línea de tiempo: se dibuja pegada al borde derecho del área, desplazándose con la hoja.
    let strip_x = ui.max_rect().right() - 44.0;
    let (starts, total_seconds) = project.timeline();
    let mut next_tick = 0u32;
    let mut strip_span: Option<(f32, f32)> = None;

    // Cómo se arma el texto de cada párrafo (lo usan la medición y el campo de edición, así coinciden).
    let line_h = m.size;
    let text_color = p.text;
    let make_galley = |ctx: &egui::Context,
                       kind: ParagraphKind,
                       links: &[Link],
                       text: &str,
                       wrap: f32|
     -> Arc<egui::Galley> {
        // Los nombres guardados siguen la opción de mayúsculas en las acciones; en un diálogo se muestran como se dicen.
        let spans = if matches!(kind, ParagraphKind::Action | ParagraphKind::Dialogue) {
            let rules = if kind == ParagraphKind::Dialogue {
                &spoken_case
            } else {
                &case
            };
            mention_spans(text, links, &mentions, rules)
        } else {
            Vec::new()
        };
        let job = styled_job(
            text,
            &font,
            &bold_font,
            if kind == ParagraphKind::Note {
                p.dim
            } else {
                text_color
            },
            wrap,
            line_h,
            &spans,
            settings.bold_names && kind == ParagraphKind::Action,
            (kind == ParagraphKind::Character && settings.bold_names) || kind == ParagraphKind::Act,
            // La alineación a la derecha o al centro la da el campo de texto (`horizontal_align`), no el texto armado.
            Align::LEFT,
        );
        ctx.fonts_mut(|f| f.layout_job(job))
    };

    // Se miden todos los párrafos y se reparten entre hojas de 11" (9" de texto, con márgenes de 1" arriba y abajo).
    let items: Vec<PageItem> = project
        .paragraphs
        .iter()
        .enumerate()
        .map(|(i, para)| {
            let (_, width) = m.geometry(para.kind, &para.text);
            let galley = make_galley(&ctx, para.kind, &para.links, &para.text, width);
            PageItem {
                dialogue: para.kind == ParagraphKind::Dialogue,
                sentence_ends: galley
                    .rows
                    .iter()
                    .map(|r| ends_sentence(&r.row.text()))
                    .collect(),
                height: galley.size().y.max(line_h),
                gap: if i == 0 {
                    0.0
                } else {
                    m.gap_before(para.kind, Some(project.paragraphs[i - 1].kind))
                },
                keep_with_next: matches!(
                    para.kind,
                    ParagraphKind::SceneHeading
                        | ParagraphKind::Character
                        | ParagraphKind::Parenthetical
                        | ParagraphKind::Act
                        | ParagraphKind::Sequence
                ),
                row_h: if matches!(
                    para.kind,
                    ParagraphKind::Action
                        | ParagraphKind::Dialogue
                        | ParagraphKind::Lyrics
                        | ParagraphKind::Note
                ) {
                    line_h
                } else {
                    0.0
                },
            }
        })
        .collect();
    let page_height = m.page.text_h() * m.inch;
    let placements = paginate(&items, page_height);
    let pages = sheet_count(&items, &placements, page_height);
    let sheet_h = m.page.height * m.inch;
    const TOP_PAD: f32 = crate::ui::tokens::SP_XL;
    const SHEET_GAP: f32 = 28.0;
    let content_w = avail.max(sheet_w);
    let content_h = TOP_PAD + pages as f32 * sheet_h + (pages - 1) as f32 * SHEET_GAP + 120.0;

    // La revisión abierta: qué párrafos cambiaron, en qué hojas caen y con qué color se marcan.
    let revised = project.revised_paragraphs();
    let revision_look: Option<(Color32, String)> = project.active_revision().map(|r| {
        let [red, green, blue] = r.color.strong();
        (
            Color32::from_rgb(red, green, blue),
            format!("Revisión {}: {}", r.color.label().to_lowercase(), r.name),
        )
    });
    let mut revised_pages: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (i, para) in project
        .paragraphs
        .iter()
        .enumerate()
        .filter(|(_, q)| revised.contains(&q.id))
    {
        let last = match placements[i].cut {
            Some(cut) => {
                let rows = (items[i].height / line_h).round() as usize;
                let per_page = ((page_height / line_h).floor() as usize).max(1);
                placements[i].page
                    + split_plan(rows, cut, per_page, para.kind == ParagraphKind::Dialogue).len()
                    - 1
            }
            None => placements[i].page,
        };
        revised_pages.extend(placements[i].page..=last);
    }

    // Lo que ocupa la selección de varios párrafos en cada uno: (desde, hasta) en caracteres.
    let mut multi_ranges: std::collections::HashMap<u64, (usize, usize)> =
        std::collections::HashMap::new();
    if let Some(sel) = state.multi {
        if let Some(((i1, c1), (i2, c2))) = project.order_positions(sel.anchor, sel.head) {
            for i in i1..=i2 {
                let len = project.paragraphs[i].text.chars().count();
                multi_ranges.insert(
                    project.paragraphs[i].id,
                    (if i == i1 { c1 } else { 0 }, if i == i2 { c2 } else { len }),
                );
            }
        }
    }

    egui::ScrollArea::both()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let (content, _) =
                ui.allocate_exact_size(egui::vec2(content_w, content_h), egui::Sense::hover());
            let sheet_left = content.min.x + (content_w - sheet_w) / 2.0;
            let text_left = sheet_left + margin_left;
            let sheet_top = |k: usize| content.min.y + TOP_PAD + k as f32 * (sheet_h + SHEET_GAP);

            // Las hojas: una por cada 9" de texto, con el número de página arriba a la derecha desde la segunda.
            for k in 0..pages {
                let sheet = egui::Rect::from_min_size(
                    egui::pos2(sheet_left, sheet_top(k)),
                    egui::vec2(sheet_w, sheet_h),
                );
                ui.painter().rect(
                    sheet,
                    2,
                    p.panel,
                    Stroke::new(1.0, theme::mix(p.panel, p.border, 0.5)),
                    egui::StrokeKind::Inside,
                );
                // Una hoja con cambios de la revisión abierta se tiñe de su color y lo dice arriba.
                if let (true, Some((color, label))) = (revised_pages.contains(&k), &revision_look) {
                    ui.painter()
                        .rect_filled(sheet.shrink(1.0), 2, color.linear_multiply(0.10));
                    ui.painter().text(
                        egui::pos2(sheet.left() + margin_left, sheet.top() + 0.5 * m.inch),
                        egui::Align2::LEFT_TOP,
                        label,
                        FontId::monospace(m.size * 0.8),
                        *color,
                    );
                }
                if k > 0 {
                    ui.painter().text(
                        egui::pos2(sheet.right() - margin_right, sheet.top() + 0.5 * m.inch),
                        egui::Align2::RIGHT_TOP,
                        format!("{}.", k + 1),
                        font.clone(),
                        p.dim,
                    );
                }
            }

            let left_edge = text_left;
            let scene_labels = project.scene_labels();
            let conts = project.cont_marks();
            let locked_numbers = project.scene_numbers_locked;
            let mut scene_no = 0;
            let mut current_char: Option<Color32> = None;
            let mut geoms: Vec<Geom> = Vec::new();

            for i in 0..project.paragraphs.len() {
                let kind = project.paragraphs[i].kind;
                let pid = project.paragraphs[i].id;
                let id = Id::unique(("paragraph", pid));

                match kind {
                    ParagraphKind::SceneHeading => {
                        scene_no += 1;
                        current_char = None;
                    }
                    ParagraphKind::Character => {
                        // Dos personajes con el mismo nombre tienen colores distintos: se usa su id.
                        let key = match project.paragraphs[i].links.first() {
                            Some(Link {
                                target: EntityRef::Character(id),
                                ..
                            }) => id.to_string(),
                            _ => clean_character_name(&project.paragraphs[i].text),
                        };
                        current_char = Some(theme::character_color(&key));
                    }
                    ParagraphKind::Action | ParagraphKind::Transition => current_char = None,
                    _ => {}
                }

                if let Some((fid, pos)) = focus_req {
                    if fid == pid {
                        apply_focus(&ctx, id, &project.paragraphs[i].text, pos);
                        state.caret = Some((pid, 0, 1));
                    }
                }

                let has_focus = ctx.memory(|mem| mem.has_focus(id));
                let sugg = if has_focus
                    && state.dismissed.as_ref() != Some(&(pid, project.paragraphs[i].text.clone()))
                {
                    let cur = TextEditState::load(&ctx, id)
                        .and_then(|s| s.cursor.char_range())
                        .map_or(project.paragraphs[i].text.chars().count(), |r| {
                            r.primary.index.into()
                        });
                    suggestions(
                        &locations,
                        &mentions,
                        if kind == ParagraphKind::Dialogue {
                            &spoken_case
                        } else {
                            &case
                        },
                        kind,
                        &project.paragraphs[i].text,
                        cur,
                        &project.paragraphs[i].links,
                    )
                } else {
                    Vec::new()
                };
                if has_focus {
                    if let Some(e) =
                        handle_keys(&ctx, state, id, i, project, &sugg, &mut suggestion_pick)
                    {
                        edit = Some(e);
                    }
                }

                let text_before = has_focus.then(|| project.paragraphs[i].text.clone());
                let links_for_layout = project.paragraphs[i].links.clone();
                let (indent, width) = m.geometry(kind, &project.paragraphs[i].text);
                // El párrafo se coloca donde le tocó en su hoja.
                let y = sheet_top(placements[i].page) + m.page.top * m.inch + placements[i].y;
                let slot = egui::Rect::from_min_size(
                    egui::pos2(text_left + indent, y),
                    egui::vec2(width, items[i].height + 8.0),
                );
                // El fondo de lo que cambió, con el color de la revisión.
                if let (true, None, Some((color, _))) =
                    (revised.contains(&pid), placements[i].cut, &revision_look)
                {
                    let band = egui::Rect::from_min_size(
                        egui::pos2(text_left + indent - 4.0, y),
                        egui::vec2(width + 8.0, items[i].height),
                    );
                    ui.painter()
                        .rect_filled(band, 2, color.linear_multiply(0.18));
                }
                let output = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(slot), |ui| {
                        let para = &mut project.paragraphs[i];
                        let mut layouter = |ui: &Ui,
                                            buf: &dyn egui::TextBuffer,
                                            wrap: f32|
                         -> Arc<egui::Galley> {
                            let galley =
                                make_galley(ui.ctx(), kind, &links_for_layout, buf.as_str(), wrap);
                            match placements[i].cut {
                                Some(cut) => split_galley(
                                    galley,
                                    cut,
                                    placements[i].y,
                                    page_height,
                                    m.page.margins_v() * m.inch + SHEET_GAP,
                                    line_h,
                                    kind == ParagraphKind::Dialogue,
                                ),
                                None => galley,
                            }
                        };
                        let mut te = TextEdit::multiline(&mut para.text)
                            .id(id)
                            .layouter(&mut layouter)
                            .desired_rows(1)
                            .desired_width(width)
                            .margin(egui::Margin::ZERO)
                            .frame(egui::Frame::NONE)
                            .align(match kind {
                                ParagraphKind::Transition => egui::Align2::RIGHT_TOP,
                                ParagraphKind::Act | ParagraphKind::Sequence => {
                                    egui::Align2::CENTER_TOP
                                }
                                _ => egui::Align2::LEFT_TOP,
                            })
                            .lock_focus(true);
                        if kind == ParagraphKind::Character {
                            te = te.hint_text(
                                RichText::new("PERSONAJE").color(p.dim).font(font.clone()),
                            );
                        }
                        te.show(ui)
                    })
                    .inner;
                let rect = output.response.rect;
                // Un diálogo partido entre hojas: «(MÁS)» al pie de cada una y, arriba de la siguiente, el nombre con «(CONT.)».
                if let (Some(cut), ParagraphKind::Dialogue) = (placements[i].cut, kind) {
                    let painter = ui.painter().clone();
                    let per_page = ((page_height / line_h).floor() as usize).max(1);
                    let plan = split_plan(output.galley.rows.len(), cut, per_page, true);
                    let cue_x = text_left + 2.2 * m.inch;
                    let speaker = project.paragraphs[..i]
                        .iter()
                        .rev()
                        .find(|q| q.kind == ParagraphKind::Character)
                        .map(|q| clean_character_name(&q.text).to_uppercase())
                        .unwrap_or_default();
                    let page_top =
                        |k: usize| sheet_top(placements[i].page + k) + m.page.top * m.inch;
                    for (k, rows_here) in plan.iter().enumerate() {
                        let last = k + 1 == plan.len();
                        if k > 0 {
                            painter.text(
                                egui::pos2(cue_x, page_top(k)),
                                egui::Align2::LEFT_TOP,
                                format!("{speaker} (CONT.)"),
                                font.clone(),
                                p.text,
                            );
                        }
                        if !last {
                            let used = if k == 0 {
                                placements[i].y / line_h + *rows_here as f32
                            } else {
                                1.0 + *rows_here as f32
                            };
                            painter.text(
                                egui::pos2(cue_x, page_top(k) + used * line_h),
                                egui::Align2::LEFT_TOP,
                                "(MÁS)",
                                font.clone(),
                                p.text,
                            );
                        }
                    }
                }
                geoms.push(Geom {
                    pid,
                    rect,
                    galley: output.galley.clone(),
                    pos: output.galley_pos,
                    len: project.paragraphs[i].text.chars().count(),
                });
                // Un arrastre que empieza en este párrafo: si el mouse sale hacia otro, pasa a ser una selección de varios.
                if output.response.drag_started() {
                    let at = output.cursor_range.map_or(0, |r| r.primary.index.into());
                    state.drag = Some((pid, at));
                }

                if output.response.changed() {
                    text_changed = true;
                    let para = &mut project.paragraphs[i];
                    if let Some(before) = &text_before {
                        adjust_links(&mut para.links, before, &para.text);
                    }
                    if kind.is_uppercase() {
                        let up = para.text.to_uppercase();
                        if up.chars().count() == para.text.chars().count() {
                            para.text = up;
                        }
                    }
                }

                if output.response.has_focus() {
                    let rows = output.galley.rows.len().max(1);
                    let row = output
                        .cursor_range
                        .map(|r| output.galley.layout_from_cursor(r.primary).row)
                        .unwrap_or(0);
                    new_caret = Some((pid, row, rows));
                    if let Some(r) = output.cursor_range {
                        let (a, b): (usize, usize) =
                            (r.primary.index.into(), r.secondary.index.into());
                        new_sel = Some((pid, a.min(b), a.max(b)));
                    }
                }
                // Fondo de lo comentado (solo los comentarios sin resolver).
                let para_text = &project.paragraphs[i].text;
                for c in project
                    .comments
                    .iter()
                    .filter(|c| c.paragraph == pid && !c.resolved)
                {
                    if let Some((a, b)) = comment_range(para_text, c.quote.as_deref()) {
                        highlight_range(
                            &ui.painter().clone(),
                            &output.galley,
                            output.galley_pos,
                            a,
                            b,
                            p.accent.linear_multiply(0.22),
                            None,
                        );
                    }
                }
                // Lo marcado para el desglose, con el color de su categoría.
                for tag in project.breakdown_tags.iter().filter(|t| t.paragraph == pid) {
                    let color = project
                        .breakdown_items
                        .iter()
                        .find(|it| it.id == tag.item)
                        .and_then(|it| {
                            project
                                .breakdown_categories
                                .iter()
                                .find(|c| c.id == it.category)
                        })
                        .map(|c| Color32::from_rgb(c.color[0], c.color[1], c.color[2]));
                    if let (Some(color), Some((a, b))) =
                        (color, comment_range(para_text, Some(&tag.quote)))
                    {
                        highlight_range(
                            &ui.painter().clone(),
                            &output.galley,
                            output.galley_pos,
                            a,
                            b,
                            color.linear_multiply(0.28),
                            Some(color),
                        );
                    }
                }
                // La selección de varios párrafos.
                if let Some(&(a, b)) = multi_ranges.get(&pid) {
                    highlight_range(
                        &ui.painter().clone(),
                        &output.galley,
                        output.galley_pos,
                        a,
                        b,
                        p.accent.linear_multiply(0.38),
                        None,
                    );
                }
                if scroll_to == Some(pid) || focus_req.is_some_and(|(f, _)| f == pid) {
                    output.response.scroll_to_me(Some(Align::Center));
                }
                if output.response.hovered() {
                    ctx.set_cursor_icon(CursorIcon::Text);
                }

                let painter = ui.painter();
                // Lo que cambió desde que se abrió la revisión activa lleva un asterisco en el margen derecho, por renglón.
                if revised.contains(&pid) {
                    for row in &output.galley.rows {
                        painter.text(
                            egui::pos2(
                                left_edge + text_w + 0.12 * m.inch,
                                output.galley_pos.y + row.rect().top(),
                            ),
                            egui::Align2::LEFT_TOP,
                            "*",
                            font.clone(),
                            revision_look.as_ref().map_or(p.accent, |(c, _)| *c),
                        );
                    }
                }
                // «(CONT.)» automático: el personaje retoma la palabra después de una acción.
                if kind == ParagraphKind::Character && conts.get(i).copied().unwrap_or(false) {
                    let row = output.galley.rows.last();
                    let right = row.map_or(0.0, |r| r.rect().right());
                    let top = row.map_or(0.0, |r| r.rect().top());
                    painter.text(
                        egui::pos2(
                            output.galley_pos.x + right + m.char_w,
                            output.galley_pos.y + top,
                        ),
                        egui::Align2::LEFT_TOP,
                        "(CONT.)",
                        font.clone(),
                        p.dim,
                    );
                }
                // Los paréntesis de una acotación se dibujan pegados al texto.
                if kind == ParagraphKind::Parenthetical {
                    let top = output.galley_pos.y;
                    painter.text(
                        egui::pos2(output.galley_pos.x - m.char_w, top),
                        egui::Align2::LEFT_TOP,
                        "(",
                        font.clone(),
                        p.text,
                    );
                    let last_right = output.galley.rows.last().map_or(0.0, |r| r.rect().right());
                    let last_top = output.galley.rows.last().map_or(0.0, |r| r.rect().top());
                    painter.text(
                        egui::pos2(output.galley_pos.x + last_right, top + last_top),
                        egui::Align2::LEFT_TOP,
                        ")",
                        font.clone(),
                        p.text,
                    );
                }
                if kind == ParagraphKind::SceneHeading {
                    painter.text(
                        egui::pos2(left_edge - m.char_w * 2.0, rect.top()),
                        egui::Align2::RIGHT_TOP,
                        format!(
                            "{}.",
                            scene_labels
                                .get(scene_no - 1)
                                .map_or("", |(_, n)| n.as_str())
                        ),
                        font.clone(),
                        p.dim,
                    );
                    // En un guion con la numeración bloqueada, el número también va del lado derecho.
                    if locked_numbers {
                        painter.text(
                            egui::pos2(left_edge + text_w + m.char_w * 2.0, rect.top()),
                            egui::Align2::LEFT_TOP,
                            format!(
                                "{}.",
                                scene_labels
                                    .get(scene_no - 1)
                                    .map_or("", |(_, n)| n.as_str())
                            ),
                            font.clone(),
                            p.dim,
                        );
                    }
                }
                if let Some(color) = current_char {
                    if matches!(
                        kind,
                        ParagraphKind::Character
                            | ParagraphKind::Parenthetical
                            | ParagraphKind::Dialogue
                    ) {
                        let x = left_edge + 1.0 * m.inch - 10.0;
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            Stroke::new(2.0, color),
                        );
                    }
                }
                if project
                    .comments
                    .iter()
                    .any(|c| c.paragraph == pid && !c.resolved)
                {
                    painter.text(
                        egui::pos2(left_edge + text_w + 0.35 * m.inch, rect.top()),
                        egui::Align2::LEFT_TOP,
                        icon::CHAT_TEXT,
                        FontId::proportional(16.0),
                        p.accent,
                    );
                }

                if settings.show_timeline {
                    let t = starts.get(i).copied().unwrap_or(0.0);
                    let minute = (t / 60.0).floor() as u32;
                    // Una marca por cada minuto nuevo, a la altura del párrafo donde empieza.
                    if minute >= next_tick {
                        painter.line_segment(
                            [
                                egui::pos2(strip_x - 4.0, rect.top() + 2.0),
                                egui::pos2(strip_x + 4.0, rect.top() + 2.0),
                            ],
                            Stroke::new(1.0, p.dim),
                        );
                        painter.text(
                            egui::pos2(strip_x - 9.0, rect.top() - 2.0),
                            egui::Align2::RIGHT_TOP,
                            format!("{minute}:00"),
                            FontId::proportional(11.0),
                            p.dim,
                        );
                        next_tick = minute + 1;
                    }
                    // Cada escena nueva, con una marca de color.
                    if kind == ParagraphKind::SceneHeading {
                        painter.line_segment(
                            [
                                egui::pos2(strip_x + 4.0, rect.top() + 2.0),
                                egui::pos2(strip_x + 11.0, rect.top() + 2.0),
                            ],
                            Stroke::new(2.0, p.accent),
                        );
                    }
                    // Dónde está el cursor y a qué minuto corresponde.
                    if has_focus {
                        painter.circle_filled(
                            egui::pos2(strip_x, rect.top() + m.size * 0.55),
                            4.5,
                            p.accent,
                        );
                        painter.text(
                            egui::pos2(strip_x + 9.0, rect.top() + m.size * 0.55),
                            egui::Align2::LEFT_CENTER,
                            crate::pages::fmt_duration(t),
                            FontId::proportional(11.0),
                            p.accent,
                        );
                    }
                    strip_span = Some((strip_span.map_or(rect.top(), |s| s.0), rect.bottom()));
                }

                if has_focus && !sugg.is_empty() {
                    popup = Some((rect.left_bottom(), sugg, i));
                }
            }

            // Arrastrar el mouse de un párrafo a otro selecciona varios; cerca del borde, el área se desplaza sola.
            #[cfg(test)]
            {
                state.rects = geoms.iter().map(|g| (g.pid, g.rect)).collect();
            }
            if let Some((anchor_pid, anchor_char)) = state.drag {
                if ctx.input(|i| i.pointer.primary_down()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if let Some((idx, at)) = hit(&geoms, pos) {
                            if geoms[idx].pid != anchor_pid || state.multi.is_some() {
                                state.multi = Some(MultiSel {
                                    anchor: (anchor_pid, anchor_char),
                                    head: (geoms[idx].pid, at),
                                });
                                // El párrafo donde empezó deja de editar: la selección pasa a ser del editor.
                                ctx.memory_mut(|mem| {
                                    mem.surrender_focus(Id::unique(("paragraph", anchor_pid)))
                                });
                            }
                        }
                        let view = ui.clip_rect();
                        if pos.y > view.bottom() - 36.0 {
                            ui.scroll_with_delta(egui::vec2(0.0, -16.0));
                        } else if pos.y < view.top() + 36.0 {
                            ui.scroll_with_delta(egui::vec2(0.0, 16.0));
                        }
                        ctx.request_repaint();
                    }
                } else {
                    state.drag = None;
                }
            }

            state.last_geoms = geoms;

            if let Some((top, bottom)) = strip_span {
                let line = Stroke::new(1.0, theme::mix(p.bg, p.dim, 0.5));
                ui.painter().line_segment(
                    [egui::pos2(strip_x, top), egui::pos2(strip_x, bottom)],
                    line,
                );
                ui.painter().text(
                    egui::pos2(strip_x - 9.0, bottom + 6.0),
                    egui::Align2::RIGHT_TOP,
                    format!("Total {}", crate::pages::fmt_duration(total_seconds)),
                    FontId::proportional(11.0),
                    p.dim,
                );
            }
        });

    if let Some(c) = new_caret {
        state.caret = Some(c);
    }
    // Al soltar el foco de un párrafo, sus nombres se vinculan solos (y un encabezado de personaje nuevo crea
    // su ficha).
    let focused_now = new_caret.map(|(pid, ..)| pid);
    if let Some(prev) = state.prev_focus {
        if Some(prev) != focused_now {
            if let Some(idx) = project.paragraphs.iter().position(|q| q.id == prev) {
                let before = (
                    project.paragraphs[idx].links.clone(),
                    project.characters.len(),
                );
                project.auto_link_paragraph(idx);
                if before
                    != (
                        project.paragraphs[idx].links.clone(),
                        project.characters.len(),
                    )
                {
                    changed = true;
                    state.adopt_links(project);
                }
            }
        }
    }
    state.prev_focus = focused_now;
    if new_sel.is_some() {
        state.sel = new_sel;
    }

    if let Some((i, pick)) = suggestion_pick {
        apply_suggestion(&mut project.paragraphs[i], &pick);
        text_changed = true;
    }

    // Lista de sugerencias de personajes (flotante bajo el párrafo).
    if let Some((pos, list, i)) = popup {
        state.suggest_idx = state.suggest_idx.min(list.len() - 1);
        let mut clicked: Option<Suggestion> = None;
        egui::Area::new(Id::unique("character_suggestions"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    // Nunca se parten las palabras, aunque el espacio hasta el borde sea poco.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.set_min_width(180.0);
                    for (n, sugg) in list.iter().enumerate() {
                        if widgets::suggestion_item(ui, n == state.suggest_idx, &sugg.label) {
                            clicked = Some(sugg.clone());
                        }
                    }
                });
            });
        if let Some(pick) = clicked {
            state.focus = Some((project.paragraphs[i].id, Pos::At(pick.cursor)));
            apply_suggestion(&mut project.paragraphs[i], &pick);
            text_changed = true;
        }
    } else {
        state.suggest_idx = 0;
    }

    if text_changed {
        state.commit(project, false, now);
        changed = true;
    }
    if let Some(e) = edit {
        let content = e.changes_text();
        apply_edit(state, project, e);
        if content {
            state.commit(project, true, now);
            changed = true;
        }
    }
    // Con una selección de varios párrafos, ningún párrafo conserva el teclado (lo lleva el editor).
    if state.multi.is_some() {
        for para in &project.paragraphs {
            let id = Id::unique(("paragraph", para.id));
            if ctx.memory(|mem| mem.has_focus(id)) {
                ctx.memory_mut(|mem| mem.surrender_focus(id));
            }
        }
    }
    changed
}

/// Interpreta las teclas del párrafo con el foco. Consume las que gestiona para que el
/// `TextEdit` no las procese.
fn handle_keys(
    ctx: &egui::Context,
    state: &mut EditorState,
    id: Id,
    i: usize,
    project: &mut Project,
    sugg: &[Suggestion],
    suggestion_pick: &mut Option<(usize, Suggestion)>,
) -> Option<Edit> {
    let text = project.paragraphs[i].text.clone();
    let kind = project.paragraphs[i].kind;
    let n_chars = text.chars().count();
    let range = TextEditState::load(ctx, id)
        .and_then(|s| s.cursor.char_range())
        .unwrap_or(CCursorRange::one(CCursor::new(n_chars)));
    let cursor: usize = range.primary.index.into();
    let collapsed = range.primary == range.secondary;
    let at_end = collapsed && cursor >= n_chars;
    let at_start = collapsed && cursor == 0;
    let empty = text.trim().is_empty();
    let (row, rows) = state
        .caret
        .filter(|(pid, ..)| *pid == project.paragraphs[i].id)
        .map_or((0, 1), |(_, r, n)| (r, n));

    let key = |m: Modifiers, k: Key| ctx.input_mut(|inp| inp.consume_key(m, k));

    // Cambio directo de tipo: Ctrl+1..9 y Ctrl+0 para el décimo.
    let numbers = [
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
        Key::Num0,
    ];
    for (n, k) in numbers.into_iter().enumerate() {
        if key(Modifiers::COMMAND, k) {
            return Some(Edit::SetKind {
                i,
                kind: ParagraphKind::ALL[n],
            });
        }
    }

    // Lista de sugerencias abierta: ↑↓ eligen, Esc la descarta, Tab acepta y Enter acepta si la sugerencia lo
    // permite (en un personaje, además, sigue con el diálogo).
    if !sugg.is_empty() {
        if key(Modifiers::NONE, Key::Escape) {
            state.dismissed = Some((project.paragraphs[i].id, text.clone()));
            return None;
        }
        if key(Modifiers::NONE, Key::ArrowDown) {
            state.suggest_idx = (state.suggest_idx + 1) % sugg.len();
            return None;
        }
        if key(Modifiers::NONE, Key::ArrowUp) {
            state.suggest_idx = (state.suggest_idx + sugg.len() - 1) % sugg.len();
            return None;
        }
        let idx = state.suggest_idx.min(sugg.len() - 1);
        let enter = sugg[idx].enter && key(Modifiers::NONE, Key::Enter);
        if enter || key(Modifiers::NONE, Key::Tab) {
            let pick = sugg[idx].clone();
            *suggestion_pick = Some((i, pick.clone()));
            if enter && !pick.stay {
                return Some(Edit::Split {
                    i,
                    at: pick.text.chars().count(),
                });
            }
            state.focus = Some((project.paragraphs[i].id, Pos::At(pick.cursor)));
            return None;
        }
    }

    // Enter en un diálogo vacío vuelve a la acción.
    if empty && kind == ParagraphKind::Dialogue && key(Modifiers::NONE, Key::Enter) {
        return Some(Edit::SetKind {
            i,
            kind: ParagraphKind::Action,
        });
    }

    // Enter en una acción vacía la convierte en encabezado de escena.
    if empty && kind == ParagraphKind::Action && key(Modifiers::NONE, Key::Enter) {
        return Some(Edit::SetKind {
            i,
            kind: ParagraphKind::SceneHeading,
        });
    }

    if key(Modifiers::NONE, Key::Enter) {
        return Some(Edit::Split { i, at: cursor });
    }

    if key(Modifiers::NONE, Key::Tab) {
        if empty {
            return Some(Edit::SetKind {
                i,
                kind: kind.after_tab(),
            });
        }
        if kind == ParagraphKind::Character && at_end {
            return Some(Edit::InsertAfter {
                i,
                kind: ParagraphKind::Parenthetical,
            });
        }
        return None;
    }

    if at_start && key(Modifiers::NONE, Key::Backspace) {
        // Un párrafo vacío primero vuelve a Acción; si ya lo es, se borra.
        if empty && kind != ParagraphKind::Action {
            return Some(Edit::SetKind {
                i,
                kind: ParagraphKind::Action,
            });
        }
        return Some(Edit::MergeWithPrev { i });
    }
    if at_end && key(Modifiers::NONE, Key::Delete) {
        return Some(Edit::MergeWithNext { i });
    }

    // Mayús + flecha en el borde de un párrafo: la selección sigue en el vecino.
    if ctx.input(|inp| inp.modifiers.shift) {
        let me = project.paragraphs[i].id;
        let fixed: usize = range.secondary.index.into();
        let len_of = |j: usize| project.paragraphs[j].text.chars().count();
        let extend = |head: (u64, usize)| {
            Some(Edit::Extend {
                anchor: (me, fixed),
                head,
            })
        };
        if cursor == 0 && i > 0 && key(Modifiers::SHIFT, Key::ArrowLeft) {
            return extend((project.paragraphs[i - 1].id, len_of(i - 1)));
        }
        if cursor >= n_chars
            && i + 1 < project.paragraphs.len()
            && key(Modifiers::SHIFT, Key::ArrowRight)
        {
            return extend((project.paragraphs[i + 1].id, 0));
        }
        if row == 0 && i > 0 && key(Modifiers::SHIFT, Key::ArrowUp) {
            return extend((project.paragraphs[i - 1].id, cursor.min(len_of(i - 1))));
        }
        if row + 1 >= rows
            && i + 1 < project.paragraphs.len()
            && key(Modifiers::SHIFT, Key::ArrowDown)
        {
            return extend((project.paragraphs[i + 1].id, cursor.min(len_of(i + 1))));
        }
    }

    // Navegación entre párrafos en los bordes.
    if row == 0 && key(Modifiers::NONE, Key::ArrowUp) && i > 0 {
        return Some(Edit::Goto {
            i: i - 1,
            pos: Pos::End,
        });
    }
    if row + 1 >= rows && key(Modifiers::NONE, Key::ArrowDown) && i + 1 < project.paragraphs.len() {
        return Some(Edit::Goto {
            i: i + 1,
            pos: Pos::Start,
        });
    }
    if at_start && key(Modifiers::NONE, Key::ArrowLeft) && i > 0 {
        return Some(Edit::Goto {
            i: i - 1,
            pos: Pos::End,
        });
    }
    if at_end && key(Modifiers::NONE, Key::ArrowRight) && i + 1 < project.paragraphs.len() {
        return Some(Edit::Goto {
            i: i + 1,
            pos: Pos::Start,
        });
    }
    None
}

/// Reemplaza el texto del párrafo por el de la sugerencia, conservando los vínculos que quedan intactos y
/// añadiendo el de la sugerencia.
fn apply_suggestion(para: &mut Paragraph, pick: &Suggestion) {
    let old = std::mem::replace(&mut para.text, pick.text.clone());
    adjust_links(&mut para.links, &old, &para.text);
    if let Some(link) = pick.link {
        para.links
            .retain(|l| !(l.start < link.end && l.end > link.start));
        para.links.push(link);
        para.links.sort_by_key(|l| l.start);
        validate_links(&mut para.links, &para.text);
    }
}

fn apply_edit(state: &mut EditorState, project: &mut Project, edit: Edit) {
    match edit {
        Edit::Split { i, at } => {
            let para = &mut project.paragraphs[i];
            let at = at.min(para.text.chars().count());
            let byte = byte_index(&para.text, at);
            let tail = para.text.split_off(byte);
            let kind = if tail.is_empty() {
                para.kind.after_enter()
            } else {
                para.kind
            };
            let skip = tail.chars().count() - tail.trim_start().chars().count();
            let (head_links, tail_links) = split_links(&para.links, at, skip);
            para.links = head_links;
            let mut new = project.make_paragraph(kind, tail.trim_start());
            new.links = tail_links;
            let id = new.id;
            project.paragraphs.insert(i + 1, new);
            state.focus = Some((id, Pos::Start));
        }
        Edit::InsertAfter { i, kind } => {
            let new = project.make_paragraph(kind, "");
            let id = new.id;
            project.paragraphs.insert(i + 1, new);
            state.focus = Some((id, Pos::Start));
        }
        Edit::SetKind { i, kind } => {
            let para = &mut project.paragraphs[i];
            para.kind = kind;
            if kind.is_uppercase() {
                para.text = para.text.to_uppercase();
            }
            state.focus = Some((para.id, Pos::End));
        }
        Edit::MergeWithPrev { i } => {
            if i == 0 {
                return;
            }
            let removed = project.paragraphs.remove(i);
            let prev = &mut project.paragraphs[i - 1];
            let at = prev.text.chars().count();
            prev.text.push_str(&removed.text);
            prev.links.extend(shifted_links(&removed.links, at));
            validate_links(&mut prev.links, &prev.text);
            let prev_id = prev.id;
            for c in project
                .comments
                .iter_mut()
                .filter(|c| c.paragraph == removed.id)
            {
                c.paragraph = prev_id;
            }
            for t in project
                .breakdown_tags
                .iter_mut()
                .filter(|t| t.paragraph == removed.id)
            {
                t.paragraph = prev_id;
            }
            state.focus = Some((prev_id, Pos::At(at)));
        }
        Edit::MergeWithNext { i } => {
            if i + 1 >= project.paragraphs.len() {
                return;
            }
            let next = project.paragraphs.remove(i + 1);
            let para = &mut project.paragraphs[i];
            let at = para.text.chars().count();
            para.text.push_str(&next.text);
            para.links.extend(shifted_links(&next.links, at));
            validate_links(&mut para.links, &para.text);
            let id = para.id;
            for c in project
                .comments
                .iter_mut()
                .filter(|c| c.paragraph == next.id)
            {
                c.paragraph = id;
            }
            for t in project
                .breakdown_tags
                .iter_mut()
                .filter(|t| t.paragraph == next.id)
            {
                t.paragraph = id;
            }
            state.focus = Some((id, Pos::At(at)));
        }
        Edit::Goto { i, pos } => {
            state.focus = Some((project.paragraphs[i].id, pos));
        }
        Edit::Extend { anchor, head } => {
            state.multi = Some(MultiSel { anchor, head });
            state.focus = None;
        }
    }
}

/// Herramientas del guion en la barra de herramientas: deshacer, rehacer, tipo del párrafo actual y comentar.
pub fn toolbar(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    project: &mut Project,
    p: &Palette,
    show_right: &mut bool,
    settings: &crate::storage::Settings,
) -> bool {
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    let now = ctx.input(|i| i.time);
    let current = state
        .current_id()
        .and_then(|id| project.paragraphs.iter().position(|q| q.id == id));
    let mut picked = None;
    let mut changed = false;
    let (mut comment, mut mark, mut create) = (false, false, false);
    // Deshacer y rehacer los dibuja el marco; el zoom y Exportar van a la derecha (`toolbar_end`).
    crate::ui::kit::toolbar_groups(ui, "script_tools", &[7, 6, 2, 3, 1, 4], |ui, group| {
        match group {
            0 => {
                let label = current.map_or("Sin párrafo", |i| project.paragraphs[i].kind.label());
                ui.add_enabled_ui(current.is_some(), |ui| {
                    egui::ComboBox::from_id_salt("kind_combo")
                        .width(crate::ui::kit::fill_width(ui, 120.0, 170.0))
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (n, kind) in ParagraphKind::ALL.iter().enumerate() {
                                let active =
                                    current.is_some_and(|i| project.paragraphs[i].kind == *kind);
                                if ui
                                    .selectable_label(
                                        active,
                                        format!("{}    Ctrl+{}", kind.label(), (n + 1) % 10),
                                    )
                                    .clicked()
                                {
                                    picked = Some(*kind);
                                }
                            }
                        });
                });
            }
            1 => {
                comment |= widgets::icon_button(
                    ui,
                    p,
                    icon::CHAT_TEXT,
                    "Comentar este párrafo",
                    current.is_some(),
                )
                .clicked();
                mark |= widgets::icon_button(
                    ui,
                    p,
                    icon::TAG,
                    "Marcar para el desglose",
                    current.is_some(),
                )
                .clicked();
                create |= widgets::icon_button(
                    ui,
                    p,
                    icon::USER_PLUS,
                    "Crear personaje, locación, objeto… (Ctrl+Mayús+N)",
                    true,
                )
                .clicked();
            }
            2 => {
                // La llave bloquea o desbloquea la numeración; con letras provisorias, el tilde las fija.
                let locked = project.scene_numbers_locked;
                let resp = widgets::icon_button(
                    ui,
                    p,
                    icon::KEY,
                    if locked {
                        "Numeración bloqueada: tocá para desbloquearla"
                    } else {
                        "Bloquear la numeración: cada escena conserva su número"
                    },
                    true,
                );
                if locked {
                    ui.painter().rect_stroke(
                        resp.rect,
                        crate::ui::tokens::R_CONTROL,
                        Stroke::new(1.0, p.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if resp.clicked() {
                    if locked {
                        project.unlock_scene_numbers();
                    } else {
                        project.lock_scene_numbers();
                    }
                    state.numbering_changed = true;
                }
                if locked
                    && project.provisional_scene_count() > 0
                    && widgets::icon_button(
                        ui,
                        p,
                        icon::CHECK,
                        "Fijar las letras provisorias de las escenas",
                        true,
                    )
                    .clicked()
                {
                    project.fix_scene_numbers();
                    state.numbering_changed = true;
                }
            }
            3 => {
                // La bandera abre una revisión nueva (con el color siguiente); el tilde deja de marcar cambios.
                if widgets::icon_button(
                    ui,
                    p,
                    icon::FLAG,
                    "Nueva revisión: marca los cambios que se hagan desde ahora",
                    true,
                )
                .clicked()
                {
                    project.new_revision("", &crate::storage::today());
                    state.numbering_changed = true;
                }
                if project.active_revision.is_some()
                    && widgets::icon_button(ui, p, icon::CHECK, "Dejar de marcar cambios", true)
                        .clicked()
                {
                    project.active_revision = None;
                    state.numbering_changed = true;
                }
            }
            4 => {
                let now = ui.ctx().input(|i| i.time);
                let pending = pending_outline_scenes(project);
                let tip = if pending > 0 {
                    "Importar escaleta: trae los encabezados del tratamiento, sin el texto"
                } else {
                    "Importar escaleta: no hay escenas nuevas en el tratamiento"
                };
                if widgets::icon_button(ui, p, icon::DOWNLOAD_SIMPLE, tip, pending > 0).clicked() {
                    if state.snapshot.is_none() {
                        state.snapshot = Some(project.paragraphs.clone());
                    }
                    project.import_outline_into_script();
                    state.adopt_links(project);
                    state.commit(project, true, now);
                    changed = true;
                }
            }
            _ => {
                ui.menu_button(format!("{}  Ver  {}", icon::EYE, icon::CARET_DOWN), |ui| {
                    let mut bold = settings.bold_names;
                    if ui.checkbox(&mut bold, "Personajes en negrita").changed() {
                        state.set_bold = Some(bold);
                    }
                    let mut timeline = settings.show_timeline;
                    if ui
                        .checkbox(&mut timeline, "Mostrar la duración junto a la hoja")
                        .changed()
                    {
                        state.set_timeline = Some(timeline);
                    }
                    ui.separator();
                    format_tab(ui, state, project, p);
                });
            }
        }
    });
    if let (Some(kind), Some(i)) = (picked, current) {
        apply_edit(state, project, Edit::SetKind { i, kind });
        state.commit(project, true, now);
        changed = true;
    }
    if create {
        state.open_entity(project);
    }
    if let Some(form) = crate::create::dialog(ctx, &mut state.entity, project, p) {
        changed |= insert_created(ctx, state, project, &form, now);
    }
    if comment {
        state.tab_request = Some(2);
        *show_right = true;
    }
    if mark {
        state.tab_request = Some(1);
        *show_right = true;
    }
    changed
}

/// Crea lo que pide el formulario y escribe su nombre en el párrafo: en lugar del texto seleccionado, si lo había, o
/// en el cursor. Sin párrafo donde escribir, solo se crea la ficha.
fn insert_created(
    ctx: &egui::Context,
    state: &mut EditorState,
    project: &mut Project,
    form: &crate::create::NewEntity,
    now: f64,
) -> bool {
    let Some(target) = crate::create::create_entity(project, form) else {
        return false;
    };
    let Some(pid) = form.para.or_else(|| state.current_id()) else {
        return true;
    };
    let Some(i) = project.paragraphs.iter().position(|q| q.id == pid) else {
        return true;
    };
    let chars: Vec<char> = project.paragraphs[i].text.chars().collect();
    let (a, b, typed) = match form.replace {
        Some((a, b)) => {
            let (a, b) = (a.min(chars.len()), b.min(chars.len()));
            (
                a,
                b,
                chars[a..b].iter().collect::<String>().trim().to_string(),
            )
        }
        None => {
            let at = TextEditState::load(ctx, Id::unique(("paragraph", pid)))
                .and_then(|s| s.cursor.char_range())
                .map_or(chars.len(), |r| {
                    usize::from(r.primary.index).min(chars.len())
                });
            (at, at, "A".to_string())
        }
    };
    let mention = Mention {
        name: crate::create::entity_name(project, target).unwrap_or_default(),
        character: matches!(target, EntityRef::Character(_)),
        target: Some(target),
        detail: String::new(),
    };
    let mut name = format_mention(&mention, &typed, false);
    if project.paragraphs[i].kind.is_uppercase() {
        name = name.to_uppercase();
    }
    let lead = if a > 0 && chars[a - 1].is_alphanumeric() {
        " "
    } else {
        ""
    };
    let tail = if chars.get(b).is_some_and(|c| c.is_alphanumeric()) {
        " "
    } else {
        ""
    };
    let start = a + lead.chars().count();
    let end = start + name.chars().count();
    let text = chars[..a].iter().collect::<String>()
        + lead
        + &name
        + tail
        + &chars[b..].iter().collect::<String>();
    let pick = Suggestion {
        label: name,
        text,
        stay: false,
        cursor: end,
        enter: false,
        link: Some(Link { start, end, target }),
    };
    if state.snapshot.is_none() {
        state.snapshot = Some(project.paragraphs.clone());
    }
    apply_suggestion(&mut project.paragraphs[i], &pick);
    state.focus = Some((pid, Pos::At(end)));
    state.adopt_links(project);
    state.commit(project, true, now);
    true
}

/// Panel derecho: formato del texto y comentarios. Devuelve si cambió el proyecto.
pub fn right_panel(
    ui: &mut Ui,
    state: &mut EditorState,
    project: &mut Project,
    p: &Palette,
    tab: usize,
) -> bool {
    let tab = [RightTab::Scenes, RightTab::Card, RightTab::Comments][tab.min(2)];
    state.right_tab = tab;

    let comments_before = project.comments.clone();
    let case_before = (
        project.name_case,
        project
            .element_categories
            .iter()
            .map(|c| c.uppercase)
            .collect::<Vec<_>>(),
    );
    let breakdown_before = (
        project.breakdown_items.len(),
        project.breakdown_tags.len(),
        project.objects.len(),
    );
    let scenes_before = project.scenes.clone();
    (|ui: &mut Ui| match tab {
        RightTab::Scenes => scenes_tab(ui, state, project, p),
        RightTab::Card => card_tab(ui, state, project, p),
        RightTab::Comments => comments_tab(ui, state, project, p),
    })(ui);
    project.comments != comments_before
        || project.scenes != scenes_before
        || case_before
            != (
                project.name_case,
                project
                    .element_categories
                    .iter()
                    .map(|c| c.uppercase)
                    .collect::<Vec<_>>(),
            )
        || breakdown_before
            != (
                project.breakdown_items.len(),
                project.breakdown_tags.len(),
                project.objects.len(),
            )
        || std::mem::take(&mut state.numbering_changed)
}

/// Pestaña ESCENAS: el navegador del guion. Un clic lleva a la escena.
fn scenes_tab(ui: &mut Ui, state: &mut EditorState, project: &mut Project, p: &Palette) {
    let scenes = project.scenes();
    if scenes.is_empty() {
        crate::ui::kit::empty_state(
            ui,
            p,
            icon::LIST_NUMBERS,
            "Todavía no hay escenas. Escribí un encabezado de escena en el guion.",
            None,
        );
        return;
    }
    let total: f32 = scenes.iter().map(|s| s.seconds).sum();
    ui.label(
        RichText::new(format!(
            "{} escenas · {}",
            scenes.len(),
            crate::pages::fmt_duration(total)
        ))
        .size(12.0)
        .color(p.dim),
    );
    ui.add_space(crate::ui::tokens::SP_S);
    let current = state.current_id().and_then(|id| project.scene_of(id));
    for s in &scenes {
        let selected = current == Some(s.id);
        let title = if s.heading.is_empty() {
            "(sin encabezado)"
        } else {
            s.heading.as_str()
        };
        let eighths = crate::model::fmt_eighths(project.scene_eighths(s.id));
        let text = format!("{}. {}", s.number, title);
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), crate::ui::tokens::ROW_H),
            egui::Sense::click(),
        );
        if selected {
            ui.painter().rect_filled(
                rect,
                crate::ui::tokens::R_CONTROL,
                p.selection.linear_multiply(0.55),
            );
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, crate::ui::tokens::R_CONTROL, p.container_hi);
        }
        let color = if selected { p.accent } else { p.text };
        let painter = ui
            .painter()
            .with_clip_rect(rect.shrink2(egui::vec2(crate::ui::tokens::SP_XS, 0.0)));
        let eighths_galley =
            painter.layout_no_wrap(eighths, egui::FontId::proportional(12.0), p.dim);
        let eighths_w = eighths_galley.size().x;
        painter.galley(
            egui::pos2(
                rect.right() - crate::ui::tokens::SP_S - eighths_w,
                rect.center().y - eighths_galley.size().y / 2.0,
            ),
            eighths_galley,
            p.dim,
        );
        let text_clip = painter.with_clip_rect(egui::Rect::from_min_max(
            rect.min,
            egui::pos2(
                rect.right() - crate::ui::tokens::SP_M - eighths_w,
                rect.max.y,
            ),
        ));
        text_clip.text(
            egui::pos2(rect.left() + crate::ui::tokens::SP_S, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text.as_str(),
            egui::FontId::new(13.0, theme::bold_family()),
            color,
        );
        if resp
            .on_hover_text(text.as_str())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            state.jump_to(s.id);
        }
    }
}

/// Pestaña FICHA: la ficha de producción de la escena donde está el cursor y sus marcas de desglose.
fn card_tab(ui: &mut Ui, state: &mut EditorState, project: &mut Project, p: &Palette) {
    let Some(scene) = state.current_id().and_then(|id| project.scene_of(id)) else {
        crate::ui::kit::empty_state(
            ui,
            p,
            icon::CLIPBOARD_TEXT,
            "Poné el cursor en una escena para ver su ficha de producción.",
            None,
        );
        return;
    };
    let heading = project
        .script_scenes()
        .into_iter()
        .find(|(id, _)| *id == scene)
        .map(|(_, h)| h)
        .unwrap_or_default();
    let mut out = crate::scenes::Out::default();
    crate::scenes::card(
        ui,
        &mut state.scene_drafts,
        project,
        p,
        scene,
        &heading,
        &mut out,
    );
    // Los enlaces de la ficha llevan a otras pantallas, que atiende la app.
    state.scene_link = out.link.or(state.scene_link);
    crate::ui::kit::section(ui, p, "Desglose de la escena", None);
    breakdown_tab(ui, state, project, p);
}

fn section_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(p.dim));
    ui.add_space(tokens::SP_S);
}

/// Menú «Ver»: lo que no está en la barra de herramientas: las revisiones, el (CONT.) automático, los nombres en
/// mayúsculas y los atajos.
fn format_tab(ui: &mut Ui, state: &mut EditorState, project: &mut Project, p: &Palette) {
    ui.set_max_width(crate::ui::tokens::DIALOG_MAX);
    if !project.revisions.is_empty() {
        section_label(ui, p, "REVISIONES");
        for rev in project.revisions.iter().rev() {
            let on = project.active_revision == Some(rev.id);
            if ui
                .selectable_label(
                    on,
                    format!("{} — {} ({})", rev.color.label(), rev.name, rev.date),
                )
                .clicked()
                && !on
            {
                project.active_revision = Some(rev.id);
                state.numbering_changed = true;
            }
        }
        ui.add_space(tokens::SP_L);
    }
    section_label(ui, p, "PERSONAJES");
    if ui
        .checkbox(&mut project.auto_cont, "(CONT.) automático")
        .changed()
    {
        state.numbering_changed = true;
    }

    ui.add_space(tokens::SP_L);
    section_label(ui, p, "NOMBRES EN MAYÚSCULAS");
    // La lista va en un recuadro con desplazamiento propio: con muchas categorías no empuja los atajos hacia abajo.
    egui::Frame::new()
        .fill(p.field)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("name_case_list")
                .max_height(150.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // Sin fijar el ancho aquí: con la barra de desplazamiento visible, el contenido quedaba un poco más
                    // ancho que el espacio y el panel entero crecía un píxel por cuadro.
                    let nc = &mut project.name_case;
                    ui.checkbox(&mut nc.characters, format!("{}  Personajes", icon::USER));
                    ui.checkbox(&mut nc.locations, format!("{}  Locaciones", icon::HOUSE));
                    ui.checkbox(&mut nc.objects, format!("{}  Objetos clave", icon::CUBE));
                    ui.checkbox(&mut nc.worlds, format!("{}  Mundos", icon::GLOBE));
                    for cat in &mut project.element_categories {
                        ui.checkbox(&mut cat.uppercase, format!("{}  {}", cat.icon, cat.name));
                    }
                });
        });
}

fn comments_tab(ui: &mut Ui, state: &mut EditorState, project: &mut Project, p: &Palette) {
    // Lo comentado es lo seleccionado; sin selección, el párrafo donde está el cursor.
    let selection = state
        .sel
        .filter(|(pid, a, b)| a < b && project.paragraphs.iter().any(|q| q.id == *pid));
    let target = selection
        .map(|(pid, ..)| pid)
        .or_else(|| state.current_id());
    let quote = selection.and_then(|(pid, a, b)| {
        let text = &project.paragraphs.iter().find(|q| q.id == pid)?.text;
        Some(text.chars().skip(a).take(b - a).collect::<String>())
    });

    section_label(ui, p, "NUEVO COMENTARIO");
    match (&quote, target) {
        (Some(q), _) => {
            let shown: String = q.chars().take(60).collect();
            ui.label(
                RichText::new(format!("Sobre la selección: «{shown}»"))
                    .size(12.0)
                    .color(p.dim),
            );
        }
        (None, Some(_)) => {}
        (None, None) => {
            ui.label(
                RichText::new("Hacé clic en el guion para elegir dónde comentar.")
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
        target.is_some() && !state.comment_draft.trim().is_empty(),
        |ui| {
            if ui.button("Añadir comentario").clicked() {
                if let Some(pid) = target {
                    project.add_comment(pid, state.comment_draft.trim(), quote.clone());
                    state.comment_draft.clear();
                }
            }
        },
    );

    ui.add_space(tokens::SP_L);
    section_label(ui, p, "COMENTARIOS");
    // En el orden del guion.
    let order: Vec<(u64, String)> = project
        .paragraphs
        .iter()
        .map(|q| (q.id, q.text.clone()))
        .collect();
    let mut sorted: Vec<usize> = (0..project.comments.len())
        .filter(|&c| !project.comments[c].in_treatment)
        .collect();
    sorted.sort_by_key(|&c| {
        order
            .iter()
            .position(|(id, _)| *id == project.comments[c].paragraph)
            .unwrap_or(usize::MAX)
    });
    if sorted.is_empty() {
        ui.label(RichText::new("Todavía no hay comentarios.").color(p.dim));
    }
    let mut jump: Option<(u64, Option<String>)> = None;
    let mut remove = None;
    for ci in sorted {
        let c = &mut project.comments[ci];
        let current = state.current_id() == Some(c.paragraph);
        let cited = c.quote.clone().or_else(|| {
            order
                .iter()
                .find(|(id, _)| *id == c.paragraph)
                .map(|(_, t)| t.chars().take(40).collect::<String>())
        });
        egui::Frame::new()
            .fill(if current {
                theme::mix(p.container, p.accent, 0.1)
            } else {
                p.container
            })
            .stroke(Stroke::new(
                1.0,
                if current { p.accent } else { mix_border(p) },
            ))
            .corner_radius(4)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // Un clic en el texto del comentario lleva a esa línea (y selecciona lo citado).
                let quote_resp = ui.add(
                    egui::Label::new(
                        RichText::new(format!("«{}»", cited.unwrap_or_default()))
                            .size(12.0)
                            .color(p.dim),
                    )
                    .truncate()
                    .sense(egui::Sense::click()),
                );
                let body = if c.resolved {
                    RichText::new(&c.text).strikethrough().color(p.dim)
                } else {
                    RichText::new(&c.text)
                };
                let body_resp = ui.add(egui::Label::new(body).sense(egui::Sense::click()));
                if quote_resp.clicked() || body_resp.clicked() {
                    jump = Some((c.paragraph, c.quote.clone()));
                }
                quote_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                body_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
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
    if let Some((pid, quote)) = jump {
        state.jump_to_selection(project, pid, quote.as_deref());
    }
    if let Some(ci) = remove {
        project.comments.remove(ci);
    }
}

/// Trae al guion los encabezados de las escenas de la escaleta (sin el texto) que todavía no estén.
impl EditorState {
    /// Trae al guion los encabezados de la escaleta (sin el texto) que todavía no estén.
    pub fn import_outline(&mut self, project: &mut Project, now: f64) {
        if self.snapshot.is_none() {
            self.snapshot = Some(project.paragraphs.clone());
        }
        project.import_outline_into_script();
        self.adopt_links(project);
        self.commit(project, true, now);
    }
}

/// Cuántos encabezados de la escaleta todavía no están en el guion.
fn pending_outline_scenes(project: &Project) -> usize {
    project
        .outline
        .iter()
        .filter(|o| {
            !o.heading.trim().is_empty()
                && !o
                    .imported
                    .is_some_and(|pid| project.paragraphs.iter().any(|q| q.id == pid))
        })
        .count()
}

fn breakdown_tab(ui: &mut Ui, state: &mut EditorState, project: &mut Project, p: &Palette) {
    let selection = state
        .sel
        .filter(|(pid, a, b)| a < b && project.paragraphs.iter().any(|q| q.id == *pid));
    let quote = selection.and_then(|(pid, a, b)| {
        let text = &project.paragraphs.iter().find(|q| q.id == pid)?.text;
        Some(text.chars().skip(a).take(b - a).collect::<String>())
    });
    // Al cambiar de selección, el nombre del elemento parte del texto seleccionado.
    if let Some(q) = &quote {
        if *q != state.bd_quote {
            state.bd_quote = q.clone();
            state.bd_name = q.trim().to_string();
        }
    }

    section_label(ui, p, "MARCAR PARA EL DESGLOSE");
    match &quote {
        Some(q) => {
            let shown: String = q.chars().take(60).collect();
            ui.label(
                RichText::new(format!("Selección: «{shown}»"))
                    .size(12.0)
                    .color(p.dim),
            );
        }
        None => {}
    }
    ui.add_space(tokens::SP_S);

    let default_cat = project
        .breakdown_categories
        .iter()
        .find(|c| c.name == "Utilería")
        .or(project.breakdown_categories.first())
        .map(|c| c.id);
    if state
        .bd_category
        .is_none_or(|id| !project.breakdown_categories.iter().any(|c| c.id == id))
    {
        state.bd_category = default_cat;
    }
    let current_name = project
        .breakdown_categories
        .iter()
        .find(|c| Some(c.id) == state.bd_category)
        .map_or(String::new(), |c| c.name.clone());
    crate::ui::kit::combo_field(ui, p, "Categoría", "bd_category", &current_name, |ui| {
        for c in &project.breakdown_categories {
            let label =
                RichText::new(&c.name).color(Color32::from_rgb(c.color[0], c.color[1], c.color[2]));
            if ui
                .selectable_label(state.bd_category == Some(c.id), label)
                .clicked()
            {
                state.bd_category = Some(c.id);
            }
        }
    });
    ui.add_space(tokens::SP_M);
    widgets::field(ui, p, "Nombre del elemento", &mut state.bd_name, 1);
    ui.checkbox(&mut state.bd_all, "Marcar todas las menciones en el guion");
    ui.add_space(tokens::SP_S);
    let can_add =
        quote.is_some() && state.bd_category.is_some() && !state.bd_name.trim().is_empty();
    ui.add_enabled_ui(can_add, |ui| {
        if widgets::wide_button(ui, p, icon::TAG, "Añadir al desglose", ui.available_width())
            .clicked()
        {
            if let (Some((pid, ..)), Some(cat), Some(q)) = (selection, state.bd_category, &quote) {
                project.add_breakdown_tag(pid, q, cat, &state.bd_name, state.bd_all);
            }
        }
    });

    // Lo marcado en la escena donde está el cursor.
    ui.add_space(tokens::SP_L);
    section_label(ui, p, "MARCADO EN ESTA ESCENA");
    let scene = state.current_id().and_then(|id| project.scene_of(id));
    let mut rows: Vec<(u64, Color32, String, String, String)> = Vec::new();
    for t in &project.breakdown_tags {
        if project.scene_of(t.paragraph) != scene || scene.is_none() {
            continue;
        }
        let Some(item) = project.breakdown_items.iter().find(|i| i.id == t.item) else {
            continue;
        };
        let Some(cat) = project
            .breakdown_categories
            .iter()
            .find(|c| c.id == item.category)
        else {
            continue;
        };
        rows.push((
            t.id,
            Color32::from_rgb(cat.color[0], cat.color[1], cat.color[2]),
            item.name.clone(),
            cat.name.clone(),
            t.quote.clone(),
        ));
    }
    if rows.is_empty() {
        ui.label(RichText::new("Todavía no marcaste nada en esta escena.").color(p.dim));
    }
    let mut remove = None;
    for (id, color, name, cat, quote) in &rows {
        egui::Frame::new()
            .fill(p.container)
            .stroke(Stroke::new(1.0, mix_border(p)))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                crate::ui::kit::sides(
                    ui,
                    |ui| {
                        ui.label(RichText::new("■").color(*color));
                        ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
                    },
                    |ui| {
                        if widgets::icon_button(ui, p, icon::TRASH, "Quitar la marca", true)
                            .clicked()
                        {
                            remove = Some(*id);
                        }
                    },
                );
                ui.label(
                    RichText::new(format!("{cat} · «{quote}»"))
                        .size(12.0)
                        .color(p.dim),
                );
            });
        ui.add_space(tokens::SP_S);
    }
    if let Some(id) = remove {
        project.breakdown_tags.retain(|t| t.id != id);
    }
}

fn mix_border(p: &Palette) -> Color32 {
    theme::mix(p.container, p.border, 0.6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Settings;

    struct Harness {
        ctx: egui::Context,
        state: EditorState,
        project: Project,
        settings: Settings,
        time: f64,
        /// Textos que se dibujaron en el último cuadro.
        painted: Vec<String>,
        /// Último texto que se mandó al portapapeles.
        copied: Option<String>,
        /// Teclas modificadoras que se mantienen apretadas en los cuadros siguientes.
        modifiers: Modifiers,
    }

    fn collect_texts(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::epaint::Shape::Vec(items) => items.iter().for_each(|s| collect_texts(s, out)),
            _ => {}
        }
    }

    impl Harness {
        fn new() -> Self {
            let ctx = egui::Context::default();
            theme::install_fonts(&ctx);
            let settings = Settings::default();
            let mut h = Self {
                ctx,
                state: EditorState::default(),
                project: Project::default(),
                settings,
                time: 0.0,
                painted: Vec::new(),
                copied: None,
                modifiers: Modifiers::NONE,
            };
            h.frame(vec![]); // las fuentes se activan en el pase siguiente
            theme::apply(&h.ctx, &h.settings.scheme.palette(), 100);
            let id = h.project.paragraphs[0].id;
            h.state.jump_to(id);
            h.frame(vec![]);
            h.frame(vec![]);
            h
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.5;
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            input.focused = true;
            // En egui las teclas modificadoras llegan como un evento más.
            input
                .events
                .insert(0, egui::Event::ModifiersChanged(self.modifiers));
            let palette = self.settings.scheme.palette();
            let (state, project, settings) = (&mut self.state, &mut self.project, &self.settings);
            let mut output = self.ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    show(ui, state, project, settings, &palette);
                });
            });
            output.textures_delta.clear();
            self.painted.clear();
            for clipped in &output.shapes {
                collect_texts(&clipped.shape, &mut self.painted);
            }
            for command in &output.platform_output.commands {
                if let egui::OutputCommand::CopyText(text) = command {
                    self.copied = Some(text.clone());
                }
            }
        }

        fn key(&mut self, key: Key) {
            let ev = |pressed| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![ev(true), ev(false)]);
            self.frame(vec![]);
            self.frame(vec![]);
        }

        /// Dibuja el panel derecho en un panel redimensionable durante varios cuadros y devuelve su ancho en cada uno.
        fn right_panel_widths(&mut self, tab: RightTab, frames: usize) -> Vec<f32> {
            self.state.right_tab = tab;
            theme::apply(
                &self.ctx,
                &self.settings.scheme.palette(),
                self.settings.ui_zoom,
            );
            let mut widths = Vec::new();
            for _ in 0..frames {
                self.time += 0.5;
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    time: Some(self.time),
                    focused: true,
                    ..Default::default()
                };
                let palette = self.settings.scheme.palette();
                let (state, project) = (&mut self.state, &mut self.project);
                let mut width = 0.0;
                let mut output = self.ctx.run_ui(input, |ui| {
                    let panel = egui::Panel::right("panel_de_prueba")
                        .default_size(300.0)
                        .size_range(240.0..=560.0)
                        .resizable(true)
                        .frame(egui::Frame::new().fill(palette.panel).stroke(Stroke::new(
                            1.0,
                            theme::mix(palette.panel, palette.border, 0.5),
                        )))
                        .show(ui, |ui| {
                            right_panel(ui, state, project, &palette, 0);
                        });
                    width = panel.response.rect.width();
                });
                output.textures_delta.clear();
                widths.push(width);
            }
            widths
        }

        /// Una tecla con modificadores (Ctrl+A, por ejemplo), seguida de unos cuadros de espera.
        fn chord(&mut self, key: Key, modifiers: Modifiers) {
            let ev = |pressed| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            };
            self.modifiers = modifiers;
            self.frame(vec![ev(true), ev(false)]);
            self.frame(vec![]);
            self.frame(vec![]);
            self.modifiers = Modifiers::NONE;
        }

        fn rect_of(&self, index: usize) -> egui::Rect {
            self.state.rects[index].1
        }

        /// Arrastra el mouse desde `from` hasta `to` apretando el botón principal.
        fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(from)]);
            self.frame(vec![button(from, true)]);
            self.frame(vec![egui::Event::PointerMoved(from + egui::vec2(6.0, 0.0))]);
            self.frame(vec![egui::Event::PointerMoved(to)]);
            self.frame(vec![egui::Event::PointerMoved(to + egui::vec2(1.0, 0.0))]);
            self.frame(vec![button(to, false)]);
            self.frame(vec![]);
        }

        fn type_text(&mut self, text: &str) {
            self.frame(vec![egui::Event::Text(text.to_string())]);
            self.frame(vec![]);
        }

        fn kinds(&self) -> Vec<ParagraphKind> {
            self.project.paragraphs.iter().map(|p| p.kind).collect()
        }

        fn texts(&self) -> Vec<String> {
            self.project
                .paragraphs
                .iter()
                .map(|p| p.text.clone())
                .collect()
        }
    }

    #[test]
    fn writing_flow_with_enter_and_tab() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        h.type_text("int. casa - noche");
        assert_eq!(h.texts()[0], "INT. CASA - NOCHE");

        h.key(Key::Enter); // Encabezado → Acción
        assert_eq!(h.kinds(), vec![SceneHeading, Action]);
        h.key(Key::Tab); // Acción vacía → Personaje
        assert_eq!(h.kinds(), vec![SceneHeading, Character]);
        h.type_text("ana");
        h.key(Key::Enter); // Personaje → Diálogo
        assert_eq!(h.kinds(), vec![SceneHeading, Character, Dialogue]);
        h.key(Key::Tab); // Diálogo vacío → Acotación
        assert_eq!(h.kinds(), vec![SceneHeading, Character, Parenthetical]);
        h.type_text("bajo");
        h.key(Key::Enter); // Acotación → Diálogo
        assert_eq!(
            h.kinds(),
            vec![SceneHeading, Character, Parenthetical, Dialogue]
        );
        h.type_text("hola");
        assert_eq!(h.texts()[1..], ["ANA", "bajo", "hola"]);
    }

    #[test]
    fn enter_on_empty_action_starts_a_scene() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        assert_eq!(h.kinds(), vec![SceneHeading, Action]);
        h.key(Key::Enter); // acción vacía → encabezado
        assert_eq!(h.kinds(), vec![SceneHeading, SceneHeading]);
    }

    #[test]
    fn scene_heading_is_completed_with_suggestions() {
        let mut h = Harness::new();
        h.type_text("ex"); // aparece EXT. → Tab lo acepta
        h.key(Key::Tab);
        assert_eq!(h.texts()[0], "EXT. ");
        h.type_text("playa - no");
        h.key(Key::Enter); // acepta NOCHE y se queda en el encabezado
        assert_eq!(h.texts()[0], "EXT. PLAYA - NOCHE");
        assert_eq!(h.project.paragraphs.len(), 1);
    }

    #[test]
    fn scene_suggestion_stages() {
        let place = |name: &str, id: u64, detail: &str| Mention {
            name: name.into(),
            character: false,
            target: Some(EntityRef::Location(id)),
            detail: detail.into(),
        };
        let locs = vec![place("CASA DE ANA", 1, ""), place("PLAZA", 2, "")];
        let texts = |t: &str| {
            scene_suggestions(&locs, t, false)
                .into_iter()
                .map(|s| s.text)
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(""), vec!["INT./EXT. ", "INT. ", "EXT. "]);
        assert_eq!(texts("EX"), vec!["EXT. "]);
        assert_eq!(texts("INT. C"), vec!["INT. CASA DE ANA - "]);
        assert_eq!(
            texts("INT. CASA DE ANA - T"),
            vec!["INT. CASA DE ANA - TARDE"]
        );
        assert!(texts("INT. CASA DE ANA - NOCHE").is_empty());
        assert!(texts("CASA").is_empty());

        // Elegir una locación deja el vínculo sobre su nombre.
        let pick = scene_suggestions(&locs, "EXT. PL", false).remove(0);
        assert_eq!(
            pick.link,
            Some(Link {
                start: 5,
                end: 10,
                target: EntityRef::Location(2)
            })
        );

        // Dos locaciones con el mismo nombre: escrito entero y sin vincular, se ofrecen las dos.
        let twins = vec![place("FARO", 3, "el viejo"), place("FARO", 4, "el nuevo")];
        let labels: Vec<String> = scene_suggestions(&twins, "INT. FARO", false)
            .into_iter()
            .map(|s| s.label)
            .collect();
        assert_eq!(labels, vec!["FARO — el viejo", "FARO — el nuevo"]);
        assert!(scene_suggestions(&twins, "INT. FARO", true).is_empty());
    }

    fn item(height: f32, gap: f32, keep: bool) -> PageItem {
        PageItem {
            height,
            gap,
            keep_with_next: keep,
            ..Default::default()
        }
    }

    #[test]
    fn paragraphs_flow_onto_the_next_sheet_when_they_do_not_fit() {
        // Hojas de 100: tres párrafos de 40 con 10 de aire entre ellos no entran en una.
        let items = vec![
            item(40.0, 0.0, false),
            item(40.0, 10.0, false),
            item(40.0, 10.0, false),
        ];
        let at = paginate(&items, 100.0);
        assert_eq!(
            at[0],
            Placement {
                page: 0,
                y: 0.0,
                cut: None
            }
        );
        assert_eq!(
            at[1],
            Placement {
                page: 0,
                y: 50.0,
                cut: None
            }
        );
        // El tercero no entra: pasa arriba de la hoja 2, sin el aire de antes.
        assert_eq!(
            at[2],
            Placement {
                page: 1,
                y: 0.0,
                cut: None
            }
        );
    }

    #[test]
    fn a_character_cue_never_stays_alone_at_the_bottom() {
        // Escena, personaje y diálogo: el personaje (y la acotación) se van con su diálogo.
        let items = vec![
            item(60.0, 0.0, false), // acción
            item(10.0, 10.0, true), // personaje
            item(10.0, 0.0, true),  // acotación
            item(30.0, 0.0, false), // diálogo
        ];
        let at = paginate(&items, 100.0);
        assert_eq!(at[0].page, 0);
        // La cadena personaje + acotación + diálogo (60 de alto) no entra en los 30 que quedan.
        assert_eq!((at[1].page, at[2].page, at[3].page), (1, 1, 1));
        assert_eq!(at[1].y, 0.0);
        assert_eq!(at[3].y, 20.0);
    }

    #[test]
    fn a_paragraph_taller_than_a_sheet_starts_at_the_top_and_does_not_loop() {
        let items = vec![
            item(10.0, 0.0, false),
            item(250.0, 10.0, false),
            item(10.0, 10.0, false),
        ];
        let at = paginate(&items, 100.0);
        assert_eq!(
            at[1],
            Placement {
                page: 1,
                y: 0.0,
                cut: None
            }
        );
        assert_eq!(at[2].page, 2); // lo que sigue arranca en una hoja nueva
        assert_eq!(paginate(&[], 100.0), vec![]);
    }

    #[test]
    fn mention_suggestions_find_names_in_actions() {
        let mentions = test_mentions();
        let texts = |t: &str| {
            mention_suggestions(
                &mentions,
                t,
                t.chars().count(),
                &CaseOptions::uniform(true),
                &[],
            )
            .into_iter()
            .map(|s| s.text)
            .collect::<Vec<_>>()
        };
        // Se guarda en forma natural aunque la opción de mayúsculas esté activada.
        assert_eq!(texts("Entra en la CASA D"), vec!["Entra en la Casa de ana"]);
        assert_eq!(texts("Toma la lla"), vec!["Toma la llave maestra"]);
        assert!(texts("Va al la").is_empty()); // demasiado corto / sin coincidencias
        assert!(texts("Hola AN").is_empty()); // menos de tres letras
        assert!(texts("Mira a ANA y se va").is_empty()); // la palabra ya está completa
        // Con el cursor en medio de una palabra no se sugiere nada.
        assert!(
            mention_suggestions(
                &mentions,
                "toma la llave",
                9,
                &CaseOptions::uniform(true),
                &[]
            )
            .is_empty()
        );
        // En un diálogo también se sugiere.
        assert_eq!(
            suggestions(
                &[],
                &mentions,
                &CaseOptions::uniform(true),
                ParagraphKind::Dialogue,
                "Hola, jua",
                9,
                &[]
            )
            .len(),
            1
        );
    }

    fn mention(name: &str, character: bool) -> Mention {
        Mention {
            name: name.into(),
            character,
            target: None,
            detail: String::new(),
        }
    }

    fn test_mentions() -> Vec<Mention> {
        vec![
            mention("ANA", true),
            mention("JUAN CARLOS", true),
            mention("CASA DE ANA", false),
            mention("Llave maestra", false),
        ]
    }

    #[test]
    fn mention_is_stored_naturally_and_shown_per_the_option() {
        let mentions = test_mentions();
        let one = |t: &str, upper: bool| {
            mention_suggestions(
                &mentions,
                t,
                t.chars().count(),
                &CaseOptions::uniform(upper),
                &[],
            )
            .remove(0)
        };
        // Un personaje es un nombre propio: mayúscula inicial aunque se haya tipeado en minúsculas.
        assert_eq!(one("ve a juan", false).text, "ve a Juan Carlos");
        assert_eq!(one("ve a JUA", true).text, "ve a Juan Carlos");
        // La etiqueta de la lista sigue la opción; lo guardado no.
        assert_eq!(one("ve a juan", true).label, "JUAN CARLOS");
        assert_eq!(one("ve a juan", false).label, "Juan Carlos");
        // Locaciones y objetos: se respeta cómo se empezó a escribir.
        assert_eq!(
            one("entra en la cas", false).text,
            "entra en la casa de ana"
        );
        assert_eq!(one("Entra. Casa d", false).text, "Entra. Casa de ana");
        assert_eq!(one("toma la Lla", false).text, "toma la Llave maestra");
    }

    #[test]
    fn mention_is_accepted_with_tab_only() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        let lamp = h.project.add_object("Linterna");
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        h.type_text("Toma la lint");
        h.key(Key::Tab);
        assert_eq!(h.texts()[1], "Toma la linterna"); // guardado natural; en pantalla va en mayúsculas
        // Y queda vinculado al objeto: renombrarlo cambia este texto.
        let links = &h.project.paragraphs[1].links;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, EntityRef::Object(lamp));
        assert_eq!((links[0].start, links[0].end), (8, 16));
        assert_eq!(h.kinds(), vec![SceneHeading, Action]);
        h.key(Key::Enter); // sin sugerencias abiertas: párrafo nuevo
        assert_eq!(h.project.paragraphs.len(), 3);
    }

    #[test]
    fn display_job_changes_case_and_bold_but_not_the_text() {
        let font = FontId::monospace(14.0);
        let bold = FontId::new(14.0, theme::script_bold_family());
        let mentions = vec![mention("ANA", true), mention("Llave", false)];
        let text = "Ana toma la llave de ANAIS";
        let job_for = |upper: bool, bold_names: bool| {
            let spans = mention_spans(text, &[], &mentions, &CaseOptions::uniform(upper));
            styled_job(
                text,
                &font,
                &bold,
                Color32::WHITE,
                300.0,
                14.0,
                &spans,
                bold_names,
                false,
                Align::LEFT,
            )
        };
        assert_eq!(job_for(true, false).text, "ANA toma la LLAVE de ANAIS");
        assert_eq!(job_for(false, false).text, "Ana toma la llave de ANAIS");
        let bolds = |job: &LayoutJob| -> Vec<String> {
            job.sections
                .iter()
                .filter(|s| s.format.font_id == bold)
                .map(|s| job.text[s.byte_range.start.0..s.byte_range.end.0].to_string())
                .collect()
        };
        assert_eq!(bolds(&job_for(true, true)), vec!["ANA"]); // ANAIS no es el personaje
        assert!(bolds(&job_for(true, false)).is_empty());
    }

    #[test]
    fn a_selected_word_in_the_script_becomes_a_location() {
        let mut h = Harness::new();
        h.project.paragraphs[0].kind = ParagraphKind::Action;
        h.project.paragraphs[0].text = "Ana entra al baño.".into();
        let pid = h.project.paragraphs[0].id;
        h.state.sel = Some((pid, 13, 17));
        h.state.open_entity(&h.project);
        let mut form = h.state.entity.take().unwrap();
        assert_eq!(form.name, "baño");
        form.kind = crate::create::Kind::Location;
        assert!(insert_created(
            &h.ctx,
            &mut h.state,
            &mut h.project,
            &form,
            1.0
        ));
        assert_eq!(h.project.paragraphs[0].text, "Ana entra al baño.");
        assert_eq!(h.project.location_profiles[0].name, "BAÑO");
        let link = h.project.paragraphs[0].links[0];
        assert_eq!((link.start, link.end), (13, 17));
        // Sin selección, el nombre se escribe en el cursor.
        let mut again = crate::create::NewEntity {
            name: "llave".into(),
            kind: crate::create::Kind::Object,
            ..Default::default()
        };
        again.para = Some(pid);
        assert!(insert_created(
            &h.ctx,
            &mut h.state,
            &mut h.project,
            &again,
            2.0
        ));
        assert!(h.project.paragraphs[0].text.contains("Llave"));
        assert_eq!(h.project.objects.len(), 1);
    }

    #[test]
    fn a_character_suggestion_can_be_chosen_with_a_click() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        h.project.new_character("Juan");
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        h.key(Key::Tab); // acción vacía → personaje
        h.type_text("ju");
        h.frame(vec![]);
        let list = h
            .ctx
            .memory(|m| m.area_rect(Id::unique("character_suggestions")))
            .expect("se ofrece JUAN");
        let at = list.min + egui::vec2(20.0, 12.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        h.frame(vec![egui::Event::PointerMoved(at)]);
        h.frame(vec![button(true)]);
        h.frame(vec![button(false)]);
        h.frame(vec![]);
        let cue = h
            .project
            .paragraphs
            .iter()
            .find(|q| q.kind == Character)
            .expect("hay un personaje");
        assert_eq!(cue.text, "JUAN");
    }

    #[test]
    fn comment_range_follows_the_quote() {
        assert_eq!(
            comment_range("hola mundo cruel", Some("mundo")),
            Some((5, 10))
        );
        assert_eq!(comment_range("hola", None), Some((0, 4)));
        assert_eq!(comment_range("hola", Some("otra cosa")), None);
    }

    #[test]
    fn undo_keeps_the_current_name_after_a_rename() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        h.key(Key::Tab); // Acción vacía → Personaje
        h.type_text("ana");
        h.key(Key::Enter);
        h.type_text("hola");
        assert_eq!(h.kinds(), vec![SceneHeading, Character, Dialogue]);
        // Al soltar el foco, el encabezado quedó vinculado a una ficha nueva.
        assert_eq!(h.project.characters.len(), 1);
        assert_eq!(h.project.paragraphs[1].links.len(), 1);

        // Se renombra desde la ficha (fuera del editor).
        h.project.characters[0].name = "MARTA".into();
        h.project.sync_links();
        h.state.resync_snapshot(&h.project);
        assert_eq!(h.project.paragraphs[1].text, "MARTA");

        // Deshacer hasta el principio: en ningún estado vuelve a aparecer «ANA».
        let mut steps = 0;
        while h.state.undo(&mut h.project) {
            steps += 1;
            assert!(
                h.project.paragraphs.iter().all(|p| p.text != "ANA"),
                "paso {steps}"
            );
        }
        assert!(steps >= 2);
        // Y al rehacer tampoco.
        while h.state.redo(&mut h.project) {
            assert!(h.project.paragraphs.iter().all(|p| p.text != "ANA"));
        }
        assert!(
            h.project
                .paragraphs
                .iter()
                .any(|p| p.kind == Character && p.text == "MARTA")
        );
        assert_eq!(
            h.project.characters.len(),
            1,
            "no aparece un personaje fantasma"
        );
    }

    #[test]
    fn two_characters_with_the_same_name_are_offered_with_their_details() {
        let twin = |id, detail: &str| Mention {
            name: "JUAN".into(),
            character: true,
            target: Some(EntityRef::Character(id)),
            detail: detail.into(),
        };
        let mentions = vec![twin(1, "el padre"), twin(2, "el hijo")];
        // Escrito entero y todavía sin vincular: se ofrecen los dos para elegir.
        let list = character_suggestions(&mentions, "JUAN", false);
        let labels: Vec<&str> = list.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["JUAN — el padre", "JUAN — el hijo"]);
        assert_eq!(list[1].link.unwrap().target, EntityRef::Character(2));
        // Ya vinculado, no hace falta elegir otra vez.
        assert!(character_suggestions(&mentions, "JUAN", true).is_empty());
        // Un solo Juan: escrito entero, nada que sugerir.
        assert!(character_suggestions(&mentions[..1], "JUAN", false).is_empty());
    }

    #[test]
    fn heading_location_is_linked_and_follows_a_rename() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        let casa = h.project.add_location("Casa de Ana");
        h.type_text("int. cas");
        h.key(Key::Tab); // acepta la locación sugerida
        assert_eq!(h.texts()[0], "INT. CASA DE ANA - ");
        let links = &h.project.paragraphs[0].links;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, EntityRef::Location(casa));
        assert_eq!((links[0].start, links[0].end), (5, 16));
        h.type_text("noche");
        h.key(Key::Enter);
        assert_eq!(h.kinds(), vec![SceneHeading, Action]);
        assert_eq!(h.texts()[0], "INT. CASA DE ANA - NOCHE");

        // La ficha de la locación cambia de nombre: el encabezado la sigue.
        h.project.location_mut(casa).unwrap().name = "LA CASONA".into();
        h.project.sync_links();
        assert_eq!(h.texts()[0], "INT. LA CASONA - NOCHE");
    }

    #[test]
    fn a_heading_typed_by_hand_creates_its_location_when_focus_leaves() {
        let mut h = Harness::new();
        h.type_text("int. faro - noche");
        h.key(Key::Enter);
        h.type_text("Algo pasa.");
        assert_eq!(h.project.location_profiles.len(), 1);
        assert_eq!(h.project.location_profiles[0].name, "FARO");
        assert_eq!(h.project.paragraphs[0].links.len(), 1);
    }

    #[test]
    fn the_timeline_shows_minutes_scenes_and_the_total() {
        let mut h = Harness::new();
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        // Una acción larga: ocupa más de un minuto de guion.
        h.type_text(&"Pasa algo importante y se alarga la escena. ".repeat(60));
        h.frame(vec![]);
        assert!(h.painted.iter().any(|t| t == "0:00"), "{:?}", h.painted);
        assert!(
            h.painted.iter().any(|t| t.starts_with("Total ")),
            "{:?}",
            h.painted
        );
        // El cursor muestra a qué minuto corresponde donde está.
        assert!(h.painted.iter().any(|t| t == "0:00" || t.contains(':')));

        // Apagada, no se dibuja nada de eso.
        h.settings.show_timeline = false;
        h.frame(vec![]);
        assert!(!h.painted.iter().any(|t| t.starts_with("Total ")));
    }

    /// Un guion de tres párrafos para probar la selección.
    fn three_paragraphs() -> Harness {
        let mut h = Harness::new();
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        h.type_text("Primera acción larga.");
        h.key(Key::Enter);
        h.type_text("Segunda acción.");
        h.frame(vec![]);
        h
    }

    #[test]
    fn select_all_then_copy_cut_and_type_over_it() {
        let mut h = three_paragraphs();
        assert_eq!(h.project.paragraphs.len(), 3);
        h.chord(Key::A, Modifiers::COMMAND);
        assert!(h.state.multi.is_some(), "Ctrl+A selecciona todo el guion");

        // Copiar lleva los tres párrafos, separados por saltos de línea.
        h.frame(vec![egui::Event::Copy]);
        assert_eq!(
            h.copied.as_deref(),
            Some("INT. CASA - NOCHE\nPrimera acción larga.\nSegunda acción.")
        );
        assert!(h.state.multi.is_some(), "copiar no suelta la selección");

        // Escribir encima reemplaza todo por lo escrito, en un solo párrafo.
        h.frame(vec![egui::Event::Text("Nuevo".into())]);
        h.frame(vec![]);
        assert_eq!(h.texts(), vec!["NUEVO"]); // el primero es un encabezado: queda en mayúsculas
        assert!(h.state.multi.is_none());

        // Y se puede deshacer.
        assert!(h.state.undo(&mut h.project));
        assert_eq!(h.project.paragraphs.len(), 3);
    }

    #[test]
    fn delete_removes_the_whole_selection() {
        let mut h = three_paragraphs();
        h.chord(Key::A, Modifiers::COMMAND);
        h.key(Key::Delete);
        assert_eq!(h.texts(), vec![""]);
        assert!(h.state.multi.is_none());
    }

    #[test]
    fn dragging_the_mouse_across_paragraphs_selects_them() {
        let mut h = three_paragraphs();
        assert_eq!(h.state.rects.len(), 3);
        // Desde el medio del segundo párrafo hasta el medio del tercero.
        let from = h.rect_of(1).center();
        let to = h.rect_of(2).center();
        h.drag(from, to);
        let multi = h
            .state
            .multi
            .expect("el arrastre entre párrafos crea una selección");
        assert_eq!(multi.anchor.0, h.project.paragraphs[1].id);
        assert_eq!(multi.head.0, h.project.paragraphs[2].id);

        // Lo seleccionado es la cola del segundo párrafo más el comienzo del tercero.
        let sel = h.state.multi.unwrap();
        let text = h.project.range_text(sel.anchor, sel.head);
        assert!(text.contains('\n'), "{text:?}");

        // Borrar une lo que queda en un solo párrafo.
        h.key(Key::Backspace);
        assert_eq!(h.project.paragraphs.len(), 2);
        assert!(h.state.multi.is_none());
    }

    #[test]
    fn clicking_elsewhere_releases_the_selection_and_other_fields_keep_the_keyboard() {
        let mut h = three_paragraphs();
        h.chord(Key::A, Modifiers::COMMAND);
        assert!(h.state.multi.is_some());
        // Un clic en un párrafo suelta la selección.
        let at = h.rect_of(0).center();
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        h.frame(vec![egui::Event::PointerMoved(at)]);
        h.frame(vec![button(true)]);
        h.frame(vec![button(false)]);
        assert!(h.state.multi.is_none());
        assert_eq!(h.project.paragraphs.len(), 3); // y no se borró nada
    }

    #[test]
    fn a_long_script_is_split_into_numbered_sheets() {
        let mut h = Harness::new();
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        for n in 0..120 {
            let para = h.project.make_paragraph(
                ParagraphKind::Action,
                &format!("Línea de acción número {n}."),
            );
            h.project.paragraphs.push(para);
        }
        h.frame(vec![]);
        h.frame(vec![]);
        let pages = ["2.", "3.", "4."];
        assert!(
            pages.iter().all(|n| h.painted.iter().any(|t| t == n)),
            "{:?}",
            h.painted
                .iter()
                .filter(|t| t.ends_with('.'))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shift_arrows_extend_the_selection_into_the_next_paragraph() {
        let mut h = three_paragraphs();
        let (second, third) = (h.project.paragraphs[1].id, h.project.paragraphs[2].id);
        // Cursor al final del segundo párrafo: Mayús + → pasa al comienzo del tercero.
        h.state.jump_to(second);
        h.frame(vec![]);
        h.chord(Key::ArrowRight, Modifiers::SHIFT);
        let sel = h
            .state
            .multi
            .expect("Mayús + flecha en el borde crea una selección de varios párrafos");
        assert_eq!((sel.anchor.0, sel.head), (second, (third, 0)));
        // Otra vez: un carácter más dentro del tercero.
        h.chord(Key::ArrowRight, Modifiers::SHIFT);
        assert_eq!(h.state.multi.unwrap().head, (third, 1));
        h.frame(vec![egui::Event::Copy]);
        assert_eq!(h.copied.as_deref(), Some("\nS"));
        // Mayús + ← la achica.
        h.chord(Key::ArrowLeft, Modifiers::SHIFT);
        assert_eq!(h.state.multi.unwrap().head, (third, 0));
        // Sin Mayús, una flecha suelta la selección y deja el cursor en el extremo correspondiente.
        h.chord(Key::ArrowLeft, Modifiers::NONE);
        assert!(h.state.multi.is_none());
        assert_eq!(h.project.paragraphs.len(), 3);
    }

    #[test]
    fn shift_down_and_up_move_the_selection_by_lines() {
        let mut h = three_paragraphs();
        let (first, second, third) = (
            h.project.paragraphs[0].id,
            h.project.paragraphs[1].id,
            h.project.paragraphs[2].id,
        );
        h.state.jump_to(second);
        h.frame(vec![]);
        h.chord(Key::ArrowDown, Modifiers::SHIFT);
        let head = h.state.multi.expect("Mayús + ↓ en la última línea").head;
        assert_eq!(head.0, third);
        // Con la selección activa, otra vez hacia arriba vuelve y sigue hasta el encabezado.
        h.chord(Key::ArrowUp, Modifiers::SHIFT);
        h.chord(Key::ArrowUp, Modifiers::SHIFT);
        assert_eq!(h.state.multi.unwrap().head.0, first);
    }

    #[test]
    fn shift_click_extends_the_selection_from_the_cursor() {
        let mut h = three_paragraphs();
        let (second, third) = (h.project.paragraphs[1].id, h.project.paragraphs[2].id);
        h.state.jump_to(second);
        h.frame(vec![]);
        h.frame(vec![]);
        let at = h.rect_of(2).center();
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::SHIFT,
        };
        h.modifiers = Modifiers::SHIFT;
        h.frame(vec![egui::Event::PointerMoved(at)]);
        h.frame(vec![button(true)]);
        h.frame(vec![button(false)]);
        h.modifiers = Modifiers::NONE;
        let sel = h.state.multi.expect("Mayús + clic en otro párrafo");
        assert_eq!((sel.anchor.0, sel.head.0), (second, third));
        // Un clic común la suelta.
        let plain = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        h.frame(vec![plain(true)]);
        h.frame(vec![plain(false)]);
        assert!(h.state.multi.is_none());
    }

    #[test]
    fn the_right_panel_keeps_its_width_instead_of_growing_on_its_own() {
        // Con distintos zooms: el defecto que hacía crecer el panel dependía del redondeo a píxeles.
        for zoom in [70, 90, 100, 105, 110, 115, 125, 140, 175, 200] {
            for tab in [RightTab::Scenes, RightTab::Comments, RightTab::Card] {
                let mut h = three_paragraphs();
                h.settings.ui_zoom = zoom;
                // Con contenido que llene cada pestaña: categorías, comentarios y marcas del desglose.
                for n in 0..6 {
                    h.project.add_element_category(
                        &format!("Categoría con un nombre bastante largo {n}"),
                        crate::icon::PAW_PRINT,
                    );
                }
                let pid = h.project.paragraphs[1].id;
                h.project.add_comment(pid, "Un comentario largo que ocupa varias líneas del panel para probar el ajuste de texto", None);
                let widths = h.right_panel_widths(tab, 12);
                let (first, last) = (widths[1], *widths.last().unwrap());
                assert!(
                    (last - first).abs() < 0.5,
                    "con zoom {zoom} la pestaña {tab:?} sigue creciendo: {widths:?}"
                );
                assert!(
                    (last - 300.0).abs() < 3.0,
                    "con zoom {zoom} la pestaña {tab:?} cambió de ancho: {widths:?}"
                );
            }
        }
    }

    #[test]
    fn pasting_several_lines_creates_paragraphs() {
        let mut h = three_paragraphs();
        // El cursor queda al final del último párrafo; se pega texto de tres líneas.
        h.frame(vec![egui::Event::Paste("Uno\nDos\nTres".into())]);
        h.frame(vec![]);
        let texts = h.texts();
        assert_eq!(texts.len(), 5, "{texts:?}");
        assert_eq!(texts[2], "Segunda acción.Uno");
        assert_eq!(texts[3], "Dos");
        assert_eq!(texts[4], "Tres");
    }

    #[test]
    fn backspace_and_undo() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        h.type_text("int. casa - noche");
        h.key(Key::Enter);
        h.key(Key::Tab);
        assert_eq!(h.kinds(), vec![SceneHeading, Character]);
        h.key(Key::Backspace); // vacío → vuelve a Acción
        assert_eq!(h.kinds(), vec![SceneHeading, Action]);
        h.key(Key::Backspace); // ya es Acción → se borra
        assert_eq!(h.kinds(), vec![SceneHeading]);

        assert!(h.state.undo(&mut h.project));
        assert_eq!(h.kinds(), vec![SceneHeading, Action]);
        assert!(h.state.redo(&mut h.project));
        assert_eq!(h.kinds(), vec![SceneHeading]);
    }

    #[test]
    fn a_long_paragraph_is_split_between_sheets_without_leaving_a_gap_or_lone_lines() {
        let long = |rows: u32| PageItem {
            height: rows as f32 * 10.0,
            gap: 10.0,
            row_h: 10.0,
            ..Default::default()
        };
        // Una hoja de 10 renglones: lo que sobra de un párrafo de 25 sigue en las dos hojas siguientes.
        let at = paginate(&[long(25), long(2)], 100.0);
        assert_eq!(
            at[0],
            Placement {
                page: 0,
                y: 0.0,
                cut: Some(10)
            }
        );
        assert_eq!(at[1].page, 2);
        assert_eq!(at[1].y, 50.0 + 10.0, "sigue donde terminó el párrafo");
        // Con renglones de sobra en la hoja, el párrafo empieza ahí y se parte.
        let at = paginate(&[long(4), long(12)], 100.0);
        assert_eq!(
            at[1],
            Placement {
                page: 0,
                y: 50.0,
                cut: Some(5)
            }
        );
        assert_eq!(at[1].cut.map(|c| 12 - c), Some(7));
        // Nunca queda un renglón solo arriba o abajo: si no entran dos, o si sobraría uno, pasa entero a la hoja siguiente.
        let at = paginate(&[long(8), long(3)], 100.0);
        assert_eq!(
            at[1],
            Placement {
                page: 1,
                y: 0.0,
                cut: None
            }
        );
        let at = paginate(&[long(6), long(4)], 100.0);
        assert_eq!(
            at[1],
            Placement {
                page: 1,
                y: 0.0,
                cut: None
            }
        );
    }

    #[test]
    fn a_paragraph_longer_than_a_page_makes_more_sheets() {
        let mut h = Harness::new();
        h.project.paragraphs[0].kind = ParagraphKind::Action;
        h.project.paragraphs[0].text = (0..600)
            .map(|n| format!("palabra{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(
            ["2.", "3."]
                .iter()
                .all(|n| h.painted.iter().any(|t| t == n)),
            "{:?}",
            h.painted
                .iter()
                .filter(|t| t.ends_with('.'))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_view_zoom_scales_the_sheet_without_changing_how_much_fits_on_it() {
        use crate::storage::Paper;
        let at = |zoom: u32, paper: Paper| {
            let m = Metrics::new(crate::storage::SCRIPT_PT as f32, zoom, paper);
            (
                m.inch,
                (m.page.text_h() * m.inch / m.size + 1e-3).floor() as u32,
                (m.page.text_w() * m.inch / m.char_w + 1e-3).floor() as u32,
            )
        };
        // Carta: 54 renglones y 60 caracteres. A4 (1,5 in a la izquierda, 0,77 in a la derecha, 2,5 cm arriba y abajo): 58 renglones y 60 caracteres.
        for zoom in [40, 100, 300] {
            assert_eq!(at(zoom, Paper::Letter).1, 54);
            assert_eq!(at(zoom, Paper::Letter).2, 60);
            assert_eq!(at(zoom, Paper::A4).1, 58);
            assert_eq!(at(zoom, Paper::A4).2, 60);
        }
        assert_eq!(at(200, Paper::A4).0, 2.0 * at(100, Paper::A4).0);
    }

    #[test]
    fn letter_note_act_and_sequence_formats_are_chosen_with_ctrl_and_laid_out_apart() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        for (key, kind) in [
            (Key::Num7, Lyrics),
            (Key::Num8, Note),
            (Key::Num9, Act),
            (Key::Num0, Sequence),
            (Key::Num6, Transition),
        ] {
            h.chord(key, Modifiers::COMMAND);
            assert_eq!(h.kinds(), vec![kind], "Ctrl+{key:?}");
        }
        assert_eq!(ParagraphKind::ALL.len(), 10);
        // Actos y secuencias van centrados y se escriben en mayúsculas; las notas y los actos no suman duración.
        let m = Metrics::new(12.0, 100, crate::storage::Paper::A4);
        assert_eq!(m.geometry(Act, "").0, 0.0);
        assert_eq!(m.geometry(Lyrics, "").0, m.geometry(Dialogue, "").0);
        assert_eq!(
            m.gap_before(Lyrics, Some(Lyrics)),
            0.0,
            "los renglones de una letra van pegados"
        );
        assert!(m.gap_before(Lyrics, Some(Action)) > 0.0);
        assert_eq!(
            m.gap_before(Act, Some(Action)),
            m.gap_before(SceneHeading, Some(Action))
        );
        assert!(
            Act.is_uppercase()
                && Sequence.is_uppercase()
                && !Lyrics.is_uppercase()
                && !Note.is_uppercase()
        );
        // Enter en una letra sigue con otro renglón de letra.
        assert_eq!(Lyrics.after_enter(), Lyrics);
        // Transición: a la derecha, con su borde en 7,1 in de la hoja. Personaje: 3,8 in de ancho sin pasar el margen.
        let a4 = Metrics::new(12.0, 100, crate::storage::Paper::A4);
        assert!((a4.geometry(Transition, "").1 / a4.inch - 5.6).abs() < 1e-3);
        assert!((a4.geometry(Character, "").1 / a4.inch - 3.8).abs() < 1e-3);
        assert!(
            (a4.geometry(Character, "").0 + a4.geometry(Character, "").1) / a4.inch
                <= a4.page.text_w() + 1e-3
        );
    }

    #[test]
    fn transitions_are_flush_right_and_acts_are_centered_inside_their_boxes() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        let a = h.project.make_paragraph(Action, "Una acción.");
        let t = h.project.make_paragraph(Transition, "CORTE A:");
        let act = h.project.make_paragraph(Act, "ACTO I");
        h.project.paragraphs = vec![a, t, act];
        h.frame(vec![]);
        h.frame(vec![]);
        // Borde izquierdo y derecho del texto dibujado, comparado con el cuadro del párrafo.
        let ink = |h: &Harness, n: usize| {
            let g = &h.state.last_geoms[n];
            let row = &g.galley.rows[0];
            let (left, right) = (g.pos.x + row.rect().min.x, g.pos.x + row.rect().max.x);
            (left - g.rect.left(), g.rect.right() - right)
        };
        let (gap_left, gap_right) = ink(&h, 1);
        assert!(
            gap_right < 2.0 && gap_left > 50.0,
            "la transición va pegada a la derecha: {gap_left} {gap_right}"
        );
        let (gap_left, gap_right) = ink(&h, 2);
        assert!(
            gap_left > 50.0 && (gap_left - gap_right).abs() < 3.0,
            "el acto va centrado: {gap_left} {gap_right}"
        );
        let (gap_left, _) = ink(&h, 0);
        assert!(gap_left < 2.0, "la acción va a la izquierda");
    }

    #[test]
    fn a_split_dialogue_reserves_a_line_for_more_and_cont_and_cuts_after_a_sentence() {
        let dialogue = |rows: u32, ends: Vec<bool>| PageItem {
            height: rows as f32 * 10.0,
            gap: 10.0,
            row_h: 10.0,
            dialogue: true,
            sentence_ends: ends,
            ..Default::default()
        };
        // 25 renglones en hojas de 10: 9 + «(MÁS)», luego «(CONT.)» + 8 + «(MÁS)», y «(CONT.)» + 8.
        let at = paginate(
            &[
                dialogue(25, vec![]),
                PageItem {
                    height: 10.0,
                    gap: 0.0,
                    ..Default::default()
                },
            ],
            100.0,
        );
        assert_eq!(
            at[0],
            Placement {
                page: 0,
                y: 0.0,
                cut: Some(9)
            }
        );
        assert_eq!(at[1].page, 2);
        assert_eq!(
            at[1].y, 90.0,
            "después del renglón del nombre con (CONT.) y los 8 renglones"
        );
        assert_eq!(split_plan(25, 9, 10, true), vec![9, 8, 8]);
        assert_eq!(split_plan(25, 10, 10, false), vec![10, 10, 5]);
        // Si un renglón cerca del final termina una oración, el corte cae ahí.
        let mut ends = vec![false; 25];
        ends[6] = true;
        let at = paginate(&[dialogue(25, ends)], 100.0);
        assert_eq!(at[0].cut, Some(7));
        // Con la oración demasiado lejos, se corta donde se llena la hoja.
        let mut far = vec![false; 25];
        far[2] = true;
        assert_eq!(paginate(&[dialogue(25, far)], 100.0)[0].cut, Some(9));
        // Nunca quedan menos de dos renglones del lado de la hoja siguiente.
        assert_eq!(paginate(&[dialogue(11, vec![])], 100.0)[0].cut, Some(9));
        assert_eq!(paginate(&[dialogue(10, vec![])], 100.0)[0].cut, None);
        assert!(
            ends_sentence("Eso es todo.")
                && ends_sentence("«¿Vamos?»")
                && !ends_sentence("y entonces")
        );
    }

    #[test]
    fn a_long_dialogue_shows_more_at_the_foot_and_the_speaker_with_cont_on_the_next_sheet() {
        use ParagraphKind::*;
        let mut h = Harness::new();
        let c = h.project.make_paragraph(Character, "ANA");
        let long = (0..700)
            .map(|n| format!("palabra{n}."))
            .collect::<Vec<_>>()
            .join(" ");
        let d = h.project.make_paragraph(Dialogue, &long);
        h.project.paragraphs = vec![c, d];
        h.frame(vec![]);
        h.frame(vec![]);
        let count =
            |h: &Harness, text: &str| h.painted.iter().filter(|t| t.as_str() == text).count();
        assert!(
            count(&h, "(MÁS)") >= 1,
            "{:?}",
            h.painted
                .iter()
                .filter(|t| t.len() < 14)
                .collect::<Vec<_>>()
        );
        assert!(count(&h, "ANA (CONT.)") >= 1);
    }

    #[test]
    fn the_sheets_and_paragraphs_changed_in_a_revision_show_its_color_and_stars() {
        let mut h = Harness::new();
        let a = h
            .project
            .make_paragraph(ParagraphKind::Action, "Una acción.");
        let b = h
            .project
            .make_paragraph(ParagraphKind::Action, "Otra acción.");
        h.project.paragraphs = vec![a, b];
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(
            !h.painted.iter().any(|t| t.starts_with("Revisión")),
            "sin revisión nada se marca"
        );
        h.project.new_revision("Primera entrega", "2026-10-30");
        h.frame(vec![]);
        assert!(
            !h.painted.iter().any(|t| t == "*"),
            "recién abierta no hay cambios"
        );
        h.project.paragraphs[1].text = "Otra acción cambiada.".into();
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(
            h.painted
                .iter()
                .any(|t| t == "Revisión azul: Primera entrega"),
            "{:?}",
            h.painted
        );
        assert_eq!(h.painted.iter().filter(|t| *t == "*").count(), 1);
    }
}
