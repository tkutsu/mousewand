//! One mousewand per session: a second one would grab the mice the first
//! already holds and fire every gesture twice.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;

/// Holds the lock for as long as it lives. The kernel drops the lock when the
/// process ends, crashed or not, so there is never a stale one to clean up.
pub struct Instance {
    _lock: File,
}

/// The lock, or `None` (and a log line) when another mousewand holds it.
pub fn claim() -> anyhow::Result<Option<Instance>> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    claim_at(&dir.join("mousewand.lock"))
}

fn claim_at(path: &Path) -> anyhow::Result<Option<Instance>> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => {
            // The pid is only for the message below.
            file.set_len(0)?;
            file.rewind()?;
            write!(file, "{}", std::process::id())?;
            Ok(Some(Instance { _lock: file }))
        }
        Err(TryLockError::WouldBlock) => {
            let mut pid = String::new();
            let _ = file.read_to_string(&mut pid);
            log::info!("mousewand is already running (pid {}), so this one quits", pid.trim());
            Ok(None)
        }
        Err(TryLockError::Error(e)) => Err(e).with_context(|| format!("locking {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_holds_the_lock() {
        let path = std::env::temp_dir().join(format!("mousewand-test-{}.lock", std::process::id()));
        let first = claim_at(&path).unwrap();
        assert!(first.is_some());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), std::process::id().to_string());
        assert!(
            claim_at(&path).unwrap().is_none(),
            "a second claim must fail while the first lives"
        );
        drop(first);
        assert!(
            claim_at(&path).unwrap().is_some(),
            "the lock frees when its holder goes"
        );
        let _ = std::fs::remove_file(&path);
    }
}
