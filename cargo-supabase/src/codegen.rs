//! Parse PostgREST's OpenAPI introspection document and emit Rust row structs.
//!
//! PostgREST returns Swagger 2.0 from `GET /rest/v1/?apikey=<key>`. Each
//! exposed table appears under `definitions`, with a `properties` map and an
//! optional `required` list:
//!
//! ```json
//! { "swagger": "2.0",
//!   "definitions": {
//!     "users": {
//!       "required": ["id"],
//!       "properties": {
//!         "id":         { "format": "uuid",                       "type": "string" },
//!         "email":      { "format": "text",                       "type": "string" },
//!         "created_at": { "format": "timestamp with time zone",   "type": "string" }
//!       }
//!     }
//!   }
//! }
//! ```
//!
//! We translate `format` (preferred) and fall back to `type` when the format
//! is unknown. Columns absent from `required` are wrapped in `Option<T>`.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OpenApi {
    #[serde(default)]
    pub definitions: BTreeMap<String, TableDef>,
}

#[derive(Debug, Deserialize)]
pub struct TableDef {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, ColumnDef>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ColumnDef {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// OpenAPI `enum` keyword — PostgREST emits this for columns whose Postgres
    /// type is a user-defined enum. Element-level for arrays of enum.
    #[serde(default, rename = "enum")]
    pub variants: Option<Vec<String>>,
}

/// Resolved Postgres enum, after de-duping by `format` name across all tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumInfo {
    /// Original PostgREST format string, e.g. `"public.user_status"`.
    pub format: String,
    /// Generated Rust type name, e.g. `UserStatus` (or `PublicUserStatus` on
    /// cross-schema collision).
    pub rust_name: String,
    /// Variant labels exactly as Postgres reports them, in declaration order.
    pub variants: Vec<String>,
}

/// Codegen settings, surfaced as CLI flags.
#[derive(Debug, Clone)]
pub struct Options {
    /// Optional schema label baked into the generated docs / `Row::SCHEMA`.
    pub schema: String,
    /// Optional table allow-list. Empty = emit every discovered table.
    pub only: Vec<String>,
    /// Optional table deny-list.
    pub exclude: Vec<String>,
    /// Whether to use `chrono` types for timestamp/date columns. When `false`,
    /// timestamps are emitted as `String`.
    pub chrono: bool,
    /// Whether to use `uuid::Uuid` for `uuid`-format columns. When `false`,
    /// they are emitted as `String`.
    pub uuid_type: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            schema: "public".to_string(),
            only: Vec::new(),
            exclude: Vec::new(),
            chrono: true,
            uuid_type: false,
        }
    }
}

/// Generate the contents of a Rust module from a parsed OpenAPI document.
pub fn emit(api: &OpenApi, opts: &Options) -> String {
    let mut out = String::new();
    out.push_str(HEADER);

    let mut tables: Vec<(&String, &TableDef)> = api
        .definitions
        .iter()
        .filter(|(name, _)| {
            if !opts.only.is_empty() && !opts.only.iter().any(|p| p == *name) {
                return false;
            }
            !opts.exclude.iter().any(|p| p == *name)
        })
        .collect();
    tables.sort_by(|a, b| a.0.cmp(b.0));

    // Collect every distinct Postgres enum referenced by an included table,
    // emit a single Rust enum per format up-front, then thread the map into
    // table emission so column types resolve to the enum name.
    let enums = collect_enums(&tables, opts);
    for info in enums.values() {
        emit_enum(&mut out, info);
    }

    for (table_name, def) in tables {
        emit_table(&mut out, &opts.schema, table_name, def, opts, &enums);
    }

    out
}

/// Walk every column in the included tables, group enum columns by their
/// PostgREST `format` string, and pick a Rust type name for each. When two
/// schemas expose enums with the same simple name (e.g. `public.status` vs
/// `audit.status`), we fall back to fully-qualified PascalCase so they don't
/// collide.
fn collect_enums(
    tables: &[(&String, &TableDef)],
    opts: &Options,
) -> BTreeMap<String, EnumInfo> {
    // Group variants by `format`. First non-empty list wins; subsequent
    // identical lists are ignored. Conflicting lists for the same format are
    // unioned (declaration order from the first occurrence is preserved, then
    // any new labels appended) so we never silently drop a value.
    let mut by_format: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, def) in tables {
        for col in def.properties.values() {
            let Some(fmt) = col.format.as_ref() else { continue };
            let Some(vars) = col.variants.as_ref() else { continue };
            if vars.is_empty() {
                continue;
            }
            let entry = by_format.entry(fmt.clone()).or_default();
            if entry.is_empty() {
                entry.extend(vars.iter().cloned());
            } else {
                for v in vars {
                    if !entry.iter().any(|e| e == v) {
                        entry.push(v.clone());
                    }
                }
            }
        }
    }

    // Compute each format's *candidate* Rust name from the last `.`-segment of
    // its format string (so `public.user_status` and `audit.user_status` both
    // produce `UserStatus`). Any candidate shared by ≥2 formats falls back to
    // a fully-qualified name (`Schema_Type`) so both sides remain distinct.
    let _ = opts; // schema is no longer used here, but kept in the signature
                  // for future opts-driven naming overrides.
    let mut candidate_for: BTreeMap<String, String> = BTreeMap::new();
    let mut candidate_counts: BTreeMap<String, usize> = BTreeMap::new();
    for fmt in by_format.keys() {
        let candidate = to_struct_name(last_segment(fmt));
        *candidate_counts.entry(candidate.clone()).or_insert(0) += 1;
        candidate_for.insert(fmt.clone(), candidate);
    }

    let mut out = BTreeMap::new();
    for (fmt, variants) in by_format {
        let candidate = candidate_for.get(&fmt).cloned().unwrap_or_default();
        let collides = candidate_counts.get(&candidate).copied().unwrap_or(0) > 1;
        let rust_name = if collides {
            to_struct_name(&fmt.replace('.', "_"))
        } else {
            candidate
        };
        out.insert(
            fmt.clone(),
            EnumInfo { format: fmt, rust_name, variants },
        );
    }
    out
}

/// Substring after the last `.` in a PostgREST format name —
/// `"public.user_status"` → `"user_status"`, bare names pass through.
fn last_segment(fmt: &str) -> &str {
    fmt.rsplit('.').next().unwrap_or(fmt)
}

fn emit_enum(out: &mut String, info: &EnumInfo) {
    out.push_str(&format!("/// Postgres enum `{}`.\n", info.format));
    out.push_str(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]\n",
    );
    out.push_str(&format!("pub enum {} {{\n", info.rust_name));
    for label in &info.variants {
        let variant = to_variant_name(label);
        if variant != *label {
            out.push_str(&format!("    #[serde(rename = \"{label}\")]\n"));
        }
        out.push_str(&format!("    {variant},\n"));
    }
    out.push_str("}\n\n");
}

