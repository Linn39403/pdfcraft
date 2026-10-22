//! Synthetic reproduction of #3215. No embedded font or third-party fixture.
use std::sync::Arc;

use pdfcraft_cos::{Document, SaveOptions, write_full};
use pdfcraft_edit::text::{replace_block, replace_line, text_blocks, text_lines};

fn fixture(content: &str) -> Document {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    Document::open(Arc::new(bytes)).unwrap()
}

fn untouched_word_width(doc: &Document) -> f64 {
    let lines = text_lines(doc, 0).unwrap();
    let ink = lines.iter().flat_map(|line| &line.ink).last().unwrap();
    ink[2] - ink[0]
}

fn check_untouched_word(content: &str, old: &str, new: &str, paragraph: bool) {
    let mut doc = fixture(content);
    let before = untouched_word_width(&doc);
    let lines = text_lines(&doc, 0).unwrap();
    let blocks = text_blocks(&doc, 0).unwrap();
    eprintln!(
        "before: lines={:?}, blocks={:?}, untouched width={before:.6}",
        lines.iter().map(|l| (&l.text, &l.ink)).collect::<Vec<_>>(),
        blocks.iter().map(|b| &b.editor_text).collect::<Vec<_>>()
    );
    if paragraph {
        replace_block(&mut doc, 0, 0, &blocks[0].editor_text.replacen(old, new, 1)).unwrap();
    } else {
        replace_line(&mut doc, 0, 0, &lines[0].text.replacen(old, new, 1)).unwrap();
    }
    let saved = write_full(&doc, &SaveOptions::default()).unwrap();
    hayro_syntax::Pdf::new(saved.clone()).expect("saved PDF parses independently");
    let reopened = Document::open(Arc::new(saved)).unwrap();
    let after = untouched_word_width(&reopened);
    let after_lines = text_lines(&reopened, 0).unwrap();
    let edited_text = after_lines.iter().map(|line| line.text.as_str()).collect::<String>();
    assert!(edited_text.contains(new) && !edited_text.contains(old), "edit was saved: {edited_text}");
    eprintln!(
        "after: lines={:?}, untouched width={after:.6}, delta={:.6}",
        after_lines.iter().map(|l| (&l.text, &l.ink)).collect::<Vec<_>>(),
        after - before
    );
    assert!(
        (after - before).abs() < 0.001,
        "editing only {old:?} to {new:?} changed untouched word width from {before:.6} to {after:.6} (paragraph={paragraph})"
    );
}

#[test]
fn line_edit_keeps_later_horizontal_scale_3215() {
    check_untouched_word("BT /F1 24 Tf 72 700 Td (Plain) Tj 60 Tz ( condensed) Tj ET", "Plain", "Plxin", false);
}

#[test]
fn paragraph_edit_keeps_later_horizontal_scale_3215() {
    check_untouched_word("BT /F1 24 Tf 72 700 Td (Plain) Tj 60 Tz ( condensed) Tj ET", "Plain", "Plxin", true);
}

#[test]
fn line_edit_keeps_later_character_spacing_3215() {
    check_untouched_word("BT /F1 24 Tf 72 700 Td (Tight) Tj 3 Tc ( spaced) Tj ET", "Tight", "Tigxt", false);
}

#[test]
fn paragraph_edit_keeps_later_character_spacing_3215() {
    check_untouched_word("BT /F1 24 Tf 72 700 Td (Tight) Tj 3 Tc ( spaced) Tj ET", "Tight", "Tigxt", true);
}

#[test]
fn unchanged_character_spacing_and_scale_keep_one_line_3215() {
    for state in ["", "3 Tc", "60 Tz", "3 Tc 60 Tz"] {
        let mut doc = fixture(&format!("BT /F1 24 Tf {state} 72 700 Td (Plain) Tj {state} ( condensed) Tj ET"));
        assert_eq!(text_lines(&doc, 0).unwrap().len(), 1, "same state: {state}");
        let blocks = text_blocks(&doc, 0).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].editor_text, "Plain condensed");
        let before = untouched_word_width(&doc);
        replace_line(&mut doc, 0, 0, "Plxin condensed").unwrap();
        let reopened = Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap();
        assert_eq!(text_lines(&reopened, 0).unwrap().len(), 1, "saved same state: {state}");
        assert!((untouched_word_width(&reopened) - before).abs() < 0.001, "same state: {state}");
    }
}

#[test]
fn restored_character_spacing_and_scale_keep_each_run_3215() {
    for (changed, restored) in [("60 Tz", "100 Tz"), ("3 Tc", "0 Tc")] {
        for paragraph in [false, true] {
            let mut doc = fixture(&format!("BT /F1 24 Tf 72 700 Td (Plain) Tj {changed} ( middle) Tj {restored} ( last) Tj ET"));
            let widths =
                |doc: &Document| text_lines(doc, 0).unwrap().iter().flat_map(|line| &line.ink).skip(1).map(|ink| ink[2] - ink[0]).collect::<Vec<_>>();
            let before = widths(&doc);
            assert_eq!(before.len(), 2);
            if paragraph {
                let block = text_blocks(&doc, 0).unwrap().remove(0);
                replace_block(&mut doc, 0, 0, &block.editor_text.replace("Plain", "Plxin")).unwrap();
            } else {
                let line = text_lines(&doc, 0).unwrap().remove(0);
                replace_line(&mut doc, 0, 0, &line.text.replace("Plain", "Plxin")).unwrap();
            }
            let reopened = Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap();
            let after = widths(&reopened);
            let edited_text = text_blocks(&reopened, 0).unwrap()[0].editor_text.clone();
            assert!(edited_text.contains("Plxin") && !edited_text.contains("Plain"), "edit was saved: {edited_text}");
            assert_eq!(after.len(), before.len());
            assert!(before.iter().zip(after).all(|(a, b)| (a - b).abs() < 0.001), "state {changed}, paragraph={paragraph}");
        }
    }
}

#[test]
fn encoded_spaces_with_different_state_stay_in_the_paragraph_3215() {
    for (changed, restored) in [("20 Tz", "100 Tz"), ("-5 Tc", "0 Tc")] {
        let mut doc = fixture(&format!("BT /F1 24 Tf 72 700 Td (Hello) Tj {changed} ( ) Tj {restored} (world) Tj ET"));
        assert_eq!(text_blocks(&doc, 0).unwrap()[0].editor_text, "Hello world", "whitespace state: {changed}");
        replace_block(&mut doc, 0, 0, "Hallo world").unwrap();
        let reopened = Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap();
        assert_eq!(text_blocks(&reopened, 0).unwrap()[0].editor_text, "Hallo world", "saved whitespace state: {changed}");
    }
}
