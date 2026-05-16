use clap::{ArgAction, Parser, Subcommand};
use libscoop::{operation, Session};

use crate::{util, Result};
use comfy_table::presets::NOTHING;
use comfy_table::{Attribute, Cell, Color, Table};

/// Package cache management
#[derive(Debug, Parser)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List download caches
    #[clap(alias = "ls")]
    List {
        /// List caches matching the query
        query: Option<String>,
    },
    /// Remove download caches
    #[clap(alias = "rm")]
    #[clap(arg_required_else_help = true)]
    Remove {
        /// Remove caches matching the query
        query: Option<String>,
        /// Remove all caches
        #[arg(short, long, action = ArgAction::SetTrue, conflicts_with = "query")]
        all: bool,
    },
}

pub fn execute(args: Args, session: &Session) -> Result<()> {
    match args.command {
        Command::List { query } => {
            let query = query.unwrap_or("*".to_string());
            let files = operation::cache_list(session, query.as_str())?;
            let mut total_size: u64 = 0;
            let total_count = files.len();

            let mut table = Table::new();
            table.load_preset(NOTHING);
            let header_cells =
                vec!["Name", "Version", "Filename", "Size"]
                    .into_iter()
                    .map(|title| {
                        Cell::new(title)
                            .add_attribute(Attribute::Bold)
                            .fg(Color::Green)
                    });

            table.set_header(header_cells);

            for f in files.into_iter() {
                let size = f.path().metadata()?.len();
                total_size += size;

                table.add_row(vec![
                    Cell::new(f.package_name()),
                    Cell::new(f.version().to_string()),
                    Cell::new(f.file_name()).add_attribute(Attribute::Dim),
                    Cell::new(util::humansize(size, true)).fg(Color::Green),
                ]);
            }
            table.add_row(vec!["", "", "", ""]);
            table.add_row(vec![
                Cell::new("Total:")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(comfy_table::CellAlignment::Right),
                Cell::new(format!("{} files", total_count)),
                Cell::new(""),
                Cell::new(util::humansize(total_size, true)).add_attribute(Attribute::Bold),
            ]);
            if total_count > 0 {
                // if let Some(column) = table.column_mut(0) {
                //     column.set_cell_alignment(CellAlignment::Right);
                // }

                println!("{table}");
            } else {
                println!("No files found in cache.");
            }

            Ok(())
        }
        Command::Remove { query, all } => {
            if all {
                match operation::cache_remove(session, "*") {
                    Ok(_) => {
                        println!("All download caches were removed.");
                        return Ok(());
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            if let Some(query) = query {
                match operation::cache_remove(session, query.as_str()) {
                    Ok(_) => {
                        if query == "*" {
                            println!("All download caches were removed.");
                        } else {
                            println!("All caches matching '{}' were removed.", query);
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            Ok(())
        }
    }
}
