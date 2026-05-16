use clap::Parser;
use crossterm::style::Stylize;
use crossterm::{cursor, ExecutableCommand};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use libscoop::{operation, Event, Session};
use std::collections::HashMap;
use std::time::Duration;

use crate::Result;

/// Fetch and update subscribed buckets
#[derive(Debug, Parser)]
pub struct Args {}

pub fn execute(_: Args, session: &Session) -> Result<()> {
    let rx = session.event_bus().receiver();

    let handle = std::thread::spawn(move || {
        let mp = MultiProgress::new();
        // Maps bucket name -> its spinner ProgressBar
        let mut spinners: HashMap<String, ProgressBar> = HashMap::new();

        while let Ok(event) = rx.recv() {
            match event {
                Event::BucketUpdateProgress(ctx) => {
                    if ctx.state().started() {
                        let pb = mp.add(ProgressBar::new_spinner());
                        pb.set_style(
                            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                                .unwrap()
                                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
                        );
                        pb.set_message(format!(" {}", ctx.name()));
                        pb.enable_steady_tick(Duration::from_millis(80));
                        spinners.insert(ctx.name().to_owned(), pb);
                    } else if ctx.state().succeeded() {
                        if let Some(pb) = spinners.remove(ctx.name()) {
                            pb.set_style(ProgressStyle::with_template("{msg}").unwrap());
                            pb.finish_with_message(format!("{}  {}", "✓".green(), ctx.name()));
                        }
                    } else {
                        if let Some(pb) = spinners.remove(ctx.name()) {
                            let err_msg = ctx.state().failed().unwrap_or("error");
                            pb.set_style(ProgressStyle::with_template("{msg}").unwrap());
                            pb.finish_with_message(format!(
                                "{}  {} ({})",
                                "✗".red(),
                                ctx.name(),
                                err_msg
                            ));
                        }
                    }
                }
                Event::BucketUpdateDone => break,
                _ => {}
            }
        }
    });

    println!("Updating buckets");

    let mut stdout = std::io::stdout();
    let _ = stdout.execute(cursor::Hide);

    operation::bucket_update(session)?;

    handle.join().unwrap();

    let _ = stdout.execute(cursor::Show);

    Ok(())
}
