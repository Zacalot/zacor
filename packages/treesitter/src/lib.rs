use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Declaration {
    pub file: String,
    pub kind: String,
    pub name: String,
    pub signature: String,
}

pub fn parse(source: &str, ext: &str, rel_path: &str) -> Vec<Declaration> {
    let Some(language) = parse_language_for_extension(ext) else {
        return Vec::new();
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }

    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let mut declarations = Vec::new();
    collect_declarations(tree.root_node(), source, rel_path, ext, &mut declarations);
    declarations
}

fn parse_language_for_extension(ext: &str) -> Option<tree_sitter::Language> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "ts" | "tsx" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        _ => None,
    }
}

fn collect_declarations(
    node: tree_sitter::Node,
    source: &str,
    file: &str,
    ext: &str,
    out: &mut Vec<Declaration>,
) {
    match ext {
        "rs" => collect_rust(node, source, file, out),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => collect_js_ts(node, source, file, out),
        "py" => collect_python(node, source, file, out),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_declarations(child, source, file, ext, out);
    }
}

fn decl(file: &str, kind: &str, name: &str, signature: &str) -> Declaration {
    Declaration {
        file: file.to_string(),
        kind: kind.to_string(),
        name: name.to_string(),
        signature: signature.to_string(),
    }
}

fn child_name(node: tree_sitter::Node, source: &str) -> Option<String> {
    node.child_by_field_name("name").map(|node| node_text(node, source))
}

fn node_text(node: tree_sitter::Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").to_string()
}

fn collect_rust(node: tree_sitter::Node, source: &str, file: &str, out: &mut Vec<Declaration>) {
    match node.kind() {
        "function_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "function", &name, &first_line(&node_text(node, source))));
            }
        }
        "struct_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "struct", &name, &format!("struct {name}")));
            }
        }
        "enum_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "enum", &name, &format!("enum {name}")));
            }
        }
        "trait_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "trait", &name, &format!("trait {name}")));
            }
        }
        "impl_item" => {
            let text = first_line(&node_text(node, source));
            out.push(decl(file, "impl", text.trim_start_matches("impl "), &text));
        }
        "type_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "type", &name, &first_line(&node_text(node, source))));
            }
        }
        _ => {}
    }
}

fn collect_js_ts(node: tree_sitter::Node, source: &str, file: &str, out: &mut Vec<Declaration>) {
    match node.kind() {
        "function_declaration" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "function", &name, &first_line(&node_text(node, source))));
            }
        }
        "class_declaration" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "class", &name, &format!("class {name}")));
            }
        }
        "interface_declaration" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "interface", &name, &format!("interface {name}")));
            }
        }
        "type_alias_declaration" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "type", &name, &first_line(&node_text(node, source))));
            }
        }
        _ => {}
    }
}

fn collect_python(node: tree_sitter::Node, source: &str, file: &str, out: &mut Vec<Declaration>) {
    match node.kind() {
        "function_definition" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "function", &name, &first_line(&node_text(node, source))));
            }
        }
        "class_definition" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "class", &name, &first_line(&node_text(node, source))));
            }
        }
        _ => {}
    }
}
