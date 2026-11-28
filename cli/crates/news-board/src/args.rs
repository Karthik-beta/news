//! `news board` arguments. Board is not routed through the CLI's clap parser (`news` hands it argv
//! after `board`), so this is its own small parser.

/// Which screen an item opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Runs,
    Settings,
    Docker,
    Labels,
}

/// What a toast's click (`news board --open <kind>:<id>`) points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTarget {
    /// `gate`, `run`, `lineup`, `usage`, `source`, `huddle`, `settings`, `docker` or `labels`.
    pub kind: String,
    /// A run's short id (a gate adds `.<gate>`), a source id, or empty.
    pub id: String,
}

impl OpenTarget {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (kind, id) = s.split_once(':').unwrap_or((s, ""));
        match kind {
            "gate" | "run" | "lineup" | "usage" | "source" | "huddle" | "settings" | "docker"
            | "labels" => Ok(Self {
                kind: kind.to_owned(),
                id: id.to_owned(),
            }),
            other => Err(format!("unknown item kind {other:?}")),
        }
    }

    pub fn screen(&self) -> Screen {
        match self.kind.as_str() {
            "settings" | "usage" => Screen::Settings,
            "docker" => Screen::Docker,
            "labels" => Screen::Labels,
            _ => Screen::Runs,
        }
    }

    /// The run this points at, for `gate:<run>.<gate>` and `run:<run>`.
    pub fn run(&self) -> Option<&str> {
        match self.kind.as_str() {
            "gate" | "run" | "huddle" => {
                Some(self.id.split('.').next().unwrap_or_default()).filter(|r| !r.is_empty())
            }
            _ => None,
        }
    }

    /// The command line a toast uses to open Board here.
    pub fn arg(&self) -> String {
        if self.id.is_empty() {
            self.kind.clone()
        } else {
            format!("{}:{}", self.kind, self.id)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autostart {
    On,
    Off,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Run,
    Autostart(Autostart),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub minimized: bool,
    pub open: Option<OpenTarget>,
    pub action: Action,
}

pub const HELP: &str = "news board [--minimized] [--open <kind:id>]\n\
    news board autostart on|off|status\n\n\
    Board is the desktop app: runs and approvals (with Huddle's form), settings and schedules, Docker\n\
    health, and the labelling screens (judges, story pairs).\n\
    It talks only to the Bureau service over the Hotline. --minimized starts it in the taskbar\n\
    (the log-on entry uses this); --open opens it on an item, e.g. gate:r_51d2.plan.";

pub fn parse(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        minimized: false,
        open: None,
        action: Action::Run,
    };
    let mut it = argv.iter();
    while let Some(x) = it.next() {
        match x.as_str() {
            "--minimized" => a.minimized = true,
            "--open" => {
                let v = it.next().ok_or("--open needs kind:id")?;
                a.open = Some(OpenTarget::parse(v)?);
            }
            "-h" | "--help" => a.action = Action::Help,
            "autostart" => {
                a.action = Action::Autostart(match it.next().map(String::as_str) {
                    Some("on") => Autostart::On,
                    Some("off") => Autostart::Off,
                    Some("status") => Autostart::Status,
                    _ => return Err("autostart takes on, off or status".to_owned()),
                });
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(a)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn parses_flags_and_open_targets() {
        let a = parse(&v(&["--minimized", "--open", "gate:r_51d2.plan"])).unwrap();
        assert!(a.minimized);
        let t = a.open.unwrap();
        assert_eq!((t.kind.as_str(), t.run()), ("gate", Some("r_51d2")));
        assert_eq!(t.screen(), Screen::Runs);
        assert_eq!(t.arg(), "gate:r_51d2.plan");
        assert_eq!(
            parse(&v(&["--open", "docker"]))
                .unwrap()
                .open
                .unwrap()
                .screen(),
            Screen::Docker
        );
        assert_eq!(
            parse(&v(&["--open", "labels"]))
                .unwrap()
                .open
                .unwrap()
                .screen(),
            Screen::Labels
        );
        assert_eq!(
            parse(&v(&["autostart", "on"])).unwrap().action,
            Action::Autostart(Autostart::On)
        );
    }

    #[test]
    fn rejects_unknown_arguments_and_kinds() {
        assert!(parse(&v(&["--bogus"])).is_err());
        assert!(parse(&v(&["--open", "nope:1"])).is_err());
        assert!(parse(&v(&["--open"])).is_err());
        assert!(parse(&v(&["autostart"])).is_err());
    }
}
