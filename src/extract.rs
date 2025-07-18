use crate::document::{Document, MAX_TEXT_BYTES};
use anyhow::{Context, Result, bail, ensure};
use quick_xml::{Reader, events::{BytesStart, Event}};
use std::{collections::HashMap, fs::File, io::Read, path::{Component, Path, PathBuf}};
use zip::ZipArchive;

const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_ENTRY: u64 = 8 * 1024 * 1024;

pub fn open(path: &Path) -> Result<Document> {
    let file = File::open(path).context("Cannot open the selected file")?;
    ensure!(file.metadata()?.len() <= MAX_FILE, "File exceeds the 32 MiB import limit.");
    let title = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = path.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" => {
            let mut bytes = Vec::new();
            file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
            Document::from_text(title, decode_text(&bytes)?)
        }
        "docx" => {
            let mut zip = ZipArchive::new(file).context("Invalid DOCX archive")?;
            let xml = entry(&mut zip, "word/document.xml")?;
            Document::from_text(title, xml_text(&xml, true)?)
        }
        "epub" => epub(title, ZipArchive::new(file).context("Invalid EPUB archive")?),
        "pdf" => bail!("PDF extraction is not included in v0.1. Export the text as TXT or paste it here. Scanned PDFs require external OCR."),
        _ => bail!("Unsupported format. Choose UTF-8/UTF-16 TXT, Markdown, DOCX or an unencrypted EPUB."),
    }
}

pub fn decode_text(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        ensure!(bytes.len().is_multiple_of(2), "Truncated UTF-16 text.");
        let le = bytes[0] == 0xff;
        let units = bytes[2..].chunks_exact(2).map(|b| if le {u16::from_le_bytes([b[0],b[1]])} else {u16::from_be_bytes([b[0],b[1]])}).collect::<Vec<_>>();
        Ok(String::from_utf16(&units).context("Invalid UTF-16 text")?)
    } else {
        let bytes = bytes.strip_prefix(&[0xef,0xbb,0xbf]).unwrap_or(bytes);
        let text = std::str::from_utf8(bytes).context("Text is not UTF-8. Save it as UTF-8 or UTF-16 with a BOM.")?;
        ensure!(!text.contains('\0'), "Binary or BOM-less UTF-16 input. Save as UTF-8 first.");
        Ok(text.to_owned())
    }
}

fn entry<R: Read + std::io::Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<String> {
    let mut f = zip.by_name(name).with_context(|| format!("Missing document part: {name}"))?;
    ensure!(f.size() <= MAX_ENTRY, "Document part exceeds the 8 MiB extraction limit.");
    let mut bytes = Vec::new();
    (&mut f).take(MAX_ENTRY+1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_ENTRY, "Expanded document part is too large.");
    decode_text(&bytes)
}

