//! Subtitles, ebooks and archives end to end. Archives and subtitle conversion run in
//! Rust and are always tested; FFmpeg, Calibre and 7-Zip tests are skipped, with a
//! message, when the tool is missing. Fixtures are written here, including hostile
//! archives (paths leaving the folder, links, encrypted entries).

mod common;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::*;
use easy_converter_lib::error::ErrorCode;
use easy_converter_lib::ops;
use easy_converter_lib::tools::{ffmpeg, ToolId};
use serde_json::json;

async fn env(name: &str) -> Env {
    Env::new(name).with_tools().await
}

/// Every file under `dir`, as sorted relative paths with `/`.
fn tree(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                walk(base, &path, out);
            } else {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(if meta.file_type().is_symlink() {
                    format!("{rel} (link)")
                } else {
                    rel
                });
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn build_error(env: &Env, request: serde_json::Value) -> ErrorCode {
    let request = serde_json::from_value(request).unwrap();
    ops::build_job(request, &env.ops_context())
        .err()
        .unwrap()
        .code()
}

// ---------------------------------------------------------------------------------------
// Subtitles
// ---------------------------------------------------------------------------------------

const SRT_1252: &[u8] = b"1\r\n00:00:00,500 --> 00:00:01,500\r\nOl\xe1, <i>Jo\xe3o</i>!\r\n\r\n2\r\n00:00:02,000 --> 00:00:03,000\r\nTchau & at\xe9 logo\r\n";

#[tokio::test]
async fn subtitles_convert_and_shift_in_rust() {
    let env = env("subs").await;
    let srt = env.path("filme.srt");
    fs::write(&srt, SRT_1252).unwrap();

    let vtt = assert_done(
        &env.run_json(json!({ "type": "subtitleConvert", "input": srt, "format": "vtt" }))
            .await,
    );
    assert_eq!(
        fs::read_to_string(&vtt).unwrap(),
        "WEBVTT\n\n00:00:00.500 --> 00:00:01.500\nOlá, <i>João</i>!\n\n00:00:02.000 --> 00:00:03.000\nTchau &amp; até logo\n\n"
    );
    let ass = assert_done(
        &env.run_json(json!({ "type": "subtitleConvert", "input": vtt, "format": "ass" }))
            .await,
    );
    let text = fs::read_to_string(&ass).unwrap();
    assert!(
        text.contains("Dialogue: 0,0:00:00.50,0:00:01.50,Default,,0,0,0,,Olá, {\\i1}João{\\i0}!"),
        "{text}"
    );

    let shifted = assert_done(
        &env.run_json(json!({ "type": "subtitleShift", "input": srt, "offsetMs": -1000 }))
            .await,
    );
    assert_eq!(shifted.extension().unwrap(), "srt");
    assert_eq!(
        fs::read_to_string(&shifted).unwrap(),
        "1\n00:00:00,000 --> 00:00:00,500\nOlá, <i>João</i>!\n\n2\n00:00:01,000 --> 00:00:02,000\nTchau & até logo\n\n"
    );

    assert_eq!(
        build_error(
            &env,
            json!({ "type": "subtitleShift", "input": srt, "offsetMs": 0 })
        ),
        ErrorCode::InvalidParameter
    );
    assert_eq!(
        build_error(
            &env,
            json!({ "type": "subtitleConvert", "input": srt, "format": "srt" })
        ),
        ErrorCode::InvalidParameter
    );
    let empty = env.path("vazio.vtt");
    fs::write(&empty, "WEBVTT\n\nNOTE nada\n").unwrap();
    let job = env
        .run_json(json!({ "type": "subtitleConvert", "input": empty, "format": "srt" }))
        .await;
    assert_failed(&job, ErrorCode::InvalidFile);
}

#[tokio::test]
async fn subtitles_are_embedded_as_tracks() {
    let env = env("subs-embed").await;
    if !env.has(ToolId::Ffmpeg) || !env.has(ToolId::Ffprobe) {
        return;
    }
    let srt = env.path("legenda.srt");
    fs::write(&srt, SRT_1252).unwrap();
    for (name, codec) in [
        ("clip.mp4", "libx264"),
        ("clip.mkv", "libx264"),
        ("clip.avi", "mpeg4"),
    ] {
        let video = env.path(name);
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
            ])
            .arg("testsrc=size=160x120:rate=10:duration=3")
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=duration=3",
                "-c:v",
                codec,
                "-pix_fmt",
                "yuv420p",
                "-shortest",
            ])
            .arg(&video)
            .status()
            .unwrap();
        assert!(status.success());
        let job = env
            .run_json(json!({ "type": "videoEmbedSubtitle", "input": video, "subtitle": srt, "language": "por" }))
            .await;
        let out = assert_done(&job);
        let expected = if name.ends_with("avi") {
            "mkv"
        } else {
            &name[5..]
        };
        assert_eq!(out.extension().unwrap(), expected, "{name}");
        let info = ffmpeg::probe(&env.tools, &out).await.unwrap();
        assert_eq!(info.subtitle_count, 1, "{name}");
        assert!(info.video.is_some() && !info.audio.is_empty(), "{name}");
    }
    assert_eq!(
        build_error(
            &env,
            json!({ "type": "videoEmbedSubtitle", "input": env.path("clip.mp4"), "subtitle": srt, "language": "portuguese" })
        ),
        ErrorCode::InvalidParameter
    );
}

