use std::{
    io::Write,
    sync::atomic::{AtomicBool, AtomicU8, Ordering::Relaxed},
};

use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::cli::Coloring;

static COLORING: AtomicU8 = AtomicU8::new(AUTO);

const AUTO: u8 = Coloring::Auto as _;
const ALWAYS: u8 = Coloring::Always as _;
const NEVER: u8 = Coloring::Never as _;

pub(crate) fn set_coloring(coloring: &mut Option<Coloring>) {
    let mut color = coloring.unwrap_or(Coloring::Auto);
    if color == Coloring::Auto && !atty::is(atty::Stream::Stderr) {
        *coloring = Some(Coloring::Never);
        color = Coloring::Never;
    }
    COLORING.store(color as _, Relaxed);
}

fn coloring() -> ColorChoice {
    match COLORING.load(Relaxed) {
        AUTO => ColorChoice::Auto,
        ALWAYS => ColorChoice::Always,
        NEVER => ColorChoice::Never,
        _ => unreachable!(),
    }
}

static QUIET: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Relaxed);
}

pub(crate) fn quiet() -> bool {
    QUIET.load(Relaxed)
}

pub(crate) fn print_inner(status: &str, color: Option<Color>, justified: bool) -> StandardStream {
    let mut stream = StandardStream::stderr(coloring());
    let _ = stream.set_color(ColorSpec::new().set_bold(true).set_fg(color));
    if justified {
        let _ = write!(stream, "{:>12}", status);
    } else {
        let _ = write!(stream, "{}", status);
        let _ = stream.set_color(ColorSpec::new().set_bold(true));
        let _ = write!(stream, ":");
    }
    let _ = stream.reset();
    let _ = write!(stream, " ");
    stream
}

macro_rules! error {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_inner("error", Some(termcolor::Color::Red), false);
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! warn {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_inner("warning", Some(termcolor::Color::Yellow), false);
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! info {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_inner("info", None, false);
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! status {
    ($status:expr, $($msg:expr),* $(,)?) => {{
        use std::io::Write;
        if !crate::term::quiet() {
            let mut stream = crate::term::print_inner($status, Some(termcolor::Color::Cyan), true);
            let _ = writeln!(stream, $($msg),*);
        }
    }};
}
