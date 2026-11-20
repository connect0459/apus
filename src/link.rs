use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct LinkError {
    message: String,
}

impl LinkError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn link(object: &[u8], output: &Path) -> Result<(), LinkError> {
    let mut object_path = OsString::from(output);
    object_path.push(".o");
    let object_path = Path::new(&object_path);
    std::fs::write(object_path, object).map_err(|e| LinkError {
        message: e.to_string(),
    })?;
    let result = Command::new("cc")
        .arg(object_path)
        .arg("-o")
        .arg(output)
        .output();
    let _ = std::fs::remove_file(object_path);
    let output = result.map_err(|e| LinkError {
        message: format!("cannot run cc: {e}"),
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(LinkError {
            message: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
