use std::io::Write;

pub fn log_to_file(message: &str) {
  if let Some(mut docs_dir) = dirs_next::document_dir() {
    docs_dir.push("vad_debug.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(&docs_dir)
    {
      let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
      let _ = writeln!(file, "[{}] {}", timestamp, message);
      let _ = file.flush();
    }
  }
}

