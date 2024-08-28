use std::{
    collections::HashMap,
    env::current_dir,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{anyhow, Context};

use crate::Result;

pub struct ArgParser;

impl ArgParser {
    #[cfg(unix)]
    pub fn parse<S: AsRef<str>>(cmdstr: S) -> Result<Vec<String>> {
        let Some(argv) = shlex::split(cmdstr.as_ref()) else {
            return Err(anyhow!("invalid command string: {}", cmdstr.as_ref()));
        };
        Ok(argv)
    }

    #[cfg(windows)]
    pub fn parse<S: AsRef<str>>(cmdstr: S) -> Result<Vec<String>> {
        Ok(winsplit::split(cmdstr.as_ref()))
    }
}

pub struct Cmd {
    argv: Vec<String>,
    cwd: PathBuf,
    stream: bool,
    envs: HashMap<String, String>,
}

impl Cmd {
    pub fn output_in_bytes(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let Some(prog) = self.argv.first() else {
            return Err(anyhow!("command is empty"));
        };
        let mut proc = Command::new(prog);
        if self.argv.len() > 1 {
            proc.args(&self.argv[1..]);
        }
        if self.stream {
            proc.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }
        proc.current_dir(self.cwd.as_path());
        proc.envs(&self.envs);

        let outbuf =
            proc.output().context(format!("spawn command: {proc:?}"))?;
        if !outbuf.status.success() {
            return Err(anyhow!("failed to run command: {proc:?}"));
        }
        Ok((outbuf.stdout, outbuf.stderr))
    }

    pub fn output(&self, lossy: bool) -> Result<(String, String)> {
        let (stdout, stderr) = self.output_in_bytes()?;
        let r = if lossy {
            (
                String::from_utf8_lossy(&stdout).to_string(),
                String::from_utf8_lossy(&stderr).to_string(),
            )
        } else {
            (
                std::str::from_utf8(&stdout)
                    .context(format!(
                        "stdout contains invalid utf-8 bytes: {stdout:?}"
                    ))?
                    .to_string(),
                std::str::from_utf8(&stderr)
                    .context(format!(
                        "stdout contains invalid utf-8 bytes: {stderr:?}"
                    ))?
                    .to_string(),
            )
        };
        Ok(r)
    }

    pub fn run(&self) -> Result<()> {
        self.output(true)?;
        Ok(())
    }
}

pub struct CmdBuilder {
    cmd: Cmd,
}

impl CmdBuilder {
    pub fn new<S: AsRef<str>>(cmdstr: S) -> Result<Self> {
        let cmdstr = cmdstr.as_ref();
        let argv = ArgParser::parse(cmdstr)
            .context(format!("parse command string: {cmdstr}"))?;
        Ok(CmdBuilder {
            cmd: Cmd {
                argv,
                cwd: current_dir().context(format!(
                    "Get current working directory for running command: {cmdstr}"
                ))?,
                stream: false,
                envs: HashMap::new(),
            },
        })
    }

    pub fn cwd<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.cmd.cwd = path.as_ref().to_path_buf();
        self
    }

    pub fn stream(mut self, stream: bool) -> Self {
        self.cmd.stream = stream;
        self
    }

    pub fn env<S1: AsRef<str>, S2: AsRef<str>>(
        mut self,
        key: S1,
        val: S2,
    ) -> Self {
        self.cmd
            .envs
            .insert(key.as_ref().to_string(), val.as_ref().to_string());
        self
    }

    pub fn build(self) -> Cmd {
        self.cmd
    }
}
