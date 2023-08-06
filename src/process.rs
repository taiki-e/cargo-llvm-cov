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
#[derive(Clone)]
pub(crate) struct ProcessBuilder {
    /// The program to execute.
    program: OsString,
    /// A list of arguments to pass to the program.
    args: Vec<OsString>,
    /// The environment variables in the process's environment.
    env: BTreeMap<String, Option<OsString>>,
    /// The working directory where the process will execute.
    dir: Option<PathBuf>,
    stdout_to_stderr: bool,
    /// `true` to include environment variables in display.
    display_env_vars: Cell<bool>,
}

impl From<cargo_config2::PathAndArgs> for ProcessBuilder {
    fn from(value: cargo_config2::PathAndArgs) -> Self {
        let mut cmd = ProcessBuilder::new(value.path);
        cmd.args(value.args);
        cmd
    }
}

impl ProcessBuilder {
    /// Creates a new `ProcessBuilder`.
    pub(crate) fn new(program: impl Into<OsString>) -> Self {
        let mut this = Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            dir: None,
            stdout_to_stderr: false,
            display_env_vars: Cell::new(false),
        };
        this.env_remove("LLVM_COV_FLAGS");
        this.env_remove("LLVM_PROFDATA_FLAGS");
        this
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

    /// Remove a variable from the process's environment.
    pub(crate) fn env_remove(&mut self, key: impl Into<String>) -> &mut Self {
        self.env.insert(key.into(), None);
        self
    }

    /// Set the working directory where the process will execute.
    pub(crate) fn dir(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.dir = Some(path.into());
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

    /// Executes a process, waiting for completion, and mapping non-zero exit
    /// status to an error.
    pub(crate) fn run(&mut self) -> Result<Output> {
        let output = self.build().unchecked().run().with_context(|| {
            ProcessError::new(&format!("could not execute process {self}"), None, None)
        })?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(ProcessError::new(
                &format!("process didn't exit successfully: {self}"),
                Some(output.status),
                Some(&output),
            )
            .into())
        }
    }

    /// Executes a process, captures its stdio output, returning the captured
    /// output, or an error if non-zero exit status.
    pub(crate) fn run_with_output(&mut self) -> Result<Output> {
        let output =
            self.build().stdout_capture().stderr_capture().unchecked().run().with_context(
                || ProcessError::new(&format!("could not execute process {self}"), None, None),
            )?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(ProcessError::new(
                &format!("process didn't exit successfully: {self}"),
                Some(output.status),
                Some(&output),
            )
            .into())
        }
    }

    /// Executes a process, captures its stdio output, returning the captured
    /// standard output as a `String`.
    pub(crate) fn read(&mut self) -> Result<String> {
        assert!(!self.stdout_to_stderr);
        let mut output = String::from_utf8(self.run_with_output()?.stdout)
            .with_context(|| format!("failed to parse output from {self}"))?;
        while output.ends_with('\n') || output.ends_with('\r') {
            output.pop();
        }
        Ok(output)
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
        if self.stdout_to_stderr {
            cmd = cmd.stdout_to_stderr();
        }

        cmd
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
                        write!(f, "set {key}={val}&& ")?;
                    } else {
                        write!(f, "{key}={val} ")?;
                    }
                }
            }
        }

        write!(f, "{}", self.program.to_string_lossy())?;

        for arg in &self.args {
            write!(f, " {}", escape(arg.to_string_lossy()))?;
        }

        write!(f, "`")?;

        Ok(())
    }
}

// Based on https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/errors.rs
#[derive(Debug)]
struct ProcessError {
    /// A detailed description to show to the user why the process failed.
    desc: String,
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
        let mut desc = format!("{msg} ({exit})");

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

        Self { desc }
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.desc, f)
    }
}

impl std::error::Error for ProcessError {}
