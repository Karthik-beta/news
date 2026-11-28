//! Tool children: the programs the service starts that are not `claude -p` (docker, bun and the repo's
//! Bun scripts). Security review on #10, findings 1 and 8:
//!
//! - **Path.** A tool is the absolute path `news service install` recorded in the admin-only install
//!   folder ([`TOOLS_FILE`]), never a PATH lookup at run time. A dev build that is not the installed
//!   `news.exe`, with nothing recorded, still finds the tool on PATH (absolute folders only).
//! - **Token.** On Windows the child runs with a restricted Medium-integrity token, Administrators
//!   deny-only ([`crate::spawn_medium`]), so a planted `docker.exe` or `bun.exe` gains nothing over
//!   Karthik's own terminal.
//! - **Environment.** On Windows the child gets exactly [`tool_env`]: an allow-list plus what the tool
//!   needs, never `DATABASE_URL` or a key. Off Windows nothing changes: the child inherits, plus `env`.

use std::ffi::OsStr;
use std::io::{self, PipeReader, PipeWriter};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};

/// The file beside `repo-root` in `%ProgramFiles%\News` that holds `name=absolute path` per tool.
pub const TOOLS_FILE: &str = "tools";

/// The tools `news service install` records.
pub const TOOLS: &[&str] = &["docker", "bun"];

/// What every tool child may see of this process's environment (upper case; Windows names are
/// case-insensitive). Nothing secret: no `DATABASE_URL`, no `NEWS_*`, no keys.
pub const TOOL_ENV_ALLOW: &[&str] = &[
    "ALLUSERSPROFILE",
    "APPDATA",
    "COMMONPROGRAMFILES",
    "COMMONPROGRAMFILES(X86)",
    "COMMONPROGRAMW6432",
    "COMPUTERNAME",
    "COMSPEC",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "LANG",
    "LOCALAPPDATA",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "PATH",
    "PATHEXT",
    "PROCESSOR_ARCHITECTURE",
    "PROCESSOR_IDENTIFIER",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "PUBLIC",
    "SYSTEMDRIVE",
    "SYSTEMROOT",
    "TEMP",
    "TMP",
    "USERDOMAIN",
    "USERNAME",
    "USERPROFILE",
    "WINDIR",
];

/// The allow-listed part of `parent`, plus the variables named in `keep`, then `set`.
pub fn filter_env(
    parent: impl IntoIterator<Item = (String, String)>,
    keep: &[&str],
    set: &[(&str, &str)],
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = parent
        .into_iter()
        .filter(|(k, _)| {
            let k = k.to_ascii_uppercase();
            TOOL_ENV_ALLOW.contains(&k.as_str()) || keep.iter().any(|x| x.eq_ignore_ascii_case(&k))
        })
        .filter(|(k, _)| !set.iter().any(|(s, _)| s.eq_ignore_ascii_case(k)))
        .collect();
    env.extend(set.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
    env
}

/// A tool child's environment from this process's: [`filter_env`].
pub fn tool_env(keep: &[&str], set: &[(&str, &str)]) -> Vec<(String, String)> {
    let parent = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    filter_env(parent, keep, set)
}

/// `name` (`name.exe` on Windows) in the first absolute folder of `path` that has it. A relative
/// entry (`.`, `bin`) is skipped: it would depend on the working folder.
pub fn find_on_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    std::env::split_paths(path)
        .filter(|d| d.is_absolute())
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// The tools file's text for `found` (`name`, absolute path).
pub fn tools_text(found: &[(&str, PathBuf)]) -> String {
    found
        .iter()
        .map(|(n, p)| format!("{n}={}\n", p.display()))
        .collect()
}

/// `name`'s path in the tools file's `text`. A missing, relative or bare (PATH-only) entry is refused.
pub fn recorded(text: &str, name: &str) -> Result<PathBuf, String> {
    let value = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == name)
        .map(|(_, v)| v.trim())
        .ok_or_else(|| format!("no path for {name} was recorded"))?;
    let p = PathBuf::from(value);
    if !p.is_absolute() {
        return Err(format!(
            "the recorded path for {name} ({value:?}) is not absolute"
        ));
    }
    Ok(p)
}

