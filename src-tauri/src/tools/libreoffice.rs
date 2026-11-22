//! Running LibreOffice headless conversions.
//!
//! Every job gets its own temporary user profile (`-env:UserInstallation`), so jobs run
//! in parallel without sharing state, and the user's own LibreOffice settings, macros and
//! running instance are never touched. The profile is written before the first start with
//! settings that harden the conversion:
//! - macros never run;
//! - links to untrusted remote resources are blocked;
//! - every HTTP(S) request goes to a proxy on a closed local port, so a document that
//!   references remote images or links cannot make LibreOffice reach the network.
//!
//! LibreOffice exits with 0 even when it cannot open the input, so success is decided by
//! the output file it leaves behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use super::process::harden;
use super::{ToolBox, ToolId};
use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;

/// Exit code of a first start that asks to be restarted (a fresh profile was created).
const RESTART_EXIT_CODE: i32 = 81;
/// Keep only the end of the output in error messages.
const MESSAGE_TAIL_CHARS: usize = 600;
/// A proxy address nothing listens on (the "discard" port on the loopback interface).
pub const DEAD_PROXY: &str = "http://127.0.0.1:9";

const PROFILE_SETTINGS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item>
<item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="DisableMacrosExecution" oor:op="fuse"><value>true</value></prop></item>
<item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="BlockUntrustedRefererLinks" oor:op="fuse"><value>true</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetProxyType" oor:op="fuse"><value>1</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetHTTPProxyName" oor:op="fuse"><value>127.0.0.1</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetHTTPProxyPort" oor:op="fuse"><value>9</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetHTTPSProxyName" oor:op="fuse"><value>127.0.0.1</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetHTTPSProxyPort" oor:op="fuse"><value>9</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetFTPProxyName" oor:op="fuse"><value>127.0.0.1</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetFTPProxyPort" oor:op="fuse"><value>9</value></prop></item>
<item oor:path="/org.openoffice.Inet/Settings"><prop oor:name="ooInetNoProxy" oor:op="fuse"><value></value></prop></item>
<item oor:path="/org.openoffice.Office.Jobs/Jobs/org.openoffice.Office.Jobs:Job['UpdateCheck']/Arguments"><prop oor:name="AutoCheckEnabled" oor:op="fuse"><value>false</value></prop></item>
</oor:items>
"#;

/// One conversion: `input` saved with the `convert_to` filter (as given to
/// `--convert-to`), optionally opened with a specific import filter.
#[derive(Debug, Clone)]
pub struct Conversion<'a> {
    pub input: &'a Path,
    pub convert_to: &'a str,
    pub infilter: Option<&'a str>,
}

/// Points the HTTP clients of a child process to a closed local port.
pub fn block_network(cmd: &mut Command) {
    for var in [
        "http_proxy",
        "https_proxy",
        "ftp_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "FTP_PROXY",
        "ALL_PROXY",
    ] {
        cmd.env(var, DEAD_PROXY);
    }
    cmd.env_remove("no_proxy").env_remove("NO_PROXY");
}

