//! Las imágenes de un proyecto en la nube: subir las que faltan, bajar las que no están en esta carpeta y sacar las que ya no
//! se usan. Los límites (tipo, tamaño, cuota del grupo, ritmo de subida) los pone el servidor; acá se cuida además que nada se
//! lea ni se escriba fuera de la carpeta `recursos/` del proyecto.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    cloud::{self, Client, CloudError, PreparedImage, Transport},
    cloud_ui::{ImageResult, ImageWork},
};

/// El nombre de archivo de una ruta de imagen del proyecto: solo se acepta «recursos/<nombre>» con un nombre simple.
pub fn resource_name(relative: &str) -> Option<&str> {
    let name = relative.strip_prefix("recursos/")?;
    let simple = !name.is_empty()
        && name.len() <= 120
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    simple.then_some(name)
}

/// El tipo de la nube para un archivo de imagen, por su extensión.
fn cloud_ext(name: &str) -> Option<&'static str> {
    match Path::new(name).extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("jpg"),
        "png" => Some("png"),
        "webp" => Some("webp"),
        _ => None,
    }
}

/// ¿Es realmente una imagen del tipo que dice? Se mira el encabezado del archivo, no solo el nombre.
fn looks_like(ext: &str, bytes: &[u8]) -> bool {
    match ext {
        "jpg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "webp" => bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

/// La carpeta `recursos/` del proyecto, ya resuelta. Todo lo que se lea o escriba tiene que quedar adentro.
fn resources_dir(base: &Path) -> Option<PathBuf> {
    let dir = base.join("recursos");
    fs::create_dir_all(&dir).ok()?;
    fs::canonicalize(dir).ok()
}

pub fn run<T: Transport>(client: &mut Client<T>, work: ImageWork) -> ImageResult {
    let mut result = ImageResult::default();
    let Some(resources) = resources_dir(&work.base_dir) else {
        return result;
    };
    // Subir.
    for relative in &work.upload {
        let Some(name) = resource_name(relative) else {
            result.skipped.push(format!("{relative}: ruta no permitida"));
            continue;
        };
        let (Some(ext), Ok(path)) = (cloud_ext(name), fs::canonicalize(resources.join(name))) else {
            result.skipped.push(format!("{name}: tipo no permitido o no existe"));
            continue;
        };
        if !path.starts_with(&resources) {
            result.skipped.push(format!("{name}: ruta no permitida"));
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        if bytes.len() > cloud::MAX_IMAGE_BYTES {
            result.skipped.push(format!("{name}: pesa más de 2 MB"));
            continue;
        }
        if !looks_like(ext, &bytes) {
            result.skipped.push(format!("{name}: no es una imagen del tipo que dice"));
            continue;
        }
        let image = PreparedImage { hash: cloud::sha256_hex(&bytes), ext, bytes };
        match client.upload_image(&work.group, &work.project, &image) {
            Ok(_) => result.uploaded.push((relative.clone(), image.hash, ext.to_string())),
            Err(CloudError::QuotaFull) => {
                result.skipped.push(format!("{name}: el grupo llenó su espacio para imágenes"));
                break;
            }
            Err(e) => result.skipped.push(format!("{name}: {}", e.message())),
        }
    }
    // Bajar.
    for (relative, hash, ext) in &work.download {
        let Some(name) = resource_name(relative) else { continue };
        if !cloud::is_hash(hash) || !cloud::is_image_ext(ext) {
            continue;
        }
        let target = resources.join(name);
        if target.exists() {
            continue;
        }
        match client.download_image(&work.group, &work.project, hash, ext) {
            Ok(bytes) if looks_like(ext, &bytes) => {
                if fs::write(&target, bytes).is_ok() {
                    result.downloaded += 1;
                }
            }
            Ok(_) => result.skipped.push(format!("{name}: la nube devolvió algo que no es una imagen")),
            Err(e) => result.skipped.push(format!("{name}: {}", e.message())),
        }
    }
    // Sacar las que ya no se usan.
    for (hash, ext) in &work.remove {
        if client.delete_image(&work.group, &work.project, hash, ext).is_ok() {
            result.removed.push(hash.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_simple_names_inside_recursos_are_accepted() {
        assert_eq!(resource_name("recursos/foto_1a2b.png"), Some("foto_1a2b.png"));
        for bad in [
            "../recursos/x.png",
            "recursos/../x.png",
            "recursos/sub/x.png",
            "/etc/passwd",
            "x.png",
            "recursos/",
            "recursos/.oculto",
            "recursos/a b.png",
            "recursos/a\\b.png",
        ] {
            assert_eq!(resource_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_file_must_start_like_the_image_type_it_claims() {
        assert!(looks_like("png", &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]));
        assert!(!looks_like("png", b"#!/bin/sh"));
        assert!(looks_like("jpg", &[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!looks_like("jpg", b"MZ..."));
        assert!(looks_like("webp", b"RIFF\x00\x00\x00\x00WEBPVP8 "));
        assert_eq!(cloud_ext("a.JPEG"), Some("jpg"));
        assert_eq!(cloud_ext("a.exe"), None);
    }
}