/// Where to run tool `name` from: the recorded path when the install folder has a tools file; an
/// error when this is the installed `news.exe` and nothing was recorded; otherwise PATH (a dev build).
/// Off Windows: `name`, found by the OS as before.
pub fn tool_path(name: &str) -> io::Result<PathBuf> {
    if !cfg!(windows) {
        return Ok(PathBuf::from(name));
    }
    let install = crate::install_paths()?;
    let not_found = |m: String| io::Error::new(io::ErrorKind::NotFound, m);
    match std::fs::read_to_string(install.dir.join(TOOLS_FILE)) {
        Ok(text) => {
            return recorded(&text, name)
                .map_err(|m| not_found(format!("{m}; run `news service install` again")));
        }
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        Err(_) => {}
    }
    let me = std::env::current_exe()?;
    if crate::same_image(&me.to_string_lossy(), &install.exe.to_string_lossy()) {
        return Err(not_found(format!(
            "{} has no tools file; run `news service install` again to record where {name} is",
            install.dir.display()
        )));
    }
    find_on_path(name, &std::env::var_os("PATH").unwrap_or_default())
        .ok_or_else(|| not_found(format!("{name} is not on PATH")))
}

/// A running tool child. `stdin`, `stdout` and `stderr` are pipes when it was started `piped`.
#[derive(Debug)]
pub struct ToolChild {
    #[cfg(windows)]
    child: crate::RestrictedChild,
    #[cfg(not(windows))]
    child: std::process::Child,
    pub stdin: Option<PipeWriter>,
    pub stdout: Option<PipeReader>,
    pub stderr: Option<PipeReader>,
}

impl ToolChild {
    /// Terminate it (an error once it has exited).
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    /// Block until it exits.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            self.child.wait().map(ExitStatus::from_raw)
        }
        #[cfg(not(windows))]
        {
            self.child.wait()
        }
    }

    pub fn id(&self) -> u32 {
        #[cfg(windows)]
        {
            self.child.pid
        }
        #[cfg(not(windows))]
        {
            self.child.id()
        }
    }
}

