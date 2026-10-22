//! Synthetic reproductions of #3215 and #3825; only the attributed OFL Inter atomicity test embeds a font.
use std::sync::Arc;

use pdfcraft_cos::{Document, SaveOptions, write_full};
use pdfcraft_edit::text::{replace_block, replace_line, text_blocks, text_lines};

/// Build a single-page PDF from synthetic operators, with standard-font metrics and no embedded font.
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

/// Measure the final word's horizontal ink extent, independent of source-run line grouping.
fn untouched_word_width(doc: &Document) -> f64 {
    let lines = text_lines(doc, 0).unwrap();
    let ink = lines.iter().flat_map(|line| &line.ink).last().unwrap();
    ink[2] - ink[0]
}

/// Edit only the earlier word through the selected API, then verify the later word's width after full save/reopen.
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

const CONTENT: &str = "BT /F1 24 Tf 72 700 Td 0.001 Tc -0.002 Tw (Section 1 ) Tj 0 Tc 0 Tw (- ) Tj (Main Heading) Tj ET";
const BEFORE: &str = "Section 1 - Main Heading";
const AFTER: &str = "Section 2 - Main Heading";

/// Capture public source-line text and diagnostic row geometry.
fn snapshot(doc: &Document) -> String {
    let lines = text_lines(doc, 0).unwrap();
    let blocks = text_blocks(doc, 0).unwrap();
    eprintln!(
        "lines={:?}; blocks={:?}",
        lines.iter().map(|line| (&line.text, &line.ink)).collect::<Vec<_>>(),
        blocks.iter().map(|block| &block.editor_text).collect::<Vec<_>>()
    );
    lines.iter().map(|line| line.text.as_str()).collect()
}

/// Full-save and independently parse before reopening for post-edit assertions.
fn reopen(doc: &Document) -> Document {
    let bytes = write_full(doc, &SaveOptions::default()).unwrap();
    hayro_syntax::Pdf::new(bytes.clone()).expect("saved PDF parses independently");
    Document::open(Arc::new(bytes)).unwrap()
}

#[test]
fn jitter_ordinary_heading_stays_one_line() {
    let doc = fixture(CONTENT);
    assert_eq!(snapshot(&doc), BEFORE);
    assert_eq!(text_lines(&doc, 0).unwrap().len(), 1, "tiny Tc jitter must not fragment this ordinary heading");
}

#[test]
fn jitter_full_line_replacement_does_not_duplicate_suffix() {
    let mut doc = fixture(CONTENT);
    assert_eq!(snapshot(&doc), BEFORE);
    replace_line(&mut doc, 0, 0, AFTER).unwrap();
    let reopened = reopen(&doc);
    assert_eq!(snapshot(&reopened), AFTER, "full-line edit duplicated the retained suffix");
}

#[test]
fn jitter_current_source_run_replacement_control() {
    let mut doc = fixture(CONTENT);
    let line = text_lines(&doc, 0).unwrap().remove(0);
    replace_line(&mut doc, 0, 0, &line.text.replacen("Section 1", "Section 2", 1)).unwrap();
    assert_eq!(snapshot(&reopen(&doc)), AFTER);
}

#[test]
fn jitter_paragraph_replacement_control() {
    let mut doc = fixture(CONTENT);
    let blocks = text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].editor_text, BEFORE);
    replace_block(&mut doc, 0, 0, AFTER).unwrap();
    let reopened = reopen(&doc);
    assert_eq!(text_lines(&reopened, 0).unwrap().len(), 1, "paragraph save must not fragment the public line");
    snapshot(&reopened);
    assert_eq!(text_blocks(&reopened, 0).unwrap()[0].editor_text, AFTER);
}

