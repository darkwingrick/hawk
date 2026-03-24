use std::path::Path;

use crate::CodeChunk;

const MAX_CHUNK_BYTES: usize = 8 * 1024;
const TEXT_WINDOW_LINES: usize = 60;

#[derive(Default)]
pub struct SemanticChunker;

impl SemanticChunker {
    pub fn chunk_file(&self, path: &Path, source: &str) -> Vec<CodeChunk> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "rs" => chunk_rust(path, source)
                .unwrap_or_else(|_| text_window_chunks(path, source, "rust")),
            "py" => text_window_chunks(path, source, "python"),
            "ts" | "tsx" => text_window_chunks(path, source, "typescript"),
            "js" | "jsx" => text_window_chunks(path, source, "javascript"),
            "go" => text_window_chunks(path, source, "go"),
            "c" | "h" => text_window_chunks(path, source, "c"),
            "cpp" | "cc" | "cxx" | "hpp" => text_window_chunks(path, source, "cpp"),
            other => text_window_chunks(path, source, other),
        }
    }
}

fn chunk_rust(path: &Path, source: &str) -> anyhow::Result<Vec<CodeChunk>> {
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter failed to parse Rust source"))?;

    let source_bytes = source.as_bytes();
    let root = tree.root_node();

    const TOP_LEVEL_KINDS: &[&str] = &[
        "function_item",
        "impl_item",
        "struct_item",
        "enum_item",
        "trait_item",
        "type_alias",
        "macro_definition",
        "mod_item",
        "static_item",
        "const_item",
    ];

    let mut chunks = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !TOP_LEVEL_KINDS.contains(&child.kind()) {
            continue;
        }
        let text = match child.utf8_text(source_bytes) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let symbol_path = extract_name(&child, source_bytes);

        if text.len() <= MAX_CHUNK_BYTES {
            chunks.push(CodeChunk {
                path: path.to_path_buf(),
                language: Some("rust".to_string()),
                symbol_path,
                content: text.to_string(),
            });
        } else {
            // Large impl blocks: sub-chunk by method
            sub_chunk_rust_node(&child, path, source_bytes, &mut chunks);
        }
    }

    if chunks.is_empty() {
        return Ok(text_window_chunks(path, source, "rust"));
    }
    Ok(chunks)
}

fn sub_chunk_rust_node(
    node: &tree_sitter::Node,
    path: &Path,
    source_bytes: &[u8],
    chunks: &mut Vec<CodeChunk>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "declaration_list" {
            let mut inner = child.walk();
            for method in child.children(&mut inner) {
                if method.kind() == "function_item" {
                    let text = match method.utf8_text(source_bytes) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    // Prepend parent context (impl header) for better retrieval
                    let parent_header = extract_impl_header(node, source_bytes);
                    let content = match parent_header {
                        Some(header) => format!("// {}\n{}", header, text),
                        None => text.to_string(),
                    };
                    chunks.push(CodeChunk {
                        path: path.to_path_buf(),
                        language: Some("rust".to_string()),
                        symbol_path: extract_name(&method, source_bytes),
                        content,
                    });
                }
            }
        }
    }
}

fn extract_impl_header(node: &tree_sitter::Node, source_bytes: &[u8]) -> Option<String> {
    // Grab everything before the declaration_list (the `impl ... {` header)
    let mut cursor = node.walk();
    let mut parts = Vec::new();
    for child in node.children(&mut cursor) {
        if child.kind() == "declaration_list" {
            break;
        }
        if let Ok(text) = child.utf8_text(source_bytes) {
            parts.push(text);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn extract_name(node: &tree_sitter::Node, source_bytes: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "identifier" | "type_identifier") {
            return child.utf8_text(source_bytes).ok().map(|s| s.to_string());
        }
    }
    None
}

fn text_window_chunks(path: &Path, source: &str, language: &str) -> Vec<CodeChunk> {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < lines.len() {
        let nominal_end = (start + TEXT_WINDOW_LINES).min(lines.len());
        // Extend to the next blank-line boundary, up to 50% more lines
        let mut end = nominal_end;
        if end < lines.len() {
            let max_extend = nominal_end + TEXT_WINDOW_LINES / 2;
            while end < lines.len().min(max_extend) && !lines[end].trim().is_empty() {
                end += 1;
            }
        }

        let content = lines[start..end].join("\n");
        if !content.trim().is_empty() {
            chunks.push(CodeChunk {
                path: path.to_path_buf(),
                language: Some(language.to_string()),
                symbol_path: None,
                content,
            });
        }

        start = end;
        // Skip leading blank lines for the next window
        while start < lines.len() && lines[start].trim().is_empty() {
            start += 1;
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn chunks_rust_function() {
        let source = r#"
fn hello() -> &'static str {
    "hello"
}

struct Foo {
    x: i32,
}

impl Foo {
    fn new(x: i32) -> Self {
        Self { x }
    }
}
"#;
        let path = PathBuf::from("src/lib.rs");
        let chunker = SemanticChunker;
        let chunks = chunker.chunk_file(&path, source);
        assert!(chunks.len() >= 2, "expected at least fn + struct/impl chunks");
        assert!(
            chunks.iter().any(|c| c.symbol_path.as_deref() == Some("hello")),
            "expected chunk for fn hello"
        );
    }

    #[test]
    fn chunks_text_file_into_windows() {
        let lines: Vec<String> = (0..200).map(|i| format!("line {}", i)).collect();
        let source = lines.join("\n");
        let path = PathBuf::from("README.md");
        let chunker = SemanticChunker;
        let chunks = chunker.chunk_file(&path, &source);
        // 200 lines / 60 per window = at least 3 chunks
        assert!(chunks.len() >= 3, "expected multiple text-window chunks");
        for chunk in &chunks {
            assert_eq!(chunk.language.as_deref(), Some("md"));
        }
    }

    #[test]
    fn empty_file_yields_no_chunks() {
        let path = PathBuf::from("empty.rs");
        let chunker = SemanticChunker;
        let chunks = chunker.chunk_file(&path, "");
        assert!(chunks.is_empty());
    }
}