/// Start `exe` as a tool child (see the module). `piped`: stdin, stdout and stderr are pipes the
/// caller holds; otherwise all three are the null device.
pub fn spawn_tool(
    exe: &Path,
    args: &[String],
    cwd: Option<&Path>,
    env: &[(String, String)],
    piped: bool,
) -> io::Result<ToolChild> {
    let (mut stdin, mut stdout, mut stderr) = (None, None, None);
    #[cfg(windows)]
    let child = {
        let null = || {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("NUL")
        };
        let files = if piped {
            let (r_in, w_in) = std::io::pipe()?;
            let (r_out, w_out) = std::io::pipe()?;
            let (r_err, w_err) = std::io::pipe()?;
            (stdin, stdout, stderr) = (Some(w_in), Some(r_out), Some(r_err));
            use std::os::windows::io::OwnedHandle;
            [
                OwnedHandle::from(r_in),
                OwnedHandle::from(w_out),
                OwnedHandle::from(w_err),
            ]
            .map(std::fs::File::from)
        } else {
            [null()?, null()?, null()?]
        };
        // The child's ends are closed here when `files` drops, so EOF reaches the caller's ends.
        crate::spawn_medium(exe, args, env, cwd, [&files[0], &files[1], &files[2]])?
    };
    #[cfg(not(windows))]
    let child = {
        use std::process::Stdio;
        let mut cmd = std::process::Command::new(exe);
        cmd.args(args).envs(env.iter().map(|(k, v)| (k, v)));
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        if piped {
            let (r_in, w_in) = std::io::pipe()?;
            let (r_out, w_out) = std::io::pipe()?;
            let (r_err, w_err) = std::io::pipe()?;
            cmd.stdin(Stdio::from(r_in))
                .stdout(Stdio::from(w_out))
                .stderr(Stdio::from(w_err));
            (stdin, stdout, stderr) = (Some(w_in), Some(r_out), Some(r_err));
        } else {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
        // `cmd` holds the child's ends; they close when it drops at the end of this block.
        cmd.spawn()?
    };
    Ok(ToolChild {
        child,
        stdin,
        stdout,
        stderr,
    })
}

/// Run a tool child to the end with an empty stdin and return what it printed, like
/// `Command::output`.
pub fn run_tool(
    exe: &Path,
    args: &[String],
    cwd: Option<&Path>,
    env: &[(String, String)],
) -> io::Result<Output> {
    use std::io::Read;
    let mut child = spawn_tool(exe, args, cwd, env, true)?;
    drop(child.stdin.take());
    let mut err_pipe = child.stderr.take();
    let err = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut b);
        }
        b
    });
    let mut stdout = Vec::new();
    if let Some(mut p) = child.stdout.take() {
        p.read_to_end(&mut stdout)?;
    }
    let stderr = err.join().unwrap_or_default();
    let status = child.wait()?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_sees_only_the_allow_list_what_it_keeps_and_what_is_set() {
        let parent = [
            ("Path", "C:\\bin"),
            ("SystemRoot", "C:\\Windows"),
            ("DATABASE_URL", "postgres://news:SECRET@localhost/news"),
            ("NEWS_WRITE_API_KEY", "SECRET"),
            ("AWS_SECRET_ACCESS_KEY", "SECRET"),
            ("DOCKER_HOST", "npipe:////./pipe/kiri"),
            ("NODE_ENV", "development"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        let env = filter_env(parent, &["docker_host"], &[("NODE_ENV", "production")]);
        let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["Path", "SystemRoot", "DOCKER_HOST", "NODE_ENV"]);
        assert!(env.iter().all(|(_, v)| !v.contains("SECRET")));
        assert_eq!(env[3].1, "production");
    }

    #[test]
    fn a_recorded_tool_must_be_an_absolute_path() {
        let me = std::env::current_exe().unwrap();
        let text = tools_text(&[("docker", me.clone())]) + "bun=bun\nnode=bin\\node.exe\n";
        assert_eq!(recorded(&text, "docker").unwrap(), me);
        // A bare name would be a PATH lookup at run time; a relative path depends on the working folder.
        assert!(recorded(&text, "bun").unwrap_err().contains("not absolute"));
        assert!(
            recorded(&text, "node")
                .unwrap_err()
                .contains("not absolute")
        );
        assert!(recorded(&text, "git").unwrap_err().contains("no path"));
    }

    #[test]
    fn path_lookup_skips_relative_folders() {
        let me = std::env::current_exe().unwrap();
        let dir = me.parent().unwrap();
        let stem = me.file_stem().unwrap().to_string_lossy().into_owned();
        let only_relative = std::env::join_paths([".", "target"]).unwrap();
        assert_eq!(find_on_path(&stem, &only_relative), None);
        let path = std::env::join_paths([Path::new("."), dir]).unwrap();
        assert_eq!(find_on_path(&stem, &path).as_deref(), Some(me.as_path()));
    }

    #[cfg(windows)]
    #[test]
    fn a_tool_child_runs_at_medium_with_administrators_deny_only_and_only_its_env() {
        let root = std::env::var("SystemRoot").unwrap();
        let cmd = Path::new(&root).join("System32").join("cmd.exe");
        let env = filter_env(
            [("SystemRoot".to_owned(), root.clone())],
            &[],
            &[("NEWS_TOOL_TEST", "1")],
        );
        // `cmd` with a piped stdin waits for commands until it is killed.
        let mut child = spawn_tool(&cmd, &[], None, &env, true).unwrap();
        let t = crate::process_token(child.id()).unwrap();
        child.kill().unwrap();
        assert_eq!(t.integrity_rid, 0x2000, "not at Medium integrity");
        assert!(!t.admins_enabled, "Administrators is enabled");
        assert_eq!(t.privileges_enabled, ["SeChangeNotifyPrivilege"]);
        let out = run_tool(&cmd, &["/c".to_owned(), "set".to_owned()], None, &env).unwrap();
        assert!(out.status.success());
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("NEWS_TOOL_TEST=1"), "{text}");
        assert!(
            !text.to_ascii_uppercase().contains("DATABASE_URL"),
            "{text}"
        );
    }
}
