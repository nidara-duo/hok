// Copyright (c) 2026 nidara-duo
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option. This file may not be copied, modified, or distributed
// except according to those terms.

use clap::Args as ClapArgs;
use comfy_table::presets::NOTHING;
use comfy_table::{Attribute, Cell, Color, Table};
use libscoop::status::{StatusEntry, StatusInfoFlag};
use libscoop::Session;

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
    let report = libscoop::status::collect_status(session, args.local)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    // let summary = build_summary_table(&report);
    // println!("{summary}");

    // println!();

    if report.entries.is_empty() {
        println!("Everything is ok!");
        return Ok(());
    }

    let details = build_entries_table(&report.entries);
    println!("{details}");

    Ok(())
}

// fn build_summary_table(report: &libscoop::status::StatusReport) -> Table {
//     let mut table = Table::new();
//     table.load_preset(NOTHING);

//     table.set_header(vec![
//         Cell::new("Check")
//             .add_attribute(Attribute::Bold)
//             .fg(Color::Green),
//         Cell::new("Value")
//             .add_attribute(Attribute::Bold)
//             .fg(Color::Green),
//     ]);

//     table.add_row(vec![
//         Cell::new("Scoop repository"),
//         bool_status_cell(
//             report.scoop_outdated,
//             "outdated",
//             "up to date",
//             Color::Yellow,
//         ),
//     ]);

//     table.add_row(vec![
//         Cell::new("Buckets"),
//         bool_status_cell(
//             report.buckets_outdated,
//             "outdated",
//             "up to date",
//             Color::Yellow,
//         ),
//     ]);

//     table.add_row(vec![
//         Cell::new("Git check"),
//         bool_status_cell(report.git_check_failed, "failed", "ok", Color::Red),
//     ]);

//     table
// }

fn build_entries_table(entries: &[StatusEntry]) -> Table {
    let mut table = Table::new();
    table.load_preset(NOTHING);

    table.set_header(vec![
        Cell::new("Name")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
        Cell::new("Version")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
        Cell::new("Available")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
        Cell::new("Missing")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
        Cell::new("Info")
            .add_attribute(Attribute::Bold)
            .fg(Color::Green),
    ]);

    for entry in entries {
        let installed_cell = match entry.installed_version.as_deref() {
            Some(v) => Cell::new(v).add_attribute(Attribute::Dim),
            None => Cell::new("-"),
        };

        let latest_cell = match entry.latest_version.as_deref() {
            Some(v) => Cell::new(v).fg(Color::Blue),
            None => Cell::new("-"),
        };

        let missing_deps = if entry.missing_dependencies.is_empty() {
            String::from("-")
        } else {
            entry.missing_dependencies.join(", ")
        };

        let missing_deps_cell = if missing_deps == "-" {
            Cell::new("-")
        } else {
            Cell::new(missing_deps).fg(Color::Yellow)
        };

        let info_cell = build_flags_cell(&entry.flags);

        table.add_row(vec![
            Cell::new(&entry.name),
            installed_cell,
            latest_cell,
            missing_deps_cell,
            info_cell,
        ]);
    }

    table
}

fn build_flags_cell(flags: &[StatusInfoFlag]) -> Cell {
    if flags.is_empty() {
        return Cell::new("-");
    }

    let text = flags
        .iter()
        .map(StatusInfoFlag::as_str)
        .collect::<Vec<_>>()
        .join(", ");

    let color = if flags.contains(&StatusInfoFlag::InstallFailed)
        || flags.contains(&StatusInfoFlag::ManifestRemoved)
    {
        Color::Red
    } else if flags.contains(&StatusInfoFlag::Outdated)
        || flags.contains(&StatusInfoFlag::MissingDependencies)
    {
        Color::Yellow
    } else if flags.contains(&StatusInfoFlag::Held) {
        Color::Magenta
    } else {
        Color::White
    };

    Cell::new(text).fg(color)
}

// fn bool_status_cell(
//     value: bool,
//     true_text: &'static str,
//     false_text: &'static str,
//     color: Color,
// ) -> Cell {
//     if value {
//         Cell::new(true_text)
//             .fg(color)
//             .add_attribute(Attribute::Bold)
//     } else {
//         Cell::new(false_text).fg(Color::Green)
//     }
// }
