use clap::{ArgAction, Parser, Subcommand};
use crossterm::style::Stylize;
use libscoop::{operation, Session};

use crate::{cui, Result};

/// Manage manifest buckets
#[derive(Debug, Parser)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a bucket
    #[clap(arg_required_else_help = true)]
    Add {
        /// The bucket name
        name: String,
        /// The bucket repository url (optional for known buckets)
        repo: Option<String>,
    },
    /// List buckets
    #[clap(alias = "ls")]
    List {
        /// List known buckets
        #[arg(short = 'k', long, action = ArgAction::SetTrue)]
        known: bool,
    },
    /// Remove bucket(s)
    #[clap(alias = "rm")]
    #[clap(arg_required_else_help = true)]
    Remove {
        /// The bucket name(s)
        #[arg(required = true, action = ArgAction::Append)]
        name: Vec<String>,
    },
}

pub fn execute(args: Args, session: &Session) -> Result<()> {
    match args.command {
        Command::Add { name, repo } => {
            let pb = cui::make_spinner(format!("Adding bucket '{}'...", name));
            let repo = repo.as_deref().unwrap_or_default();

            // Progress callback: updates pb message with object counts
            let pb_clone = pb.clone();
            let bucket_name = name.clone();
            let mut progress_cb = move |received: usize, total: usize| {
                if total > 0 {
                    pb_clone.set_message(format!(
                        "Cloning '{}'... [{} / {} objects]",
                        bucket_name, received, total
                    ));
                }
            };

            match operation::bucket_add(session, name.as_str(), repo, Some(&mut progress_cb)) {
                Ok(..) => {
                    cui::spinner_success(&pb, format!("✓ Bucket '{}' added successfully.", name));
                }
                Err(err) => {
                    cui::spinner_error(&pb, format!("✗ Failed to add bucket '{}'.", name));
                    return Err(err.into());
                }
            }
            Ok(())
        }
        Command::List { known } => {
            if known {
                for (name, repo) in operation::bucket_list_known() {
                    println!("{} {}", name.green(), repo);
                }
                Ok(())
            } else {
                match operation::bucket_list(session) {
                    Err(e) => Err(e.into()),
                    Ok(buckets) => {
                        for bucket in buckets {
                            println!(
                                "{}\n ├─manifests: {}\n └─source: {}",
                                bucket.name().green(),
                                bucket.manifest_count(),
                                bucket.source(),
                            );
                        }
                        Ok(())
                    }
                }
            }
        }
        Command::Remove { name } => {
            for name in name {
                let pb = cui::make_spinner(format!("Removing bucket '{}'...", name));

                match operation::bucket_remove(session, name.as_str()) {
                    Ok(..) => {
                        cui::spinner_success(&pb, format!("✓ Bucket '{}' removed.", name));
                    }
                    Err(err) => {
                        cui::spinner_error(&pb, format!("✗ Failed to remove bucket '{}'.", name));
                        return Err(err.into());
                    }
                }
            }
            Ok(())
        }
    }
}
