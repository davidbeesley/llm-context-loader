use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use syn::{File, Item, parse_file};

/// Represents different kinds of Rust code entities
#[derive(Debug, Clone, PartialEq)]
pub enum EntityKind {
    Function,
    Struct,
    Enum,
    Trait,
    TraitImpl,
    Module,
    Constant,
    TypeAlias,
    Macro,
}

/// Visibility of a code entity
#[derive(Debug, Clone, PartialEq)]
pub enum Visibility {
    Public,
    Private,
    Crate,
    Restricted(String), // pub(in path)
}

/// Represents a code entity extracted from Rust source
#[derive(Debug, Clone)]
pub struct CodeEntity {
    pub kind: EntityKind,
    pub name: String,
    pub path: String, // Module path
    pub visibility: Visibility,
    pub signature: String,
    pub body: Option<String>,
    pub doc_comments: Vec<String>,
    pub file_path: PathBuf,
    pub line: usize,
    pub column: usize,
}

/// Parser for Rust source code
pub struct RustParser {
    /// Root directory for resolving relative paths
    project_root: PathBuf,
}

impl RustParser {
    /// Create a new parser instance
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// Parse a Rust source file and extract entities
    pub fn parse_file(&self, file_path: &Path) -> Result<Vec<CodeEntity>> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        self.parse_source(&content, file_path)
    }

    /// Parse Rust source code and extract entities
    pub fn parse_source(&self, source: &str, file_path: &Path) -> Result<Vec<CodeEntity>> {
        let syntax_tree = parse_file(source)
            .map_err(|e| anyhow::anyhow!("Failed to parse Rust source: {}", e))?;

        let mut entities = Vec::new();
        self.extract_entities(&syntax_tree, &mut entities, file_path, "")?;

        Ok(entities)
    }

    /// Extract entities from a parsed file
    fn extract_entities(
        &self,
        file: &File,
        entities: &mut Vec<CodeEntity>,
        file_path: &Path,
        module_path: &str,
    ) -> Result<()> {
        for item in &file.items {
            match item {
                Item::Fn(item_fn) => {
                    let entity = self.extract_function(item_fn, file_path, module_path)?;
                    entities.push(entity);
                }
                Item::Struct(item_struct) => {
                    let entity = self.extract_struct(item_struct, file_path, module_path)?;
                    entities.push(entity);
                }
                Item::Enum(item_enum) => {
                    let entity = self.extract_enum(item_enum, file_path, module_path)?;
                    entities.push(entity);
                }
                Item::Trait(item_trait) => {
                    let entity = self.extract_trait(item_trait, file_path, module_path)?;
                    entities.push(entity);
                }
                Item::Impl(item_impl) => {
                    if let Some(entity) = self.extract_impl(item_impl, file_path, module_path)? {
                        entities.push(entity);
                    }
                }
                Item::Const(item_const) => {
                    let entity = self.extract_const(item_const, file_path, module_path)?;
                    entities.push(entity);
                }
                Item::Type(item_type) => {
                    let entity = self.extract_type_alias(item_type, file_path, module_path)?;
                    entities.push(entity);
                }
                Item::Mod(item_mod) => {
                    self.extract_module(item_mod, entities, file_path, module_path)?;
                }
                Item::Macro(item_macro) => {
                    if let Some(entity) = self.extract_macro(item_macro, file_path, module_path)? {
                        entities.push(entity);
                    }
                }
                _ => {} // Skip other items for now
            }
        }
        Ok(())
    }

    /// Extract a function entity
    fn extract_function(
        &self,
        item: &syn::ItemFn,
        file_path: &Path,
        module_path: &str,
    ) -> Result<CodeEntity> {
        let name = item.sig.ident.to_string();
        let signature = quote::quote!(#item.sig).to_string();
        let body = quote::quote!(#item.block).to_string();
        let visibility = self.extract_visibility(&item.vis);
        let doc_comments = self.extract_doc_comments(&item.attrs);

        let span = item.sig.ident.span();
        let (line, column) = self.get_line_column(span);

        Ok(CodeEntity {
            kind: EntityKind::Function,
            name,
            path: self.build_path(module_path, &item.sig.ident.to_string()),
            visibility,
            signature,
            body: Some(body),
            doc_comments,
            file_path: file_path.to_path_buf(),
            line,
            column,
        })
    }

    /// Extract a struct entity
    fn extract_struct(
        &self,
        item: &syn::ItemStruct,
        file_path: &Path,
        module_path: &str,
    ) -> Result<CodeEntity> {
        let name = item.ident.to_string();
        let signature = format!("struct {}", name);
        let visibility = self.extract_visibility(&item.vis);
        let doc_comments = self.extract_doc_comments(&item.attrs);

        let span = item.ident.span();
        let (line, column) = self.get_line_column(span);

        Ok(CodeEntity {
            kind: EntityKind::Struct,
            name,
            path: self.build_path(module_path, &item.ident.to_string()),
            visibility,
            signature,
            body: None,
            doc_comments,
            file_path: file_path.to_path_buf(),
            line,
            column,
        })
    }

    /// Extract an enum entity
    fn extract_enum(
        &self,
        item: &syn::ItemEnum,
        file_path: &Path,
        module_path: &str,
    ) -> Result<CodeEntity> {
        let name = item.ident.to_string();
        let signature = format!("enum {}", name);
        let visibility = self.extract_visibility(&item.vis);
        let doc_comments = self.extract_doc_comments(&item.attrs);

        let span = item.ident.span();
        let (line, column) = self.get_line_column(span);

        Ok(CodeEntity {
            kind: EntityKind::Enum,
            name,
            path: self.build_path(module_path, &item.ident.to_string()),
            visibility,
            signature,
            body: None,
            doc_comments,
            file_path: file_path.to_path_buf(),
            line,
            column,
        })
    }

    /// Extract a trait entity
    fn extract_trait(
        &self,
        item: &syn::ItemTrait,
        file_path: &Path,
        module_path: &str,
    ) -> Result<CodeEntity> {
        let name = item.ident.to_string();
        let signature = format!("trait {}", name);
        let visibility = self.extract_visibility(&item.vis);
        let doc_comments = self.extract_doc_comments(&item.attrs);

        let span = item.ident.span();
        let (line, column) = self.get_line_column(span);

        Ok(CodeEntity {
            kind: EntityKind::Trait,
            name,
            path: self.build_path(module_path, &item.ident.to_string()),
            visibility,
            signature,
            body: None,
            doc_comments,
            file_path: file_path.to_path_buf(),
            line,
            column,
        })
    }

    /// Extract impl blocks (trait implementations)
    fn extract_impl(
        &self,
        item: &syn::ItemImpl,
        file_path: &Path,
        module_path: &str,
    ) -> Result<Option<CodeEntity>> {
        // Only extract trait implementations for now
        if let Some((_, trait_path, _)) = &item.trait_ {
            let trait_name = quote::quote!(#trait_path).to_string();
            let self_ty = quote::quote!(#item.self_ty).to_string();
            let name = format!("{} for {}", trait_name, self_ty);
            let signature = format!("impl {} for {}", trait_name, self_ty);

            let span = item.impl_token.span;
            let (line, column) = self.get_line_column(span);

            Ok(Some(CodeEntity {
                kind: EntityKind::TraitImpl,
                name,
                path: module_path.to_string(),
                visibility: Visibility::Private, // Impls don't have visibility
                signature,
                body: None,
                doc_comments: self.extract_doc_comments(&item.attrs),
                file_path: file_path.to_path_buf(),
                line,
                column,
            }))
        } else {
            Ok(None)
        }
    }

    /// Extract a constant entity
    fn extract_const(
        &self,
        item: &syn::ItemConst,
        file_path: &Path,
        module_path: &str,
    ) -> Result<CodeEntity> {
        let name = item.ident.to_string();
        let signature = quote::quote!(#item).to_string();
        let visibility = self.extract_visibility(&item.vis);
        let doc_comments = self.extract_doc_comments(&item.attrs);

        let span = item.ident.span();
        let (line, column) = self.get_line_column(span);

        Ok(CodeEntity {
            kind: EntityKind::Constant,
            name,
            path: self.build_path(module_path, &item.ident.to_string()),
            visibility,
            signature,
            body: None,
            doc_comments,
            file_path: file_path.to_path_buf(),
            line,
            column,
        })
    }

    /// Extract a type alias entity
    fn extract_type_alias(
        &self,
        item: &syn::ItemType,
        file_path: &Path,
        module_path: &str,
    ) -> Result<CodeEntity> {
        let name = item.ident.to_string();
        let signature = quote::quote!(#item).to_string();
        let visibility = self.extract_visibility(&item.vis);
        let doc_comments = self.extract_doc_comments(&item.attrs);

        let span = item.ident.span();
        let (line, column) = self.get_line_column(span);

        Ok(CodeEntity {
            kind: EntityKind::TypeAlias,
            name,
            path: self.build_path(module_path, &item.ident.to_string()),
            visibility,
            signature,
            body: None,
            doc_comments,
            file_path: file_path.to_path_buf(),
            line,
            column,
        })
    }

    /// Extract entities from a module
    fn extract_module(
        &self,
        item: &syn::ItemMod,
        entities: &mut Vec<CodeEntity>,
        file_path: &Path,
        parent_path: &str,
    ) -> Result<()> {
        let module_name = item.ident.to_string();
        let module_path = self.build_path(parent_path, &module_name);

        // Add the module itself as an entity
        let span = item.ident.span();
        let (line, column) = self.get_line_column(span);

        entities.push(CodeEntity {
            kind: EntityKind::Module,
            name: module_name.clone(),
            path: module_path.clone(),
            visibility: self.extract_visibility(&item.vis),
            signature: format!("mod {}", module_name),
            body: None,
            doc_comments: self.extract_doc_comments(&item.attrs),
            file_path: file_path.to_path_buf(),
            line,
            column,
        });

        // Extract entities from module content if inline
        if let Some((_, items)) = &item.content {
            for inner_item in items {
                match inner_item {
                    Item::Fn(item_fn) => {
                        let entity = self.extract_function(item_fn, file_path, &module_path)?;
                        entities.push(entity);
                    }
                    Item::Struct(item_struct) => {
                        let entity = self.extract_struct(item_struct, file_path, &module_path)?;
                        entities.push(entity);
                    }
                    Item::Enum(item_enum) => {
                        let entity = self.extract_enum(item_enum, file_path, &module_path)?;
                        entities.push(entity);
                    }
                    Item::Trait(item_trait) => {
                        let entity = self.extract_trait(item_trait, file_path, &module_path)?;
                        entities.push(entity);
                    }
                    Item::Impl(item_impl) => {
                        if let Some(entity) =
                            self.extract_impl(item_impl, file_path, &module_path)?
                        {
                            entities.push(entity);
                        }
                    }
                    Item::Const(item_const) => {
                        let entity = self.extract_const(item_const, file_path, &module_path)?;
                        entities.push(entity);
                    }
                    Item::Type(item_type) => {
                        let entity = self.extract_type_alias(item_type, file_path, &module_path)?;
                        entities.push(entity);
                    }
                    Item::Mod(item_mod) => {
                        self.extract_module(item_mod, entities, file_path, &module_path)?;
                    }
                    Item::Macro(item_macro) => {
                        if let Some(entity) =
                            self.extract_macro(item_macro, file_path, &module_path)?
                        {
                            entities.push(entity);
                        }
                    }
                    _ => {} // Skip other items
                }
            }
        }

        Ok(())
    }

    /// Extract visibility from syn visibility
    fn extract_visibility(&self, vis: &syn::Visibility) -> Visibility {
        match vis {
            syn::Visibility::Public(_) => Visibility::Public,
            syn::Visibility::Restricted(restricted) => {
                if restricted.path.is_ident("crate") {
                    Visibility::Crate
                } else {
                    Visibility::Restricted(quote::quote!(#restricted).to_string())
                }
            }
            syn::Visibility::Inherited => Visibility::Private,
        }
    }

    /// Extract documentation comments from attributes
    fn extract_doc_comments(&self, attrs: &[syn::Attribute]) -> Vec<String> {
        attrs
            .iter()
            .filter(|attr| attr.path().is_ident("doc"))
            .filter_map(|attr| {
                if let syn::Meta::NameValue(meta) = &attr.meta {
                    if let syn::Expr::Lit(expr_lit) = &meta.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            return Some(lit_str.value().trim().to_string());
                        }
                    }
                }
                None
            })
            .collect()
    }

    /// Build a full module path
    fn build_path(&self, parent: &str, name: &str) -> String {
        if parent.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", parent, name)
        }
    }

    /// Extract a macro definition
    fn extract_macro(
        &self,
        item: &syn::ItemMacro,
        file_path: &Path,
        module_path: &str,
    ) -> Result<Option<CodeEntity>> {
        // Only extract macro_rules! for now
        if item.mac.path.is_ident("macro_rules") {
            // Extract the macro name from the ident
            if let Some(ident) = &item.ident {
                let name = ident.to_string();
                let signature = format!("macro_rules! {}", name);
                let body = Some(quote::quote!(#item).to_string());
                let doc_comments = self.extract_doc_comments(&item.attrs);

                let span = ident.span();
                let (line, column) = self.get_line_column(span);

                return Ok(Some(CodeEntity {
                    kind: EntityKind::Macro,
                    name,
                    path: self.build_path(module_path, &ident.to_string()),
                    visibility: Visibility::Private, // macro_rules are module-private by default
                    signature,
                    body,
                    doc_comments,
                    file_path: file_path.to_path_buf(),
                    line,
                    column,
                }));
            }
        }
        Ok(None)
    }

    /// Get line and column from a span (placeholder - real impl would use source map)
    fn get_line_column(&self, _span: proc_macro2::Span) -> (usize, usize) {
        // TODO: Implement proper line/column tracking with source map
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_function() {
        let source = r#"
            /// This is a test function
            pub fn test_function(x: i32) -> i32 {
                x + 1
            }
        "#;

        let parser = RustParser::new(".");
        let entities = parser.parse_source(source, Path::new("test.rs")).unwrap();

        assert_eq!(entities.len(), 1);
        let entity = &entities[0];
        assert_eq!(entity.kind, EntityKind::Function);
        assert_eq!(entity.name, "test_function");
        assert_eq!(entity.visibility, Visibility::Public);
        assert_eq!(entity.doc_comments, vec!["This is a test function"]);
    }

    #[test]
    fn test_parse_struct() {
        let source = r#"
            /// A test struct
            pub struct TestStruct {
                field: String,
            }
        "#;

        let parser = RustParser::new(".");
        let entities = parser.parse_source(source, Path::new("test.rs")).unwrap();

        assert_eq!(entities.len(), 1);
        let entity = &entities[0];
        assert_eq!(entity.kind, EntityKind::Struct);
        assert_eq!(entity.name, "TestStruct");
        assert_eq!(entity.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_module() {
        let source = r#"
            mod inner {
                pub fn inner_function() {}
            }
        "#;

        let parser = RustParser::new(".");
        let entities = parser.parse_source(source, Path::new("test.rs")).unwrap();

        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].kind, EntityKind::Module);
        assert_eq!(entities[0].name, "inner");
        assert_eq!(entities[1].kind, EntityKind::Function);
        assert_eq!(entities[1].name, "inner_function");
        assert_eq!(entities[1].path, "inner::inner_function");
    }

    #[test]
    fn test_parse_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "pub fn test() {{}}").unwrap();

        let parser = RustParser::new(".");
        let entities = parser.parse_file(temp_file.path()).unwrap();

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].kind, EntityKind::Function);
        assert_eq!(entities[0].name, "test");
    }
}