// ---------------------------------------------------------------------------------------
// Ebooks
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn ebooks_convert_with_calibre() {
    let env = env("ebooks").await;
    if !env.has(ToolId::Calibre) {
        return;
    }
    // Calibre makes the EPUB fixture from plain text.
    let txt = env.path("livro.txt");
    fs::write(
        &txt,
        "Capítulo um\n\nEra uma vez um conversor.\n\nCapítulo dois\n\nFim.\n",
    )
    .unwrap();
    let epub = env.path("livro.epub");
    let status = Command::new("ebook-convert")
        .env("QT_QPA_PLATFORM", "offscreen")
        .arg(&txt)
        .arg(&epub)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());

    let azw3 = assert_done(
        &env.run_json(json!({ "type": "ebookConvert", "input": epub, "format": "azw3" }))
            .await,
    );
    assert_eq!(&fs::read(&azw3).unwrap()[60..68], b"BOOKMOBI");
    let pdf = assert_done(
        &env.run_json(json!({ "type": "ebookConvert", "input": azw3, "format": "pdf" }))
            .await,
    );
    assert!(fs::read(&pdf).unwrap().starts_with(b"%PDF-"));

    let fake = env.path("falso.epub");
    fs::write(&fake, "<html>not an epub</html>").unwrap();
    assert_eq!(
        build_error(
            &env,
            json!({ "type": "ebookConvert", "input": fake, "format": "pdf" })
        ),
        ErrorCode::InvalidFile
    );
    assert_eq!(
        build_error(
            &env,
            json!({ "type": "ebookConvert", "input": epub, "format": "epub" })
        ),
        ErrorCode::InvalidParameter
    );
}

// ---------------------------------------------------------------------------------------
// Archives
// ---------------------------------------------------------------------------------------

fn sample_files(env: &Env) -> Vec<PathBuf> {
    let dir = env.path("in");
    fs::create_dir_all(dir.join("outra")).unwrap();
    let a = dir.join("a.txt");
    let b = dir.join("outra").join("a.txt");
    let big = dir.join("dados.bin");
    fs::write(&a, "primeiro").unwrap();
    fs::write(&b, "segundo").unwrap();
    let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    fs::write(&big, data).unwrap();
    vec![a, b, big]
}

#[tokio::test]
async fn archives_round_trip_in_every_format() {
    let env = env("archives").await;
    let inputs = sample_files(&env);
    for (format, name) in [
        ("zip", "a-convertido.zip"),
        ("tar", "a-convertido.tar"),
        ("tarGz", "a-convertido.tar.gz"),
        ("sevenZ", "a-convertido.7z"),
    ] {
        let archive = assert_done(
            &env.run_json(json!({ "type": "archiveCreate", "inputs": inputs, "format": format }))
                .await,
        );
        assert_eq!(archive.file_name().unwrap(), name);
        let folder = assert_done(
            &env.run_json(json!({ "type": "archiveExtract", "input": archive }))
                .await,
        );
        assert!(folder.is_dir(), "{format}");
        assert_eq!(
            tree(&folder),
            ["a (2).txt", "a.txt", "dados.bin"],
            "{format}"
        );
        assert_eq!(
            fs::read_to_string(folder.join("a (2).txt")).unwrap(),
            "segundo"
        );
        assert_eq!(
            fs::read(folder.join("dados.bin")).unwrap(),
            fs::read(&inputs[2]).unwrap()
        );
        fs::remove_dir_all(&folder).unwrap();
        fs::remove_file(&archive).unwrap();
    }

    // A single gzip-compressed file.
    let gz = env.path("notas.txt.gz");
    let mut encoder = flate2::write::GzEncoder::new(
        fs::File::create(&gz).unwrap(),
        flate2::Compression::default(),
    );
    encoder.write_all(b"texto comprimido").unwrap();
    encoder.finish().unwrap();
    let folder = assert_done(
        &env.run_json(json!({ "type": "archiveExtract", "input": gz }))
            .await,
    );
    assert_eq!(folder.file_name().unwrap(), "notas-convertido");
    assert_eq!(
        fs::read_to_string(folder.join("notas.txt")).unwrap(),
        "texto comprimido"
    );

    // Extracting twice never overwrites the first folder.
    let again = assert_done(
        &env.run_json(json!({ "type": "archiveExtract", "input": gz }))
            .await,
    );
    assert_eq!(again.file_name().unwrap(), "notas-convertido (2)");
}

