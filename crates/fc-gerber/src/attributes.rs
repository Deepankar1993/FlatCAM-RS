//! Parsing of Gerber X2/X3 attribute commands.
//!
//! Attribute commands appear inside an extended (`%...%`) block. This module
//! parses the *content* of such a block, i.e. the text without the surrounding
//! `%` delimiters. The recognized forms are:
//!
//! ```text
//! TF.GenerationSoftware,FlatCAM,1.0   // file attribute
//! TA.AperFunction,Conductor           // aperture attribute
//! TO.N,Net1                           // object attribute
//! TD                                  // delete all attributes
//! TD.AperFunction                     // delete a named attribute
//! ```

/// Scope (kind) of an attribute command, determined by the leading two-letter
/// code.
#[derive(Clone, Debug, PartialEq)]
pub enum AttrScope {
    /// `TF` — file attribute.
    File,
    /// `TA` — aperture attribute.
    Aperture,
    /// `TO` — object attribute.
    Object,
    /// `TD` — delete attribute(s).
    Delete,
}

/// A parsed Gerber attribute command.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    /// The scope, derived from the two-letter code (`TF`/`TA`/`TO`/`TD`).
    pub scope: AttrScope,
    /// The attribute name (the text between the `.` and the first `,`).
    /// Empty for a bare `TD`.
    pub name: String,
    /// The comma-separated values following the name.
    pub values: Vec<String>,
}

/// Parse a single attribute command token (the content of a `%...%` block,
/// without the surrounding `%`).
///
/// Returns `None` if the token does not begin with one of the four recognized
/// two-letter codes.
pub fn parse_attribute(token: &str) -> Option<Attribute> {
    let token = token.trim();

    // The code is the first two ASCII characters. Guard against tokens shorter
    // than two bytes (and against splitting a multi-byte char) by requiring a
    // valid ASCII prefix.
    if token.len() < 2 || !token.is_char_boundary(2) {
        return None;
    }

    let (code, rest) = token.split_at(2);
    let scope = match code {
        "TF" => AttrScope::File,
        "TA" => AttrScope::Aperture,
        "TO" => AttrScope::Object,
        "TD" => AttrScope::Delete,
        _ => return None,
    };

    // After the code, an optional ".name,value,value..." section may follow.
    let (name, values) = if let Some(body) = rest.strip_prefix('.') {
        let mut parts = body.split(',');
        // `split` always yields at least one element, so the first `next()`
        // is guaranteed to be `Some`.
        let name = parts.next().unwrap_or("").to_string();
        let values: Vec<String> = parts.map(|s| s.to_string()).collect();
        (name, values)
    } else {
        // Bare code (e.g. "TD") or unexpected trailing text without a leading
        // dot: no name, no values.
        (String::new(), Vec::new())
    };

    Some(Attribute {
        scope,
        name,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_attribute_with_multiple_values() {
        let attr = parse_attribute("TF.GenerationSoftware,FlatCAM,1.0").unwrap();
        assert_eq!(attr.scope, AttrScope::File);
        assert_eq!(attr.name, "GenerationSoftware");
        assert_eq!(attr.values, vec!["FlatCAM".to_string(), "1.0".to_string()]);
    }

    #[test]
    fn parse_aperture_attribute() {
        let attr = parse_attribute("TA.AperFunction,Conductor").unwrap();
        assert_eq!(attr.scope, AttrScope::Aperture);
        assert_eq!(attr.name, "AperFunction");
        assert_eq!(attr.values, vec!["Conductor".to_string()]);
    }

    #[test]
    fn parse_object_attribute() {
        let attr = parse_attribute("TO.N,Net1").unwrap();
        assert_eq!(attr.scope, AttrScope::Object);
        assert_eq!(attr.name, "N");
        assert_eq!(attr.values, vec!["Net1".to_string()]);
    }

    #[test]
    fn parse_bare_delete() {
        let attr = parse_attribute("TD").unwrap();
        assert_eq!(attr.scope, AttrScope::Delete);
        assert_eq!(attr.name, "");
        assert!(attr.values.is_empty());
    }

    #[test]
    fn parse_named_delete() {
        let attr = parse_attribute("TD.AperFunction").unwrap();
        assert_eq!(attr.scope, AttrScope::Delete);
        assert_eq!(attr.name, "AperFunction");
        assert!(attr.values.is_empty());
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert_eq!(parse_attribute("XYZ"), None);
    }

    #[test]
    fn parse_too_short_returns_none() {
        assert_eq!(parse_attribute("T"), None);
        assert_eq!(parse_attribute(""), None);
    }

    #[test]
    fn parse_trims_whitespace() {
        let attr = parse_attribute("  TO.N,Net1  ").unwrap();
        assert_eq!(attr.scope, AttrScope::Object);
        assert_eq!(attr.name, "N");
        assert_eq!(attr.values, vec!["Net1".to_string()]);
    }
}
