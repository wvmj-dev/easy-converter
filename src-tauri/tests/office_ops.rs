//! Document, spreadsheet and presentation operations end to end. LibreOffice and Pandoc
//! are optional: tests that need them are skipped, with a message, when missing. Fixtures
//! are text files written here, or made from them by the tools themselves.

mod common;

use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use common::*;
use easy_converter_lib::error::ErrorCode;
use easy_converter_lib::ops;
use easy_converter_lib::tools::ToolId;
use serde_json::json;

async fn env(name: &str) -> Env {
    Env::new(name).with_tools().await
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

/// ZIP files (DOCX, XLSX, PPTX, ODF) store member names uncompressed.
fn zip_has(path: &Path, member: &str) -> bool {
    let bytes = read(path);
    bytes.starts_with(b"PK") && contains(&bytes, member.as_bytes())
}

fn is_pdf(path: &Path) -> bool {
    read(path).starts_with(b"%PDF-")
}

async fn pdf_text(env: &Env, pdf: &Path) -> Option<String> {
    if !env.tools.is_available(ToolId::Pdfium) {
        return None;
    }
    let job = env
        .run_json(json!({ "type": "pdfExtractText", "input": pdf }))
        .await;
    Some(fs::read_to_string(assert_done(&job)).unwrap())
}

/// A local server that records whether anything connected to it.
struct Tripwire {
    listener: TcpListener,
}

impl Tripwire {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        Self { listener }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}/{path}", self.listener.local_addr().unwrap())
    }

    fn was_contacted(&self) -> bool {
        std::thread::sleep(Duration::from_millis(200));
        self.listener.accept().is_ok()
    }
}

#[tokio::test]
async fn csv_and_json_convert_in_rust() {
    let env = env("csvjson").await;
    let csv = env.path("people.csv");
    fs::write(
        &csv,
        b"nome;idade;cidade\nJo\xe3o;30;S\xe3o Paulo\nAna;007;\n",
    )
    .unwrap();

    let job = env
        .run_json(json!({ "type": "csvToJson", "input": csv, "header": true, "typed": true }))
        .await;
    let out = assert_done(&job);
    assert_eq!(out.file_name().unwrap(), "people-convertido.json");
    let value: serde_json::Value = serde_json::from_slice(&read(&out)).unwrap();
    assert_eq!(
        value,
        json!([
            {"nome": "João", "idade": 30, "cidade": "São Paulo"},
            {"nome": "Ana", "idade": "007", "cidade": null}
        ])
    );

    let job = env
        .run_json(json!({
            "type": "jsonToCsv",
            "input": out,
            "delimiter": "comma",
            "bom": false,
            "escapeFormulas": true
        }))
        .await;
    let back = assert_done(&job);
    assert_eq!(
        fs::read_to_string(back).unwrap(),
        "nome,idade,cidade\nJoão,30,São Paulo\nAna,007,\n"
    );

    let broken = env.path("broken.json");
    fs::write(&broken, "[{\"a\": 1}").unwrap();
    let job = env
        .run_json(json!({
            "type": "jsonToCsv",
            "input": broken,
            "delimiter": "semicolon",
            "bom": true,
            "escapeFormulas": true
        }))
        .await;
    assert_failed(&job, ErrorCode::InvalidFile);
}

#[tokio::test]
async fn missing_tools_and_same_format_are_refused_before_queueing() {
    let env = Env::new("office-refused");
    let doc = env.path("a.docx");
    fs::write(&doc, b"PK\x03\x04").unwrap();
    let request =
        serde_json::from_value(json!({ "type": "documentConvert", "input": doc, "format": "pdf" }))
            .unwrap();
    let err = ops::build_job(request, &env.ops_context()).err().unwrap();
    assert_eq!(err.code(), ErrorCode::ToolMissing);

    let env = Env::new("office-same").with_tools().await;
    if !env.has(ToolId::Libreoffice) {
        return;
    }
    let sheet = env.path("a.ods");
    fs::write(&sheet, b"PK\x03\x04").unwrap();
    for request in [
        json!({ "type": "documentConvert", "input": doc, "format": "docx" }),
        json!({ "type": "spreadsheetConvert", "input": sheet, "format": "ods" }),
        json!({ "type": "documentConvert", "input": sheet, "format": "pdf" }),
    ] {
        let request = serde_json::from_value(request).unwrap();
        let err = ops::build_job(request, &env.ops_context()).err().unwrap();
        assert_eq!(err.code(), ErrorCode::InvalidParameter, "{err}");
    }
}

