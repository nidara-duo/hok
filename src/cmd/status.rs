// Copyright (c) 2026 nidara-duo
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option. This file may not be copied, modified, or distributed
// except according to those terms.

use clap::Args as ClapArgs;
use crossterm::style::Stylize;
use libscoop::{operation, Session};

use crate::Result;

/// Show status and check for new app versions
#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Check only local manifests, skip git-based outdated detection
    #[arg(short = 'l', long)]
    pub local: bool,

    #[arg(long)]
    pub json: bool,
}

pub fn execute(args: Args, session: &Session) -> Result<()> {
    let report = operation::status(session, args.local)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if report.scoop_outdated {
        println!("Scoop out of date. Run 'hok update' to get the latest changes.");
    }
    if report.buckets_outdated {
        println!("Scoop bucket(s) out of date. Run 'hok update' to get the latest changes.");
    }

    if report.git_check_failed {
        eprintln!(
            "{}",
            "Warning: could not read some local git repositories (try 'hok update').".yellow()
        );
    }

    if !report.scoop_outdated && !report.buckets_outdated && report.entries.is_empty() {
        println!("Everything is ok!");
        return Ok(());
    }

    println!(
        "{:<20} {:<18} {:<18} {:<25} {}",
        "Name".bold(),
        "Installed Version".bold(),
        "Latest Version".bold(),
        "Missing Dependencies".bold(),
        "Info".bold()
    );
    println!(
        "{} {} {} {} {}",
        "-".repeat(20),
        "-".repeat(18),
        "-".repeat(18),
        "-".repeat(25),
        "-".repeat(4)
    );

    for entry in &report.entries {
        let info = entry
            .flags
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let outdated = entry.flags.contains(&libscoop::StatusInfoFlag::Outdated);
        let name_display = if outdated {
            entry.name.as_str().yellow().to_string()
        } else {
            entry.name.clone()
        };
        println!(
            "{:<20} {:<18} {:<18} {:<25} {}",
            name_display,
            entry.installed_version.as_deref().unwrap_or("-"),
            entry.latest_version.as_deref().unwrap_or("-"),
            entry.missing_dependencies.join(" | "),
            info
        );
    }

    Ok(())
}
