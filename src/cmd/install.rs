#![allow(unused_assignments)]
use clap::{ArgAction, Parser};
use crossterm::{
    cursor,
    style::Stylize,
    ExecutableCommand,
};
use libscoop::{operation, Event, Session, SyncOption};

use crate::{cui, util, Result};

/// Install package(s)
#[derive(Debug, Parser)]
#[clap(arg_required_else_help = true)]
pub struct Args {
    /// The package(s) to install
    #[arg(required = true, action = ArgAction::Append)]
    package: Vec<String>,
    /// Download package(s) without performing installation
    #[arg(short = 'd', long, action = ArgAction::SetTrue)]
    download_only: bool,
    /// Ignore failures to ensure a complete transaction
    #[arg(short = 'f', long, action = ArgAction::SetTrue)]
    ignore_failure: bool,
    /// Leverage cache and suppress network access
    #[arg(short = 'o', long, action = ArgAction::SetTrue)]
    offline: bool,
    /// Assume yes to all prompts and run non-interactively
    #[arg(short = 'y', long, action = ArgAction::SetTrue)]
    assume_yes: bool,
    /// Ignore cache and force download
    #[arg(short = 'D', long, action = ArgAction::SetTrue)]
    ignore_cache: bool,
    /// Do not install dependencies (may break packages)
    #[arg(short = 'I', long, action = ArgAction::SetTrue)]
    independent: bool,
    /// Do not replace package(s)
    #[arg(short = 'R', long, action = ArgAction::SetTrue)]
    no_replace: bool,
    /// Escape hold to allow changes on held package(s)
    #[arg(short = 'S', long, action = ArgAction::SetTrue)]
    escape_hold: bool,
    /// Do not upgrade package(s)
    #[arg(short = 'U', long, action = ArgAction::SetTrue)]
    no_upgrade: bool,
    /// Skip package integrity check
    #[arg(long, action = ArgAction::SetTrue)]
    no_hash_check: bool,
}

pub fn execute(args: Args, session: &Session) -> Result<()> {
    let mut options = vec![];

    if args.assume_yes {
        options.push(SyncOption::AssumeYes);
    }

    if args.download_only {
        options.push(SyncOption::DownloadOnly);
    }

    if args.escape_hold {
        options.push(SyncOption::EscapeHold);
    }

    if args.ignore_failure {
        options.push(SyncOption::IgnoreFailure);
    }

    if args.ignore_cache {
        options.push(SyncOption::IgnoreCache);
    }

    if args.no_upgrade {
        options.push(SyncOption::NoUpgrade);
    }

    if args.no_replace {
        options.push(SyncOption::NoReplace);
    }

    if args.offline {
        options.push(SyncOption::Offline);
    }

    if args.independent {
        options.push(SyncOption::NoDependencies);
    }

    if args.no_hash_check {
        options.push(SyncOption::NoHashCheck);
    }

    let rx = session.event_bus().receiver();
    let tx = session.event_bus().sender();

    let mut stdout = std::io::stdout();
    let _ = stdout.execute(cursor::Hide);

    let mut dlprogress = cui::MultiProgressUI::new();

    let handle = std::thread::spawn(move || {
        let mut status: Option<crate::cui::StatusLine> = None;
        while let Ok(event) = rx.recv() {
            match event {
                Event::PackageResolveStart => {
                    status = Some(crate::cui::StatusLine::start("Resolving packages..."));
                }
                Event::PackageDownloadSizingStart => {
                    if let Some(s) = status.take() {
                        s.success("Resolved packages");
                    }
                    status = Some(crate::cui::StatusLine::start("Calculating download size..."));
                }
                Event::PackageDownloadProgress(ctx) => {
                    dlprogress.update(
                        ctx.ident.to_owned(),
                        ctx.url.to_owned(),
                        ctx.filename.to_owned(),
                        ctx.dltotal,
                        ctx.dlnow,
                    );
                }
                Event::PromptTransactionNeedConfirm(transaction) => {
                    if let Some(s) = status.take() {
                        s.success("Calculated download size");
                    }
                    // ... (rest of confirm handling)
                    if let Some(install) = transaction.install_view() {
                        println!("The following packages will be INSTALLED:");
                        let output = install
                            .iter()
                            .map(|p| {
                                format!(
                                    "{}{}{}",
                                    p.ident(),
                                    "-".dark_grey(),
                                    p.version().dark_grey(),
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("  ");
                        println!("  {}", output);
                    }
                    if let Some(upgrade) = transaction.upgrade_view() {
                        if transaction.install_view().is_some() {
                            println!();
                        }
                        println!("The following packages will be UPGRADED:");
                        let output = upgrade
                            .iter()
                            .map(|p| {
                                format!(
                                    "{}{}{}",
                                    p.ident(),
                                    "-".dark_grey(),
                                    p.upgradable_version().unwrap().dark_grey(),
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("  ");
                        println!("  {}", output);
                    }
                    if let Some(replace) = transaction.replace_view() {
                        if transaction.install_view().is_some()
                            || transaction.upgrade_view().is_some()
                        {
                            println!();
                        }
                        println!("The following packages will be REPLACED:");
                        let output = replace
                            .iter()
                            .map(|p| {
                                format!(
                                    "{}{}/{}",
                                    p.installed_bucket().unwrap().dark_grey().crossed_out(),
                                    p.bucket(),
                                    p.name(),
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("  ");
                        println!("  {}", output);
                    }
                    if let Some(download_size) = transaction.download_size() {
                        let out = util::humansize(download_size.total, true);
                        if download_size.total > 0 {
                            if download_size.estimated {
                                println!(
                                    "\nTotal download size: {} {}",
                                    out,
                                    "(estimated)".dark_grey()
                                );
                            } else {
                                println!("\nTotal download size: {}", out);
                            }
                        } else {
                            println!("\nNothing to download, all cached.");
                        }
                    }

                    let mut stdout = std::io::stdout();
                    let _ = stdout.execute(cursor::Show);
                    let answer = cui::prompt_yes_no();
                    let _ = tx.send(Event::PromptTransactionNeedConfirmResult(answer));
                    let _ = stdout.execute(cursor::Hide);
                }
                Event::PackageDownloadStart => {
                    if let Some(s) = status.take() {
                        s.success("Calculated download size");
                    }
                }
                Event::PackageDownloadDone => {}
                Event::PackageIntegrityCheckStart => {
                    status = Some(crate::cui::StatusLine::start("Checking package integrity..."));
                }
                Event::PackageIntegrityCheckDone => {
                    if let Some(s) = status.take() {
                        s.success("Integrity check passed");
                    }
                }
                Event::PackageSyncDone => break,
                _ => {}
            }
        }
    });

    let queries = args.package.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    operation::package_sync(session, queries, options)?;

    handle.join().unwrap();

    let mut stdout = std::io::stdout();
    let _ = stdout.execute(cursor::Show);

    Ok(())
}
