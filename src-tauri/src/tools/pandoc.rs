//! Running Pandoc conversions.
//!
//! Pandoc downloads remote images when it writes formats that embed them (DOCX, ODT,
//! RTF), and its `--sandbox` mode cannot be used: distribution builds without embedded
//! data files then fail to write DOCX. Instead every run goes through a Lua filter,
//! written by the app, that removes images before any writer sees them:
//! - for formats that embed images, only relative paths inside the input's folder are
//!   kept (no URL scheme, no absolute path, no `..`);
//! - for text formats (Markdown, HTML, plain text) every image becomes its description,
//!   since the image files would not travel with the output.
//!
//! Removed images are replaced by their description. HTTP clients are also pointed to a
//! closed local port, as a second barrier.

use std::fs;
use std::path::{Path, PathBuf};

use super::libreoffice::block_network;
use super::process::run_streaming;
use super::{ToolBox, ToolId};
use crate::error::{AppError, AppResult};
use crate::jobs::JobContext;

const IMAGE_FILTER: &str = r#"-- Written by Easy Converter: keeps Pandoc from reading remote or outside files.
local keep_local = KEEP_LOCAL
local function is_local(src)
  local first = src:sub(1, 1)
  if src:find('^%a[%w+.-]*:') or first == '/' or first == '\\' or first == '' then
    return false
  end
  for part in src:gmatch('[^/\\]+') do
    if part == '..' then return false end
  end
  return true
end
function Image(el)
  if keep_local and is_local(el.src) then return el end
  return el.caption
end
"#;

/// Input formats, as Pandoc names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    Markdown,
    Html,
    Docx,
    Odt,
}

impl Reader {
    fn name(self) -> &'static str {
        match self {
            Reader::Markdown => "markdown",
            Reader::Html => "html",
            Reader::Docx => "docx",
            Reader::Odt => "odt",
        }
    }
}

/// Output formats, as Pandoc names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    Docx,
    Odt,
    Rtf,
    Html,
    /// GitHub-flavored Markdown, the most widely understood dialect.
    Markdown,
    Plain,
}

impl Writer {
    fn name(self) -> &'static str {
        match self {
            Writer::Docx => "docx",
            Writer::Odt => "odt",
            Writer::Rtf => "rtf",
            Writer::Html => "html5",
            Writer::Markdown => "gfm",
            Writer::Plain => "plain",
        }
    }

    /// Whether the format carries the images inside the file.
    fn embeds_images(self) -> bool {
        matches!(self, Writer::Docx | Writer::Odt | Writer::Rtf)
    }

    /// Text formats need `--standalone` to be complete documents.
    fn standalone(self) -> bool {
        matches!(self, Writer::Rtf | Writer::Html)
    }
}

/// Converts `input` into `output`.
pub async fn convert(
    tools: &ToolBox,
    ctx: &JobContext,
    input: &Path,
    reader: Reader,
    writer: Writer,
    output: &Path,
) -> AppResult<()> {
    let filter = write_filter(&ctx.temp_dir, writer.embeds_images())?;
    let mut cmd = tools.command(ToolId::Pandoc)?;
    block_network(&mut cmd);
    cmd.current_dir(&ctx.temp_dir);
    cmd.args(arguments(&filter, input, reader, writer, output));
    run_streaming(cmd, |_| {})
        .await
        .map(|_| ())
        .map_err(|err| match err {
            AppError::ProcessFailed(message) => {
                AppError::ProcessFailed(format!("Pandoc: {message}"))
            }
            other => other,
        })?;
    if output.is_file() {
        Ok(())
    } else {
        Err(AppError::ProcessFailed("Pandoc produced no file".into()))
    }
}

fn arguments(
    filter: &Path,
    input: &Path,
    reader: Reader,
    writer: Writer,
    output: &Path,
) -> Vec<String> {
    let resources = input
        .parent()
        .map_or_else(|| ".".to_owned(), |dir| dir.display().to_string());
    let mut args = vec![
        "--lua-filter".to_owned(),
        filter.display().to_string(),
        "--resource-path".to_owned(),
        resources,
        "--from".to_owned(),
        reader.name().to_owned(),
        "--to".to_owned(),
        writer.name().to_owned(),
    ];
    if writer.standalone() {
        args.push("--standalone".to_owned());
    }
    args.push("--output".to_owned());
    args.push(output.display().to_string());
    args.push(input.display().to_string());
    args
}

fn write_filter(temp_dir: &Path, keep_local: bool) -> AppResult<PathBuf> {
    let name = if keep_local {
        "images-local.lua"
    } else {
        "images-none.lua"
    };
    let path = temp_dir.join(name);
    if !path.is_file() {
        let flag = if keep_local { "true" } else { "false" };
        fs::write(&path, IMAGE_FILTER.replace("KEEP_LOCAL", flag))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_always_carry_the_filter() {
        let args = arguments(
            Path::new("/t/f.lua"),
            Path::new("/docs/notes.md"),
            Reader::Markdown,
            Writer::Html,
            Path::new("/t/out.html"),
        );
        assert_eq!(
            args,
            [
                "--lua-filter",
                "/t/f.lua",
                "--resource-path",
                "/docs",
                "--from",
                "markdown",
                "--to",
                "html5",
                "--standalone",
                "--output",
                "/t/out.html",
                "/docs/notes.md"
            ]
        );
    }

    #[test]
    fn filter_keeps_local_images_only_for_embedding_formats() {
        let dir = std::env::temp_dir().join(format!("ec-pandoc-filter-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let local = fs::read_to_string(write_filter(&dir, true).unwrap()).unwrap();
        let none = fs::read_to_string(write_filter(&dir, false).unwrap()).unwrap();
        assert!(local.contains("local keep_local = true"));
        assert!(none.contains("local keep_local = false"));
        assert!(!local.contains("KEEP_LOCAL"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
