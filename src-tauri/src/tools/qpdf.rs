//! qpdf: structural PDF operations (merge, split, select, rotate, encrypt, decrypt,
//! compress) and document information.
//!
//! - Every path given to qpdf is absolute and canonical, so it can never be taken for an
//!   option.
//! - Passwords never appear on the command line, where other users could see them: they
//!   go in an argument file (`@file`) or a password file, created only readable by the
//!   user inside the job's temporary folder.
//! - `--warning-exit-0` makes qpdf succeed when it only had warnings (for example about a
//!   damaged but recoverable file).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tokio::process::Command;
use ts_rs::TS;

use super::process::{capture, run_streaming};
use super::{ToolBox, ToolId};
use crate::error::{AppError, AppResult};

/// Timeout for the quick information commands.
const INFO_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest password accepted (PDF 2.0 limits UTF-8 passwords to 127 bytes).
pub const MAX_PASSWORD_BYTES: usize = 127;

/// What the app knows about a PDF before any operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PdfInfo {
    /// Number of pages; unknown when the file needs a password.
    pub pages: Option<u32>,
    pub encrypted: bool,
    /// A password is needed to open the file.
    pub needs_password: bool,
}

/// Reads the page count and encryption state. `None` when qpdf is not available or the
/// file is not a readable PDF.
pub async fn info(tools: &ToolBox, path: &Path) -> Option<PdfInfo> {
    let mut cmd = tools.command(ToolId::Qpdf).ok()?;
    cmd.arg("--requires-password").arg(path);
    let out = capture(cmd, INFO_TIMEOUT).await.ok()?;
    // 0: password needed; 2: not encrypted; 3: encrypted, opens without a password.
    let (encrypted, needs_password) = match out.status.code() {
        Some(0) => (true, true),
        Some(2) => (false, false),
        Some(3) => (true, false),
        _ => return None,
    };
    let pages = if needs_password {
        None
    } else {
        let mut cmd = tools.command(ToolId::Qpdf).ok()?;
        cmd.arg("--show-npages").arg(path);
        let out = capture(cmd, INFO_TIMEOUT).await.ok()?;
        if !out.status.success() {
            return None;
        }
        out.stdout.trim().parse().ok()
    };
    Some(PdfInfo {
        pages,
        encrypted,
        needs_password,
    })
}

/// A qpdf command with the options every operation uses.
pub fn command(tools: &ToolBox) -> AppResult<Command> {
    let mut cmd = tools.command(ToolId::Qpdf)?;
    cmd.arg("--warning-exit-0");
    Ok(cmd)
}

/// Runs qpdf, turning its error messages into app errors.
pub async fn run(cmd: Command) -> AppResult<()> {
    run_streaming(cmd, |_| {})
        .await
        .map(|_| ())
        .map_err(map_error)
}

fn map_error(err: AppError) -> AppError {
    let AppError::ProcessFailed(message) = &err else {
        return err;
    };
    let lower = message.to_lowercase();
    if lower.contains("invalid password") {
        AppError::Password(tail(message))
    } else if lower.contains("not a pdf file")
        || lower.contains("can't find pdf header")
        || lower.contains("unable to find trailer")
        || lower.contains("file is damaged")
    {
        AppError::InvalidFile(tail(message))
    } else if lower.contains("out of range") || lower.contains("page range") {
        AppError::InvalidParameter(tail(message))
    } else {
        err
    }
}

/// Last line of qpdf's message, which carries the actual error.
fn tail(message: &str) -> String {
    message
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(message)
        .trim()
        .to_owned()
}

/// Checks a password before it is written to an argument file.
pub fn validate_password(password: &str, what: &str) -> AppResult<()> {
    if password.is_empty() {
        return Err(AppError::InvalidParameter(format!("the {what} is empty")));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(AppError::InvalidParameter(format!(
            "the {what} is longer than {MAX_PASSWORD_BYTES} bytes"
        )));
    }
    if password.chars().any(char::is_control) {
        return Err(AppError::InvalidParameter(format!(
            "the {what} has control characters"
        )));
    }
    Ok(())
}

/// Writes a file only the current user can read (passwords, argument lists).
pub fn write_private(path: &Path, content: &str) -> AppResult<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

/// What a protected PDF still allows to whoever has only the user password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Restrictions {
    pub no_print: bool,
    pub no_copy: bool,
    pub no_edit: bool,
}

impl Restrictions {
    pub fn any(self) -> bool {
        self.no_print || self.no_copy || self.no_edit
    }
}

/// Arguments (one per line) to encrypt `input` into `output` with AES-256.
pub fn encrypt_args(
    input: &Path,
    output: &Path,
    user: &str,
    owner: &str,
    restrictions: Restrictions,
) -> AppResult<String> {
    let mut args = vec![
        "--warning-exit-0".to_owned(),
        path_arg(input)?,
        "--encrypt".to_owned(),
        user.to_owned(),
        owner.to_owned(),
        "256".to_owned(),
    ];
    if restrictions.no_print {
        args.push("--print=none".into());
    }
    if restrictions.no_copy {
        args.push("--extract=n".into());
    }
    if restrictions.no_edit {
        args.push("--modify=none".into());
    }
    args.push("--".into());
    args.push(path_arg(output)?);
    Ok(args.join("\n") + "\n")
}

/// A path as one line of an argument file.
fn path_arg(path: &Path) -> AppResult<String> {
    let text = path
        .to_str()
        .ok_or_else(|| AppError::InvalidPath(format!("{} is not valid UTF-8", path.display())))?;
    if text.contains(['\n', '\r']) {
        return Err(AppError::InvalidPath(format!(
            "{} has a line break in its name",
            path.display()
        )));
    }
    Ok(text.to_owned())
}

/// Files qpdf produced with `--split-pages`, in page order.
pub fn split_outputs(dir: &Path, prefix: &str) -> AppResult<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".pdf"))
        })
        .collect();
    // qpdf zero-pads the page numbers, so names sort in page order.
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_arguments_keep_passwords_as_single_lines() {
        let args = encrypt_args(
            Path::new("/in.pdf"),
            Path::new("/out dir/out.pdf"),
            "user pass",
            "owner",
            Restrictions {
                no_print: true,
                no_copy: false,
                no_edit: true,
            },
        )
        .unwrap();
        assert_eq!(
            args,
            "--warning-exit-0\n/in.pdf\n--encrypt\nuser pass\nowner\n256\n--print=none\n--modify=none\n--\n/out dir/out.pdf\n"
        );
    }

    #[test]
    fn rejects_passwords_that_could_break_the_argument_file() {
        assert!(validate_password("ok pass", "password").is_ok());
        assert!(validate_password("", "password").is_err());
        assert!(validate_password("a\nb", "password").is_err());
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_BYTES + 1), "password").is_err());
        assert!(path_arg(Path::new("/a\nb.pdf")).is_err());
    }

    #[test]
    fn maps_qpdf_errors() {
        let failed = |m: &str| map_error(AppError::ProcessFailed(m.into()));
        assert!(matches!(
            failed("process exited with 2: qpdf: /a.pdf: invalid password"),
            AppError::Password(_)
        ));
        assert!(matches!(
            failed("WARNING: x\nqpdf: /a.pdf: not a PDF file"),
            AppError::InvalidFile(_)
        ));
        assert!(matches!(
            failed("qpdf: number 9 out of range"),
            AppError::InvalidParameter(_)
        ));
        assert!(matches!(
            failed("something else"),
            AppError::ProcessFailed(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_owner_only_and_never_overwritten() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ec-qpdf-private-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("secret");
        write_private(&file, "pw").unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(write_private(&file, "again").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
