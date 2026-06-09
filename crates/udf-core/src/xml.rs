//! Read-only XML access over `content.xml` via `roxmltree`, plus attribute readers that
//! mirror `XMLHelpers.swift` semantics exactly.
//!
//! The semantic that bites if you get it wrong: `attr_str` returns the value even when it is
//! the empty string (only a *missing* attribute yields the fallback), whereas the numeric
//! readers treat empty/invalid as the fallback. This matches the reference's `attrStr` vs.
//! `attrFloat`/`attrInt`.

use roxmltree::Node;

/// A node's attribute value, or `None` if the attribute is absent.
#[inline]
pub fn attr_opt<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute(name)
}

/// True when the attribute is present at all (regardless of value).
#[inline]
pub fn attr_present(node: Node, name: &str) -> bool {
    node.attribute(name).is_some()
}

/// String attribute: present -> value (even if empty); missing -> `fallback`.
#[inline]
pub fn attr_str(node: Node, name: &str, fallback: &str) -> String {
    node.attribute(name).unwrap_or(fallback).to_string()
}

/// Optional string attribute: `None` only when absent (empty string stays `Some("")`).
#[inline]
pub fn attr_str_opt(node: Node, name: &str) -> Option<String> {
    node.attribute(name).map(|s| s.to_string())
}

/// Float attribute: missing/empty/invalid -> `fallback`.
#[inline]
pub fn attr_f64(node: Node, name: &str, fallback: f64) -> f64 {
    match node.attribute(name) {
        Some(s) if !s.is_empty() => s.parse::<f64>().unwrap_or(fallback),
        _ => fallback,
    }
}

/// Optional float: `None` when missing/empty/invalid.
#[inline]
pub fn attr_f64_opt(node: Node, name: &str) -> Option<f64> {
    match node.attribute(name) {
        Some(s) if !s.is_empty() => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Integer attribute (Swift `Int`, i.e. i64): missing/empty/invalid -> `fallback`.
#[inline]
pub fn attr_i64(node: Node, name: &str, fallback: i64) -> i64 {
    match node.attribute(name) {
        Some(s) if !s.is_empty() => s.parse::<i64>().unwrap_or(fallback),
        _ => fallback,
    }
}

/// `i32` attribute (Swift `Int32`): missing/empty/invalid -> `fallback`.
#[inline]
pub fn attr_i32(node: Node, name: &str, fallback: i32) -> i32 {
    match node.attribute(name) {
        Some(s) if !s.is_empty() => s.parse::<i32>().unwrap_or(fallback),
        _ => fallback,
    }
}

/// Optional `i32`: `None` when missing/empty/invalid.
#[inline]
pub fn attr_i32_opt(node: Node, name: &str) -> Option<i32> {
    match node.attribute(name) {
        Some(s) if !s.is_empty() => s.parse::<i32>().ok(),
        _ => None,
    }
}

/// Boolean attribute: true only when the value is exactly "true".
#[inline]
pub fn attr_bool(node: Node, name: &str) -> bool {
    node.attribute(name) == Some("true")
}

/// First immediate child element with the given tag name.
pub fn first_child_element<'a, 'input>(
    parent: Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    parent
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

/// All immediate child elements with the given tag name, in document order.
pub fn child_elements<'a, 'input>(
    parent: Node<'a, 'input>,
    name: &str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    let name = name.to_string();
    parent
        .children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
}

/// Number of immediate child *elements* (ignoring text/whitespace nodes).
pub fn element_child_count(parent: Node) -> usize {
    parent.children().filter(|n| n.is_element()).count()
}

/// The text content of a node (concatenation of its text/CDATA children). roxmltree exposes
/// CDATA as text nodes, so this returns the raw CDATA payload for `<content>`.
pub fn node_text(node: Node) -> String {
    let mut out = String::new();
    for child in node.children() {
        if child.is_text() {
            if let Some(t) = child.text() {
                out.push_str(t);
            }
        }
    }
    out
}
