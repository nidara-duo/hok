#![allow(dead_code)]
use std::path::Path;

use crate::{error::Fallible, internal, package::Package, Event, Session};

#[derive(Debug)]
pub struct Shim<'a> {
    pub name: &'a str,
    pub real_name: &'a str,
    pub ty: ShimType,
    pub args: Option<Vec<&'a str>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShimType {
    /// Bash script
    ///
    /// A shim will be treated as a Bash script if it does not have a file
    /// extension.
    Bash,

    /// Batch script
    ///
    /// A shim will be treated as a Batch script if it has a `.bat`/`.cmd` file
    /// extension.
    Batch,

    /// Executable
    ///
    /// A shim will be treated as an executable if it has a `.exe`/`.com` file
    /// extension.
    Exe,

    /// Java JAR
    ///
    /// A shim will be treated as a Java JAR if it has a `.jar` file extension.
    Java,

    /// PowerShell script
    ///
    /// A shim will be treated as a PowerShell script if it has a `.ps1` file
    /// extension.
    PowerShell,

    /// Python script
    ///
    /// A shim will be treated as a Python script if it has a `.py` file
    /// extension.
    Python,
}

impl Shim<'_> {
    pub fn new(def: Vec<&str>) -> Shim<'_> {
        let length = def.len();
        assert_ne!(length, 0);

        let real_name = def[0];
        let name = if length == 1 {
            internal::path::leaf_base(real_name).unwrap_or(real_name)
        } else {
            def[1]
        };

        let args = if length < 2 {
            None
        } else {
            Some(def[2..].to_vec())
        };

        let ty = Path::new(real_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext.to_lowercase().as_str() {
                "bat" | "cmd" => ShimType::Batch,
                "exe" | "com" => ShimType::Exe,
                "jar" => ShimType::Java,
                "ps1" => ShimType::PowerShell,
                "py" => ShimType::Python,
                _ => ShimType::Bash,
            })
            .unwrap_or(ShimType::Bash);

        Shim {
            name,
            real_name,
            ty,
            args,
        }
    }
}

pub fn add(session: &Session, package: &Package, version_dir: &Path) -> Fallible<()> {
    let config = session.config();
    let shims_dir = config.root_path().join("shims");
    internal::fs::ensure_dir(&shims_dir)?;

    if let Some(bins) = package.manifest().bin() {
        for bin_entry in bins {
            let shim = Shim::new(bin_entry.clone());
            
            // For now, simple shim creation: just copy a standard shim exe or create a cmd shim.
            // A more robust implementation would use a proper shim engine.
            let shim_exe_dst = shims_dir.join(shim.name).with_extension("exe");
            
            // This is a placeholder for a real shim engine.
            // Using a simple batch-based shim for demo/PoC if no executable shim exists.
            if !shim_exe_dst.exists() {
                let mut batch_file = std::fs::File::create(shims_dir.join(shim.name).with_extension("cmd"))?;
                use std::io::Write;
                writeln!(batch_file, "@echo off")?;
                writeln!(batch_file, "\"{}\" %*", version_dir.join(shim.real_name).display())?;
            }
            
            // Write .shim file
            let shim_file_path = shims_dir.join(shim.name).with_extension("shim");
            let mut shim_file = std::fs::File::create(&shim_file_path)?;
            use std::io::Write;
            writeln!(shim_file, "path = {}", version_dir.join(shim.real_name).display())?;
            writeln!(shim_file, "args =")?;
        }
    }

    Ok(())
}

/// Remove shims for a package.
pub fn remove(session: &Session, package: &Package) -> Fallible<()> {
    assert!(package.is_installed());

    let config = session.config();
    let shims_dir = config.root_path().join("shims");

    if let Some(bins) = package.manifest().bin() {
        let pkg_name = package.name();
        let shims_dir_entries = shims_dir
            .read_dir()?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageShimRemoveStart);
        }

        for shim in bins.into_iter().map(Shim::new) {
            let exts = match shim.ty {
                ShimType::Exe => vec!["exe", "shim", "cmd"],
                ShimType::PowerShell => vec!["cmd", "ps1", ""],
                _ => vec!["cmd", ""],
            };

            for ext in exts.into_iter() {
                // If ext is empty, it means no extension, otherwise it's e.g., "cmd" or "shim"
                let base_name = if ext.is_empty() {
                    shim.name.to_string()
                } else {
                    format!("{}.{}", shim.name, ext)
                };

                let shim_paths = vec![
                    shims_dir.join(format!("{}.{}", base_name, pkg_name)),
                    shims_dir.join(&base_name),
                ];

                for _shim_path in shim_paths {
                    if _shim_path.exists() {
                        if let Some(tx) = session.emitter() {
                            let shim_name = _shim_path.file_name().unwrap().to_string_lossy().to_string();
                            let _ = tx.send(Event::PackageShimRemoveProgress(shim_name));
                        }
                        let _ = std::fs::remove_file(&_shim_path);
                    }
                }

                // Restore alternate shim logic: only restore if the plain base name was removed
                if !shims_dir.join(&base_name).exists() {
                    let mut alt_shims = shims_dir_entries
                        .iter()
                        .filter(|entry| {
                            let path = entry.path();
                            let name = path.file_name().unwrap().to_str().unwrap();
                            name.starts_with(&base_name) && name != base_name
                        })

                        .collect::<Vec<_>>();

                    if !alt_shims.is_empty() {
                        if alt_shims.len() > 1 {
                            alt_shims.sort_by_key(|de| {
                                std::cmp::Reverse(de.metadata().unwrap().modified().unwrap())
                            });
                        }

                        let alt_shim = alt_shims.first().unwrap();
                        let alt_path = alt_shim.path();
                        let alt_path_new = alt_path.with_file_name(&base_name);
                        let _ = std::fs::rename(&alt_path, &alt_path_new);
                    }
                }
            }
        }

        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageShimRemoveDone);
        }
    }

    Ok(())
}
