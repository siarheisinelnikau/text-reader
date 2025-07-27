//! Synthetic, isolated imports only. These tests never create a speech adapter or launch a GUI.
use std::{fs, io::Write, path::PathBuf};
use text_reader::{document::Document, extract};
use zip::{ZipWriter, write::SimpleFileOptions};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".test-output")
            .join(format!("import-{}-{name}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn archive(&self, name: &str, parts: &[(&str, &str)]) -> PathBuf {
        let path = self.0.join(name);
        let mut writer = ZipWriter::new(fs::File::create(&path).unwrap());
        for (part, text) in parts {
            writer
                .start_file(
                    *part,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn docx_text_runs_and_paragraphs() {
    let f = Fixture::new("docx");
    let path=f.archive("sample.docx",&[("word/document.xml",r#"<?xml version="1.0"?><w:document xmlns:w="urn:fixture"><w:body><w:p><w:r><w:t>Hello </w:t></w:r><w:r><w:t>reader &amp; friends.</w:t></w:r></w:p><w:p><w:r><w:t>Second paragraph.</w:t></w:r></w:p></w:body></w:document>"#)]);
    let doc = extract::open(&path).unwrap();
    assert_eq!(doc.chunks[0].text, "Hello reader & friends.");
    assert_eq!(doc.chunks[1].text, "Second paragraph.");
    assert_ne!(doc.chunks[0].paragraph, doc.chunks[1].paragraph);
}

#[test]
fn epub_uses_spine_not_zip_or_manifest_order() {
    let f = Fixture::new("epub");
    let path=f.archive("sample.epub",&[
        ("META-INF/container.xml",r#"<container><rootfiles><rootfile full-path="OPS/book.opf"/></rootfiles></container>"#),
        ("OPS/book.opf",r#"<package><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="b"/><itemref idref="a"/></spine></package>"#),
        ("OPS/a.xhtml","<html><body><p>Second in spine.</p></body></html>"),
        ("OPS/b.xhtml","<html><head><title>Ignore this</title></head><body><h1>First chapter.</h1><p>Content.</p><script>Ignore script</script></body></html>"),
    ]);
    let doc = extract::open(&path).unwrap();
    assert_eq!(doc.chunks[0].text, "First chapter.");
    assert_eq!(doc.chunks.last().unwrap().text, "Second in spine.");
    assert_eq!(doc.chapters.len(), 2);
    assert!(!doc.chunks.iter().any(|c| c.text.contains("Ignore")));
}

#[test]
fn invalid_archives_pdf_and_boms_have_clear_outcomes() {
    let f = Fixture::new("errors");
    let missing = f.archive("missing.docx", &[("unrelated.xml", "<empty/>")]);
    assert!(
        extract::open(&missing)
            .unwrap_err()
            .to_string()
            .contains("Missing document part")
    );
    let pdf = f.0.join("scan.pdf");
    fs::write(&pdf, b"synthetic PDF placeholder").unwrap();
    assert!(
        extract::open(&pdf)
            .unwrap_err()
            .to_string()
            .contains("PDF extraction is not included")
    );
    let txt = f.0.join("sample.txt");
    let mut bytes = vec![0xfe, 0xff];
    for unit in "Unicode café.".encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    fs::write(&txt, bytes).unwrap();
    assert_eq!(extract::open(&txt).unwrap().chunks[0].text, "Unicode café.");
    let drm = f.archive("drm.epub", &[("META-INF/encryption.xml", "<encryption/>")]);
    assert!(
        extract::open(&drm)
            .unwrap_err()
            .to_string()
            .contains("Encrypted/DRM")
    );
}

#[test]
fn repository_reading_fixture_has_real_navigation() {
    let doc = Document::from_text(
        "Fixture".into(),
        include_str!("fixtures/reading.txt").into(),
    )
    .unwrap();
    assert_eq!(doc.chapters.len(), 2);
    let last = doc.chapters[1].start;
    assert!(last > 0);
    assert!(doc.paragraph_target(last, false) < last);
    assert!(doc.chunks.iter().any(|c| c.text.contains("日本語")));
}
