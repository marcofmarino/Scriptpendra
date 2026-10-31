//! Carpeta del proyecto y copia de las imágenes vinculadas.
//!
//! Un proyecto nuevo vive en su propia carpeta:
//!
//! ```text
//! MiGuion/
//!   MiGuion.spd
//!   recursos/        ← copias de las imágenes (portada, fotos, galerías)
//! ```
//!
//! Las imágenes se guardan con rutas relativas a la carpeta del archivo, así el proyecto se puede mover o
//! compartir entero sin perder nada. Los proyectos guardados antes de esto, que no están en su propia carpeta,
//! usan una carpeta de recursos al lado, con el nombre del proyecto (`Viejo_recursos/`).

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{model::Project, storage::EXTENSION};

pub const ASSETS_DIR: &str = "recursos";

/// Dónde va el archivo de un proyecto nuevo a partir de lo que se eligió en el diálogo de guardar: dentro de una
/// carpeta con el mismo nombre (que se crea al guardar). Si ya se eligió esa carpeta, no se anida otra vez.
pub fn project_file_for(chosen: &Path) -> PathBuf {
    let stem = chosen.file_stem().map_or_else(
        || "proyecto".to_string(),
        |s| s.to_string_lossy().to_string(),
    );
    let parent = chosen.parent().unwrap_or_else(|| Path::new("."));
    let inside_own_folder = parent
        .file_name()
        .is_some_and(|n| n.to_string_lossy() == stem);
    let dir = if inside_own_folder {
        parent.to_path_buf()
    } else {
        parent.join(&stem)
    };
    dir.join(format!("{stem}.{EXTENSION}"))
}

/// Carpeta de recursos de un proyecto guardado en `file`.
pub fn assets_dir_for(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let own_folder = parent
        .file_name()
        .is_some_and(|n| n.to_string_lossy() == stem);
    if own_folder {
        parent.join(ASSETS_DIR)
    } else {
        parent.join(format!("{stem}_{ASSETS_DIR}"))
    }
}

/// Hash estable (FNV-1a de 64 bits) del contenido, para nombrar las copias y no repetirlas.
fn content_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Nombre de la copia: el original sin caracteres raros, más un trozo del hash del contenido.
fn copy_name(original: &Path, bytes: &[u8]) -> String {
    let stem: String = original
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "imagen".into())
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    let ext = original
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "png".into());
    format!("{stem}-{:010x}.{ext}", content_hash(bytes) & 0xff_ffff_ffff)
}

/// Resultado de copiar las imágenes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub copied: usize,
    /// Imágenes cuyo archivo ya no existe: se dejan como están.
    pub missing: usize,
}

