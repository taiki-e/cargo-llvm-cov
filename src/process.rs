use std::{
    cell::Cell,
    collections::BTreeMap,
    ffi::OsString,
    fmt,
    path::PathBuf,
    process::{ExitStatus, Output},
    str,
};

use anyhow::{Context as _, Result};
use shell_escape::escape;

macro_rules! cmd {
    ($program:expr $(, $arg:expr)* $(,)?) => {{
        let mut _cmd = $crate::process::ProcessBuilder::new($program);
        $(
            _cmd.arg($arg);
        )*
        _cmd
    }};
}

// A builder for an external process, inspired by https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/process_builder.rs
#[must_use]
pub(crate) struct ProcessBuilder {
    /// The program to execute.
    program: OsString,
    /// A list of arguments to pass to the program.
    pub(crate) args: Vec<OsString>,
    /// The environment variables in the process's environment.
    env: BTreeMap<String, Option<OsString>>,
    /// The working directory where the process will execute.
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

    /// Adds an argument to pass to the program.
    pub(crate) fn arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    /// Adds multiple arguments to pass to the program.
    pub(crate) fn args(
        &mut self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> &mut Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set a variable in the process's environment.
    pub(crate) fn env(&mut self, key: impl Into<String>, val: impl Into<OsString>) -> &mut Self {
        self.env.insert(key.into(), Some(val.into()));
        self
    }

    // /// Remove a variable from the process's environment.
    // pub(crate) fn env_remove(&mut self, key: impl Into<String>) -> &mut Self {
    //     self.env.insert(key.into(), None);
    //     self
    // }

    /// Set the working directory where the process will execute.
    pub(crate) fn dir(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.dir = Some(path.into());
        self
    }

    // /// Enables [`duct::Expression::stdout_capture`].
    // pub(crate) fn stdout_capture(&mut self) -> &mut Self {
    //     self.stdout_capture = true;
    //     self
    // }

    /// Enables [`duct::Expression::stderr_capture`].
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

    /// Executes a process, waiting for completion, and mapping non-zero exit
    /// status to an error.
    pub(crate) fn run(&mut self) -> Result<Output> {
        let output = self.build().unchecked().run().with_context(|| {
            ProcessError::new(&format!("could not execute process {}", self), None, None)
        })?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(ProcessError::new(
                &format!("process didn't exit successfully: {}", self),
                Some(output.status),
                Some(&output),
            )
            .into())
        }
    }

    /// Executes a process, captures its standard output, returning the captured
    /// output as a `String`.
    pub(crate) fn read(&mut self) -> Result<String> {
        Ok(self.build().read()?)
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

        if !f.alternate() && self.display_env_vars.get() {
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

        if !f.alternate() && self.display_dir.get() {
            if let Some(dir) = &self.dir {
                write!(f, " (")?;
                write!(f, "{}", dir.display())?;
                write!(f, ")")?;
            }
        }

        Ok(())
    }
}

// Based on https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/errors.rs
#[derive(Debug)]
struct ProcessError {
    /// A detailed description to show to the user why the process failed.
    desc: String,
    /// The exit status of the process.
    ///
    /// This can be `None` if the process failed to launch (like process not found).
    status: Option<ExitStatus>,
    /// The output from the process.
    ///
    /// This can be `None` if the process failed to launch, or the output was not captured.
    output: Option<Output>,
}

impl ProcessError {
    /// Creates a new process error.
    ///
    /// `status` can be `None` if the process did not launch.
    /// `output` can be `None` if the process did not launch, or output was not captured.
    fn new(msg: &str, status: Option<ExitStatus>, output: Option<&Output>) -> Self {
        let exit = match status {
            Some(s) => s.to_string(),
            None => "never executed".to_string(),
        };
        let mut desc = format!("{} ({})", &msg, exit);

        if let Some(out) = output {
            match str::from_utf8(&out.stdout) {
                Ok(s) if !s.trim().is_empty() => {
                    desc.push_str("\n--- stdout\n");
                    desc.push_str(s);
                }
                Ok(_) | Err(_) => {}
            }
            match str::from_utf8(&out.stderr) {
                Ok(s) if !s.trim().is_empty() => {
                    desc.push_str("\n--- stderr\n");
                    desc.push_str(s);
                }
                Ok(_) | Err(_) => {}
            }
        }

        Self { desc, status, output: output.cloned() }
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.desc, f)
    }
}

impl std::error::Error for ProcessError {}
