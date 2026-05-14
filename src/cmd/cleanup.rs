use clap::Parser;
use crossterm::style::Stylize;
use libscoop::{operation, package::CleanupOption, Event, Session};
use std::thread;

use crate::Result;

/// Remove old versions of packages
#[derive(Debug, Parser)]
pub struct Args {
    /// App(s) to clean up (`*` to clean all)
    #[arg(required_unless_present = "all")]
    apps: Vec<String>,

    /// Clean all apps
    #[arg(short = 'a', long, conflicts_with = "apps")]
    all: bool,

    /// Also remove outdated download cache files
    #[arg(short = 'k', long)]
    cache: bool,
}

pub fn execute(args: Args, session: &Session) -> Result<()> {
    let apps = if args.all {
        vec!["*".to_string()]
    } else {
        args.apps
    };

    let mut options = Vec::new();
    if args.cache {
        options.push(CleanupOption::Cache);
    }

    let rx = session.event_bus().receiver();
    let handle = thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            match event {
                Event::PackageCleanupStart(name) => {
                    println!("Cleaning {} ...", name.yellow());
                }
                Event::PackageCleanupVersionRemoved(name, version) => {
                    println!("  {} Removed {} ({})", "✔".green(), name, version);
                }
                Event::PackageCleanupAlreadyClean(name) => {
                    println!("  {} {} is already clean", "✔".green(), name);
                }
                Event::PackageCleanupDone => break,
                _ => continue,
            }
        }
    });

    operation::package_cleanup(session, &apps, &options)?;

    handle.join().ok();

    if args.all || apps.iter().any(|a| a == "*") || apps.len() > 1 {
        println!("{}", "Everything is shiny now!".green());
    }

    Ok(())
}
