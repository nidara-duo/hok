use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{collections::HashMap, io::Write, time::Duration};

static BAR_FMT: &str = " {wide_msg} {total_bytes:>12} [{bar:>20}] {percent:>3}%";

/// Multiple progress bars for concurrent package downloads.
pub struct MultiProgressUI {
    mp: MultiProgress,
    ctx: HashMap<String, HashMap<String, (u64, u64)>>,
    bars: HashMap<String, ProgressBar>,
}

impl MultiProgressUI {
    pub fn new() -> MultiProgressUI {
        MultiProgressUI {
            mp: MultiProgress::new(),
            ctx: HashMap::new(),
            bars: HashMap::new(),
        }
    }

    /// Update or create a progress bar for the given package download.
    pub fn update(
        &mut self,
        ident: String,
        _url: String,
        _filename: String,
        dltotal: u64,
        dlnow: u64,
    ) {
        if dltotal == 0 {
            return;
        }

        let mut total = 0u64;
        let mut now = 0u64;

        self.ctx
            .entry(ident.clone())
            .and_modify(|inner| {
                inner.insert(_url.clone(), (dltotal, dlnow));
            })
            .or_insert_with(|| {
                let mut ctx = HashMap::new();
                ctx.insert(_url.clone(), (dltotal, dlnow));
                ctx
            })
            .iter()
            .for_each(|(_, (t, n))| {
                total += t;
                now += n;
            });

        self.bars
            .entry(ident.clone())
            .and_modify(|bar| {
                bar.set_length(total);
                bar.set_position(now);
                if total == now {
                    bar.finish();
                }
            })
            .or_insert_with(|| {
                let bar = self.mp.add(ProgressBar::new(total));
                bar.set_message(ident.clone());
                bar.set_position(0);
                bar.set_style(
                    ProgressStyle::default_bar()
                        .template(BAR_FMT)
                        .unwrap()
                        .progress_chars("#> "),
                );
                bar
            });
    }
}

/// Creates a standard hok spinner with the given initial message.
/// Call pb.set_style(ProgressStyle::with_template("{msg}").unwrap())
/// before pb.finish_with_message() to hide the spinner char on completion.
pub fn make_spinner(msg: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(msg.into());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Finishes a spinner with a success message (✓ prefix, no spinner char).
pub fn spinner_success(pb: &ProgressBar, msg: impl Into<String>) {
    pb.set_style(ProgressStyle::with_template("{msg}").unwrap());
    pb.finish_with_message(msg.into());
}

/// Finishes a spinner with an error message (✗ prefix, no spinner char).
pub fn spinner_error(pb: &ProgressBar, msg: impl Into<String>) {
    pb.set_style(ProgressStyle::with_template("{msg}").unwrap());
    pb.finish_with_message(msg.into());
}

/// A single-line status indicator that shows a spinner while active
/// and replaces it with ✓ or ✗ on completion.
///
/// Usage:
///   let s = StatusLine::start("Resolving packages");
///   // ... work happens ...
///   s.success("Resolving packages");   // prints "✓ Resolving packages"
///   // or:
///   s.fail("Resolving packages");      // prints "✗ Resolving packages"
pub struct StatusLine {
    pub pb: ProgressBar,
}

impl StatusLine {
    /// Creates a spinner with the given message and starts animating it.
    pub fn start(msg: impl Into<String>) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message(msg.into());
        pb.enable_steady_tick(Duration::from_millis(80));
        StatusLine { pb }
    }

    #[allow(dead_code)]
    /// Updates the live message while spinner is running.
    pub fn update(&self, msg: impl Into<String>) {
        self.pb.set_message(msg.into());
    }

    /// Stops the spinner and prints "✓ <msg>".
    pub fn success(self, msg: impl Into<String>) {
        self.pb
            .set_style(ProgressStyle::with_template("{msg}").unwrap());
        self.pb.finish_with_message(format!("✓ {}", msg.into()));
    }

    #[allow(dead_code)]
    /// Stops the spinner and prints "✗ <msg>".
    pub fn fail(self, msg: impl Into<String>) {
        self.pb
            .set_style(ProgressStyle::with_template("{msg}").unwrap());
        self.pb.finish_with_message(format!("✗ {}", msg.into()));
    }
}

/// Prompt user to continue or not.
pub fn prompt_yes_no() -> bool {
    loop {
        print!("\nDo you want to continue? [y/N]: ");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let c = input.trim_end();
        if c.chars().count() == 1 {
            let ch: char = c.chars().next().unwrap();
            if ['y', 'Y', 'n', 'N'].contains(&ch) {
                return ch == 'y' || ch == 'Y';
            }
        }
    }
}
