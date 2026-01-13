//! Derive macros for mcpkit-template.
//!
//! This crate provides procedural macros for defining compile-time validated
//! prompt templates.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::path::PathBuf;
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, Ident, LitStr, Meta, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

/// Derive the `Template` trait for a struct.
///
/// Each field of the struct becomes a template variable. The template string
/// is specified via the `#[template(path = "...")]` or `#[template(source = "...")]`
/// attribute.
///
/// # Attributes
///
/// - `source = "..."`: Inline template string
/// - `path = "..."`: Path to template file (relative to `CARGO_MANIFEST_DIR`)
///
/// # Examples
///
/// ## Inline template
///
/// ```ignore
/// use mcpkit_template::Template;
///
/// #[derive(Template)]
/// #[template(source = "Hello, {{name}}! You are {{age}} years old.")]
/// struct Greeting {
///     name: String,
///     age: u32,
/// }
///
/// let greeting = Greeting { name: "Alice".into(), age: 30 };
/// assert_eq!(greeting.render(), "Hello, Alice! You are 30 years old.");
/// ```
///
/// ## Template from file
///
/// ```ignore
/// use mcpkit_template::Template;
///
/// // Template file at "templates/greeting.txt" contains:
/// // "Hello, {{name}}! Welcome to {{location}}."
///
/// #[derive(Template)]
/// #[template(path = "templates/greeting.txt")]
/// struct FileGreeting {
///     name: String,
///     location: String,
/// }
/// ```
#[proc_macro_derive(Template, attributes(template, var))]
pub fn derive_template(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match derive_template_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_template_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Parse template attribute
    let template_attr = parse_template_attr(&input.attrs)?;

    // Get fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input,
                    "Template derive only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "Template derive only supports structs",
            ));
        }
    };

    // Extract field names and validate against template
    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    // Parse variable attributes for custom formatting
    let var_attrs: Vec<_> = fields
        .iter()
        .map(|f| parse_var_attr(&f.attrs))
        .collect::<syn::Result<_>>()?;

    // Validate that all template variables have corresponding fields
    let template_vars = extract_template_variables(&template_attr.source);
    for var in &template_vars {
        if !field_names.iter().any(|f| f.to_string() == *var) {
            return Err(syn::Error::new_spanned(
                &input,
                format!(
                    "Template variable '{{{{{}}}}}' has no corresponding field",
                    var
                ),
            ));
        }
    }

    // Generate render implementation
    let render_impl = generate_render_impl(&template_attr.source, &field_names, &var_attrs);

    // Generate variables method
    let variables: Vec<_> = field_names.iter().map(|n| n.to_string()).collect();

    let expanded = quote! {
        impl #impl_generics ::mcpkit_template::Template for #name #ty_generics #where_clause {
            fn render(&self) -> String {
                #render_impl
            }

            fn variables() -> &'static [&'static str] {
                &[#(#variables),*]
            }

            fn source() -> &'static str {
                #template_attr
            }
        }

        impl #impl_generics #name #ty_generics #where_clause {
            /// Validate the template at compile time.
            #[allow(dead_code, path_statements, clippy::no_effect)]
            const fn __validate_template() {
                // Validation happens at compile time via type checking
            }
        }

        // Force compile-time validation
        const _: () = {
            #name::__validate_template();
        };
    };

    Ok(expanded)
}

struct TemplateAttr {
    source: String,
}

impl quote::ToTokens for TemplateAttr {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let source = &self.source;
        tokens.extend(quote! { #source });
    }
}

fn parse_template_attr(attrs: &[Attribute]) -> syn::Result<TemplateAttr> {
    for attr in attrs {
        if attr.path().is_ident("template") {
            let meta = &attr.meta;
            match meta {
                Meta::List(list) => {
                    let nested: TemplateArgs = syn::parse2(list.tokens.clone())?;
                    return Ok(TemplateAttr {
                        source: nested.source,
                    });
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "Expected #[template(source = \"...\")]",
                    ));
                }
            }
        }
    }

    Err(syn::Error::new(
        proc_macro2::Span::call_site(),
        "Missing #[template(source = \"...\")] or #[template(path = \"...\")] attribute",
    ))
}

struct TemplateArgs {
    source: String,
}

impl Parse for TemplateArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut source = None;
        let mut has_path = false;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let _: Token![=] = input.parse()?;

            if key == "source" {
                let value: LitStr = input.parse()?;
                if has_path {
                    return Err(syn::Error::new_spanned(
                        value,
                        "Cannot specify both 'source' and 'path' attributes",
                    ));
                }
                source = Some(value.value());
            } else if key == "path" {
                let value: LitStr = input.parse()?;
                if source.is_some() {
                    return Err(syn::Error::new_spanned(
                        value,
                        "Cannot specify both 'source' and 'path' attributes",
                    ));
                }
                // Resolve path relative to CARGO_MANIFEST_DIR
                let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
                    syn::Error::new_spanned(
                        &value,
                        "CARGO_MANIFEST_DIR not set; cannot resolve template path",
                    )
                })?;

                let template_path = PathBuf::from(&manifest_dir).join(value.value());

                // Read the file contents at compile time
                let content = std::fs::read_to_string(&template_path).map_err(|e| {
                    syn::Error::new_spanned(
                        &value,
                        format!(
                            "Failed to read template file '{}': {}",
                            template_path.display(),
                            e
                        ),
                    )
                })?;

                source = Some(content);
                has_path = true;
            } else {
                return Err(syn::Error::new_spanned(key, "Unknown template attribute"));
            }

            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(TemplateArgs {
            source: source.ok_or_else(|| {
                syn::Error::new(input.span(), "Missing 'source' or 'path' attribute")
            })?,
        })
    }
}