fn attr(e: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>> {
    for a in e.attributes() {
        let a = a?;
        if a.key.local_name().as_ref() == key {
            return Ok(Some(a.unescape_value()?.into_owned()));
        }
    }
    Ok(None)
}

fn epub<R: Read + std::io::Seek>(title: String, mut zip: ZipArchive<R>) -> Result<Document> {
    ensure!(zip.len() <= 5000, "EPUB has too many archive entries (limit 5000).");
    ensure!(zip.by_name("META-INF/encryption.xml").is_err(), "Encrypted/DRM EPUBs are not supported.");
    let container = entry(&mut zip, "META-INF/container.xml")?;
    let mut reader = Reader::from_str(&container);
    let mut package = None;
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"rootfile" => {
                package = attr(&e,b"full-path")?; break;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let package = safe_part(Path::new(""), &package.context("EPUB has no package root")?)?;
    let opf = entry(&mut zip, &package)?;
    let base = Path::new(&package).parent().unwrap_or(Path::new(""));
    let mut reader = Reader::from_str(&opf);
    let mut manifest = HashMap::new();
    let mut spine = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) => match e.local_name().as_ref() {
                b"item" => {
                    if let (Some(id),Some(href),Some(media)) = (attr(&e,b"id")?,attr(&e,b"href")?,attr(&e,b"media-type")?)
                        && matches!(media.as_str(), "application/xhtml+xml" | "text/html") { manifest.insert(id,href); }
                }
                b"itemref" => if attr(&e,b"linear")?.as_deref() != Some("no")
                    && let Some(id) = attr(&e,b"idref")? { spine.push(id); },
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(spine.len() <= 1000, "EPUB exceeds the 1000 chapter limit.");
    let mut sections = Vec::new();
    let mut total = 0;
    for id in spine {
        let href = manifest.get(&id).context("EPUB spine refers to an unsupported or missing chapter")?;
        let part = safe_part(base,href)?;
        let xml = entry(&mut zip,&part)?;
        total += xml.len();
        ensure!(total <= MAX_FILE as usize, "EPUB expanded content exceeds 32 MiB.");
        let text = xml_text(&xml,false)?;
        let label = text.lines().find(|l| !l.trim().is_empty()).unwrap_or(&id).chars().take(72).collect();
        sections.push((label,text));
    }
    Document::from_sections(title, sections)
}

fn safe_part(base: &Path, href: &str) -> Result<String> {
    ensure!(!href.contains([':', '\\', '%', '?', '#']), "Unsupported or unsafe EPUB part reference.");
    let mut path = PathBuf::from(base);
    for c in Path::new(href).components() {
        match c {
            Component::Normal(s) => path.push(s),
            Component::CurDir => {},
            Component::ParentDir => ensure!(path.pop(), "EPUB path escapes the archive root."),
            _ => bail!("Absolute EPUB part paths are not supported."),
        }
    }
    Ok(path.to_string_lossy().replace('\\',"/"))
}

fn xml_text(xml: &str, docx: bool) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    let mut output = String::new();
    let mut in_text = false;
    let mut body = false;
    let mut hidden = 0usize;
    loop {
        match reader.read_event().context("Malformed document XML/XHTML")? {
            Event::Start(e) => {
                match e.local_name().as_ref() {
                    b"body" => body=true,
                    b"t" if docx => in_text=true,
                    b"script" | b"style" | b"head" => hidden+=1,
                    _ => {}
                }
            }
            Event::End(e) => {
                match e.local_name().as_ref() {
                    b"t" if docx => in_text=false,
                    b"script" | b"style" | b"head" => hidden=hidden.saturating_sub(1),
                    b"p" | b"h1" | b"h2" | b"h3" | b"li" | b"div" | b"tr" => output.push_str("\n\n"),
                    b"body" => body=false,
                    _ => {}
                }
            }
            Event::Empty(e) if matches!(e.local_name().as_ref(), b"br" | b"tab") => output.push(' '),
            Event::Text(e) if hidden==0 && (if docx {in_text} else {body}) => output.push_str(&e.decode()?),
            Event::CData(e) if hidden==0 && (if docx {in_text} else {body}) => output.push_str(&e.decode()?),
            Event::GeneralRef(e) if hidden==0 && (if docx {in_text} else {body}) => {
                if let Some(c) = e.resolve_char_ref()? { output.push(c); }
                else {
                    let name=e.decode()?;
                    output.push_str(match name.as_ref() { "amp"=>"&", "lt"=>"<", "gt"=>">", "quot"=>"\"", "apos"=>"'", "nbsp"=>" ", _=>" " });
                }
            }
            Event::DocType(_) => {}, // Never resolve external entities or fetch network resources.
            Event::Eof => break,
            _ => {}
        }
        ensure!(output.len() <= MAX_TEXT_BYTES, "Extracted text exceeds 4 MiB.");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoding_and_entities() {
        assert_eq!(decode_text(&[255,254,65,0]).unwrap(), "A");
        assert!(decode_text(&[255,254,65]).is_err());
        assert!(decode_text(&[0xff]).is_err());
        assert_eq!(xml_text("<w:document><w:body><w:p><w:r><w:t>A &amp; B &#x1F600;</w:t></w:r></w:p></w:body></w:document>",true).unwrap().trim(), "A & B 😀");
        let t=xml_text("<html><head><title>Hidden</title></head><body><h1>Chapter</h1><p>Hello <b>world</b>.</p><script>bad</script></body></html>",false).unwrap();
        assert!(t.contains("Hello world.")); assert!(!t.contains("bad")); assert!(!t.contains("Hidden"));
    }
    #[test]
    fn archive_paths() {
        assert_eq!(safe_part(Path::new("OEBPS/text"),"../a.xhtml").unwrap(),"OEBPS/a.xhtml");
        assert!(safe_part(Path::new(""),"../a").is_err());
        assert!(safe_part(Path::new(""),"https://example.org/a").is_err());
        assert!(safe_part(Path::new(""),"/a").is_err());
    }
}