#[tokio::test]
async fn text_and_word_documents_with_libreoffice() {
    let env = env("office-docs").await;
    if !env.has(ToolId::Libreoffice) {
        return;
    }
    // Windows-1252 text, as written by old Windows programs.
    let txt = env.path("carta.txt");
    fs::write(&txt, b"Ol\xe1, Jo\xe3o!\nSegunda linha.\n").unwrap();

    let docx = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": txt, "format": "docx" }))
            .await,
    );
    assert!(zip_has(&docx, "word/document.xml"));

    let pdf = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": docx, "format": "pdf" }))
            .await,
    );
    assert!(is_pdf(&pdf));
    if let Some(text) = pdf_text(&env, &pdf).await {
        assert!(text.contains("Olá, João!"), "{text}");
    }

    let back = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": docx, "format": "txt" }))
            .await,
    );
    let text = fs::read_to_string(back).unwrap();
    assert!(text.contains("Olá, João!"), "{text}");

    let odt = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": docx, "format": "odt" }))
            .await,
    );
    assert!(zip_has(&odt, "content.xml"));

    // A file whose content does not match its name is refused before queueing.
    let fake = env.path("fake.docx");
    fs::write(&fake, "<html><body>not a docx</body></html>").unwrap();
    let request = serde_json::from_value(
        json!({ "type": "documentConvert", "input": fake, "format": "pdf" }),
    )
    .unwrap();
    let err = ops::build_job(request, &env.ops_context()).err().unwrap();
    assert_eq!(err.code(), ErrorCode::InvalidFile);

    // A damaged document: LibreOffice cannot open it, and the job says so.
    let damaged = env.path("damaged.docx");
    fs::write(&damaged, b"PK\x03\x04 this is not a zip archive").unwrap();
    let job = env
        .run_json(json!({ "type": "documentConvert", "input": damaged, "format": "pdf" }))
        .await;
    assert_failed(&job, ErrorCode::InvalidFile);
}

#[tokio::test]
async fn markdown_and_html_with_pandoc_never_reach_the_network() {
    let env = env("office-markup").await;
    if !env.has(ToolId::Pandoc) {
        return;
    }
    let tripwire = Tripwire::new();
    write_png(&env.path("local.png"), &rgba_pattern(8, 8), None);
    let md = env.path("notes.md");
    fs::write(
        &md,
        format!(
            "# Título\n\nTexto **forte**.\n\n![local](local.png)\n\n![remota]({})\n\n![fora](/etc/hostname)\n",
            tripwire.url("x.png")
        ),
    )
    .unwrap();
    let html = env.path("page.html");
    fs::write(
        &html,
        format!(
            "<html><body><h1>Página</h1><img src=\"{}\" alt=\"remota\"><p>fim</p></body></html>",
            tripwire.url("y.png")
        ),
    )
    .unwrap();

    let docx = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": md, "format": "docx" }))
            .await,
    );
    assert!(
        zip_has(&docx, "word/media/"),
        "local image should be embedded"
    );
    let media = {
        let mut bytes = Vec::new();
        fs::File::open(&docx)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
            .windows(b"word/media/".len())
            .filter(|w| *w == b"word/media/")
            .count()
    };
    // One image: its name appears in the local header and the central directory.
    assert_eq!(media, 2, "only the local image may be embedded");

    let rtf = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": html, "format": "rtf" }))
            .await,
    );
    assert!(read(&rtf).starts_with(b"{\\rtf"));

    let markdown = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": docx, "format": "md" }))
            .await,
    );
    let text = fs::read_to_string(markdown).unwrap();
    assert!(
        text.contains("# Título") && text.contains("**forte**"),
        "{text}"
    );
    assert!(
        !text.contains("media/"),
        "images are not kept in Markdown: {text}"
    );

    let page = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": md, "format": "html" }))
            .await,
    );
    let text = fs::read_to_string(page).unwrap();
    assert!(text.contains("<h1") && !text.contains("<img"), "{text}");

    if env.has(ToolId::Libreoffice) {
        let pdf = assert_done(
            &env.run_json(json!({ "type": "documentConvert", "input": md, "format": "pdf" }))
                .await,
        );
        assert!(is_pdf(&pdf));
        if let Some(text) = pdf_text(&env, &pdf).await {
            assert!(text.contains("Título"), "{text}");
        }
    }
    assert!(!tripwire.was_contacted(), "a remote image was fetched");
}

