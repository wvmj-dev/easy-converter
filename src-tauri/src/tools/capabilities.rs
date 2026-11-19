//! Parsing of tool output: versions, FFmpeg encoders and Tesseract languages.

const MAX_VERSION_LEN: usize = 100;

/// Extracts a short version string from the output of a `--version` style command:
/// the first non-empty line, without the copyright notice.
pub fn parse_version(output: &str) -> Option<String> {
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line
        .split(" : Copyright")
        .next()
        .and_then(|l| l.split(" Copyright").next())
        .unwrap_or(line)
        .trim();
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(MAX_VERSION_LEN).collect())
}

/// Parses `ffmpeg -hide_banner -encoders`. Encoder lines look like
/// ` V....D libx264              libx264 H.264 / AVC ...`.
pub fn parse_ffmpeg_encoders(output: &str) -> Vec<String> {
    let mut encoders: Vec<String> = output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            let is_flags = flags.len() == 6
                && flags.starts_with(['V', 'A', 'S'])
                && flags.chars().all(|c| "VASFXBD.".contains(c));
            // The legend at the top uses "=" instead of an encoder name.
            (is_flags && name != "=").then(|| name.to_owned())
        })
        .collect();
    encoders.sort();
    encoders.dedup();
    encoders
}

/// Parses `tesseract --list-langs`: a header line followed by one language per line.
pub fn parse_tesseract_languages(output: &str) -> Vec<String> {
    let mut langs: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("List of") && !l.contains(' '))
        .map(str::to_owned)
        .collect();
    langs.sort();
    langs.dedup();
    langs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        let cases = [
            (
                "ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023 the FFmpeg developers\nbuilt with gcc",
                "ffmpeg version 6.1.1-3ubuntu5",
            ),
            ("qpdf version 11.9.0\nRun qpdf --copyright", "qpdf version 11.9.0"),
            ("tesseract 5.3.4\n leptonica-1.82.0", "tesseract 5.3.4"),
            ("10.02.1\n", "10.02.1"),
            ("LibreOffice 24.2.7.2 420(Build:2)\n", "LibreOffice 24.2.7.2 420(Build:2)"),
            ("ebook-convert (calibre 7.6.0)\nCreated by: Kovid Goyal", "ebook-convert (calibre 7.6.0)"),
            (
                "\n7-Zip (z) 24.08 (x64) : Copyright (c) 1999-2024 Igor Pavlov : 2024-08-11\n",
                "7-Zip (z) 24.08 (x64)",
            ),
        ];
        for (output, expected) in cases {
            assert_eq!(parse_version(output).as_deref(), Some(expected));
        }
        assert_eq!(parse_version("  \n\n"), None);
    }

    #[test]
    fn parses_ffmpeg_encoders() {
        let output = "Encoders:\n \
            V..... = Video\n \
            A..... = Audio\n \
            ------\n \
            V....D libx264              libx264 H.264 / AVC\n \
            V....D libvpx-vp9           libvpx VP9\n \
            A....D aac                  AAC (Advanced Audio Coding)\n \
            S..... srt                  SubRip subtitle\n";
        assert_eq!(
            parse_ffmpeg_encoders(output),
            vec!["aac", "libvpx-vp9", "libx264", "srt"]
        );
    }

    #[test]
    fn parses_tesseract_languages() {
        let output = "List of available languages in \"/usr/share/tesseract-ocr/5/tessdata/\" (3):\neng\nosd\npor\n";
        assert_eq!(parse_tesseract_languages(output), vec!["eng", "osd", "por"]);
    }
}