/// Runs one conversion and returns the file LibreOffice wrote, inside the job's
/// temporary folder.
pub async fn convert(
    tools: &ToolBox,
    ctx: &JobContext,
    conversion: Conversion<'_>,
) -> AppResult<PathBuf> {
    let profile = prepare_profile(&ctx.temp_dir)?;
    let out_dir = fresh_dir(&ctx.temp_dir, "office-out")?;
    let program = program(tools)?;

    // A brand-new profile makes the first start exit with "restart requested".
    for attempt in 0..2 {
        let mut cmd = Command::new(&program);
        harden(&mut cmd);
        block_network(&mut cmd);
        cmd.current_dir(&ctx.temp_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(arguments(&profile, &out_dir, &conversion));
        let output = cmd
            .spawn()
            .map_err(|err| AppError::ProcessFailed(format!("could not start LibreOffice: {err}")))?
            .wait_with_output()
            .await?;
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if output.status.code() == Some(RESTART_EXIT_CODE) && attempt == 0 {
            continue;
        }
        if let Some(file) = single_file(&out_dir)? {
            return Ok(file);
        }
        return Err(failure(&text, output.status.success()));
    }
    Err(AppError::ProcessFailed(
        "LibreOffice asked to restart again".into(),
    ))
}

/// The program to run. On Linux and Windows `soffice` is a launcher that starts
/// `soffice.bin` as a child; running `soffice.bin` directly means cancelling or timing
/// out a job kills the process that does the work, not just the launcher.
fn program(tools: &ToolBox) -> AppResult<PathBuf> {
    let path = tools.path(ToolId::Libreoffice)?;
    let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    Ok(real
        .parent()
        .map(|dir| dir.join("soffice.bin"))
        .filter(|bin| bin.is_file())
        .unwrap_or(path))
}

fn arguments(profile: &Path, out_dir: &Path, conversion: &Conversion<'_>) -> Vec<String> {
    let mut args = vec![
        format!("-env:UserInstallation={}", file_url(profile)),
        "--headless".into(),
        "--invisible".into(),
        "--nologo".into(),
        "--nodefault".into(),
        "--nofirststartwizard".into(),
        "--nolockcheck".into(),
        "--norestore".into(),
    ];
    if let Some(filter) = conversion.infilter {
        args.push(format!("--infilter={filter}"));
    }
    args.push("--convert-to".into());
    args.push(conversion.convert_to.into());
    args.push("--outdir".into());
    args.push(out_dir.display().to_string());
    args.push(conversion.input.display().to_string());
    args
}

/// Creates the job's profile with the hardened settings, once per job.
fn prepare_profile(temp_dir: &Path) -> AppResult<PathBuf> {
    let profile = temp_dir.join("office-profile");
    let user = profile.join("user");
    if !user.is_dir() {
        fs::create_dir_all(&user)?;
        fs::write(user.join("registrymodifications.xcu"), PROFILE_SETTINGS)?;
    }
    Ok(profile)
}

/// An empty folder that did not exist yet: `<name>-1`, `<name>-2`...
pub fn fresh_dir(parent: &Path, name: &str) -> AppResult<PathBuf> {
    for n in 1..=1000 {
        let dir = parent.join(format!("{name}-{n}"));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(AppError::Internal(format!("too many {name} folders")))
}

/// The only regular file in `dir`, if there is exactly one.
fn single_file(dir: &Path) -> AppResult<Option<PathBuf>> {
    let mut files = fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file());
    match (files.next(), files.next()) {
        (Some(file), None) => Ok(Some(file)),
        (None, _) => Ok(None),
        (Some(_), Some(_)) => Err(AppError::ProcessFailed(
            "LibreOffice wrote more than one file".into(),
        )),
    }
}

fn failure(text: &str, exited_ok: bool) -> AppError {
    let detail = tail(text);
    if text.contains("source file could not be loaded") {
        AppError::InvalidFile(
            "LibreOffice could not open the file (damaged, password protected or unsupported)"
                .into(),
        )
    } else if exited_ok {
        AppError::ProcessFailed(format!("LibreOffice produced no file: {detail}"))
    } else {
        AppError::ProcessFailed(format!("LibreOffice failed: {detail}"))
    }
}

fn tail(text: &str) -> String {
    let text = text.trim();
    let count = text.chars().count();
    text.chars()
        .skip(count.saturating_sub(MESSAGE_TAIL_CHARS))
        .collect()
}

/// `file://` URL of an absolute path, as LibreOffice expects for the profile location.
/// Everything but unreserved characters, `/` and `:` is percent-encoded.
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    if !text.starts_with('/') {
        // Windows drive path: file:///C:/...
        url.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                url.push(char::from(byte));
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_file_urls() {
        assert_eq!(
            file_url(Path::new("/tmp/job 1/perfil-ção")),
            "file:///tmp/job%201/perfil-%C3%A7%C3%A3o"
        );
        assert_eq!(
            file_url(Path::new(r"C:\Users\Ana Maria\Temp")),
            "file:///C:/Users/Ana%20Maria/Temp"
        );
        assert_eq!(file_url(Path::new("/a#b?c%d")), "file:///a%23b%3Fc%25d");
    }

    #[test]
    fn arguments_are_a_list_with_the_input_last() {
        let conversion = Conversion {
            input: Path::new("/docs/--convert-to x.docx"),
            convert_to: "pdf",
            infilter: Some("Text (encoded):UTF8"),
        };
        let args = arguments(Path::new("/t/p"), Path::new("/t/out"), &conversion);
        assert_eq!(args[0], "-env:UserInstallation=file:///t/p");
        assert!(args.contains(&"--headless".to_owned()));
        assert!(args.contains(&"--infilter=Text (encoded):UTF8".to_owned()));
        assert_eq!(args.last().unwrap(), "/docs/--convert-to x.docx");
        let to = args.iter().position(|a| a == "--convert-to").unwrap();
        assert_eq!(args[to + 1], "pdf");
    }

    #[test]
    fn explains_failures() {
        assert!(matches!(
            failure("Error: source file could not be loaded\n", true),
            AppError::InvalidFile(_)
        ));
        assert!(matches!(failure("odd", true), AppError::ProcessFailed(_)));
        let long = "x".repeat(5000);
        let AppError::ProcessFailed(message) = failure(&long, false) else {
            panic!("expected a process failure");
        };
        assert!(message.len() < 700);
    }

    #[test]
    fn profile_blocks_macros_and_network() {
        let dir = std::env::temp_dir().join(format!("ec-lo-profile-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let profile = prepare_profile(&dir).unwrap();
        let xcu = fs::read_to_string(profile.join("user/registrymodifications.xcu")).unwrap();
        assert!(xcu.contains("DisableMacrosExecution"));
        assert!(xcu.contains("ooInetHTTPSProxyPort"));
        assert_eq!(fresh_dir(&dir, "o").unwrap(), dir.join("o-1"));
        assert_eq!(fresh_dir(&dir, "o").unwrap(), dir.join("o-2"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
