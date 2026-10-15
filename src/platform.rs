use std::io;
use std::path::Path;

pub fn reveal_in_file_manager(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ());
    }
    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ());
    }
    #[cfg(target_os = "linux")]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| path.to_path_buf())
        };
        return std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map(|_| ());
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        return Err(io::Error::new(io::ErrorKind::Unsupported, "reveal in file manager not supported on this platform"));
    }
}

