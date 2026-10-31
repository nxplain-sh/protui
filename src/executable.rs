//! Locating executables on `PATH`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the first executable file called `name` in an absolute `PATH`
/// directory.
///
/// Relative entries are skipped: they resolve against the working directory,
/// which can be a freshly cloned repository and so is not trusted to supply
/// binaries.
#[must_use]
pub fn find(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    env::split_paths(&path)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_binary_every_unix_has() {
        let found = find("sh").expect("sh is on PATH");

        assert!(found.is_absolute());
    }

    #[test]
    fn reports_a_missing_binary() {
        assert_eq!(find("protui-test-no-such-binary"), None);
    }
}
