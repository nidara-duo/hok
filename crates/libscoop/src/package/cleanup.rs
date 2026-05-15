// Copyright (c) 2026 nidara-duo
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
    error::Fallible, event::Event, internal, package::Manifest, persist, session::Session,
    CleanupOption,
};
use std::fs;

pub fn cleanup(session: &Session, apps: &[String], options: &[CleanupOption]) -> Fallible<()> {
    let config = session.config();
    let apps_dir = config.root_path().join("apps");
    let remove_cache = options.contains(&CleanupOption::Cache);

    // If apps slice contains "*", expand to all installed apps
    let expanded: Vec<String> = if apps.iter().any(|a| a == "*") {
        fs::read_dir(&apps_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        apps.to_vec()
    };

    for app_name in &expanded {
        cleanup_one(session, app_name, remove_cache)?;
    }

    if let Some(tx) = session.emitter() {
        let _ = tx.send(Event::PackageCleanupDone);
    }
    Ok(())
}

fn cleanup_one(session: &Session, app_name: &str, remove_cache: bool) -> Fallible<()> {
    let config = session.config();
    let app_dir = config.root_path().join("apps").join(app_name);

    if !app_dir.exists() {
        return Ok(());
    }

    if let Some(tx) = session.emitter() {
        let _ = tx.send(Event::PackageCleanupStart(app_name.to_string()));
    }

    // Resolve current version from the "current" symlink/junction
    let current_link = app_dir.join("current");
    let current_version = match fs::read_link(&current_link)
        .ok()
        .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
    {
        Some(v) => v,
        None => {
            // current is a real dir or link target is unresolvable —
            // nothing to clean up.
            if let Some(tx) = session.emitter() {
                let _ = tx.send(Event::PackageCleanupAlreadyClean(app_name.to_string()));
            }
            return Ok(());
        }
    };

    // Collect old version directories
    let old_versions: Vec<String> = fs::read_dir(&app_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n != "current" && n != &current_version)
                .collect()
        })
        .unwrap_or_default();

    if old_versions.is_empty() {
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageCleanupAlreadyClean(app_name.to_string()));
        }
    } else {
        for version in &old_versions {
            let version_dir = app_dir.join(version);

            // Try to load manifest to unlink persist data
            let manifest_path = version_dir.join("manifest.json");
            if let Ok(manifest) = Manifest::parse(&manifest_path) {
                if let Some(persists) = manifest.persist() {
                    let persist_paths: Vec<Vec<String>> = persists
                        .iter()
                        .map(|v| v.iter().map(|s| s.to_string()).collect())
                        .collect();
                    let _ = persist::unlink_paths(version_dir.clone(), &persist_paths);
                }
            }

            internal::fs::remove_dir(&version_dir)?;

            if let Some(tx) = session.emitter() {
                let _ = tx.send(Event::PackageCleanupVersionRemoved(
                    app_name.to_string(),
                    version.clone(),
                ));
            }
        }
    }

    if remove_cache {
        let cache_dir = config.cache_path();
        if let Ok(entries) = fs::read_dir(cache_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let fname = entry.file_name();
                let name = fname.to_string_lossy();
                let prefix_all = format!("{}#", app_name);
                let prefix_current = format!("{}#{}#", app_name, current_version);
                if name.starts_with(&prefix_all) && !name.starts_with(&prefix_current) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    Ok(())
}
