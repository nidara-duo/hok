// Copyright (c) 2026 nidara-duo
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option. This file may not be copied, modified, or distributed
// except according to those terms.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::bucket::Bucket;
use crate::error::Fallible;
use crate::package::{InstallInfo, Manifest};
use crate::Session;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub scoop_outdated: bool,
    pub buckets_outdated: bool,
    /// True if one or more git repositories could not be read locally.
    pub git_check_failed: bool,
    pub entries: Vec<StatusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEntry {
    pub name: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub missing_dependencies: Vec<String>,
    pub flags: Vec<StatusInfoFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatusInfoFlag {
    Outdated,
    InstallFailed,
    Held,
    Deprecated,
    ManifestRemoved,
    MissingDependencies,
}

impl StatusInfoFlag {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Outdated => "Outdated",
            Self::InstallFailed => "Install failed",
            Self::Held => "Held package",
            Self::Deprecated => "Deprecated",
            Self::ManifestRemoved => "Manifest removed",
            Self::MissingDependencies => "Missing dependencies",
        }
    }
}

fn normalize_pkg_name(s: &str) -> String {
    s.split_once('/')
        .map(|(_, name)| name)
        .unwrap_or(s)
        .to_ascii_lowercase()
}

pub fn collect_status(session: &Session, local_only: bool) -> Fallible<StatusReport> {
    let mut report = StatusReport {
        scoop_outdated: false,
        buckets_outdated: false,
        git_check_failed: false,
        entries: vec![],
    };

    if !local_only {
        let config = session.config();
        let scoop_path = config.root_path().to_path_buf();
        let buckets = crate::bucket::bucket_added(session).unwrap_or_default();
        let bucket_paths: Vec<PathBuf> = buckets.iter().map(|b| b.path().to_path_buf()).collect();

        let (scoop_result, bucket_results): (_, Vec<Result<bool, _>>) = rayon::join(
            || check_repo_outdated(&scoop_path),
            || {
                bucket_paths
                    .par_iter()
                    .map(|p| check_repo_outdated(p))
                    .collect()
            },
        );

        match scoop_result {
            Ok(outdated) => report.scoop_outdated = outdated,
            Err(_) => report.git_check_failed = true,
        }

        for result in bucket_results {
            match result {
                Ok(outdated) => {
                    if outdated {
                        report.buckets_outdated = true;
                    }
                }
                Err(_) => report.git_check_failed = true,
            }
        }
    }

    report.entries = collect_status_entries(session)?;
    report.entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(report)
}

fn collect_status_entries(session: &Session) -> Fallible<Vec<StatusEntry>> {
    let config = session.config();
    let apps_dirs = vec![
        config.root_path().join("apps"),
        crate::config::global_path().join("apps"),
    ];

    let installed_packages: HashSet<String> = apps_dirs
        .iter()
        .filter(|d| d.exists())
        .flat_map(|d| std::fs::read_dir(d).ok().into_iter().flatten().flatten())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| normalize_pkg_name(&e.file_name().to_string_lossy()))
        .filter(|n| n != "scoop")
        .collect();

    let buckets_root = session.config().root_path().join("buckets");
    let mut bucket_cache: HashMap<String, Option<Bucket>> = HashMap::new();
    if let Ok(dir_entries) = std::fs::read_dir(&buckets_root) {
        for entry in dir_entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let name = entry.file_name().to_string_lossy().to_string();
                let bucket = Bucket::from(&entry.path()).ok();
                bucket_cache.insert(name, bucket);
            }
        }
    }

    let entries: Vec<StatusEntry> = apps_dirs
        .iter()
        .filter(|d| d.exists())
        .flat_map(|apps_dir| {
            std::fs::read_dir(apps_dir)
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter(|e| e.file_name().to_string_lossy().to_ascii_lowercase() != "scoop")
                .map(move |e| (e, apps_dir.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|(entry, apps_dir)| {
            let app_name = entry.file_name();
            let app_name = app_name.to_string_lossy();
            collect_status_entry(&apps_dir, &app_name, &installed_packages, &bucket_cache)
                .ok()
                .flatten()
        })
        .collect();

    Ok(entries)
}

fn collect_status_entry(
    apps_dir: &Path,
    app_name: &str,
    installed_packages: &HashSet<String>,
    bucket_cache: &HashMap<String, Option<Bucket>>,
) -> Fallible<Option<StatusEntry>> {
    let app_dir = apps_dir.join(app_name);
    let current_dir = app_dir.join("current");
    let manifest_path = current_dir.join("manifest.json");
    let install_path = current_dir.join("install.json");

    let mut entry = StatusEntry {
        name: app_name.to_owned(),
        installed_version: None,
        latest_version: None,
        missing_dependencies: vec![],
        flags: vec![],
    };

    if !manifest_path.exists() || !install_path.exists() {
        entry.flags.push(StatusInfoFlag::InstallFailed);
        return Ok(Some(entry));
    }

    let manifest = Manifest::parse(&manifest_path)?;
    let install_info = InstallInfo::parse(&install_path).ok();

    entry.installed_version = Some(manifest.version().to_owned());

    if let Some(info) = &install_info {
        if info.is_held() {
            entry.flags.push(StatusInfoFlag::Held);
        }

        if let Some(bucket_name) = info.bucket() {
            if let Some(Some(bucket)) = bucket_cache.get(bucket_name) {
                if let Some(origin_path) = bucket.path_of_manifest(app_name) {
                    if let Ok(origin_manifest) = Manifest::parse(&origin_path) {
                        if crate::internal::compare_versions(
                            &origin_manifest.version(),
                            &manifest.version(),
                        ) == std::cmp::Ordering::Greater
                        {
                            entry.latest_version = Some(origin_manifest.version().to_owned());
                            entry.flags.push(StatusInfoFlag::Outdated);
                        }
                    }
                } else {
                    entry.flags.push(StatusInfoFlag::ManifestRemoved);
                }
            }
        }
    }

    entry.missing_dependencies = detect_missing_dependencies(&manifest, installed_packages);
    if !entry.missing_dependencies.is_empty() {
        entry.flags.push(StatusInfoFlag::MissingDependencies);
    }

    if entry.flags.is_empty() {
        Ok(None)
    } else {
        Ok(Some(entry))
    }
}

fn check_repo_outdated(repo_path: &Path) -> Fallible<bool> {
    if !repo_path.join(".git").exists() {
        return Ok(false);
    }
    let repo = git2::Repository::open(repo_path)?;
    let head = repo.head()?;
    let head_id = match head.target() {
        Some(id) => id,
        None => return Ok(false),
    };
    let branch = head.shorthand().unwrap_or("main");
    let remote_ref_name = format!("refs/remotes/origin/{}", branch);
    let remote_id = match repo.find_reference(&remote_ref_name) {
        Ok(remote_ref) => remote_ref.target(),
        Err(_) => None,
    };
    match remote_id {
        Some(id) => Ok(head_id != id),
        None => Ok(false),
    }
}
fn detect_missing_dependencies(manifest: &Manifest, installed: &HashSet<String>) -> Vec<String> {
    let deps: Vec<String> = manifest
        .dependencies()
        .iter()
        .map(|d| normalize_pkg_name(d))
        .filter(|dep_name| {
            // Filter out hook-based dependencies that Scoop status ignores
            let ignored = ["lessmsi", "innounp", "7zip", "dark"];
            !ignored.contains(&dep_name.as_str()) && !installed.contains(dep_name)
        })
        .collect();

    deps
}