/// Read every content stream in order for an independent effective-state oracle.
fn content_bytes(doc: &Document) -> Vec<u8> {
    use pdfcraft_cos::Object;
    let page = pdfcraft_model::pages(doc).remove(0);
    let contents = doc.resolve(page.dict.get(b"Contents").unwrap());
    let objects = match &*contents {
        Object::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    objects
        .iter()
        .flat_map(|object| match &*doc.resolve(object) {
            Object::Stream(stream) => stream.decoded().unwrap(),
            _ => Vec::new(),
        })
        .collect()
}

/// Effective Tc/Tw/Tz/Tf size for each ASCII character, without the edit crate's run grouping.
fn character_states(doc: &Document) -> Vec<(char, f64, f64, f64, f64)> {
    use pdfcraft_cos::Object;
    let mut state = (0.0, 0.0, 100.0, 0.0);
    let mut stack = Vec::new();
    let mut result = Vec::new();
    for op in pdfcraft_content::parse(&content_bytes(doc)).ops {
        match op.op.as_slice() {
            b"q" => stack.push(state),
            b"Q" => state = stack.pop().unwrap(),
            b"Tc" => state.0 = op.num(0).unwrap(),
            b"Tw" => state.1 = op.num(0).unwrap(),
            b"Tz" => state.2 = op.num(0).unwrap(),
            b"Tf" => state.3 = op.num(1).unwrap(),
            b"Tj" | b"'" => {
                let text = op.operands.first().and_then(Object::as_string).unwrap().to_text();
                result.extend(text.chars().map(|c| (c, state.0, state.1, state.2, state.3)));
            }
            b"TJ" => {
                for string in op.operands.iter().filter_map(Object::as_array).flatten().filter_map(Object::as_string) {
                    result.extend(string.to_text().chars().map(|c| (c, state.0, state.1, state.2, state.3)));
                }
            }
            b"\"" => {
                state.1 = op.num(0).unwrap();
                state.0 = op.num(1).unwrap();
                result.extend(
                    op.operands.get(2).and_then(Object::as_string).unwrap().to_text().chars().map(|c| (c, state.0, state.1, state.2, state.3)),
                );
            }
            _ => {}
        }
    }
    result
}

/// Replace the initial A while preserving every later source character's exact spacing state.
fn check_exact_states(content: &str, paragraph: bool) {
    let mut doc = fixture(content);
    let before = character_states(&doc);
    let before_ink: Vec<_> = text_lines(&doc, 0).unwrap().into_iter().flat_map(|line| line.ink).collect();
    let source = text_blocks(&doc, 0).unwrap().remove(0).editor_text;
    let replacement = source.replacen('A', "B", 1);
    let width = untouched_word_width(&doc);
    if paragraph {
        replace_block(&mut doc, 0, 0, &replacement).unwrap();
    } else {
        replace_line(&mut doc, 0, 0, &replacement).unwrap();
    }
    let reopened = reopen(&doc);
    let after = character_states(&reopened);
    assert_eq!(after.first().unwrap().0, 'B');
    assert_eq!(after.len(), before.len(), "source character count, paragraph={paragraph}");
    assert!(
        after.iter().skip(1).zip(before.iter().skip(1)).all(|(a, b)| a == b),
        "exact authored state survives, paragraph={paragraph}; first mismatch {:?}",
        after.iter().zip(&before).enumerate().skip(1).find(|(_, (a, b))| a != b)
    );
    let after_ink: Vec<_> = text_lines(&reopened, 0).unwrap().into_iter().flat_map(|line| line.ink).collect();
    assert_eq!(after_ink.len(), before_ink.len());
    for (before, after) in before_ink.iter().zip(&after_ink) {
        for (a, b) in before.iter().zip(after) {
            assert!((a - b).abs() <= 1e-9 * a.abs().max(1.0), "every run retains its position: {before:?} -> {after:?}");
        }
    }
    assert!((untouched_word_width(&reopened) - width).abs() <= 1e-6 * width.abs().max(1e-6), "untouched glyph geometry, paragraph={paragraph}");
}

#[test]
fn jitter_exact_states_survive_long_repeated_and_magnified_runs() {
    let mut repeated = String::from("BT /F1 24 Tf 72 700 Td (A) Tj ");
    for _ in 0..64 {
        repeated.push_str("0.0000001 Tc (x) Tj -0.0000001 Tc (y) Tj 0 Tc (z) Tj ");
    }
    repeated.push_str("ET");
    let cases = [
        "BT /F1 24 Tf 72 700 Td (A ) Tj 0.0000001 Tc (tail) Tj ET".to_string(),
        format!("BT /F1 1 Tf 72 700 Td (A ) Tj 0.0000001 Tc ({}) Tj ET", "x".repeat(2000)),
        repeated,
        "q 100000 0 0 1 0 0 cm BT /F1 24 Tf 0.00001 0 0 1 0.00072 700 Tm (A ) Tj 0.0000001 Tc (tail) Tj ET Q".into(),
        "q 100000 0 0 100000 0 0 cm BT /F1 0.00024 Tf 0.00072 0.007 Td (A ) Tj 0.0000001 Tc (tail) Tj ET Q".into(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj 99.9999999 Tz (tail) Tj ET".into(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj -0.001 Tc (tail) Tj ET".into(),
        "BT /F1 0.00024 Tf 72 700 Td (A ) Tj 0.0000001 Tc (tail) Tj ET".into(),
        "BT /F1 24 Tf 0.0000000001 0 0 1 72 700 Tm (A ) Tj 0.0000001 Tc (tail) Tj ET".into(),
    ];
    for (index, content) in cases.iter().enumerate() {
        for paragraph in [false, true] {
            eprintln!("exact state case {index}, paragraph={paragraph}");
            check_exact_states(content, paragraph);
        }
    }
}

#[test]
fn mixed_line_edit_keeps_relative_same_bt_neighbor_and_does_not_wrap() {
    for prefix in ["72 700 Td", "24 TL 72 724 Td T*"] {
        let content = format!("BT /F1 24 Tf {prefix} (A ) Tj 3 Tc (tail) Tj /F1 30 Tf ( NEXT) Tj ET");
        let mut doc = fixture(&content);
        let before = text_lines(&doc, 0).unwrap();
        assert_eq!(before.len(), 2);
        let next = before[1].rect;
        let replacement = format!("B {}tail", "wide ".repeat(80));
        replace_line(&mut doc, 0, 0, &replacement).unwrap();
        let reopened = reopen(&doc);
        let after = text_lines(&reopened, 0).unwrap();
        assert_eq!(after.len(), 2, "line editing never wraps");
        assert_eq!(after[0].text, replacement);
        assert_eq!(after[1].text, " NEXT");
        let advance = after[0].rect[2] - before[0].rect[2];
        assert!((after[1].rect[0] - next[0] - advance).abs() < 0.001, "following same-BT text follows the edited advance");
        assert_eq!(after[1].baseline()[1], before[1].baseline()[1]);
    }
}

#[test]
fn mixed_paragraph_edit_preserves_public_selection_and_other_paragraphs() {
    let mut doc = fixture("BT /F1 24 Tf 72 700 Td (Before) Tj 0 -70 Td (A ) Tj 0.001 Tc (tail) Tj 0 Tc 0 -70 Td (After) Tj ET");
    let before = text_lines(&doc, 0).unwrap();
    assert_eq!(text_blocks(&doc, 0).unwrap().len(), 3);
    replace_block(&mut doc, 0, 1, "B tail").unwrap();
    let reopened = reopen(&doc);
    let after = text_lines(&reopened, 0).unwrap();
    assert_eq!(after.len(), 3);
    assert_eq!(after[0].text, before[0].text);
    assert_eq!(after[0].rect, before[0].rect);
    assert_eq!(after[2].text, before[2].text);
    assert_eq!(after[2].rect, before[2].rect);
    assert_eq!(text_blocks(&reopened, 0).unwrap()[1].editor_text, "B tail");
    replace_line(&mut doc, 0, 1, "C tail").unwrap();
    assert_eq!(text_blocks(&reopen(&doc), 0).unwrap()[1].editor_text, "C tail");
}

#[test]
fn unsupported_mixed_positioning_refuses_without_mutating() {
    let infinity = "9".repeat(400);
    let cases = [
        "BT /F1 24 Tf 72 700 Td (A ) Tj 3 Tc [(t) -100 (ai) 100 (l)] TJ ET".to_string(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj [100] TJ 3 Tc (tail) Tj ET".into(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj 1 0 0 1 100 700 Tm 3 Tc (tail) Tj ET".into(),
        "BT /F1 24 Tf 0 1 -1 0 300 300 Tm (A ) Tj 3 Tc (tail) Tj ET".into(),
        "BT /F1 24 Tf 1 0 0.2 1 72 700 Tm (A ) Tj 3 Tc (tail) Tj ET".into(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj -30 Tc (tail) Tj ET".into(),
        format!("BT /F1 24 Tf 72 700 Td (A ) Tj {infinity} Tc (tail) Tj ET"),
        "BT /F1 24 Tf 24 TL 72 700 Td (A ) Tj 3 Tc (tail) Tj 0 Tc T* [(Other) -100 (row)] TJ ET".into(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj 3 Tc 3 Tr (tail) Tj ET".into(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj 3 Tc 1 Ts (tail) Tj ET".into(),
        "BT /F1 24 Tf 72 700 Td (A ) Tj 3 Tr (hidden) Tj 0 Tr ( visible) Tj 0.001 Tc (tail) Tj ET".into(),
    ];
    for (index, content) in cases.iter().enumerate() {
        for paragraph in [false, true] {
            let mut doc = fixture(content);
            let before = write_full(&doc, &SaveOptions::default()).unwrap();
            let result = if paragraph { replace_block(&mut doc, 0, 0, "B tail") } else { replace_line(&mut doc, 0, 0, "B tail") };
            if index == 7 && !paragraph {
                assert!(result.is_ok(), "unselected positioned row does not block line editing");
            } else {
                assert!(result.is_err(), "case {index}, paragraph={paragraph}");
                assert_eq!(write_full(&doc, &SaveOptions::default()).unwrap(), before, "refusal changes nothing");
            }
        }
    }
}

#[test]
fn mixed_paragraph_relative_scoped_neighbor_refuses_without_mutating() {
    let mut doc = fixture("BT /F1 24 Tf 72 700 Td /P BMC (A ) Tj 0.001 Tc (tail) Tj EMC /P BMC ( NEXT) Tj EMC ET");
    assert_eq!(text_blocks(&doc, 0).unwrap().len(), 2);
    let before = write_full(&doc, &SaveOptions::default()).unwrap();
    assert!(replace_block(&mut doc, 0, 0, "B tail").is_err());
    assert_eq!(write_full(&doc, &SaveOptions::default()).unwrap(), before);
    replace_line(&mut doc, 0, 0, "B tail").unwrap();
    assert_eq!(text_lines(&reopen(&doc), 0).unwrap().len(), 2);
}

#[test]
fn mixed_paragraph_uses_semantic_duplicate_selection_only() {
    for copy in [
        "BT /F1 24 Tf 0 Tc 72 700 Td (A tail) Tj ET",
        "BT /F1 24 Tf 0 Tc 94.68 700 Td (tail) Tj ET",
        "BT /F1 24 Tf 0 Tc 4 Tr 94.68 700 Td (tail) Tj ET",
    ] {
        let mut doc = fixture(&format!("BT /F1 24 Tf 72 700 Td (A ) Tj 0.001 Tc (tail) Tj ET {copy}"));
        assert_eq!(text_blocks(&doc, 0).unwrap().len(), 2);
        let before = text_lines(&doc, 0).unwrap();
        replace_block(&mut doc, 0, 0, "B tail").unwrap();
        let reopened = reopen(&doc);
        let after = text_lines(&reopened, 0).unwrap();
        if copy.contains("(A tail)") {
            assert_eq!(after.len(), 1, "full visible duplicate is replaced together with its semantic target");
            assert_eq!(after[0].text, "B tail");
        } else {
            assert_eq!(after.len(), 2, "private fragments do not newly select a partial-text neighbour");
            assert_eq!(after[1].text, before[1].text);
            assert_eq!(after[1].rect, before[1].rect);
        }
    }
}

#[test]
fn mixed_line_quote_entry_restores_original_following_state() {
    for first in ["(A ) '", "-0.002 0.001 (A ) \""] {
        let mut doc = fixture(&format!("BT /F1 24 Tf 24 TL 72 724 Td {first} 3 Tc (tail) Tj /F1 30 Tf ( NEXT) Tj ET"));
        let before = character_states(&doc);
        replace_line(&mut doc, 0, 0, "B tail").unwrap();
        let reopened = reopen(&doc);
        assert_eq!(text_lines(&reopened, 0).unwrap()[0].text, "B tail");
        assert_eq!(&character_states(&reopened)[1..], &before[1..]);
    }
}

#[test]
fn mixed_paragraph_fallback_failure_is_atomic() {
    use pdfcraft_cos::{Dict, ObjRef, Object, Stream};
    let mut doc = fixture("BT /F1 24 Tf 72 700 Td (A ) Tj 0.001 Tc (tail) Tj ET");
    // The repo's attributed OFL Inter program deliberately has more glyphs than this
    // synthetic WinAnsi source encoding, forcing full-program completion for the first č.
    let program = doc.add(Object::Stream(Stream::from_raw(Dict::new(), include_bytes!("../../../assets/fonts/Inter-Regular.ttf").to_vec())));
    let mut descriptor = Dict::new();
    descriptor.set(b"FontFile2".to_vec(), Object::Ref(program));
    doc.update_dict(ObjRef::new(5, 0), |font| {
        font.set(b"FontDescriptor".to_vec(), Object::Dict(descriptor));
    })
    .unwrap();
    let before = write_full(&doc, &SaveOptions::default()).unwrap();
    let mut control = doc.clone();
    replace_block(&mut control, 0, 0, "č tail").unwrap();
    assert!(write_full(&control, &SaveOptions::default()).unwrap().len() > before.len(), "control completes the source font");
    assert!(replace_block(&mut doc, 0, 0, "č\u{10ffff} tail").is_err());
    assert_eq!(write_full(&doc, &SaveOptions::default()).unwrap(), before, "a later unsupported glyph rolls back earlier font allocation");
    for replacement in ["א tail", "क tail", "B\u{0301} tail"] {
        assert!(replace_block(&mut doc, 0, 0, replacement).is_err());
        assert_eq!(write_full(&doc, &SaveOptions::default()).unwrap(), before);
    }
}
