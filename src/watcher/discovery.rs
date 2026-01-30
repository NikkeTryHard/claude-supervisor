//! Session path discovery utilities.
//!
//! Provides functions to locate Claude Code session files on disk.

use std::path::{Path, PathBuf};

/// Convert a project path to the hash format used by Claude Code.
///
/// Claude Code stores sessions in `~/.claude/projects/<hash>/` where
/// the hash is the project path with `/` replaced by `-`.
///
/// # Examples
///
/// ```
/// use claude_supervisor::watcher::project_path_hash;
///
/// let hash = project_path_hash(Path::new("/home/user/project"));
/// assert_eq!(hash, "-home-user-project");
/// ```
#[must_use]
pub fn project_path_hash(project_path: &Path) -> String {
    let path_str = project_path.to_string_lossy();
    path_str.replace('/', "-")
}

/// Find the sessions directory for a project.
///
/// Looks in `~/.claude/projects/<hash>/` for the given project path.
///
/// Returns `None` if the directory doesn't exist or home directory
/// cannot be determined.
#[must_use]
pub fn find_project_sessions_dir(project_path: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let hash = project_path_hash(project_path);
    let sessions_dir = home.join(".claude").join("projects").join(hash);

    if sessions_dir.is_dir() {
        Some(sessions_dir)
    } else {
        None
    }
}

/// Find the most recent session file in a directory.
///
/// Searches for `.jsonl` files and returns the one with the most
/// recent modification time.
///
/// Returns `None` if no `.jsonl` files are found or the directory
/// cannot be read.
#[must_use]
pub fn find_latest_session(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;

    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().ok()?;
            Some((entry.path(), modified))
        })
        .max_by_key(|(_, modified)| *modified)
        .map(|(path, _)| path)
}

/// Find a session file by its session ID.
///
/// Session IDs are typically UUIDs used as the filename (without extension).
///
/// Returns `None` if the session file doesn't exist.
#[must_use]
pub fn find_session_by_id(sessions_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let session_file = sessions_dir.join(format!("{session_id}.jsonl"));

    if session_file.is_file() {
        Some(session_file)
    } else {
        None
    }
}

/// Discover the most recent session for a project.
///
/// Convenience function that combines `find_project_sessions_dir` and
/// `find_latest_session`.
///
/// Returns `None` if no sessions exist for the project.
#[must_use]
pub fn discover_session(project_path: &Path) -> Option<PathBuf> {
    let sessions_dir = find_project_sessions_dir(project_path)?;
    find_latest_session(&sessions_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_project_path_hash_simple() {
        let path = Path::new("/home/user/project");
        assert_eq!(project_path_hash(path), "-home-user-project");
    }

    #[test]
    fn test_project_path_hash_nested() {
        let path = Path::new("/home/user/dev/rust/my-project");
        assert_eq!(project_path_hash(path), "-home-user-dev-rust-my-project");
    }

    #[test]
    fn test_project_path_hash_root() {
        let path = Path::new("/");
        assert_eq!(project_path_hash(path), "-");
    }

    #[test]
    fn test_find_latest_session_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let result = find_latest_session(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_find_latest_session_no_jsonl_files() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("readme.txt"), "hello").unwrap();
        std::fs::write(temp_dir.path().join("config.json"), "{}").unwrap();

        let result = find_latest_session(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_find_latest_session_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("session-1.jsonl");
        std::fs::write(&session_path, "{}").unwrap();

        let result = find_latest_session(temp_dir.path());
        assert_eq!(result, Some(session_path));
    }

    #[test]
    fn test_find_latest_session_multiple_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create older file
        let old_path = temp_dir.path().join("old-session.jsonl");
        std::fs::write(&old_path, "{}").unwrap();

        // Wait a bit to ensure different mtime
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Create newer file
        let new_path = temp_dir.path().join("new-session.jsonl");
        {
            let mut file = std::fs::File::create(&new_path).unwrap();
            writeln!(file, "{{}}").unwrap();
        }

        let result = find_latest_session(temp_dir.path());
        assert_eq!(result, Some(new_path));
    }

    #[test]
    fn test_find_session_by_id_exists() {
        let temp_dir = TempDir::new().unwrap();
        let session_id = "abc-123-def";
        let session_path = temp_dir.path().join(format!("{session_id}.jsonl"));
        std::fs::write(&session_path, "{}").unwrap();

        let result = find_session_by_id(temp_dir.path(), session_id);
        assert_eq!(result, Some(session_path));
    }

    #[test]
    fn test_find_session_by_id_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let result = find_session_by_id(temp_dir.path(), "nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_project_sessions_dir_not_found() {
        // Use a path that definitely doesn't have a Claude sessions dir
        let fake_project = Path::new("/tmp/nonexistent-project-12345");
        let result = find_project_sessions_dir(fake_project);
        assert!(result.is_none());
    }

    #[test]
    fn test_discover_session_not_found() {
        let fake_project = Path::new("/tmp/nonexistent-project-67890");
        let result = discover_session(fake_project);
        assert!(result.is_none());
    }
}
