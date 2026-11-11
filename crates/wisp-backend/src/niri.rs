//! niri's config. niri only tells other programs which window is focused,
//! so wisp turns on `focus-follows-mouse`, which makes that the window
//! under the cursor. It does this once: the backup it leaves marks it done,
//! so removing the setting afterwards sticks.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

const SETTING: &str = "focus-follows-mouse";

pub fn setup() -> Vec<String> {
    let path = match std::env::var_os("NIRI_CONFIG") {
        Some(path) => PathBuf::from(path),
        None => match crate::xdg_home("CONFIG", ".config") {
            Some(dir) => dir.join("niri/config.kdl"),
            None => return Vec::new(),
        },
    };
    let by_hand = format!(
        "niri only tells other programs which window is focused, so gestures act on that window wherever you \
         draw. Add `{SETTING}` to the `input` section of `{}` to make it the window under the cursor.",
        path.display()
    );
    let backup = backup_of(&path);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return vec![format!("{by_hand} (wisp couldn't read it: {e})")],
    };
    if is_on(&text) {
        return Vec::new();
    }
    if backup.exists() {
        // wisp turned it on before, and it's been turned off since.
        return vec![by_hand];
    }
    match turn_on(&path, &text, &backup) {
        Ok(()) => {
            log::info!("turned on {SETTING} in {}", path.display());
            vec![format!(
                "niri only tells other programs which window is focused, so wisp turned on `{SETTING}` in `{}`: \
                 gestures now act on the window under the cursor. The file as it was is `{}`.",
                path.display(),
                backup.display()
            )]
        }
        Err(e) => vec![format!("{by_hand} (wisp couldn't add it: {e:#})")],
    }
}

fn backup_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".wisp-backup");
    path.with_file_name(name)
}

/// Whether a line outside comments sets it. `/-` comments out a node in
/// KDL, so it doesn't count either.
fn is_on(config: &str) -> bool {
    config.lines().any(|line| {
        let code = line.split("//").next().unwrap_or_default().trim_start();
        code.strip_prefix(SETTING)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '{', ';']))
    })
}

/// The config with the setting at the top of the `input` section, or in a
/// new one.
fn with_setting(config: &str) -> String {
    let line = format!("    // Added by wisp, so gestures act on the window under the cursor.\n    {SETTING}\n");
    let mut out = String::with_capacity(config.len() + line.len());
    let mut added = false;
    for l in config.split_inclusive('\n') {
        out.push_str(l);
        let code = l.split("//").next().unwrap_or_default().trim();
        if !added && code.strip_prefix("input").is_some_and(|rest| rest.trim() == "{") {
            if !l.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&line);
            added = true;
        }
    }
    if !added {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("\ninput {{\n{line}}}\n"));
    }
    out
}

/// Checks the new config with niri itself before replacing the file. The
/// draft sits next to the original, so includes resolve the same way.
/// Writing in place keeps a symlinked config a symlink.
fn turn_on(path: &Path, text: &str, backup: &Path) -> anyhow::Result<()> {
    let new = with_setting(text);
    let draft = path.with_extension("kdl.wisp-draft");
    std::fs::write(&draft, &new).with_context(|| format!("writing {}", draft.display()))?;
    let check = Command::new("niri").arg("validate").arg("-c").arg(&draft).output();
    let _ = std::fs::remove_file(&draft);
    let check = check.context("running niri validate")?;
    if !check.status.success() {
        bail!(
            "niri rejected the change: {}",
            String::from_utf8_lossy(&check.stderr).trim()
        );
    }
    std::fs::write(backup, text).with_context(|| format!("writing {}", backup.display()))?;
    std::fs::write(path, new).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_setting_outside_comments() {
        assert!(is_on("input {\n    focus-follows-mouse\n}"));
        assert!(is_on("input {\n    focus-follows-mouse max-scroll-amount=\"0%\"\n}"));
        assert!(!is_on(
            "input {\n    // focus-follows-mouse max-scroll-amount=\"0%\"\n}"
        ));
        assert!(!is_on("input {\n    /-focus-follows-mouse\n}"));
        assert!(!is_on("input {\n    focus-follows-mouse-not\n}"));
    }

    #[test]
    fn adds_it_to_the_input_section() {
        let config = "// input { is not this\ninput {\n    keyboard {}\n}\n";
        let new = with_setting(config);
        assert!(is_on(&new));
        assert!(new.starts_with("// input { is not this\ninput {\n    // Added by wisp"));
        assert!(new.ends_with("    focus-follows-mouse\n    keyboard {}\n}\n"));
    }

    #[test]
    fn adds_an_input_section_if_there_is_none() {
        let new = with_setting("layout {}");
        assert_eq!(
            new,
            "layout {}\n\ninput {\n    // Added by wisp, so gestures act on the window under the cursor.\n    focus-follows-mouse\n}\n"
        );
    }
}