#[tokio::test]
async fn hostile_archives_stay_inside_the_folder() {
    let env = env("archives-hostile").await;

    // ZIP: escaping and absolute names, a link and a Windows device name.
    let zip_path = env.path("mau.zip");
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for name in [
            "../fora.txt",
            "/etc/absoluto.txt",
            "ok/../../fora2.txt",
            "dir/CON.txt",
            "normal.txt",
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.add_symlink("link", "/etc/passwd", options).unwrap();
        zip.finish().unwrap();
    }
    let folder = assert_done(
        &env.run_json(json!({ "type": "archiveExtract", "input": zip_path }))
            .await,
    );
    assert_eq!(
        tree(&folder),
        ["dir/_CON.txt", "etc/absoluto.txt", "normal.txt"]
    );
    assert!(
        !env.path("fora.txt").exists() && !env.dir.parent().unwrap().join("fora2.txt").exists()
    );

    // TAR: a name with `..` written straight into the header, a symlink and a hard link.
    let tar_path = env.path("mau.tar");
    {
        let mut builder = tar::Builder::new(fs::File::create(&tar_path).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_entry_type(tar::EntryType::Regular);
        let raw = b"../../escapou.txt";
        header.as_old_mut().name[..raw.len()].copy_from_slice(raw);
        header.set_cksum();
        builder.append(&header, &b"x"[..]).unwrap();
        for (kind, name) in [(tar::EntryType::Symlink, "s"), (tar::EntryType::Link, "h")] {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(0);
            builder
                .append_link(&mut header, name, "/etc/passwd")
                .unwrap();
        }
        let mut header = tar::Header::new_gnu();
        header.set_size(2);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, "bom.txt", &b"ok"[..])
            .unwrap();
        builder.finish().unwrap();
    }
    let folder = assert_done(
        &env.run_json(json!({ "type": "archiveExtract", "input": tar_path }))
            .await,
    );
    assert_eq!(tree(&folder), ["bom.txt"]);

    // Damaged and empty archives.
    let broken = env.path("quebrado.zip");
    fs::write(&broken, b"PK\x03\x04 not really").unwrap();
    let job = env
        .run_json(json!({ "type": "archiveExtract", "input": broken }))
        .await;
    assert_failed(&job, ErrorCode::InvalidFile);
    // A folder is not an archive.
    let dir = env.path("pasta.zip");
    fs::create_dir(&dir).unwrap();
    assert_eq!(
        build_error(&env, json!({ "type": "archiveExtract", "input": dir })),
        ErrorCode::InvalidPath
    );
}

#[tokio::test]
async fn seven_zip_handles_what_rust_does_not() {
    let env = env("archives-7zip").await;
    if !env.has(ToolId::SevenZip) {
        // Without 7-Zip, RAR and xz are refused before queueing.
        let rar = env.path("x.rar");
        fs::write(&rar, "Rar!").unwrap();
        assert_eq!(
            build_error(&env, json!({ "type": "archiveExtract", "input": rar })),
            ErrorCode::ToolMissing
        );
        return;
    }
    let inputs = sample_files(&env);
    let seven = which_7z();

    // Encrypted ZIP: the Rust reader hands it to 7-Zip, which needs the password.
    let locked = env.path("senha.zip");
    let status = Command::new(&seven)
        .args(["a", "-tzip", "-psegredo", "-bso0", "-bsp0"])
        .arg(&locked)
        .arg(&inputs[0])
        .status()
        .unwrap();
    assert!(status.success());
    let job = env
        .run_json(json!({ "type": "archiveExtract", "input": locked }))
        .await;
    assert_failed(&job, ErrorCode::Password);

    // tar.xz: 7-Zip takes out the .tar, the app extracts it.
    let tar = env.path("pacote.tar");
    let status = Command::new(&seven)
        .args(["a", "-ttar", "-bso0", "-bsp0"])
        .arg(&tar)
        .arg(&inputs[0])
        .arg(&inputs[2])
        .status()
        .unwrap();
    assert!(status.success());
    let xz = env.path("pacote.tar.xz");
    let status = Command::new(&seven)
        .args(["a", "-txz", "-bso0", "-bsp0"])
        .arg(&xz)
        .arg(&tar)
        .status()
        .unwrap();
    assert!(status.success());
    let folder = assert_done(
        &env.run_json(json!({ "type": "archiveExtract", "input": xz }))
            .await,
    );
    assert_eq!(folder.file_name().unwrap(), "pacote-convertido");
    assert_eq!(tree(&folder), ["a.txt", "dados.bin"]);
}

fn which_7z() -> String {
    for name in ["7zz", "7z"] {
        if Command::new(name).arg("i").output().is_ok() {
            return name.to_owned();
        }
    }
    panic!("7-Zip should be installed");
}
