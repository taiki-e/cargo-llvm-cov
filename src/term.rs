use std::{
    io::Write,
    str::FromStr,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

use anyhow::{bail, Error};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub(crate) enum Coloring {
    Auto = 0,
    Always,
    Never,
}

impl Coloring {
    const AUTO: u8 = Self::Auto as _;
    const ALWAYS: u8 = Self::Always as _;
    const NEVER: u8 = Self::Never as _;

    pub(crate) const fn cargo_color(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl FromStr for Coloring {
    type Err = Error;

    fn from_str(color: &str) -> Result<Self, Self::Err> {
        match color {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => bail!("must be auto, always, or never, but found `{other}`"),
        }
    }
}

static COLORING: AtomicU8 = AtomicU8::new(Coloring::AUTO);
// Errors during argument parsing are returned before set_coloring, so check is_terminal first.
pub(crate) fn init_coloring() {
    if !is_terminal::IsTerminal::is_terminal(&std::io::stderr()) {
        COLORING.store(Coloring::NEVER, Ordering::Relaxed);
    }
}
pub(crate) fn set_coloring(color: &mut Option<Coloring>) {
    let new = color.unwrap_or(Coloring::Auto);
    if new == Coloring::Auto && coloring() == ColorChoice::Never {
        // If coloring is already set to never by init_coloring, respect it.
        *color = Some(Coloring::Never);
    } else {
        COLORING.store(new as _, Ordering::Relaxed);
    }
}
fn coloring() -> ColorChoice {
    match COLORING.load(Ordering::Relaxed) {
        Coloring::AUTO => ColorChoice::Auto,
        Coloring::ALWAYS => ColorChoice::Always,
        Coloring::NEVER => ColorChoice::Never,
        _ => unreachable!(),
    }
}

macro_rules! global_flag {
    ($name:ident: $value:ty = $ty:ident::new($($default:expr)?)) => {
        pub(crate) mod $name {
            use super::*;
            pub(super) static VALUE: $ty = $ty::new($($default)?);
            pub(crate) fn set(value: $value) {
                VALUE.store(value, Ordering::Relaxed);
            }
            pub(crate) struct Guard {
                prev: $value,
            }
            impl Drop for Guard {
                fn drop(&mut self) {
                    set(self.prev);
                }
            }
            #[allow(dead_code)]
            #[must_use]
            pub(crate) fn ignore() -> Guard {
                Guard { prev: VALUE.swap(false, Ordering::Relaxed) }
            }
        }
        pub(crate) fn $name() -> $value {
            $name::VALUE.load(Ordering::Relaxed)
        }
    };
}
global_flag!(verbose: bool = AtomicBool::new(false));
global_flag!(error: bool = AtomicBool::new(false));
global_flag!(warn: bool = AtomicBool::new(false));

pub(crate) fn print_status(status: &str, color: Option<Color>, justified: bool) -> StandardStream {
    let mut stream = StandardStream::stderr(coloring());
    let _ = stream.set_color(ColorSpec::new().set_bold(true).set_fg(color));
    if justified {
        let _ = write!(stream, "{status:>12}");
    } else {
        let _ = write!(stream, "{status}");
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
        crate::term::error::set(true);
        let mut stream = crate::term::print_status("error", Some(termcolor::Color::Red), false);
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! warn {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        crate::term::warn::set(true);
        let mut stream = crate::term::print_status("warning", Some(termcolor::Color::Yellow), false);
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! info {
    ($($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_status("info", None, false);
        let _ = writeln!(stream, $($msg),*);
    }};
}

macro_rules! status {
    ($status:expr, $($msg:expr),* $(,)?) => {{
        use std::io::Write;
        let mut stream = crate::term::print_status($status, Some(termcolor::Color::Cyan), true);
        let _ = writeln!(stream, $($msg),*);
    }};
}
