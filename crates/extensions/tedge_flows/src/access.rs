//! Classification of flow steps by their use of the mapper context
//!
//! When flows are processed concurrently, the calls of flows sharing the mapper context
//! are ordered so that every `context.mapper` access happens in message arrival order.
//! The class of a JS step is inferred from its source text:
//! a step is a single ES module and `context` is a function argument,
//! so the mapper scope can only be reached by spelling `mapper`.

/// How a flow step uses the mapper context, ordered from weakest to strongest
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextAccess {
    /// The step never uses the mapper context
    Independent,

    /// The step only reads the mapper context
    Read,

    /// The step might update the mapper context
    #[default]
    Write,
}

impl std::fmt::Display for ContextAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextAccess::Independent => write!(f, "independent"),
            ContextAccess::Read => write!(f, "reader"),
            ContextAccess::Write => write!(f, "writer"),
        }
    }
}

/// What the source of a JS step tells about the state it shares with other flows
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceAnalysis {
    /// Use of the mapper context
    pub access: ContextAccess,

    /// True if the script uses `globalThis` for anything else than a standard built-in
    pub shares_globals: bool,
}

impl SourceAnalysis {
    pub fn of(source: &str) -> Self {
        let mut access = ContextAccess::Independent;
        let mut shares_globals = false;
        for (start, word) in identifiers(source) {
            let rest = &source[start + word.len()..];
            match word {
                "mapper" => access = access.max(mapper_access(rest)),
                "globalThis" => shares_globals |= !is_builtin_access(rest),
                _ => {}
            }
        }
        if shares_globals {
            access = ContextAccess::Write;
        }
        SourceAnalysis {
            access,
            shares_globals,
        }
    }
}

/// Standard built-ins a script can reach through `globalThis` without sharing state
const BUILTINS: &[&str] = &[
    "Array",
    "ArrayBuffer",
    "BigInt",
    "Boolean",
    "DataView",
    "Date",
    "Error",
    "Float32Array",
    "Float64Array",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "JSON",
    "Map",
    "Math",
    "Number",
    "Object",
    "Promise",
    "RangeError",
    "RegExp",
    "Set",
    "String",
    "Symbol",
    "TextDecoder",
    "TextEncoder",
    "TypeError",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "WeakMap",
    "WeakSet",
    "console",
    "crypto",
    "decodeURIComponent",
    "encodeURIComponent",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
];

/// Classify the use of `mapper`, given the text following it
///
/// Only `mapper.get(` and `mapper.keys(` are reads; anything else is assumed to be a write.
fn mapper_access(rest: &str) -> ContextAccess {
    match member_call(rest) {
        Some(("get", true)) | Some(("keys", true)) => ContextAccess::Read,
        _ => ContextAccess::Write,
    }
}

/// Check the text following `globalThis` reads a standard built-in, without assigning it
fn is_builtin_access(rest: &str) -> bool {
    let Some(rest) = rest.trim_start().strip_prefix('.') else {
        return false;
    };
    let rest = rest.trim_start();
    let name = identifier_prefix(rest);
    if !BUILTINS.contains(&name) {
        return false;
    }
    let after = rest[name.len()..].trim_start();
    !(after.starts_with('=') && !after.starts_with("=="))
}

/// Parse `.name` followed by an optional `(`, returning the name and whether it is called
fn member_call(rest: &str) -> Option<(&str, bool)> {
    let rest = rest.trim_start().strip_prefix('.')?.trim_start();
    let name = identifier_prefix(rest);
    if name.is_empty() {
        return None;
    }
    let is_call = rest[name.len()..].trim_start().starts_with('(');
    Some((name, is_call))
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn identifier_prefix(text: &str) -> &str {
    let end = text
        .find(|c: char| !is_identifier_char(c))
        .unwrap_or(text.len());
    &text[..end]
}

/// Iterate over the identifiers of a source text, with their start offset
///
/// Comments and strings are not skipped:
/// a `mapper` mentioned there makes the classification stricter, never weaker.
fn identifiers(source: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let rest = &source[offset..];
        let start = offset + rest.find(is_identifier_char)?;
        let word = identifier_prefix(&source[start..]);
        offset = start + word.len();
        Some((start, word))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(source: &str) -> ContextAccess {
        SourceAnalysis::of(source).access
    }

    fn shares_globals(source: &str) -> bool {
        SourceAnalysis::of(source).shares_globals
    }

    #[test]
    fn script_using_only_flow_and_script_scopes_is_independent() {
        let source = r#"
            export function onMessage(message, context) {
                const count = (context.flow.get("count") || 0) + 1;
                context.flow.set("count", count);
                context.script.set("last", message.topic);
                return [message];
            }
        "#;
        assert_eq!(access(source), ContextAccess::Independent);
    }

    #[test]
    fn script_only_reading_the_mapper_scope_is_a_reader() {
        let source = r#"
            export function onMessage(message, context) {
                const entity = context.mapper.get(message.topic);
                const keys = context.mapper . keys ();
                return [message];
            }
        "#;
        assert_eq!(access(source), ContextAccess::Read);
    }

    #[test]
    fn script_writing_the_mapper_scope_is_a_writer() {
        assert_eq!(
            access(r#"context.mapper.set("k", 1)"#),
            ContextAccess::Write
        );
        assert_eq!(
            access(r#"context.mapper.get("k"); context.mapper.remove("k")"#),
            ContextAccess::Write
        );
    }

    #[test]
    fn aliased_access_is_a_writer() {
        assert_eq!(
            access(r#"const { mapper } = context; mapper.get("k")"#),
            ContextAccess::Write
        );
        assert_eq!(
            access(r#"const m = context.mapper; m.get("k")"#),
            ContextAccess::Write
        );
        assert_eq!(
            access(r#"const get = context.mapper.get"#),
            ContextAccess::Write
        );
    }

    #[test]
    fn mapper_in_comments_or_strings_is_a_writer() {
        assert_eq!(access("// the mapper is updated"), ContextAccess::Write);
        assert_eq!(access(r#"const s = "mapper""#), ContextAccess::Write);
    }

    #[test]
    fn longer_identifiers_are_not_mistaken_for_mapper() {
        assert_eq!(
            access("const mappers = 1; const c8y_mapper = 2; const $mapper = 3;"),
            ContextAccess::Independent
        );
    }

    #[test]
    fn reading_builtins_through_global_this_shares_nothing() {
        let source = r#"
            const td = new globalThis.TextDecoder();
            const now = globalThis.Date.now();
            if (globalThis.JSON == undefined) {}
        "#;
        assert!(!shares_globals(source));
        assert_eq!(access(source), ContextAccess::Independent);
    }

    #[test]
    fn using_global_this_for_state_shares_globals() {
        for source in [
            "globalThis.lastSeen = 42",
            "const x = globalThis.lastSeen",
            r#"globalThis["lastSeen"] = 42"#,
            "const g = globalThis",
            "globalThis.JSON = {}",
        ] {
            assert!(shares_globals(source), "{source}");
            assert_eq!(access(source), ContextAccess::Write, "{source}");
        }
    }
}
