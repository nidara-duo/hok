use clap::{ArgAction, Parser};
use comfy_table::presets::NOTHING;
use comfy_table::{Attribute, Cell, Color, Table};
use libscoop::{collect_package_states, PackageStateFlag, Session};
use std::collections::HashSet;

use crate::Result;

/// List installed package(s)
#[derive(Debug, Parser)]
pub struct Args {
    /// The query string (regex supported by default)
    #[arg(action = ArgAction::Append)]
    query: Vec<String>,
    /// Turn regex off and use explicit matching
    #[arg(short = 'e', long, action = ArgAction::SetTrue)]
    explicit: bool,
    /// List upgradable package(s)
    #[arg(short = 'u', long, action = ArgAction::SetTrue)]
    upgradable: bool,
    /// List held package(s)
    #[arg(short = 'H', long, action = ArgAction::SetTrue)]
    held: bool,
}

pub fn execute(args: Args, session: &Session) -> Result<()> {
    match collect_package_states(session) {
        Err(e) => Err(e.into()),
        Ok(states) => {
            let held_buckets: HashSet<String> = libscoop::operation::bucket_held_names(session)
                .unwrap_or_default()
                .into_iter()
                .collect();
            let mut table = Table::new();

            table.load_preset(NOTHING);

            let header_cells = vec!["Name", "Version", "Available", "Bucket", "Status"]
                .into_iter()
                .map(|title| {
                    Cell::new(title)
                        .add_attribute(Attribute::Bold)
                        .fg(Color::Green)
                });

            table.set_header(header_cells);

            let mut has_rows = false;

            for state in states {
                // Apply filters
                if args.held && !state.held {
                    continue;
                }
                if args.upgradable && !state.flags.contains(&PackageStateFlag::Outdated) {
                    continue;
                }

                // Optional regex query matching
                if !args.query.is_empty() {
                    let matches = args.query.iter().any(|q| {
                        if args.explicit {
                            state.name == *q
                        } else {
                            regex::Regex::new(q)
                                .map(|r| r.is_match(&state.name))
                                .unwrap_or(false)
                        }
                    });
                    if !matches {
                        continue;
                    }
                }

                has_rows = true;

                let name_cell = Cell::new(state.name);

                let version_cell = if state.held || held_buckets.contains(&state.bucket) {
                    Cell::new(state.installed_version.unwrap_or_default()).fg(Color::Blue)
                } else if state.flags.contains(&PackageStateFlag::Outdated) {
                    Cell::new(state.installed_version.unwrap_or_default())
                        .add_attribute(Attribute::Dim)
                } else {
                    Cell::new(state.installed_version.unwrap_or_default())
                };

                let available_cell = match state.latest_version {
                    Some(v) => Cell::new(v).fg(Color::Blue),
                    None => Cell::new(""),
                };
                let bucket_display = if held_buckets.contains(&state.bucket) {
                    format!("{}", state.bucket)
                } else {
                    state.bucket.to_owned()
                };
                let bucket_cell = if held_buckets.contains(&state.bucket) {
                    Cell::new(bucket_display)
                        .fg(Color::Yellow)
                        .add_attribute(Attribute::Dim)
                } else {
                    Cell::new(bucket_display).fg(Color::Green)
                };
                let status_cell = if state.held || held_buckets.contains(&state.bucket) {
                    Cell::new("held").fg(Color::Yellow)
                } else if state.flags.contains(&PackageStateFlag::Outdated) {
                    Cell::new("outdated").fg(Color::Magenta)
                } else {
                    Cell::new("✓").fg(Color::Green)
                };

                table.add_row(vec![
                    name_cell,
                    version_cell,
                    available_cell,
                    bucket_cell,
                    status_cell,
                ]);
            }

            if has_rows {
                println!("{table}");
            } else {
                println!("No packages found.");
            }

            Ok(())
        }
    }
}
