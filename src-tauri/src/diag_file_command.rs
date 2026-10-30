//! Write a diagnostic dump to disk so the user can open it in an editor.

use std::fs;
use std::path::PathBuf;

#[tauri::command]
pub async fn write_diag_file(provider: String, content: String) -> Result<String, String> {
    let dir = PathBuf::from("diagnostics");
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir diagnostics: {e}"))?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("{provider}-dump-{stamp}.txt"));
    fs::write(&path, content.as_bytes()).map_err(|e| format!("write {path:?}: {e}"))?;

    // Return absolute path so the frontend can show it.
    let abs = std::fs::canonicalize(&path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());
    Ok(abs)
}
