use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    path::PathBuf,
    process::Output,
    str,
};

use anyhow::Result;

// Refs:
// - https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/process_builder.rs
// - https://docs.rs/duct

/// A builder object for an external process, similar to `std::process::Command`.
#[must_use]
#[derive(Debug)]
pub(crate) struct ProcessBuilder {
    /// The program to execute.
    program: OsString,
    /// A list of arguments to pass to the program.
    args: Vec<OsString>,
    /// The environment variables in the expression's environment.
    env: BTreeMap<String, Option<OsString>>,
    /// The working directory where the expression will execute.
    dir: Option<PathBuf>,
    /// Join the standard output of an expression to its standard error pipe, similar to `1>&2` in the shell.
    pub(crate) stdout_to_stderr: bool,
}

impl ProcessBuilder {
    /// Creates a new `ProcessBuilder`.
    pub(crate) fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            dir: None,
            stdout_to_stderr: false,
        }
    }

    /// (chainable) Adds `arg` to the args list.
    pub(crate) fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// (chainable) Adds multiple `args` to the args list.
    pub(crate) fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        self.args.extend(args.into_iter().map(|t| t.as_ref().to_os_string()));
        self
    }

    /// (chainable) Replaces the args list with the given `args`.
    pub(crate) fn args_replace(
        &mut self,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> &mut Self {
        self.args = args.into_iter().map(|t| t.as_ref().to_os_string()).collect();
        self
    }

    /// (chainable) Set a variable in the expression's environment.
    pub(crate) fn env<T: AsRef<OsStr>>(&mut self, key: &str, val: T) -> &mut Self {
        self.env.insert(key.to_string(), Some(val.as_ref().to_os_string()));
        self
    }

    // /// (chainable) Remove a variable from the expression's environment.
    // pub(crate) fn env_remove(&mut self, key: &str) -> &mut Self {
    //     self.env.insert(key.to_string(), None);
    //     self
    // }

    /// (chainable) Set the working directory where the expression will execute.
    pub(crate) fn dir(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.dir = Some(path.into());
        self
    }

    /// Execute an expression, wait for it to complete.
    pub(crate) fn run(&mut self) -> Result<()> {
        self.build().run()?;
        Ok(())
    }

    /// Execute an expression, wait for it to complete, returning the stdio output.
    pub(crate) fn run_with_output(&mut self) -> Result<Output> {
        let output = self.build().stdout_capture().stderr_capture().run()?;
        Ok(output)
    }

    pub(crate) fn build(&self) -> duct::Expression {
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

        if self.stdout_to_stderr {
            cmd = cmd.stdout_to_stderr();
        }
        if let Some(path) = &self.dir {
            cmd = cmd.dir(path);
        }

        cmd
    }
}
