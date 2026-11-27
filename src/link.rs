use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::codegen::LIBC_IMPORTS;

const MINIMUM_MACOS: &str = "11.0";
const SDK_FILE: &str = "sdk";
const STUB_FILE: &str = "libSystem.tbd";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linker {
    Ld,
    LdStub,
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
        Linker::LdStub => link_with_ld(&object_path, output, cache_dir, Library::Stub),
        Linker::Ld => link_with_ld(&object_path, output, cache_dir, Library::Stub)
            .or_else(|_| link_with_ld(&object_path, output, cache_dir, Library::System))
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

#[derive(Clone, Copy)]
enum Library {
    Stub,
    System,
}

fn link_with_ld(
    object: &Path,
    output: &Path,
    cache_dir: &Path,
    library: Library,
) -> Result<(), LinkError> {
    let sdk = sdk_for(cache_dir).ok_or_else(|| LinkError::new("cannot locate the macOS SDK"))?;
    let mut command = Command::new("ld");
    command
        .args([
            "-arch",
            "arm64",
            "-platform_version",
            "macos",
            MINIMUM_MACOS,
        ])
        .arg(&sdk.version);
    match library {
        Library::Stub => {
            command.arg(write_stub(cache_dir)?);
        }
        Library::System => {
            command.arg("-syslibroot").arg(&sdk.path).args(["-lSystem"]);
        }
    }
    let result = command
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

fn stub_text() -> String {
    let symbols: Vec<String> = LIBC_IMPORTS.iter().map(|name| format!("_{name}")).collect();
    format!(
        "--- !tapi-tbd\ntbd-version: 4\ntargets: [ arm64-macos ]\ninstall-name: '/usr/lib/libSystem.B.dylib'\ncurrent-version: 1356\nexports:\n  - targets: [ arm64-macos ]\n    symbols: [ {} ]\n...\n",
        symbols.join(", ")
    )
}

fn write_stub(cache_dir: &Path) -> Result<PathBuf, LinkError> {
    let path = cache_dir.join(STUB_FILE);
    let text = stub_text();
    if std::fs::read_to_string(&path).is_ok_and(|current| current == text) {
        return Ok(path);
    }
    std::fs::create_dir_all(cache_dir)
        .and_then(|()| std::fs::write(&path, text))
        .map_err(|e| LinkError::new(e.to_string()))?;
    Ok(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_exports_every_libc_function_that_the_object_imports() {
        let text = stub_text();
        for name in LIBC_IMPORTS {
            assert!(text.contains(&format!("_{name},")) || text.contains(&format!("_{name} ]")));
        }
        assert!(text.contains("install-name: '/usr/lib/libSystem.B.dylib'"));
    }
}
