use std::fs;
use std::path::{Path, PathBuf};

use ra_ap_rustc_lexer::{DocStyle, FrontmatterAllowed, TokenKind, tokenize};

const FIX: &str = "Explain the code through names and structure, or record the reason in an ADR (docs/adr/0001-code-carries-no-comments.md).";

fn comments_in(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    let mut offset = 0;
    for token in tokenize(source, FrontmatterAllowed::No) {
        let kind = match token.kind {
            TokenKind::LineComment { doc_style: None } => Some("//"),
            TokenKind::LineComment {
                doc_style: Some(DocStyle::Outer),
            } => Some("///"),
            TokenKind::LineComment {
                doc_style: Some(DocStyle::Inner),
            } => Some("//!"),
            TokenKind::BlockComment {
                doc_style: None, ..
            } => Some("/* */"),
            TokenKind::BlockComment {
                doc_style: Some(DocStyle::Outer),
                ..
            } => Some("/** */"),
            TokenKind::BlockComment {
                doc_style: Some(DocStyle::Inner),
                ..
            } => Some("/*! */"),
            _ => None,
        };
        if let Some(kind) = kind {
            let line = 1 + source[..offset].matches('\n').count();
            found.push((line, kind));
        }
        offset += token.len as usize;
    }
    found
}

fn rust_files_under(folder: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).expect("a folder to scan") {
        let path = entry.expect("a folder entry").path();
        if path.is_dir() {
            rust_files_under(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn no_rust_file_under_src_or_tests_has_a_comment() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for folder in ["src", "tests"] {
        rust_files_under(&root.join(folder), &mut files);
    }
    files.sort();

    let mut findings = Vec::new();
    for file in &files {
        let source = fs::read_to_string(file).expect("a readable Rust file");
        let relative = file.strip_prefix(root).expect("a file under the root");
        let name = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        for (line, kind) in comments_in(&source) {
            findings.push(format!(
                "{name}:{line}: a '{kind}' comment is not allowed. {FIX}"
            ));
        }
    }

    assert!(findings.is_empty(), "\n{}\n", findings.join("\n"));
}

#[test]
fn a_line_comment_is_reported_with_its_line() {
    assert_eq!(comments_in("fn a() {}\n// note\nfn b() {}\n"), [(2, "//")]);
}

#[test]
fn a_block_comment_is_reported_with_its_line() {
    assert_eq!(
        comments_in("fn a() {}\nfn b() {}\n/* note\nacross lines */\n"),
        [(3, "/* */")]
    );
}

#[test]
fn an_outer_doc_comment_is_reported() {
    assert_eq!(comments_in("/// note\nfn a() {}\n"), [(1, "///")]);
}

#[test]
fn an_inner_doc_comment_is_reported() {
    assert_eq!(comments_in("\n//! note\nfn a() {}\n"), [(2, "//!")]);
}

#[test]
fn an_outer_block_doc_comment_is_reported() {
    assert_eq!(comments_in("/** note */\nfn a() {}\n"), [(1, "/** */")]);
}

#[test]
fn an_inner_block_doc_comment_is_reported() {
    assert_eq!(comments_in("/*! note */\nfn a() {}\n"), [(1, "/*! */")]);
}

#[test]
fn a_comment_after_code_on_the_same_line_is_reported() {
    assert_eq!(comments_in("let a = 1; // note\n"), [(1, "//")]);
}

#[test]
fn every_comment_in_a_file_is_reported() {
    assert_eq!(
        comments_in("// one\nfn a() {}\n/// two\nfn b() {}\n"),
        [(1, "//"), (3, "///")]
    );
}

#[test]
fn slashes_inside_a_string_are_not_a_comment() {
    assert_eq!(comments_in("let url = \"https://example.com\";\n"), []);
}

#[test]
fn slashes_inside_a_raw_string_are_not_a_comment() {
    assert_eq!(
        comments_in("let a = r#\"// not a comment\"#;\nlet b = r#\"/* nor this */\"#;\n"),
        []
    );
}

#[test]
fn slashes_inside_a_byte_string_are_not_a_comment() {
    assert_eq!(comments_in("let a = b\"// not a comment\";\n"), []);
}

#[test]
fn a_slash_in_a_char_literal_is_not_a_comment() {
    assert_eq!(comments_in("let a = ('/', '/');\n"), []);
}

#[test]
fn a_lifetime_beside_a_slash_is_not_a_comment() {
    assert_eq!(
        comments_in("fn a<'a>(x: &'a u8) -> u8 { *x / 2 }\nlet b = 4 /'a'.len_utf8();\n"),
        []
    );
}

#[test]
fn a_lint_reason_in_the_attribute_is_not_a_comment() {
    assert_eq!(
        comments_in("#[allow(dead_code, reason = \"kept for the next release\")]\nfn a() {}\n"),
        []
    );
}
