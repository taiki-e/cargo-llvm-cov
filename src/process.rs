#![cfg_attr(test, allow(dead_code))]

// Refs:
// - https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/process_builder.rs
// - https://docs.rs/duct

use std::{cell::Cell, collections::BTreeMap, ffi::OsString, fmt, path::PathBuf, process::Output};

use anyhow::Result;
use shell_escape::escape;

macro_rules! process {
    ($program:expr $(, $arg:expr)* $(,)?) => {{
        let mut _cmd = process::ProcessBuilder::new($program);
        $(
            _cmd.arg($arg);
        )*
        _cmd
    }};
}

/// A builder object for an external process, similar to `std::process::Command`.
#[must_use]
pub(crate) struct ProcessBuilder {
    /// The program to execute.
    program: OsString,
    /// A list of arguments to pass to the program.
    pub(crate) args: Vec<OsString>,
    /// The environment variables in the expression's environment.
    env: BTreeMap<String, Option<OsString>>,
    /// The working directory where the expression will execute.
    dir: Option<PathBuf>,
    pub(crate) stdout_capture: bool,
    pub(crate) stderr_capture: bool,
    pub(crate) stdout_to_stderr: bool,
    /// `true` to include environment variables in display.
    display_env_vars: Cell<bool>,
    /// `true` to include working directory in display.
    display_dir: Cell<bool>,
}

impl ProcessBuilder {
    /// Creates a new `ProcessBuilder`.
    pub(crate) fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            dir: None,
            stdout_capture: false,
            stderr_capture: false,
            stdout_to_stderr: false,
            display_env_vars: Cell::new(false),
            display_dir: Cell::new(false),
        }
    }

    /// Adds `arg` to the args list.
    pub(crate) fn arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    /// Adds multiple `args` to the args list.
    pub(crate) fn args(
        &mut self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> &mut Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set a variable in the expression's environment.
    pub(crate) fn env(&mut self, key: impl Into<String>, val: impl Into<OsString>) -> &mut Self {
        self.env.insert(key.into(), Some(val.into()));
        self
    }

    /// Remove a variable from the expression's environment.
    #[cfg(test)]
    pub(crate) fn env_remove(&mut self, key: impl Into<String>) -> &mut Self {
        self.env.insert(key.into(), None);
        self
    }

    /// Set the working directory where the expression will execute.
    pub(crate) fn dir(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.dir = Some(path.into());
        self
    }

    /// Enables [`duct::Expression::stdout_capture`].
    pub(crate) fn stdout_capture(&mut self) -> &mut Self {
        self.stdout_capture = true;
        self
    }

    /// Enables [`duct::Expression::stderr_capture`].
    #[cfg(test)]
    pub(crate) fn stderr_capture(&mut self) -> &mut Self {
        self.stderr_capture = true;
        self
    }

    /// Enables [`duct::Expression::stdout_to_stderr`].
    pub(crate) fn stdout_to_stderr(&mut self) -> &mut Self {
        self.stdout_to_stderr = true;
        self
    }

    /// Enables environment variables display.
    pub(crate) fn display_env_vars(&mut self) -> &mut Self {
        self.display_env_vars.set(true);
        self
    }

    // /// Enables working directory display.
    // pub(crate) fn display_dir(&mut self) -> &mut Self {
    //     self.display_dir.set(true);
    //     self
    // }

    /// Execute an expression, wait for it to complete.
    #[track_caller]
    pub(crate) fn run(&mut self) -> Result<Output> {
        trace!(track_caller: command = ?self, "run");
        let res = self.build().run();
        trace!(track_caller: ?res, "run");
        Ok(res?)
    }

    #[track_caller]
    pub(crate) fn read(&mut self) -> Result<String> {
        trace!(track_caller: command = ?self, "read");
        let res = self.build().read();
        trace!(track_caller: ?res, "read");
        Ok(res?)
    }

    fn build(&self) -> duct::Expression {
        let mut cmd = duct::cmd(&*self.program, &self.args);

        for (k, v) in &self.env {
            match v {
                Some(v) => {
                    cmd = cmd.env(k, v);
                }
                None => {
                    cmd = cmd.env_remove(k);
                }
            }
        }

        if let Some(path) = &self.dir {
            cmd = cmd.dir(path);
        }
        if self.stdout_capture {
            cmd = cmd.stdout_capture();
        }
        if self.stderr_capture {
            cmd = cmd.stderr_capture();
        }
        if self.stdout_to_stderr {
            cmd = cmd.stdout_to_stderr();
        }

        cmd
    }
}

impl fmt::Debug for ProcessBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Always display environment variables and working directory.
        let prev_display_env_vars = self.display_env_vars.replace(true);
        let prev_display_dir = self.display_dir.replace(true);
        write!(f, "{}", self)?;
        self.display_env_vars.set(prev_display_env_vars);
        self.display_dir.set(prev_display_dir);

        Ok(())
    }
}

// Based on https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/process_builder.rs
impl fmt::Display for ProcessBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`")?;

        if self.display_env_vars.get() {
            for (key, val) in &self.env {
                if let Some(val) = val {
                    let val = escape(val.to_string_lossy());
                    if cfg!(windows) {
                        write!(f, "set {}={}&& ", key, val)?;
                    } else {
                        write!(f, "{}={} ", key, val)?;
                    }
                }
            }
        }

        write!(f, "{}", self.program.to_string_lossy())?;

        for arg in &self.args {
            write!(f, " {}", escape(arg.to_string_lossy()))?;
        }

        write!(f, "`")?;

        if self.display_dir.get() {
            if let Some(dir) = &self.dir {
                write!(f, " (")?;
                write!(f, "{}", dir.display())?;
                write!(f, ")")?;
            }
        }

        Ok(())
    }
}
