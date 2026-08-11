use crate::config::TEXT_SOURCES_DIR;
use std::fs;
use std::io;

pub fn write_file(content: &str, filename: &str) -> io::Result<()> {
    let path = format!("{TEXT_SOURCES_DIR}/{filename}");
    fs::create_dir_all(TEXT_SOURCES_DIR)?;
    fs::write(path, content)?;
    Ok(())
}
