use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const MINIMUM_MACOS: &str = "11.0";
const SDK_FILE: &str = "sdk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linker {
    Ld,
    Cc,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkError {
    message: String,
}

impl LinkError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn link(
    object: &[u8],
    output: &Path,
    linker: Linker,
    cache_dir: &Path,
) -> Result<(), LinkError> {
    let mut object_path = OsString::from(output);
    object_path.push(".o");
    let object_path = PathBuf::from(object_path);
    std::fs::write(&object_path, object).map_err(|e| LinkError::new(e.to_string()))?;
    let result = match linker {
        Linker::Cc => link_with_cc(&object_path, output),
        Linker::Ld => link_with_ld(&object_path, output, cache_dir)
            .or_else(|_| link_with_cc(&object_path, output)),
    };
    let _ = std::fs::remove_file(&object_path);
    result
}

fn link_with_cc(object: &Path, output: &Path) -> Result<(), LinkError> {
    let result = Command::new("cc")
        .arg(object)
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|e| LinkError::new(format!("cannot run cc: {e}")))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(LinkError::new(
            String::from_utf8_lossy(&result.stderr).into_owned(),
        ))
    }
}

fn link_with_ld(object: &Path, output: &Path, cache_dir: &Path) -> Result<(), LinkError> {
    let sdk = sdk_for(cache_dir).ok_or_else(|| LinkError::new("cannot locate the macOS SDK"))?;
    let result = Command::new("ld")
        .args([
            "-arch",
            "arm64",
            "-platform_version",
            "macos",
            MINIMUM_MACOS,
        ])
        .arg(&sdk.version)
        .arg("-syslibroot")
        .arg(&sdk.path)
        .args(["-lSystem"])
        .arg(object)
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|e| LinkError::new(format!("cannot run ld: {e}")))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(LinkError::new(
            String::from_utf8_lossy(&result.stderr).into_owned(),
        ))
    }
}

struct Sdk {
    path: String,
    version: String,
}

fn sdk_for(cache_dir: &Path) -> Option<Sdk> {
    if let Some(sdk) = read_cached_sdk(cache_dir).filter(|sdk| Path::new(&sdk.path).is_dir()) {
        return Some(sdk);
    }
    let sdk = Sdk {
        path: xcrun("--show-sdk-path")?,
        version: xcrun("--show-sdk-version")?,
    };
    if std::fs::create_dir_all(cache_dir).is_ok() {
        let _ = std::fs::write(
            cache_dir.join(SDK_FILE),
            format!("{}\n{}\n", sdk.path, sdk.version),
        );
    }
    Some(sdk)
}

fn read_cached_sdk(cache_dir: &Path) -> Option<Sdk> {
    let text = std::fs::read_to_string(cache_dir.join(SDK_FILE)).ok()?;
    let mut lines = text.lines();
    let path = lines.next()?.to_string();
    let version = lines.next()?.to_string();
    (!path.is_empty() && !version.is_empty()).then_some(Sdk { path, version })
}

fn xcrun(flag: &str) -> Option<String> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", flag])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}
