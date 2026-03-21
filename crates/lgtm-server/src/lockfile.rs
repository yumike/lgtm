use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerInfo {
    pub pid: u32,
    pub port: u16,
}

pub fn lgtm_dir() -> PathBuf {
    dirs::home_dir().expect("no home directory").join(".lgtm")
}

pub fn lockfile_path() -> PathBuf {
    lgtm_dir().join("server.json")
}

pub fn sessions_dir() -> PathBuf {
    lgtm_dir().join("sessions")
}

pub fn write_lockfile(path: &Path, pid: u32, port: u16) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let info = ServerInfo { pid, port };
    let content = serde_json::to_string_pretty(&info).unwrap();
    std::fs::write(path, content)
}

pub fn read_lockfile(path: &Path) -> std::io::Result<Option<ServerInfo>> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let info: ServerInfo = serde_json::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(Some(info))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn remove_lockfile(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_and_read_lockfile() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("server.json");
        write_lockfile(&path, 1234, 5678).unwrap();
        let info = read_lockfile(&path).unwrap().unwrap();
        assert_eq!(info.pid, 1234);
        assert_eq!(info.port, 5678);
    }

    #[test]
    fn test_read_missing_lockfile() {
        let info = read_lockfile(Path::new("/nonexistent/server.json")).unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn test_remove_lockfile() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("server.json");
        write_lockfile(&path, 1234, 5678).unwrap();
        remove_lockfile(&path).unwrap();
        assert!(read_lockfile(&path).unwrap().is_none());
    }
}
