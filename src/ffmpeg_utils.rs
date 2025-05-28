use std::path::{Path, PathBuf};

pub fn parse_timecode(tc: &str) -> f32 {
    let parts: Vec<&str> = tc.split(':').collect();
    if parts.len() == 3 {
        parts[0].parse::<f32>().unwrap_or(0.0) * 3600.0 + parts[1].parse::<f32>().unwrap_or(0.0) * 60.0 + parts[2].parse::<f32>().unwrap_or(0.0)
    } else { 0.0 }
}

pub fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() { return path; }
    
    // Extract the base stem without any existing numbering
    let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let ext = path.extension().map(|s| s.to_string_lossy()).unwrap_or_else(|| "".into());
    
    // Remove any existing (N) suffix to avoid (1)(2) patterns
    if let Some(pos) = stem.rfind('(') {
        if let Some(end_pos) = stem[pos..].find(')') {
            if stem[pos+1..pos+end_pos].chars().all(|c| c.is_digit(10)) {
                stem = stem[0..pos].trim().to_string();
            }
        }
    }
    
    // Try with incrementing numbers indefinitely
    let mut i = 1;
    loop {
        let file_name = if ext.is_empty() {
            format!("{}({})", stem, i)
        } else {
            format!("{}({}).{}", stem, i, ext)
        };
        let candidate = parent.join(&file_name);
        if !candidate.exists() { return candidate; }
        i += 1;
    }
}