#[tokio::test]
async fn spreadsheets_round_trip_with_libreoffice() {
    let env = env("office-sheets").await;
    if !env.has(ToolId::Libreoffice) {
        return;
    }
    let csv = env.path("vendas.csv");
    fs::write(&csv, b"produto;pre\xe7o\nCaf\xe9;10\nP\xe3o;2\n").unwrap();
    let xlsx = assert_done(
        &env.run_json(json!({ "type": "spreadsheetConvert", "input": csv, "format": "xlsx" }))
            .await,
    );
    assert!(zip_has(&xlsx, "xl/workbook.xml"));
    let ods = assert_done(
        &env.run_json(json!({ "type": "spreadsheetConvert", "input": xlsx, "format": "ods" }))
            .await,
    );
    assert!(zip_has(&ods, "content.xml"));
    let back = assert_done(
        &env.run_json(json!({ "type": "spreadsheetConvert", "input": ods, "format": "csv" }))
            .await,
    );
    let text = fs::read_to_string(back).unwrap();
    assert!(text.contains("Café") && text.contains("Pão"), "{text}");
    assert!(text.lines().next().unwrap().contains("produto"), "{text}");
    let pdf = assert_done(
        &env.run_json(json!({ "type": "spreadsheetConvert", "input": xlsx, "format": "pdf" }))
            .await,
    );
    assert!(is_pdf(&pdf));
}

#[tokio::test]
async fn presentations_become_pdf_and_one_image_per_slide() {
    let env = env("office-slides").await;
    if !env.has(ToolId::Libreoffice) || !env.has(ToolId::Pandoc) {
        return;
    }
    // Pandoc writes a two-slide PPTX from Markdown.
    let md = env.path("deck.md");
    fs::write(&md, "# Primeiro\n\nUm\n\n# Segundo\n\nDois\n").unwrap();
    let pptx = env.path("deck.pptx");
    let status = std::process::Command::new("pandoc")
        .arg("-o")
        .arg(&pptx)
        .arg(&md)
        .status()
        .unwrap();
    assert!(status.success());

    let pdf = assert_done(
        &env.run_json(json!({ "type": "presentationConvert", "input": pptx, "format": "pdf" }))
            .await,
    );
    assert!(is_pdf(&pdf));
    let odp = assert_done(
        &env.run_json(json!({ "type": "presentationConvert", "input": pptx, "format": "odp" }))
            .await,
    );
    assert!(zip_has(&odp, "content.xml"));

    if env.has(ToolId::Pdfium) {
        let job = env
            .run_json(json!({
                "type": "presentationToImages",
                "input": pptx,
                "format": "jpeg",
                "dpi": 50,
                "quality": 80
            }))
            .await;
        let images = assert_done_many(&job);
        let names: Vec<_> = images
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["deck-convertido-001.jpg", "deck-convertido-002.jpg"]
        );
        let first = decode(&images[0]);
        assert!(first.width() > 100 && first.width() > first.height());
    }
}

#[tokio::test]
async fn pdf_becomes_an_editable_document() {
    let env = env("office-pdf-import").await;
    if !env.has(ToolId::Libreoffice) {
        return;
    }
    let txt = env.path("origem.txt");
    fs::write(&txt, "Texto que volta a ser editável.\n").unwrap();
    let pdf = assert_done(
        &env.run_json(json!({ "type": "documentConvert", "input": txt, "format": "pdf" }))
            .await,
    );
    let docx = assert_done(
        &env.run_json(json!({ "type": "pdfToDocument", "input": pdf, "format": "docx" }))
            .await,
    );
    assert!(zip_has(&docx, "word/document.xml"));
    assert_eq!(docx.extension().unwrap(), "docx");

    if env.has(ToolId::Qpdf) {
        let locked = env.path("locked.pdf");
        let status = std::process::Command::new("qpdf")
            .args(["--encrypt", "abc", "abc", "256", "--"])
            .arg(&pdf)
            .arg(&locked)
            .status()
            .unwrap();
        assert!(status.success());
        let job = env
            .run_json(json!({ "type": "pdfToDocument", "input": locked, "format": "odt" }))
            .await;
        assert_failed(&job, ErrorCode::Password);
    }
}
