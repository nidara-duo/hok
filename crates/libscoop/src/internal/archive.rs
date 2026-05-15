use crate::error::Fallible;
use std::path::Path;

pub fn is_archive_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    let url_path = lower.split('?').next().unwrap_or(&lower);
    url_path.ends_with(".zip")
        || url_path.ends_with(".7z")
        || url_path.ends_with(".tar")
        || url_path.ends_with(".tar.gz")
        || url_path.ends_with(".tgz")
        || url_path.ends_with(".tar.bz2")
        || url_path.ends_with(".tar.xz")
        || url_path.ends_with(".gz")
}

pub fn extract(src: &Path, dest: &Path) -> Fallible<()> {
    let src_str = src.to_string_lossy().to_lowercase();

    if src_str.ends_with(".zip") {
        extract_zip(src, dest)
    } else {
        extract_with_7z(src, dest)
    }
}

fn extract_zip(src: &Path, dest: &Path) -> Fallible<()> {
    let file = std::fs::File::open(src).map_err(|e| {
        crate::Error::Custom(format!("Failed to open archive {}: {}", src.display(), e))
    })?;

    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        crate::Error::Custom(format!("Failed to read zip {}: {}", src.display(), e))
    })?;

    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|e| crate::Error::Custom(format!("Failed to read zip entry: {}", e)))?;

        let out_path = match zip_file.enclosed_name() {
            Some(path) => dest.join(path),
            None => continue,
        };

        if zip_file.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| {
                crate::Error::Custom(format!(
                    "Failed to create dir {}: {}",
                    out_path.display(),
                    e
                ))
            })?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    crate::Error::Custom(format!(
                        "Failed to create parent dir {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
            let mut out_file = std::fs::File::create(&out_path).map_err(|e| {
                crate::Error::Custom(format!(
                    "Failed to create file {}: {}",
                    out_path.display(),
                    e
                ))
            })?;
            std::io::copy(&mut zip_file, &mut out_file).map_err(|e| {
                crate::Error::Custom(format!(
                    "Failed to write file {}: {}",
                    out_path.display(),
                    e
                ))
            })?;
        }
    }
    Ok(())
}

fn extract_with_7z(src: &Path, dest: &Path) -> Fallible<()> {
    let args = vec![
        "x".to_string(),
        src.to_string_lossy().to_string(),
        format!("-o{}", dest.display()),
        "-y".to_string(),
    ];

    let output = std::process::Command::new("7z")
        .args(args)
        .output()
        .map_err(|e| {
            crate::Error::Custom(format!(
                "Failed to execute 7z (ensure it is in PATH): {}",
                e
            ))
        })?;

    if !output.status.success() {
        return Err(crate::Error::Custom(format!(
            "7z extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}