/// Copia a la carpeta de recursos de `file` todas las imágenes del proyecto que estén en otro lado y deja sus
/// rutas relativas a la carpeta del archivo. Lo que ya está en la carpeta de recursos solo se vuelve relativo.
/// Al terminar, el proyecto queda asociado a esa carpeta.
pub fn localize(project: &mut Project, file: &Path) -> io::Result<Report> {
    let base = file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let assets = assets_dir_for(file);
    let old_base = project.base_dir.clone();
    let mut report = Report::default();

    for path in project.image_paths_mut() {
        let absolute = if path.is_relative() {
            match &old_base {
                Some(b) => b.join(&*path),
                None => base.join(&*path),
            }
        } else {
            path.clone()
        };
        if !absolute.is_file() {
            report.missing += 1;
            continue;
        }
        let destination = if absolute.starts_with(&assets) {
            absolute.clone()
        } else {
            let bytes = fs::read(&absolute)?;
            fs::create_dir_all(&assets)?;
            // Las imágenes muy pesadas se achican al copiarlas: así entran en la nube (2 MB como máximo) y el proyecto pesa menos.
            let (bytes, named_as) = if bytes.len() > crate::cloud::MAX_IMAGE_BYTES {
                match crate::cloud::prepare_image(&bytes) {
                    Ok(small) => (small.bytes, absolute.with_extension(small.ext)),
                    Err(_) => (bytes, absolute.clone()),
                }
            } else {
                (bytes, absolute.clone())
            };
            let dest = assets.join(copy_name(&named_as, &bytes));
            if !dest.exists() {
                fs::write(&dest, &bytes)?;
                report.copied += 1;
            }
            dest
        };
        if let Ok(relative) = destination.strip_prefix(&base) {
            *path = relative.to_path_buf();
        }
    }
    project.base_dir = Some(base);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_dir(label: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "scriptpendra_{label}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_project_gets_its_own_folder() {
        let chosen = Path::new("/home/ana/Guiones/Nimbo.spd");
        assert_eq!(
            project_file_for(chosen),
            Path::new("/home/ana/Guiones/Nimbo/Nimbo.spd")
        );
        // Si ya se eligió la carpeta del proyecto, no se anida otra.
        let inside = Path::new("/home/ana/Guiones/Nimbo/Nimbo.spd");
        assert_eq!(project_file_for(inside), inside);
        assert_eq!(
            assets_dir_for(inside),
            Path::new("/home/ana/Guiones/Nimbo/recursos")
        );
        // Proyectos viejos: carpeta de recursos al lado.
        assert_eq!(
            assets_dir_for(Path::new("/x/Viejo.spd")),
            Path::new("/x/Viejo_recursos")
        );
    }

    #[test]
    fn images_are_copied_into_the_project_and_made_relative() {
        let outside = temp_dir("fuera");
        let photo = outside.join("mi foto (1).png");
        fs::write(&photo, b"contenido de la foto").unwrap();
        let twin = outside.join("copia.png"); // mismo contenido, otro nombre
        fs::write(&twin, b"contenido de la foto").unwrap();
        let ghost = outside.join("no-existe.png");

        let mut project = Project::default();
        project.cover = Some(photo.clone());
        let id = project.new_character("ANA");
        project.characters[0].photo = Some(twin);
        let gallery = project.add_document(crate::model::DocKind::Gallery, None);
        project
            .documents
            .iter_mut()
            .find(|d| d.id == gallery)
            .unwrap()
            .images = vec![photo.clone(), ghost.clone()];
        let _ = id;

        let root = temp_dir("proyecto");
        let file = project_file_for(&root.join("Guion.spd"));
        let report = localize(&mut project, &file).unwrap();

        // Dos archivos con nombres distintos son dos copias; la misma ruta usada dos veces se copia una sola vez.
        assert_eq!(report.copied, 2);
        assert_eq!(report.missing, 1);
        let cover = project.cover.clone().unwrap();
        assert!(
            cover.is_relative() && cover.starts_with("recursos"),
            "{cover:?}"
        );
        assert!(project.resolve_path(&cover).is_file());
        // El archivo original ya puede desaparecer: el proyecto conserva su copia.
        fs::remove_file(&photo).unwrap();
        assert!(project.resolve_path(&cover).is_file());
        // La que no existía se deja como estaba.
        assert_eq!(
            project
                .documents
                .iter()
                .find(|d| d.id == gallery)
                .unwrap()
                .images[1],
            ghost
        );
        // Repetir no copia nada más.
        let again = localize(&mut project, &file).unwrap();
        assert_eq!(again.copied, 0);

        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn save_as_copies_assets_from_the_old_location() {
        let first = temp_dir("a");
        let second = temp_dir("b");
        let original = first.join("retrato.jpg");
        fs::write(&original, b"retrato").unwrap();
        let mut project = Project::default();
        project.cover = Some(original);

        let file_a = project_file_for(&first.join("Uno.spd"));
        localize(&mut project, &file_a).unwrap();
        let file_b = project_file_for(&second.join("Dos.spd"));
        let report = localize(&mut project, &file_b).unwrap();

        assert_eq!(report.copied, 1);
        let cover = project.cover.clone().unwrap();
        assert!(
            project
                .resolve_path(&cover)
                .starts_with(second.join("Dos").join("recursos"))
        );
        // El proyecto nuevo no depende de la carpeta vieja.
        fs::remove_dir_all(&first).unwrap();
        assert!(project.resolve_path(&cover).is_file());
        let _ = fs::remove_dir_all(&second);
    }
}
