// Copyright (c) 2026 nidara-duo
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::bucket::Bucket;
use crate::error::Fallible;
use crate::package::{InstallInfo, Manifest};
use crate::Session;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageState {
    pub name: String,
    pub bucket: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub held: bool,
    pub missing_dependencies: Vec<String>,
    pub flags: Vec<PackageStateFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PackageStateFlag {
    Outdated,
    InstallFailed,
    Held,
    Deprecated,
    ManifestRemoved,
    MissingDependencies,
}

pub fn normalize_pkg_name(s: &str) -> String {
    s.split_once('/')
        .map(|(_, name)| name)
        .unwrap_or(s)
        .to_ascii_lowercase()
}

pub fn collect_package_states(session: &Session) -> Fallible<Vec<PackageState>> {
    let config = session.config();
    let apps_dirs = [
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

    let states: Vec<PackageState> = apps_dirs
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
            collect_package_state(&apps_dir, &app_name, &installed_packages, &bucket_cache)
                .ok()
                .flatten()
        })
        .collect();

    let mut states = states;
    states.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(states)
}

fn collect_package_state(
    apps_dir: &Path,
    app_name: &str,
    installed_packages: &HashSet<String>,
    bucket_cache: &HashMap<String, Option<Bucket>>,
) -> Fallible<Option<PackageState>> {
    let app_dir = apps_dir.join(app_name);
    let current_dir = app_dir.join("current");
    let manifest_path = current_dir.join("manifest.json");
    let install_path = current_dir.join("install.json");

    let mut state = PackageState {
        name: app_name.to_owned(),
        bucket: "unknown".to_owned(),
        installed_version: None,
        latest_version: None,
        held: false,
        missing_dependencies: vec![],
        flags: vec![],
    };

    if !manifest_path.exists() || !install_path.exists() {
        state.flags.push(PackageStateFlag::InstallFailed);
        return Ok(Some(state));
    }

    let manifest = Manifest::parse(&manifest_path)?;
    let install_info = InstallInfo::parse(&install_path).ok();

    state.installed_version = Some(manifest.version().to_owned());

    if let Some(info) = &install_info {
        if info.is_held() {
            state.held = true;
            state.flags.push(PackageStateFlag::Held);
        }

        if let Some(bucket_name) = info.bucket() {
            state.bucket = bucket_name.to_owned();
            if let Some(Some(bucket)) = bucket_cache.get(bucket_name) {
                if let Some(origin_path) = bucket.path_of_manifest(app_name) {
                    if let Ok(origin_manifest) = Manifest::parse(&origin_path) {
                        if crate::internal::compare_versions(
                            &origin_manifest.version(),
                            &manifest.version(),
                        ) == std::cmp::Ordering::Greater
                        {
                            state.latest_version = Some(origin_manifest.version().to_owned());
                            state.flags.push(PackageStateFlag::Outdated);
                        }
                    }
                } else {
                    state.flags.push(PackageStateFlag::ManifestRemoved);
                }
            }
        }
    }

    state.missing_dependencies = detect_missing_dependencies(&manifest, installed_packages);
    if !state.missing_dependencies.is_empty() {
        state.flags.push(PackageStateFlag::MissingDependencies);
    }

    Ok(Some(state))
}

fn detect_missing_dependencies(manifest: &Manifest, installed: &HashSet<String>) -> Vec<String> {
    manifest
        .dependencies()
        .iter()
        .map(|d| normalize_pkg_name(d))
        .filter(|dep_name| {
            let ignored = ["lessmsi", "innounp", "7zip", "dark"];
            !ignored.contains(&dep_name.as_str()) && !installed.contains(dep_name)
        })
        .collect()
}
