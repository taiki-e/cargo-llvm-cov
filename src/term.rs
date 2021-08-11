use std::{
    io::Write,
    sync::atomic::{AtomicU8, Ordering::Relaxed},
};

use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::cli::Coloring;

static COLORING: AtomicU8 = AtomicU8::new(AUTO);

const AUTO: u8 = Coloring::Auto as _;
const ALWAYS: u8 = Coloring::Always as _;
const NEVER: u8 = Coloring::Never as _;

pub(crate) fn set_coloring(coloring: Option<Coloring>) {
    let mut coloring = coloring.unwrap_or(Coloring::Auto);
    if coloring == Coloring::Auto && !atty::is(atty::Stream::Stderr) {
        coloring = Coloring::Never;
    }
    COLORING.store(coloring as _, Relaxed);
}

fn coloring() -> ColorChoice {
    match COLORING.load(Relaxed) {
        AUTO => ColorChoice::Auto,
        ALWAYS => ColorChoice::Always,
        NEVER => ColorChoice::Never,
        _ => unreachable!(),
    }
}

pub(crate) fn print_inner(color: Option<Color>, kind: &str) -> StandardStream {
    let mut stream = StandardStream::stderr(coloring());
    let _ = stream.set_color(ColorSpec::new().set_bold(true).set_fg(color));
    let _ = write!(stream, "{}", kind);
    let _ = stream.reset();
    let _ = write!(stream, ": ");
    stream
}

macro_rules! error {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_inner(Some(termcolor::Color::Red), "error");
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! warn {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_inner(Some(termcolor::Color::Yellow), "warning");
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! info {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_inner(None, "info");
        let _ = writeln!(stream, $($msg),*);
    }};
}
