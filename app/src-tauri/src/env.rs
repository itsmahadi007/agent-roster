//! Apps started from the Dock, Finder or a desktop launcher get a bare PATH. The person's
//! login shell knows where `claude`, `codex`, `node` and `roster` live.

const MARK: &str = "__ROSTER_PATH__";

/// The text between the two markers, if it is not empty.
#[cfg_attr(not(unix), allow(dead_code))]
pub fn parse_marked(out: &str) -> Option<String> {
    let start = out.find(MARK)? + MARK.len();
    let len = out[start..].find(MARK)?;
    let path = out[start..start + len].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// PATH as the person's login shell sets it. Gives up after 3 seconds.
#[cfg(unix)]
pub fn login_path(shell: &str) -> Option<String> {
    use roster_core::detect::{CommandRunner, RealRunner};
    let runner = RealRunner {
        timeout: std::time::Duration::from_secs(3),
    };
    let script = format!("printf '%s%s%s' '{MARK}' \"$PATH\" '{MARK}'");
    let out = runner
        .run(std::path::Path::new(shell), &["-ilc", &script], &[])
        .ok()?;
    parse_marked(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_path_parses_marked_output() {
        let out =
            format!("welcome!\nsome rc noise\n{MARK}/usr/bin:/Users/you/.local/bin{MARK}\nbye\n");
        assert_eq!(
            parse_marked(&out).as_deref(),
            Some("/usr/bin:/Users/you/.local/bin")
        );
        assert_eq!(parse_marked("no markers here"), None);
        assert_eq!(parse_marked(&format!("{MARK}{MARK}")), None);
    }
}
