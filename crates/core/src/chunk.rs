//! Split extracted text into overlapping windows for embedding.

/// Max characters per chunk (~250-350 tokens for EmbeddingGemma).
pub const CHUNK_CHARS: usize = 1200;
/// Characters shared between consecutive chunks.
pub const OVERLAP_CHARS: usize = 200;

/// Returns byte ranges into `text`. Boundaries snap to line breaks when one is
/// near, and always land on char boundaries.
pub fn chunk_ranges(text: &str) -> Vec<(usize, usize)> {
    let len = text.len();
    if len == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let mut end = floor_char(text, (start + CHUNK_CHARS).min(len));
        if end < len {
            // Prefer to cut at the last newline in the back half of the window.
            if let Some(nl) = text[start..end].rfind('\n') {
                if nl > CHUNK_CHARS / 2 {
                    end = start + nl + 1;
                }
            }
        }
        out.push((start, end));
        if end >= len {
            break;
        }
        let next = floor_char(text, end.saturating_sub(OVERLAP_CHARS));
        start = if next > start { next } else { end };
    }
    out
}

fn floor_char(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_whole_text_with_overlap_and_valid_boundaries() {
        let text = "héllo wörld\n".repeat(400);
        let ranges = chunk_ranges(&text);
        assert!(ranges.len() > 1);
        assert_eq!(ranges.first().unwrap().0, 0);
        assert_eq!(ranges.last().unwrap().1, text.len());
        for w in ranges.windows(2) {
            assert!(w[1].0 < w[0].1, "consecutive chunks must overlap");
            assert!(w[1].0 > w[0].0, "chunks must advance");
        }
        for (s, e) in ranges {
            assert!(text.is_char_boundary(s) && text.is_char_boundary(e));
            assert!(e - s <= CHUNK_CHARS);
        }
    }
}