struct VarAttr {
    format: Option<String>,
}

fn parse_var_attr(attrs: &[Attribute]) -> syn::Result<VarAttr> {
    for attr in attrs {
        if attr.path().is_ident("var") {
            let meta = &attr.meta;
            match meta {
                Meta::List(list) => {
                    let nested: VarArgs = syn::parse2(list.tokens.clone())?;
                    return Ok(VarAttr {
                        format: nested.format,
                    });
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "Expected #[var(format = \"...\")]",
                    ));
                }
            }
        }
    }

    Ok(VarAttr { format: None })
}

struct VarArgs {
    format: Option<String>,
}

impl Parse for VarArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut format = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let _: Token![=] = input.parse()?;

            if key == "format" {
                let value: LitStr = input.parse()?;
                format = Some(value.value());
            } else {
                return Err(syn::Error::new_spanned(key, "Unknown var attribute"));
            }

            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(VarArgs { format })
    }
}

fn extract_template_variables(template: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second {
            let mut var_name = String::new();

            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        if !var_name.is_empty() {
                            vars.push(var_name);
                        }
                        break;
                    }
                } else {
                    var_name.push(c);
                    chars.next();
                }
            }
        }
    }

    vars
}

fn generate_render_impl(
    template: &str,
    field_names: &[&Ident],
    var_attrs: &[VarAttr],
) -> TokenStream2 {
    // Build format string and arguments
    let mut format_str = String::new();
    let mut format_args: Vec<TokenStream2> = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second {
            let mut var_name = String::new();

            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    if chars.peek() == Some(&'}') {
                        chars.next();

                        // Find the field index
                        if let Some(idx) =
                            field_names.iter().position(|f| f.to_string() == var_name)
                        {
                            let field = field_names[idx];
                            let var_attr = &var_attrs[idx];

                            if let Some(fmt) = &var_attr.format {
                                format_str.push_str(&format!("{{:{}}}", fmt));
                            } else {
                                format_str.push_str("{}");
                            }
                            format_args.push(quote! { &self.#field });
                        }
                        break;
                    }
                } else {
                    var_name.push(c);
                    chars.next();
                }
            }
        } else if c == '{' {
            format_str.push_str("{{");
        } else if c == '}' {
            format_str.push_str("}}");
        } else {
            format_str.push(c);
        }
    }

    quote! {
        format!(#format_str, #(#format_args),*)
    }
}

/// Declarative macro for defining simple templates inline.
///
/// # Example
///
/// ```ignore
/// use mcpkit_template::template;
///
/// let name = "World";
/// let result = template!("Hello, {{name}}!", name = name);
/// assert_eq!(result, "Hello, World!");
/// ```
#[proc_macro]
pub fn template(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as TemplateInvocation);

    match template_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

struct TemplateInvocation {
    template: LitStr,
    args: Vec<(Ident, Expr)>,
}

impl Parse for TemplateInvocation {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let template: LitStr = input.parse()?;
        let mut args = Vec::new();

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }

            let name: Ident = input.parse()?;
            let _: Token![=] = input.parse()?;
            let expr: Expr = input.parse()?;

            args.push((name, expr));
        }

        Ok(TemplateInvocation { template, args })
    }
}

fn template_impl(input: TemplateInvocation) -> syn::Result<TokenStream2> {
    let template_str = input.template.value();
    let template_vars = extract_template_variables(&template_str);

    // Validate that all template variables have arguments
    for var in &template_vars {
        if !input.args.iter().any(|(name, _)| name == var) {
            return Err(syn::Error::new_spanned(
                &input.template,
                format!(
                    "Template variable '{{{{{}}}}}' has no corresponding argument",
                    var
                ),
            ));
        }
    }

    // Build format string
    let mut format_str = String::new();
    let mut format_args: Vec<TokenStream2> = Vec::new();
    let mut chars = template_str.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next();
            let mut var_name = String::new();

            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    if chars.peek() == Some(&'}') {
                        chars.next();

                        if let Some((_, expr)) = input.args.iter().find(|(n, _)| *n == var_name) {
                            format_str.push_str("{}");
                            format_args.push(quote! { #expr });
                        }
                        break;
                    }
                } else {
                    var_name.push(c);
                    chars.next();
                }
            }
        } else if c == '{' {
            format_str.push_str("{{");
        } else if c == '}' {
            format_str.push_str("}}");
        } else {
            format_str.push(c);
        }
    }

    Ok(quote! {
        format!(#format_str, #(#format_args),*)
    })
}
