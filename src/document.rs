use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chunk {
    pub text: String,
    pub paragraph: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub start: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub chunks: Vec<Chunk>,
    pub chapters: Vec<Chapter>,
}

impl Document {
    pub fn from_sections(title: String, sections: Vec<(String, String)>) -> Result<Self> {
        let size: usize = sections.iter().map(|s| s.1.len()).sum();
        if size > MAX_TEXT_BYTES {
            bail!("Extracted text exceeds the 4 MiB limit. Split the document first.");
        }
        let mut chunks = Vec::new();
        let mut chapters = Vec::new();
        let mut paragraph = 0;
        let mut hash = Sha256::new();
        for (heading, text) in sections {
            let start = chunks.len();
            // Line-wrapped prose remains a paragraph; a blank line starts the next one.
            let text = text.replace("\r\n", "\n").replace('\r', "\n");
            for block in text.split("\n\n") {
                let normalized = block.split_whitespace().collect::<Vec<_>>().join(" ");
                if normalized.is_empty() {
                    continue;
                }
                hash.update(normalized.as_bytes());
                hash.update([0]);
                for text in split_chunks(&normalized) {
                    chunks.push(Chunk { text, paragraph });
                }
                paragraph += 1;
            }
            if chunks.len() > start {
                chapters.push(Chapter {
                    title: heading,
                    start,
                });
            }
        }
        if chunks.is_empty() {
            bail!(
                "No readable text was found. Scanned documents require OCR, which is not included."
            );
        }
        Ok(Self {
            id: format!("{:x}", hash.finalize()),
            title,
            chunks,
            chapters,
        })
    }

    pub fn from_text(title: String, text: String) -> Result<Self> {
        if text.len() > MAX_TEXT_BYTES {
            bail!("Text exceeds the 4 MiB limit. Split the document first.");
        }
        // Markdown-style headings provide chapter navigation for plain text.
        let mut sections = Vec::new();
        let mut heading = "Beginning".to_owned();
        let mut body = String::new();
        for line in text.lines() {
            if line.starts_with("# ") || line.starts_with("## ") {
                if !body.trim().is_empty() {
                    sections.push((heading, std::mem::take(&mut body)));
                }
                heading = line.trim_start_matches('#').trim().to_owned();
            }
            body.push_str(line);
            body.push('\n');
        }
        sections.push((heading, body));
        Self::from_sections(title, sections)
    }

    pub fn paragraph_target(&self, cursor: usize, forward: bool) -> usize {
        let cursor = cursor.min(self.chunks.len() - 1);
        let p = self.chunks[cursor].paragraph;
        if forward {
            (cursor + 1..self.chunks.len())
                .find(|&i| self.chunks[i].paragraph != p)
                .unwrap_or(cursor)
        } else {
            let start = (0..=cursor)
                .rev()
                .take_while(|&i| self.chunks[i].paragraph == p)
                .last()
                .unwrap_or(cursor);
            if start < cursor {
                start
            } else {
                let prev = start.saturating_sub(1);
                let p = self.chunks[prev].paragraph;
                (0..=prev)
                    .rev()
                    .take_while(|&i| self.chunks[i].paragraph == p)
                    .last()
                    .unwrap_or(0)
            }
        }
    }
}

fn split_chunks(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        // Bound even pathological unbroken input without slicing inside UTF-8.
        for part in word.chars().collect::<Vec<_>>().chunks(280) {
            let part: String = part.iter().collect();
            if current.chars().count() + part.chars().count() > 300 && !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&part);
            if current.ends_with(['.', '!', '?', '。', '！', '？']) {
                result.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_chapters_and_paragraphs() {
        let d = Document::from_text(
            "Test".into(),
            "# First\n\nHello. Привет!\n\nNext paragraph.\n\n# Second\n\nLast.".into(),
        )
        .unwrap();
        assert_eq!(d.chapters.len(), 2);
        assert!(d.chunks.iter().any(|c| c.text == "Привет!"));
        let next = d.paragraph_target(0, true);
        assert_ne!(d.chunks[0].paragraph, d.chunks[next].paragraph);
        assert_eq!(d.paragraph_target(next, false), 0);
    }
    #[test]
    fn empty_and_limits() {
        assert!(Document::from_text("".into(), "  \n".into()).is_err());
        assert!(Document::from_text("".into(), "x".repeat(MAX_TEXT_BYTES + 1)).is_err());
        let d = Document::from_text("".into(), "文".repeat(2000)).unwrap();
        assert!(d.chunks.iter().all(|c| c.text.chars().count() <= 300));
    }
    #[test]
    fn identity_ignores_file_name() {
        let a = Document::from_text("A".into(), "Same text.".into()).unwrap();
        let b = Document::from_text("B".into(), "Same text.".into()).unwrap();
        assert_eq!(a.id, b.id);
    }
}