/// Sanitize an enum label into a valid Rust variant ident in PascalCase.
/// Non-alphanumeric chars split words; empty / digit-leading labels get a
/// leading underscore so they remain valid idents.
fn to_variant_name(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut capitalize = true;
    for ch in label.chars() {
        if ch.is_alphanumeric() {
            if capitalize {
                for u in ch.to_uppercase() {
                    out.push(u);
                }
                capitalize = false;
            } else {
                out.push(ch);
            }
        } else {
            capitalize = true;
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

const HEADER: &str = "//! Auto-generated by `cargo supabase gen types`. DO NOT EDIT.
//!
//! Re-generate with the same command after applying a migration.

#![allow(clippy::module_name_repetitions, dead_code)]
#![allow(non_camel_case_types, non_snake_case)]

use rust_supabase_sdk::{postgrest::Column, Row};
use serde::{Deserialize, Serialize};

";

fn emit_table(
    out: &mut String,
    schema: &str,
    table: &str,
    def: &TableDef,
    opts: &Options,
    enums: &BTreeMap<String, EnumInfo>,
) {
    let struct_name = to_struct_name(table);

    out.push_str(&format!("/// Row of `{schema}.{table}`.\n"));
    out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    out.push_str(&format!("pub struct {struct_name} {{\n"));

    let mut columns: Vec<(&String, &ColumnDef)> = def.properties.iter().collect();
    columns.sort_by(|a, b| a.0.cmp(b.0));

    let mut column_names: Vec<String> = Vec::with_capacity(columns.len());
    for (col_name, col) in &columns {
        let rust_ty = map_type_with_enums(col, opts, enums);
        let optional = !def.required.iter().any(|r| r == *col_name);
        let final_ty = if optional {
            format!("Option<{rust_ty}>")
        } else {
            rust_ty
        };

        if let Some(desc) = &col.description {
            for line in desc.lines() {
                out.push_str(&format!("    /// {line}\n"));
            }
        }
        let safe = to_field_name(col_name);
        if safe != *col_name.as_str() {
            out.push_str(&format!("    #[serde(rename = \"{col_name}\")]\n"));
        }
        out.push_str(&format!("    pub {safe}: {final_ty},\n"));
        column_names.push((*col_name).clone());
    }

    out.push_str("}\n\n");

    out.push_str(&format!("impl Row for {struct_name} {{\n"));
    out.push_str(&format!("    const TABLE: &'static str = \"{table}\";\n"));
    out.push_str(&format!(
        "    const SCHEMA: Option<&'static str> = Some(\"{schema}\");\n"
    ));
    out.push_str("    const COLUMNS: &'static [&'static str] = &[");
    for (i, c) in column_names.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(c);
        out.push('"');
    }
    out.push_str("];\n");
    out.push_str("}\n\n");

    // ------------------------------------------------------------
    // Typed column constants:
    //   impl Posts {
    //       pub const status: Column<Posts, String> = Column::new("status");
    //       ...
    //   }
    // ------------------------------------------------------------
    out.push_str(&format!(
        "#[allow(non_upper_case_globals)]\nimpl {struct_name} {{\n"
    ));
    for (col_name, col) in &columns {
        let rust_ty = map_type_with_enums(col, opts, enums);
        let optional = !def.required.iter().any(|r| r == *col_name);
        let final_ty = if optional {
            format!("Option<{rust_ty}>")
        } else {
            rust_ty
        };
        let safe = to_field_name(col_name);
        out.push_str(&format!(
            "    pub const {safe}: Column<{struct_name}, {final_ty}> = Column::new(\"{col_name}\");\n"
        ));
    }
    out.push_str("}\n\n");
}

/// Map a PostgREST column descriptor to a Rust type name. Equivalent to
/// [`map_type_with_enums`] with an empty enum map — retained for tests and
/// downstream callers that don't care about enum resolution.
#[cfg(test)]
fn map_type(col: &ColumnDef, opts: &Options) -> String {
    map_type_with_enums(col, opts, &BTreeMap::new())
}

/// Map a PostgREST column descriptor to a Rust type name, consulting the
/// pre-collected enum table for any `format` we recognized as a user-defined
/// Postgres enum.
fn map_type_with_enums(
    col: &ColumnDef,
    opts: &Options,
    enums: &BTreeMap<String, EnumInfo>,
) -> String {
    let fmt = col.format.as_deref().unwrap_or("").trim();
    let kind = col.kind.as_deref().unwrap_or("").trim();

    // Arrays — kind is "array", format describes the element type.
    if kind == "array" {
        // Array of enum: PostgREST keeps the element's enum format in `format`,
        // even though the variant list lives at the array-column level.
        if let Some(info) = enums.get(fmt) {
            return format!("Vec<{}>", info.rust_name);
        }
        // Pass "" as the element-level `kind` so that the string-type guard
        // (`kind == "string"`) does not prevent "text", "varchar" etc. from
        // resolving correctly inside the element.
        let inner = map_scalar_format(fmt, "string", opts);
        return format!("Vec<{inner}>");
    }

    // Scalar enum column.
    if let Some(info) = enums.get(fmt) {
        return info.rust_name.clone();
    }

    map_scalar_format(fmt, kind, opts)
}

fn map_scalar_format(format: &str, kind: &str, opts: &Options) -> String {
    let stripped = format.trim_start_matches("array of ").trim();
    match stripped {
        // Integers
        "smallint" | "int2" => "i16".into(),
        "integer" | "int" | "int4" | "serial" => "i32".into(),
        "bigint" | "int8" | "bigserial" => "i64".into(),
        // Floats / numeric
        "real" | "float4" => "f32".into(),
        "double precision" | "float8" => "f64".into(),
        "numeric" | "decimal" | "money" => "f64".into(),
        // Booleans
        "boolean" | "bool" => "bool".into(),
        // Strings
        "text" | "character varying" | "varchar" | "character" | "bpchar" | "name" | "citext"
        | "" if kind == "string" || (kind.is_empty() && stripped.is_empty()) => "String".into(),
        // Times
        "timestamp with time zone" | "timestamptz" => {
            if opts.chrono {
                "chrono::DateTime<chrono::Utc>".into()
            } else {
                "String".into()
            }
        }
        "timestamp without time zone" | "timestamp" => {
            if opts.chrono {
                "chrono::NaiveDateTime".into()
            } else {
                "String".into()
            }
        }
        "date" => {
            if opts.chrono {
                "chrono::NaiveDate".into()
            } else {
                "String".into()
            }
        }
        "time with time zone" | "time without time zone" | "time" => "String".into(),
        // UUID
        "uuid" => {
            if opts.uuid_type {
                "uuid::Uuid".into()
            } else {
                "String".into()
            }
        }
        // JSON
        "json" | "jsonb" => "serde_json::Value".into(),
        // Bytes
        "bytea" => "Vec<u8>".into(),
        // Catch-all: use the OpenAPI `type` keyword
        _ => match kind {
            "string" => "String".into(),
            "integer" => "i64".into(),
            "number" => "f64".into(),
            "boolean" => "bool".into(),
            "array" => "Vec<serde_json::Value>".into(),
            "object" => "serde_json::Value".into(),
            _ => "serde_json::Value".into(),
        },
    }
}

/// Convert a `snake_case` table name into `PascalCase` for the struct name.
fn to_struct_name(table: &str) -> String {
    let mut out = String::with_capacity(table.len());
    let mut capitalize = true;
    for ch in table.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            capitalize = true;
            continue;
        }
        if capitalize {
            for u in ch.to_uppercase() {
                out.push(u);
            }
            capitalize = false;
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        out.push_str("Row");
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Sanitize a column name into a valid Rust field name. Reserved keywords
/// become `r#name`; non-identifier chars are replaced with `_`.
fn to_field_name(col: &str) -> String {
    let cleaned: String = col
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    let cleaned = if cleaned
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
    {
        format!("_{cleaned}")
    } else {
        cleaned
    };
    if is_rust_keyword(&cleaned) {
        format!("r#{cleaned}")
    } else {
        cleaned
    }
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
            | "union"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> OpenApi {
        let json = serde_json::json!({
            "definitions": {
                "users": {
                    "required": ["id", "email"],
                    "properties": {
                        "id":         { "format": "uuid", "type": "string" },
                        "email":      { "format": "text", "type": "string" },
                        "age":        { "format": "integer", "type": "integer" },
                        "created_at": { "format": "timestamp with time zone", "type": "string" },
                        "metadata":   { "format": "jsonb", "type": "object" },
                        "score":      { "format": "double precision", "type": "number" },
                        "type":       { "format": "text", "type": "string" }
                    }
                },
                "audit_log": {
                    "required": [],
                    "properties": {
                        "id":   { "format": "bigint", "type": "integer" },
                        "data": { "format": "json", "type": "object" }
                    }
                }
            }
        });
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn struct_naming() {
        assert_eq!(to_struct_name("users"), "Users");
        assert_eq!(to_struct_name("audit_log"), "AuditLog");
        assert_eq!(to_struct_name("2fa_tokens"), "_2faTokens");
    }

    #[test]
    fn keyword_columns_get_raw_prefix() {
        assert_eq!(to_field_name("type"), "r#type");
        assert_eq!(to_field_name("self"), "r#self");
        assert_eq!(to_field_name("normal"), "normal");
    }

    #[test]
    fn map_type_chrono_timestamp() {
        let col = ColumnDef {
            format: Some("timestamp with time zone".into()),
            kind: Some("string".into()),
            description: None,
            variants: None,
        };
        let opts = Options { chrono: true, ..Options::default() };
        assert_eq!(map_type(&col, &opts), "chrono::DateTime<chrono::Utc>");

        let opts2 = Options { chrono: false, ..Options::default() };
        assert_eq!(map_type(&col, &opts2), "String");
    }

    #[test]
    fn map_type_uuid_optin() {
        let col = ColumnDef {
            format: Some("uuid".into()),
            kind: Some("string".into()),
            description: None,
            variants: None,
        };
        let off = Options::default();
        assert_eq!(map_type(&col, &off), "String");

        let on = Options { uuid_type: true, ..Options::default() };
        assert_eq!(map_type(&col, &on), "uuid::Uuid");
    }

    #[test]
    fn emit_renders_required_vs_optional() {
        let opts = Options::default();
        let out = emit(&fixture(), &opts);

        // Required fields are NOT wrapped in Option.
        assert!(out.contains("pub email: String,"), "missing required email field:\n{out}");
        // Non-required fields ARE wrapped in Option.
        // Postgres `integer` is int4 → i32.
        assert!(out.contains("pub age: Option<i32>,"), "expected Option<i32> for age:\n{out}");
        // Timestamps map through chrono.
        assert!(out.contains("Option<chrono::DateTime<chrono::Utc>>"));
        // jsonb columns map to serde_json::Value.
        assert!(out.contains("Option<serde_json::Value>"));
        // Reserved keywords get r# prefix and serde rename.
        assert!(out.contains("#[serde(rename = \"type\")]"));
        assert!(out.contains("pub r#type: Option<String>,"));
        // Row impl with table + columns
        assert!(out.contains("impl Row for Users"));
        assert!(out.contains("const TABLE: &'static str = \"users\";"));
        assert!(out.contains("const SCHEMA: Option<&'static str> = Some(\"public\");"));
        // Alphabetical struct order: AuditLog comes before Users.
        let audit_pos = out.find("struct AuditLog").unwrap();
        let users_pos = out.find("struct Users").unwrap();
        assert!(audit_pos < users_pos, "expected AuditLog before Users");
    }

    #[test]
    fn only_filter_keeps_subset() {
        let opts = Options { only: vec!["users".into()], ..Options::default() };
        let out = emit(&fixture(), &opts);
        assert!(out.contains("struct Users"));
        assert!(!out.contains("struct AuditLog"));
    }

    #[test]
    fn exclude_filter_drops_subset() {
        let opts = Options { exclude: vec!["audit_log".into()], ..Options::default() };
        let out = emit(&fixture(), &opts);
        assert!(out.contains("struct Users"));
        assert!(!out.contains("struct AuditLog"));
    }

    // -----------------------------------------------------------------------
    // to_struct_name — comprehensive
    // -----------------------------------------------------------------------

    #[test]
    fn struct_name_single_word_lower() {
        assert_eq!(to_struct_name("posts"), "Posts");
    }

    #[test]
    fn struct_name_hyphenated() {
        assert_eq!(to_struct_name("blog-posts"), "BlogPosts");
    }

    #[test]
    fn struct_name_space_separated() {
        assert_eq!(to_struct_name("my table"), "MyTable");
    }

    #[test]
    fn struct_name_mixed_separators() {
        assert_eq!(to_struct_name("foo_bar-baz qux"), "FooBarBazQux");
    }

    #[test]
    fn struct_name_empty_becomes_row() {
        assert_eq!(to_struct_name(""), "Row");
    }

    #[test]
    fn struct_name_already_pascal() {
        // preserves original casing in subsequent chars
        assert_eq!(to_struct_name("UserProfiles"), "UserProfiles");
    }

    #[test]
    fn struct_name_digit_prefix_gets_underscore() {
        assert_eq!(to_struct_name("2fa"), "_2fa");
        assert_eq!(to_struct_name("0th_entry"), "_0thEntry");
    }

    // -----------------------------------------------------------------------
    // to_field_name — comprehensive
    // -----------------------------------------------------------------------

    #[test]
    fn field_name_plain_passes_through() {
        assert_eq!(to_field_name("user_id"), "user_id");
    }

    #[test]
    fn field_name_non_ident_chars_replaced() {
        assert_eq!(to_field_name("my-field"), "my_field");
        assert_eq!(to_field_name("field.name"), "field_name");
    }

    #[test]
    fn field_name_digit_start_gets_prefix() {
        assert_eq!(to_field_name("2ndcol"), "_2ndcol");
    }

    #[test]
    fn field_name_all_reserved_keywords() {
        // Spot-check a handful from both Rust editions
        let kws = ["as", "fn", "impl", "let", "mod", "mut", "pub", "ref",
                   "self", "struct", "trait", "type", "use", "where",
                   "async", "await", "dyn", "try", "union"];
        for kw in kws {
            let out = to_field_name(kw);
            assert_eq!(out, format!("r#{kw}"), "keyword {kw} should be prefixed");
        }
    }

    #[test]
    fn field_name_non_keyword_not_prefixed() {
        assert_eq!(to_field_name("normal"), "normal");
        assert_eq!(to_field_name("id"), "id");
        assert_eq!(to_field_name("value"), "value");
    }

    // -----------------------------------------------------------------------
    // map_type — all scalar formats
    // -----------------------------------------------------------------------

    fn mk(format: &str, kind: &str) -> ColumnDef {
        ColumnDef {
            format: if format.is_empty() { None } else { Some(format.to_string()) },
            kind: if kind.is_empty() { None } else { Some(kind.to_string()) },
            description: None,
            variants: None,
        }
    }

    fn opts_default() -> Options { Options::default() }
    fn opts_no_chrono() -> Options { Options { chrono: false, ..Options::default() } }
    fn opts_uuid()     -> Options { Options { uuid_type: true, ..Options::default() } }

    // Integers
    #[test]
    fn map_smallint() {
        assert_eq!(map_type(&mk("smallint", "integer"), &opts_default()), "i16");
        assert_eq!(map_type(&mk("int2", "integer"), &opts_default()), "i16");
    }

    #[test]
    fn map_integer_variants() {
        for fmt in ["integer", "int", "int4", "serial"] {
            assert_eq!(map_type(&mk(fmt, "integer"), &opts_default()), "i32", "fmt={fmt}");
        }
    }

    #[test]
    fn map_bigint_variants() {
        for fmt in ["bigint", "int8", "bigserial"] {
            assert_eq!(map_type(&mk(fmt, "integer"), &opts_default()), "i64", "fmt={fmt}");
        }
    }

    // Floats
    #[test]
    fn map_real_and_float4() {
        assert_eq!(map_type(&mk("real", "number"), &opts_default()), "f32");
        assert_eq!(map_type(&mk("float4", "number"), &opts_default()), "f32");
    }

    #[test]
    fn map_double_precision() {
        assert_eq!(map_type(&mk("double precision", "number"), &opts_default()), "f64");
        assert_eq!(map_type(&mk("float8", "number"), &opts_default()), "f64");
    }

    #[test]
    fn map_numeric_decimal_money() {
        for fmt in ["numeric", "decimal", "money"] {
            assert_eq!(map_type(&mk(fmt, "number"), &opts_default()), "f64", "fmt={fmt}");
        }
    }

    // Boolean
    #[test]
    fn map_boolean() {
        assert_eq!(map_type(&mk("boolean", "boolean"), &opts_default()), "bool");
        assert_eq!(map_type(&mk("bool", "boolean"), &opts_default()), "bool");
    }

    // Strings
    #[test]
    fn map_text_string_variants() {
        for fmt in ["text", "character varying", "varchar", "character", "bpchar", "name", "citext"] {
            assert_eq!(map_type(&mk(fmt, "string"), &opts_default()), "String", "fmt={fmt}");
        }
    }

    // Timestamps with chrono
    #[test]
    fn map_timestamptz_with_chrono() {
        let col = mk("timestamp with time zone", "string");
        assert_eq!(map_type(&col, &opts_default()), "chrono::DateTime<chrono::Utc>");
        assert_eq!(map_type(&mk("timestamptz", "string"), &opts_default()), "chrono::DateTime<chrono::Utc>");
    }

    #[test]
    fn map_timestamptz_without_chrono() {
        assert_eq!(map_type(&mk("timestamp with time zone", "string"), &opts_no_chrono()), "String");
    }

    #[test]
    fn map_timestamp_naive_with_chrono() {
        assert_eq!(map_type(&mk("timestamp without time zone", "string"), &opts_default()), "chrono::NaiveDateTime");
        assert_eq!(map_type(&mk("timestamp", "string"), &opts_default()), "chrono::NaiveDateTime");
    }

    #[test]
    fn map_timestamp_naive_without_chrono() {
        assert_eq!(map_type(&mk("timestamp", "string"), &opts_no_chrono()), "String");
    }

    #[test]
    fn map_date_with_and_without_chrono() {
        assert_eq!(map_type(&mk("date", "string"), &opts_default()), "chrono::NaiveDate");
        assert_eq!(map_type(&mk("date", "string"), &opts_no_chrono()), "String");
    }

    #[test]
    fn map_time_is_always_string() {
        for fmt in ["time", "time with time zone", "time without time zone"] {
            assert_eq!(map_type(&mk(fmt, "string"), &opts_default()), "String", "fmt={fmt}");
            assert_eq!(map_type(&mk(fmt, "string"), &opts_no_chrono()), "String", "fmt={fmt} no-chrono");
        }
    }

    // UUID
    #[test]
    fn map_uuid_string_default() {
        assert_eq!(map_type(&mk("uuid", "string"), &opts_default()), "String");
    }

    #[test]
    fn map_uuid_type_with_flag() {
        assert_eq!(map_type(&mk("uuid", "string"), &opts_uuid()), "uuid::Uuid");
    }

    // JSON / JSONB
    #[test]
    fn map_json_jsonb() {
        assert_eq!(map_type(&mk("json", "object"), &opts_default()), "serde_json::Value");
        assert_eq!(map_type(&mk("jsonb", "object"), &opts_default()), "serde_json::Value");
    }

    // Bytea
    #[test]
    fn map_bytea() {
        assert_eq!(map_type(&mk("bytea", "string"), &opts_default()), "Vec<u8>");
    }

    // Arrays
    #[test]
    fn map_array_of_integer() {
        let col = ColumnDef {
            format: Some("integer".into()),
            kind: Some("array".into()),
            description: None,
            variants: None,
        };
        assert_eq!(map_type(&col, &opts_default()), "Vec<i32>");
    }

    #[test]
    fn map_array_of_text() {
        let col = ColumnDef {
            format: Some("text".into()),
            kind: Some("array".into()),
            description: None,
            variants: None,
        };
        assert_eq!(map_type(&col, &opts_default()), "Vec<String>");
    }

    #[test]
    fn map_array_of_uuid_with_flag() {
        let col = ColumnDef {
            format: Some("uuid".into()),
            kind: Some("array".into()),
            description: None,
            variants: None,
        };
        assert_eq!(map_type(&col, &opts_uuid()), "Vec<uuid::Uuid>");
    }

    // Catch-all fallbacks
    #[test]
    fn map_unknown_format_falls_back_to_kind() {
        assert_eq!(map_type(&mk("unknown_pg_type", "string"), &opts_default()), "String");
        assert_eq!(map_type(&mk("unknown_pg_type", "integer"), &opts_default()), "i64");
        assert_eq!(map_type(&mk("unknown_pg_type", "number"), &opts_default()), "f64");
        assert_eq!(map_type(&mk("unknown_pg_type", "boolean"), &opts_default()), "bool");
        assert_eq!(map_type(&mk("unknown_pg_type", "object"), &opts_default()), "serde_json::Value");
    }

    #[test]
    fn map_totally_unknown_becomes_value() {
        assert_eq!(map_type(&mk("mystery_type", "mystery_kind"), &opts_default()), "serde_json::Value");
    }

    // -----------------------------------------------------------------------
    // emit — edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn emit_empty_definitions_contains_only_header() {
        let api = OpenApi { definitions: BTreeMap::new() };
        let out = emit(&api, &Options::default());
        assert!(out.contains("Auto-generated by"), "header should be present");
        assert!(!out.contains("pub struct"), "no structs should be emitted");
    }

    #[test]
    fn emit_custom_schema_baked_in() {
        let api = OpenApi {
            definitions: {
                let mut m = BTreeMap::new();
                m.insert("items".into(), TableDef {
                    required: vec!["id".into()],
                    properties: {
                        let mut p = BTreeMap::new();
                        p.insert("id".into(), mk("bigint", "integer"));
                        p
                    },
                });
                m
            },
        };
        let opts = Options { schema: "inventory".into(), ..Options::default() };
        let out = emit(&api, &opts);
        assert!(out.contains("const SCHEMA: Option<&'static str> = Some(\"inventory\");"));
        assert!(out.contains("/// Row of `inventory.items`"));
    }

    #[test]
    fn emit_bytea_column_type() {
        let api = OpenApi {
            definitions: {
                let mut m = BTreeMap::new();
                m.insert("blobs".into(), TableDef {
                    required: vec![],
                    properties: {
                        let mut p = BTreeMap::new();
                        p.insert("data".into(), mk("bytea", "string"));
                        p
                    },
                });
                m
            },
        };
        let out = emit(&api, &Options::default());
        assert!(out.contains("pub data: Option<Vec<u8>>,"), "bytea should map to Vec<u8>: {out}");
    }

    #[test]
    fn emit_column_with_description_emits_doc_comment() {
        let api = OpenApi {
            definitions: {
                let mut m = BTreeMap::new();
                m.insert("things".into(), TableDef {
                    required: vec!["id".into()],
                    properties: {
                        let mut p = BTreeMap::new();
                        p.insert("id".into(), ColumnDef {
                            format: Some("integer".into()),
                            kind: Some("integer".into()),
                            description: Some("Primary key".into()),
                            variants: None,
                        });
                        p
                    },
                });
                m
            },
        };
        let out = emit(&api, &Options::default());
        assert!(out.contains("/// Primary key"), "description should become doc comment: {out}");
    }

    #[test]
    fn emit_columns_sorted_alphabetically() {
        let api = OpenApi {
            definitions: {
                let mut m = BTreeMap::new();
                m.insert("t".into(), TableDef {
                    required: vec![],
                    properties: {
                        let mut p = BTreeMap::new();
                        p.insert("z_col".into(), mk("integer", "integer"));
                        p.insert("a_col".into(), mk("integer", "integer"));
                        p.insert("m_col".into(), mk("integer", "integer"));
                        p
                    },
                });
                m
            },
        };
        let out = emit(&api, &Options::default());
        let a = out.find("a_col").unwrap();
        let m = out.find("m_col").unwrap();
        let z = out.find("z_col").unwrap();
        assert!(a < m && m < z, "columns should appear in alphabetical order");
    }

    #[test]
    fn emit_only_and_exclude_both_set() {
        // `only` takes priority — "users" is in only, so audit_log is excluded
        // even though it's not in `exclude`.
        let opts = Options {
            only: vec!["users".into()],
            exclude: vec!["users".into()], // also in exclude
            ..Options::default()
        };
        let out = emit(&fixture(), &opts);
        // `only` matches users but `exclude` also contains it → excluded
        assert!(!out.contains("struct Users"), "excluded via both lists: {out}");
    }

    #[test]
    fn emit_no_tables_match_only_list() {
        let opts = Options { only: vec!["nonexistent".into()], ..Options::default() };
        let out = emit(&fixture(), &opts);
        assert!(!out.contains("pub struct"), "no structs should be emitted: {out}");
    }

    // -----------------------------------------------------------------------
    // OpenApi deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn openapi_deserializes_missing_definitions() {
        let v = serde_json::json!({ "swagger": "2.0" });
        let api: OpenApi = serde_json::from_value(v).unwrap();
        assert!(api.definitions.is_empty());
    }

    #[test]
    fn openapi_deserializes_empty_definitions() {
        let v = serde_json::json!({ "definitions": {} });
        let api: OpenApi = serde_json::from_value(v).unwrap();
        assert!(api.definitions.is_empty());
    }

    #[test]
    fn table_def_required_defaults_empty() {
        let v = serde_json::json!({ "properties": {} });
        let t: TableDef = serde_json::from_value(v).unwrap();
        assert!(t.required.is_empty());
    }

    #[test]
    fn column_def_all_fields_optional() {
        let v = serde_json::json!({});
        let c: ColumnDef = serde_json::from_value(v).unwrap();
        assert!(c.format.is_none());
        assert!(c.kind.is_none());
        assert!(c.description.is_none());
    }

    // -----------------------------------------------------------------------
    // is_rust_keyword
    // -----------------------------------------------------------------------

    #[test]
    fn rust_keywords_recognized() {
        let keywords = ["as", "break", "const", "continue", "crate", "else", "enum",
                        "extern", "false", "fn", "for", "if", "impl", "in", "let",
                        "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
                        "self", "Self", "static", "struct", "super", "trait", "true",
                        "type", "unsafe", "use", "where", "while", "async", "await",
                        "dyn", "abstract", "become", "box", "do", "final", "macro",
                        "override", "priv", "typeof", "unsized", "virtual", "yield",
                        "try", "union"];
        for kw in keywords {
            assert!(is_rust_keyword(kw), "{kw} should be a keyword");
        }
    }

    #[test]
    fn non_keywords_not_recognized() {
        for name in ["id", "user", "data", "value", "name", "count", "email"] {
            assert!(!is_rust_keyword(name), "{name} should not be a keyword");
        }
    }

    // ----- typed column constants emission -----

    #[test]
    fn emit_includes_column_constants_block_per_table() {
        let out = emit(&fixture(), &Options::default());
        // Required column → bare type, no Option wrapper.
        assert!(
            out.contains("pub const email: Column<Users, String> = Column::new(\"email\");"),
            "missing column constant for email:\n{out}"
        );
        // Optional column → Option<T>.
        assert!(
            out.contains("pub const age: Column<Users, Option<i32>> = Column::new(\"age\");"),
            "missing optional column constant for age:\n{out}"
        );
        // Optional timestamp.
        assert!(
            out.contains(
                "pub const created_at: Column<Users, Option<chrono::DateTime<chrono::Utc>>> = Column::new(\"created_at\");"
            ),
            "missing optional timestamp column constant:\n{out}"
        );
        // Reserved keyword gets r# prefix on the const ident.
        assert!(
            out.contains("pub const r#type: Column<Users, Option<String>> = Column::new(\"type\");"),
            "expected r#type const with serde-renamed name:\n{out}"
        );
        // The impl block is opt-out for the lint.
        assert!(out.contains("#[allow(non_upper_case_globals)]"));
        // The block lives on the row struct itself.
        assert!(out.contains("impl Users {"));
    }

    #[test]
    fn emit_includes_column_import_in_header() {
        let out = emit(&fixture(), &Options::default());
        assert!(
            out.contains("use rust_supabase_sdk::{postgrest::Column, Row};"),
            "expected Column to be imported in the header:\n{out}"
        );
    }

    // -----------------------------------------------------------------------
    // Enum codegen
    // -----------------------------------------------------------------------

    fn enum_col(format: &str, variants: &[&str]) -> ColumnDef {
        ColumnDef {
            format: Some(format.to_string()),
            kind: Some("string".to_string()),
            description: None,
            variants: Some(variants.iter().map(|v| (*v).to_string()).collect()),
        }
    }

    fn enum_array_col(format: &str, variants: &[&str]) -> ColumnDef {
        ColumnDef {
            format: Some(format.to_string()),
            kind: Some("array".to_string()),
            description: None,
            variants: Some(variants.iter().map(|v| (*v).to_string()).collect()),
        }
    }

    fn fixture_with_enums() -> OpenApi {
        let json = serde_json::json!({
            "definitions": {
                "posts": {
                    "required": ["id", "status"],
                    "properties": {
                        "id":     { "format": "uuid", "type": "string" },
                        "status": {
                            "format": "public.post_status",
                            "type": "string",
                            "enum": ["draft", "published", "archived"]
                        },
                        "tags":   {
                            "format": "public.color",
                            "type": "array",
                            "enum": ["red", "green", "blue"]
                        }
                    }
                },
                "comments": {
                    "required": [],
                    "properties": {
                        "id":     { "format": "uuid", "type": "string" },
                        // Same enum reused on a second table — must not duplicate.
                        "status": {
                            "format": "public.post_status",
                            "type": "string",
                            "enum": ["draft", "published", "archived"]
                        }
                    }
                }
            }
        });
        serde_json::from_value(json).unwrap()
    }

    // ---- ColumnDef deserialization of `enum` ----

    #[test]
    fn column_def_parses_enum_field() {
        let v = serde_json::json!({
            "format": "public.status",
            "type": "string",
            "enum": ["a", "b", "c"]
        });
        let c: ColumnDef = serde_json::from_value(v).unwrap();
        assert_eq!(
            c.variants.as_deref().map(|v| v.to_vec()),
            Some(vec!["a".into(), "b".into(), "c".into()])
        );
    }

    #[test]
    fn column_def_enum_field_defaults_none() {
        let v = serde_json::json!({ "format": "text", "type": "string" });
        let c: ColumnDef = serde_json::from_value(v).unwrap();
        assert!(c.variants.is_none());
    }

    // ---- last_segment ----

    #[test]
    fn last_segment_strips_schema_prefix() {
        assert_eq!(last_segment("public.user_status"), "user_status");
        assert_eq!(last_segment("audit.user_status"), "user_status");
    }

    #[test]
    fn last_segment_takes_only_after_final_dot() {
        // Defensive — PostgREST formats don't currently nest, but if they ever
        // do we still want the leaf segment.
        assert_eq!(last_segment("a.b.c"), "c");
    }

    #[test]
    fn last_segment_no_dot_passes_through() {
        assert_eq!(last_segment("user_status"), "user_status");
    }

    #[test]
    fn last_segment_empty_input_is_empty() {
        assert_eq!(last_segment(""), "");
    }

    // ---- to_variant_name ----

    #[test]
    fn variant_name_simple_label() {
        assert_eq!(to_variant_name("active"), "Active");
        assert_eq!(to_variant_name("draft"), "Draft");
    }

    #[test]
    fn variant_name_snake_case_label() {
        assert_eq!(to_variant_name("in_progress"), "InProgress");
        assert_eq!(to_variant_name("not_started_yet"), "NotStartedYet");
    }

    #[test]
    fn variant_name_with_hyphens_and_spaces() {
        assert_eq!(to_variant_name("on-hold"), "OnHold");
        assert_eq!(to_variant_name("in progress"), "InProgress");
        assert_eq!(to_variant_name("very-long phrase_here"), "VeryLongPhraseHere");
    }

    #[test]
    fn variant_name_leading_digit_gets_underscore() {
        assert_eq!(to_variant_name("404"), "_404");
        assert_eq!(to_variant_name("2nd_place"), "_2ndPlace");
    }

    #[test]
    fn variant_name_empty_becomes_underscore() {
        assert_eq!(to_variant_name(""), "_");
        assert_eq!(to_variant_name("---"), "_");
    }

    #[test]
    fn variant_name_unicode_preserved() {
        // Unicode alphanumerics survive; punctuation splits words.
        assert_eq!(to_variant_name("café"), "Café");
    }

    // ---- collect_enums ----

    #[test]
    fn collect_enums_dedupes_same_format_across_tables() {
        let api = fixture_with_enums();
        let tables: Vec<_> = api.definitions.iter().collect();
        let enums = collect_enums(&tables, &Options::default());
        // post_status referenced by two tables → still only one entry.
        assert_eq!(enums.len(), 2, "expected post_status + color enums, got: {enums:?}");
        let post = enums.get("public.post_status").expect("post_status missing");
        assert_eq!(post.rust_name, "PostStatus");
        assert_eq!(post.variants, vec!["draft", "published", "archived"]);
    }

    #[test]
    fn collect_enums_collision_uses_qualified_name() {
        let json = serde_json::json!({
            "definitions": {
                "t1": {
                    "required": [],
                    "properties": {
                        "x": { "format": "public.status", "type": "string", "enum": ["a"] }
                    }
                },
                "t2": {
                    "required": [],
                    "properties": {
                        "x": { "format": "audit.status",  "type": "string", "enum": ["b"] }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let tables: Vec<_> = api.definitions.iter().collect();
        let enums = collect_enums(&tables, &Options::default());
        assert_eq!(enums.get("public.status").unwrap().rust_name, "PublicStatus");
        assert_eq!(enums.get("audit.status").unwrap().rust_name, "AuditStatus");
    }

    #[test]
    fn collect_enums_skips_columns_with_no_format() {
        let json = serde_json::json!({
            "definitions": {
                "t": {
                    "required": [],
                    "properties": {
                        // No `format` field → cannot key the enum; should be ignored.
                        "x": { "type": "string", "enum": ["a"] }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let tables: Vec<_> = api.definitions.iter().collect();
        let enums = collect_enums(&tables, &Options::default());
        assert!(enums.is_empty());
    }

    #[test]
    fn collect_enums_skips_empty_variants() {
        let json = serde_json::json!({
            "definitions": {
                "t": {
                    "required": [],
                    "properties": {
                        "x": { "format": "public.s", "type": "string", "enum": [] }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let tables: Vec<_> = api.definitions.iter().collect();
        let enums = collect_enums(&tables, &Options::default());
        assert!(enums.is_empty());
    }

    #[test]
    fn collect_enums_unions_conflicting_variant_lists() {
        // Realistically the same Postgres enum should always have the same
        // variants, but if PostgREST ever disagrees we union rather than drop.
        let json = serde_json::json!({
            "definitions": {
                "a": {
                    "required": [],
                    "properties": {
                        "x": { "format": "public.s", "type": "string", "enum": ["one", "two"] }
                    }
                },
                "b": {
                    "required": [],
                    "properties": {
                        "x": { "format": "public.s", "type": "string", "enum": ["two", "three"] }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let tables: Vec<_> = api.definitions.iter().collect();
        let enums = collect_enums(&tables, &Options::default());
        let s = enums.get("public.s").unwrap();
        assert_eq!(s.variants, vec!["one", "two", "three"]);
    }

    #[test]
    fn collect_enums_respects_table_filter() {
        // `only` filter is applied *before* enum collection — enums only
        // referenced by excluded tables should not be emitted.
        let json = serde_json::json!({
            "definitions": {
                "kept": {
                    "required": [],
                    "properties": {
                        "x": { "format": "public.kept_enum", "type": "string", "enum": ["a"] }
                    }
                },
                "dropped": {
                    "required": [],
                    "properties": {
                        "x": { "format": "public.dropped_enum", "type": "string", "enum": ["b"] }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let opts = Options { only: vec!["kept".into()], ..Options::default() };
        let out = emit(&api, &opts);
        assert!(out.contains("pub enum KeptEnum"), "kept_enum should be emitted:\n{out}");
        assert!(
            !out.contains("pub enum DroppedEnum"),
            "dropped_enum must not leak via excluded table:\n{out}"
        );
    }

    // ---- emit_enum ----

    #[test]
    fn emit_enum_basic_block() {
        let info = EnumInfo {
            format: "public.post_status".into(),
            rust_name: "PostStatus".into(),
            variants: vec!["draft".into(), "published".into()],
        };
        let mut out = String::new();
        emit_enum(&mut out, &info);
        assert!(out.contains("/// Postgres enum `public.post_status`."));
        assert!(out.contains("pub enum PostStatus {"));
        assert!(out.contains("Draft,"));
        assert!(out.contains("Published,"));
    }

    #[test]
    fn emit_enum_derives_include_serde_and_copy() {
        let info = EnumInfo {
            format: "public.s".into(),
            rust_name: "S".into(),
            variants: vec!["a".into()],
        };
        let mut out = String::new();
        emit_enum(&mut out, &info);
        // All the derives we rely on for filter ergonomics + JSON round-trip.
        for d in ["Debug", "Clone", "Copy", "PartialEq", "Eq", "Hash", "Serialize", "Deserialize"] {
            assert!(out.contains(d), "missing derive `{d}` in:\n{out}");
        }
    }

    #[test]
    fn emit_enum_serde_rename_only_when_label_differs_from_variant_ident() {
        let info = EnumInfo {
            format: "public.s".into(),
            rust_name: "S".into(),
            // "Active" round-trips as-is (variant ident is also "Active");
            // "in progress" requires a rename to preserve the original label.
            variants: vec!["Active".into(), "in progress".into()],
        };
        let mut out = String::new();
        emit_enum(&mut out, &info);
        assert!(out.contains("#[serde(rename = \"in progress\")]"));
        assert!(out.contains("InProgress,"));
        assert!(
            !out.contains("#[serde(rename = \"Active\")]"),
            "no rename should be emitted when label already matches the ident:\n{out}"
        );
        assert!(out.contains("Active,"));
    }

    #[test]
    fn emit_enum_preserves_variant_declaration_order() {
        let info = EnumInfo {
            format: "f".into(),
            rust_name: "F".into(),
            variants: vec!["zzz".into(), "aaa".into(), "mmm".into()],
        };
        let mut out = String::new();
        emit_enum(&mut out, &info);
        let z = out.find("Zzz").unwrap();
        let a = out.find("Aaa").unwrap();
        let m = out.find("Mmm").unwrap();
        assert!(z < a && a < m, "variants must preserve Postgres declaration order");
    }

    // ---- map_type_with_enums ----

    #[test]
    fn map_type_with_enums_resolves_scalar_enum() {
        let mut enums = BTreeMap::new();
        enums.insert("public.post_status".into(), EnumInfo {
            format: "public.post_status".into(),
            rust_name: "PostStatus".into(),
            variants: vec!["draft".into()],
        });
        let col = enum_col("public.post_status", &["draft"]);
        assert_eq!(map_type_with_enums(&col, &Options::default(), &enums), "PostStatus");
    }

    #[test]
    fn map_type_with_enums_resolves_enum_array() {
        let mut enums = BTreeMap::new();
        enums.insert("public.color".into(), EnumInfo {
            format: "public.color".into(),
            rust_name: "Color".into(),
            variants: vec!["red".into()],
        });
        let col = enum_array_col("public.color", &["red"]);
        assert_eq!(map_type_with_enums(&col, &Options::default(), &enums), "Vec<Color>");
    }

    #[test]
    fn map_type_with_enums_falls_back_to_string_when_not_collected() {
        // Column has variants in OpenAPI but the format wasn't registered in
        // the enum map (e.g. because the table was filtered out). The mapper
        // must not panic and must produce a usable scalar type.
        let enums = BTreeMap::new();
        let col = enum_col("public.post_status", &["draft"]);
        assert_eq!(map_type_with_enums(&col, &Options::default(), &enums), "String");
    }

    #[test]
    fn map_type_2arg_wrapper_ignores_enums_for_back_compat() {
        // The old 2-arg `map_type` still produces `String` for enum columns —
        // because it sees an empty map. This is what keeps the existing
        // map_type tests valid.
        let col = enum_col("public.s", &["a", "b"]);
        assert_eq!(map_type(&col, &Options::default()), "String");
    }

    // ---- End-to-end emit() integration ----

    #[test]
    fn emit_writes_enum_before_structs_that_use_it() {
        let out = emit(&fixture_with_enums(), &Options::default());
        let enum_pos = out.find("pub enum PostStatus").expect("PostStatus enum missing");
        let struct_pos = out.find("pub struct Posts").expect("Posts struct missing");
        assert!(
            enum_pos < struct_pos,
            "enum decl must precede the structs that reference it"
        );
    }

    #[test]
    fn emit_emits_each_enum_once_even_when_referenced_twice() {
        let out = emit(&fixture_with_enums(), &Options::default());
        let n = out.matches("pub enum PostStatus").count();
        assert_eq!(n, 1, "PostStatus referenced by two tables; should only declare once:\n{out}");
    }

    #[test]
    fn emit_uses_enum_in_struct_field_required() {
        let out = emit(&fixture_with_enums(), &Options::default());
        // `status` is in `required` for posts → bare PostStatus, not Option.
        assert!(
            out.contains("pub status: PostStatus,"),
            "expected required enum column to be bare type:\n{out}"
        );
    }

    #[test]
    fn emit_uses_enum_in_struct_field_optional() {
        let out = emit(&fixture_with_enums(), &Options::default());
        // `status` is NOT in required for comments → Option<PostStatus>.
        // Comments comes first alphabetically, so search the comments block.
        let comments_start = out.find("pub struct Comments").unwrap();
        let comments_end = out[comments_start..]
            .find("\n}\n")
            .map(|i| comments_start + i)
            .unwrap();
        let comments_block = &out[comments_start..comments_end];
        assert!(
            comments_block.contains("pub status: Option<PostStatus>,"),
            "expected optional enum on Comments:\n{comments_block}"
        );
    }

    #[test]
    fn emit_uses_enum_vec_for_array_column() {
        let out = emit(&fixture_with_enums(), &Options::default());
        assert!(
            out.contains("pub tags: Option<Vec<Color>>,"),
            "expected array-of-enum to become Vec<Color>:\n{out}"
        );
    }

    #[test]
    fn emit_typed_column_const_uses_enum_type() {
        let out = emit(&fixture_with_enums(), &Options::default());
        // Required → bare PostStatus on the const too.
        assert!(
            out.contains("pub const status: Column<Posts, PostStatus> = Column::new(\"status\");"),
            "expected typed column const to use enum type:\n{out}"
        );
        // Array variant.
        assert!(
            out.contains("pub const tags: Column<Posts, Option<Vec<Color>>> = Column::new(\"tags\");"),
            "expected Vec<Color> column const:\n{out}"
        );
    }

    #[test]
    fn emit_no_enums_when_none_declared() {
        // Original fixture has no enum columns — header should be the only
        // `pub enum` mention (there isn't one anywhere) and codegen still works.
        let out = emit(&fixture(), &Options::default());
        assert!(
            !out.contains("pub enum "),
            "no enum should be emitted for a schema without user enums:\n{out}"
        );
    }

    #[test]
    fn emit_enum_with_non_ident_label_renames_and_compiles_ident() {
        // Real-world labels can have spaces and hyphens.
        let json = serde_json::json!({
            "definitions": {
                "tickets": {
                    "required": ["priority"],
                    "properties": {
                        "priority": {
                            "format": "public.priority",
                            "type": "string",
                            "enum": ["low", "in progress", "high-priority"]
                        }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let out = emit(&api, &Options::default());
        assert!(out.contains("pub enum Priority {"));
        assert!(out.contains("Low,"));
        assert!(out.contains("#[serde(rename = \"in progress\")]"));
        assert!(out.contains("InProgress,"));
        assert!(out.contains("#[serde(rename = \"high-priority\")]"));
        assert!(out.contains("HighPriority,"));
    }

    #[test]
    fn emit_enum_collision_across_schemas_uses_qualified_names_end_to_end() {
        let json = serde_json::json!({
            "definitions": {
                "t1": {
                    "required": ["s"],
                    "properties": {
                        "s": { "format": "public.status", "type": "string", "enum": ["a"] }
                    }
                },
                "t2": {
                    "required": ["s"],
                    "properties": {
                        "s": { "format": "audit.status",  "type": "string", "enum": ["b"] }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let out = emit(&api, &Options::default());
        assert!(out.contains("pub enum PublicStatus"));
        assert!(out.contains("pub enum AuditStatus"));
        // Each table's field uses the correct qualified name.
        let t1 = out.find("pub struct T1").unwrap();
        let t1_block = &out[t1..t1 + out[t1..].find("\n}\n").unwrap()];
        assert!(t1_block.contains("pub s: PublicStatus,"), "T1 block:\n{t1_block}");
        let t2 = out.find("pub struct T2").unwrap();
        let t2_block = &out[t2..t2 + out[t2..].find("\n}\n").unwrap()];
        assert!(t2_block.contains("pub s: AuditStatus,"), "T2 block:\n{t2_block}");
    }

    #[test]
    fn emit_enum_uses_non_public_schema_name() {
        // When the user runs codegen against a non-public schema, the format
        // prefix matches and gets stripped.
        let json = serde_json::json!({
            "definitions": {
                "items": {
                    "required": ["s"],
                    "properties": {
                        "s": { "format": "inventory.bin_state", "type": "string", "enum": ["full"] }
                    }
                }
            }
        });
        let api: OpenApi = serde_json::from_value(json).unwrap();
        let opts = Options { schema: "inventory".into(), ..Options::default() };
        let out = emit(&api, &opts);
        assert!(out.contains("pub enum BinState"), "missing stripped enum name:\n{out}");
        assert!(out.contains("pub s: BinState,"));
    }
}
