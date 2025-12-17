//! Implementation of the `#[mcp_server]` attribute macro.
//!
//! This is the main macro that generates the full MCP server implementation.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, FnArg, ImplItem, ImplItemFn, ItemImpl, Result, parse2};

use crate::attrs::{PromptAttrs, ResourceAttrs, ServerAttrs, ToolAttrs};
use crate::codegen::{ToolMethod, ToolParam, extract_param, is_result_type};

/// Information about a resource method extracted from the AST.
#[derive(Debug)]
struct ResourceMethod {
    /// The method name
    name: syn::Ident,
    /// The URI pattern for this resource
    uri_pattern: String,
    /// Human-readable name
    resource_name: String,
    /// Resource description
    description: String,
    /// MIME type
    mime_type: String,
    /// Whether the method is async
    is_async: bool,
    /// Whether the return type is Result
    returns_result: bool,
}

/// Information about a prompt method extracted from the AST.
#[derive(Debug)]
struct PromptMethod {
    /// The method name
    name: syn::Ident,
    /// The prompt name (may be overridden by attribute)
    prompt_name: String,
    /// The description
    description: String,
    /// The parameters (excluding &self)
    params: Vec<PromptParam>,
    /// Whether the method is async
    is_async: bool,
    /// Whether the return type is Result
    returns_result: bool,
}

/// Information about a prompt parameter.
#[derive(Debug)]
struct PromptParam {
    /// The parameter name
    name: syn::Ident,
    /// The parameter type
    ty: syn::Type,
    /// Documentation comment (becomes description)
    doc: Option<String>,
    /// Whether the parameter is optional (Option<T>)
    is_optional: bool,
}

/// Expand the `#[mcp_server]` attribute macro.
pub fn expand_mcp_server(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Parse attributes
    let attrs =
        ServerAttrs::parse(attr).map_err(|e| Error::new(proc_macro2::Span::call_site(), e))?;

    // Parse the impl block
    let mut impl_block: ItemImpl = parse2(item)?;

    // Find all tool methods
    let tool_methods = extract_tool_methods(&mut impl_block)?;

    // Find all resource methods
    let resource_methods = extract_resource_methods(&mut impl_block)?;

    // Find all prompt methods
    let prompt_methods = extract_prompt_methods(&mut impl_block)?;

    // Extract the type name
    let self_ty = &impl_block.self_ty;

    // Generate ServerHandler impl with correct capabilities
    let server_handler_impl = generate_server_handler(
        &attrs,
        self_ty,
        !tool_methods.is_empty(),
        !resource_methods.is_empty(),
        !prompt_methods.is_empty(),
    );

    // Generate ToolHandler impl if there are any tools
    let tool_handler_impl = if tool_methods.is_empty() {
        quote!()
    } else {
        generate_tool_handler(&tool_methods, self_ty)
    };

    // Generate ResourceHandler impl if there are any resources
    let resource_handler_impl = if resource_methods.is_empty() {
        quote!()
    } else {
        generate_resource_handler(&resource_methods, self_ty)
    };

    // Generate PromptHandler impl if there are any prompts
    let prompt_handler_impl = if prompt_methods.is_empty() {
        quote!()
    } else {
        generate_prompt_handler(&prompt_methods, self_ty)
    };

    // Generate convenience methods
    let convenience_methods = generate_convenience_methods(
        self_ty,
        !tool_methods.is_empty(),
        !resource_methods.is_empty(),
        !prompt_methods.is_empty(),
    );

    // Debug output if requested
    if attrs.debug_expand {
        eprintln!("=== Generated code for {} ===", quote!(#self_ty));
        eprintln!("{server_handler_impl}");
        eprintln!("{tool_handler_impl}");
        eprintln!("{resource_handler_impl}");
        eprintln!("{prompt_handler_impl}");
        eprintln!("=== End generated code ===");
    }

    // Combine everything
    Ok(quote! {
        #impl_block

        #server_handler_impl

        #tool_handler_impl

        #resource_handler_impl

        #prompt_handler_impl

        #convenience_methods
    })
}

/// Extract tool methods from the impl block.
fn extract_tool_methods(impl_block: &mut ItemImpl) -> Result<Vec<ToolMethod>> {
    let mut tools = Vec::new();

    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Check for #[tool] attribute
            if let Some((idx, tool_attrs)) = find_tool_attr(&method.attrs)? {
                // Remove the #[tool] attribute so it doesn't cause errors
                method.attrs.remove(idx);

                let tool = extract_tool_info(method, tool_attrs)?;
                tools.push(tool);
            }
        }
    }

    Ok(tools)
}

/// Find the #[tool] attribute and parse it.
fn find_tool_attr(attrs: &[Attribute]) -> Result<Option<(usize, ToolAttrs)>> {
    for (idx, attr) in attrs.iter().enumerate() {
        if attr.path().is_ident("tool") {
            let tokens = match &attr.meta {
                syn::Meta::List(list) => list.tokens.clone(),
                syn::Meta::Path(_) => {
                    return Err(Error::new_spanned(
                        attr,
                        "missing tool attributes\n\
                         help: add description, e.g., #[tool(description = \"...\")]",
                    ));
                }
                syn::Meta::NameValue(_) => {
                    return Err(Error::new_spanned(
                        attr,
                        "invalid #[tool] syntax\n\
                         help: use #[tool(description = \"...\")]",
                    ));
                }
            };

            let tool_attrs = ToolAttrs::parse(tokens)
                .map_err(|e| Error::new(attr.bracket_token.span.join(), e))?;

            return Ok(Some((idx, tool_attrs)));
        }
    }
    Ok(None)
}

/// Extract tool information from a method.
#[allow(clippy::unnecessary_wraps)] // Returns Result for future validation extensibility
fn extract_tool_info(method: &ImplItemFn, attrs: ToolAttrs) -> Result<ToolMethod> {
    let name = method.sig.ident.clone();
    let tool_name = attrs.name.unwrap_or_else(|| name.to_string());

    // Extract parameters (skip &self)
    let params: Vec<ToolParam> = method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(_) => extract_param(arg),
        })
        .collect();

    let is_async = method.sig.asyncness.is_some();
    let returns_result = is_result_type(&method.sig.output);

    Ok(ToolMethod {
        name,
        tool_name,
        description: attrs.description,
        destructive: attrs.destructive,
        idempotent: attrs.idempotent,
        read_only: attrs.read_only,
        params,
        is_async,
        returns_result,
    })
}

/// Extract resource methods from the impl block.
fn extract_resource_methods(impl_block: &mut ItemImpl) -> Result<Vec<ResourceMethod>> {
    let mut resources = Vec::new();

    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Check for #[resource] attribute
            if let Some((idx, resource_attrs)) = find_resource_attr(&method.attrs)? {
                // Remove the #[resource] attribute so it doesn't cause errors
                method.attrs.remove(idx);

                let resource = extract_resource_info(method, resource_attrs)?;
                resources.push(resource);
            }
        }
    }

    Ok(resources)
}

/// Find the #[resource] attribute and parse it.
fn find_resource_attr(attrs: &[Attribute]) -> Result<Option<(usize, ResourceAttrs)>> {
    for (idx, attr) in attrs.iter().enumerate() {
        if attr.path().is_ident("resource") {
            let tokens = match &attr.meta {
                syn::Meta::List(list) => list.tokens.clone(),
                syn::Meta::Path(_) => {
                    return Err(Error::new_spanned(
                        attr,
                        "missing resource attributes\n\
                         help: add uri_pattern, e.g., #[resource(uri_pattern = \"myserver://data/{id}\")]",
                    ));
                }
                syn::Meta::NameValue(_) => {
                    return Err(Error::new_spanned(
                        attr,
                        "invalid #[resource] syntax\n\
                         help: use #[resource(uri_pattern = \"...\")]",
                    ));
                }
            };

            let resource_attrs = ResourceAttrs::parse(tokens)
                .map_err(|e| Error::new(attr.bracket_token.span.join(), e))?;

            return Ok(Some((idx, resource_attrs)));
        }
    }
    Ok(None)
}

/// Extract resource information from a method.
#[allow(clippy::unnecessary_wraps)] // Returns Result for future validation extensibility
fn extract_resource_info(method: &ImplItemFn, attrs: ResourceAttrs) -> Result<ResourceMethod> {
    let name = method.sig.ident.clone();
    let resource_name = attrs.name.unwrap_or_else(|| name.to_string());

    let is_async = method.sig.asyncness.is_some();
    let returns_result = is_result_type(&method.sig.output);

    Ok(ResourceMethod {
        name,
        uri_pattern: attrs.uri_pattern,
        resource_name,
        description: attrs.description.unwrap_or_default(),
        mime_type: attrs.mime_type.unwrap_or_else(|| "text/plain".to_string()),
        is_async,
        returns_result,
    })
}

/// Extract prompt methods from the impl block.
fn extract_prompt_methods(impl_block: &mut ItemImpl) -> Result<Vec<PromptMethod>> {
    let mut prompts = Vec::new();

    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Check for #[prompt] attribute
            if let Some((idx, prompt_attrs)) = find_prompt_attr(&method.attrs)? {
                // Remove the #[prompt] attribute so it doesn't cause errors
                method.attrs.remove(idx);

                let prompt = extract_prompt_info(method, prompt_attrs)?;
                prompts.push(prompt);
            }
        }
    }

    Ok(prompts)
}

/// Find the #[prompt] attribute and parse it.
fn find_prompt_attr(attrs: &[Attribute]) -> Result<Option<(usize, PromptAttrs)>> {
    for (idx, attr) in attrs.iter().enumerate() {
        if attr.path().is_ident("prompt") {
            let tokens = match &attr.meta {
                syn::Meta::List(list) => list.tokens.clone(),
                syn::Meta::Path(_) => {
                    return Err(Error::new_spanned(
                        attr,
                        "missing prompt attributes\n\
                         help: add description, e.g., #[prompt(description = \"...\")]",
                    ));
                }
                syn::Meta::NameValue(_) => {
                    return Err(Error::new_spanned(
                        attr,
                        "invalid #[prompt] syntax\n\
                         help: use #[prompt(description = \"...\")]",
                    ));
                }
            };

            let prompt_attrs = PromptAttrs::parse(tokens)
                .map_err(|e| Error::new(attr.bracket_token.span.join(), e))?;

            return Ok(Some((idx, prompt_attrs)));
        }
    }
    Ok(None)
}

/// Extract prompt information from a method.
#[allow(clippy::unnecessary_wraps)] // Returns Result for future validation extensibility
fn extract_prompt_info(method: &ImplItemFn, attrs: PromptAttrs) -> Result<PromptMethod> {
    let name = method.sig.ident.clone();
    let prompt_name = attrs.name.unwrap_or_else(|| name.to_string());

    // Extract parameters (skip &self)
    let params: Vec<PromptParam> = method
        .sig
        .inputs
        .iter()
        .filter_map(extract_prompt_param)
        .collect();

    let is_async = method.sig.asyncness.is_some();
    let returns_result = is_result_type(&method.sig.output);

    Ok(PromptMethod {
        name,
        prompt_name,
        description: attrs.description,
        params,
        is_async,
        returns_result,
    })
}

/// Extract parameter information from a function argument for prompts.
fn extract_prompt_param(arg: &FnArg) -> Option<PromptParam> {
    match arg {
        FnArg::Typed(syn::PatType { pat, ty, attrs, .. }) => {
            // Get the parameter name
            let name = match pat.as_ref() {
                syn::Pat::Ident(syn::PatIdent { ident, .. }) => ident.clone(),
                _ => return None,
            };

            // Extract doc comment
            let doc = attrs
                .iter()
                .filter_map(|attr| {
                    if attr.path().is_ident("doc") {
                        if let syn::Meta::NameValue(nv) = &attr.meta {
                            if let syn::Expr::Lit(lit) = &nv.value {
                                if let syn::Lit::Str(s) = &lit.lit {
                                    return Some(s.value().trim().to_string());
                                }
                            }
                        }
                    }
                    None
                })
                .collect::<Vec<_>>()
                .join(" ");

            let doc = if doc.is_empty() { None } else { Some(doc) };

            // Check if optional
            let is_optional = is_option_type(ty);

            Some(PromptParam {
                name,
                ty: (**ty).clone(),
                doc,
                is_optional,
            })
        }
        FnArg::Receiver(_) => None,
    }
}

/// Check if a type is Option<T>.
fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Extract the inner type from Option<T>.
/// Returns the inner type T, or the original type if not an Option.
fn extract_option_inner_type(ty: &syn::Type) -> syn::Type {
    if let syn::Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return inner.clone();
                    }
                }
            }
        }
    }
    // Fallback: return the original type (shouldn't happen if is_option_type returned true)
    ty.clone()
}

/// Generate the `ServerHandler` implementation.
fn generate_server_handler(
    attrs: &ServerAttrs,
    self_ty: &syn::Type,
    has_tools: bool,
    has_resources: bool,
    has_prompts: bool,
) -> TokenStream {
    let name = &attrs.name;
    let version = &attrs.version;
    let instructions = attrs
        .instructions
        .as_ref()
        .map_or_else(|| quote!(None), |s| quote!(Some(#s.to_string())));

    // Build capabilities chain based on what's implemented
    let mut capability_chain = vec![quote!(::mcpkit::capability::ServerCapabilities::new())];

    if has_tools {
        capability_chain.push(quote!(.with_tools()));
    }
    if has_resources {
        capability_chain.push(quote!(.with_resources()));
    }
    if has_prompts {
        capability_chain.push(quote!(.with_prompts()));
    }

    // Join the capability chain
    let capabilities = if capability_chain.len() == 1 {
        quote!(::mcpkit::capability::ServerCapabilities::new())
    } else {
        let mut result = capability_chain[0].clone();
        for cap in &capability_chain[1..] {
            result = quote!(#result #cap);
        }
        result
    };

    quote! {
        impl ::mcpkit::ServerHandler for #self_ty {
            fn server_info(&self) -> ::mcpkit::capability::ServerInfo {
                ::mcpkit::capability::ServerInfo::new(#name, #version)
            }

            fn capabilities(&self) -> ::mcpkit::capability::ServerCapabilities {
                #capabilities
            }

            fn instructions(&self) -> Option<String> {
                #instructions
            }
        }
    }
}

/// Generate the `ToolHandler` implementation.
fn generate_tool_handler(tools: &[ToolMethod], self_ty: &syn::Type) -> TokenStream {
    // Generate tool definitions
    let tool_defs: Vec<_> = tools
        .iter()
        .map(|tool| {
            let name = &tool.tool_name;
            let description = &tool.description;
            let input_schema = tool.generate_input_schema();

            // Generate annotations
            let destructive = tool.destructive;
            let idempotent = tool.idempotent;
            let read_only = tool.read_only;

            quote! {
                ::mcpkit::types::Tool {
                    name: #name.to_string(),
                    description: Some(#description.to_string()),
                    input_schema: #input_schema,
                    annotations: Some(::mcpkit::types::ToolAnnotations {
                        title: None,
                        read_only_hint: Some(#read_only),
                        destructive_hint: Some(#destructive),
                        idempotent_hint: Some(#idempotent),
                        open_world_hint: None,
                    }),
                }
            }
        })
        .collect();

    // Generate dispatch arms
    let dispatch_arms: Vec<_> = tools
        .iter()
        .map(super::codegen::ToolMethod::generate_call_dispatch)
        .collect();

    // Get the list of tool names for error message
    let tool_names: Vec<_> = tools.iter().map(|t| t.tool_name.as_str()).collect();
    let _available_tools = tool_names.join(", ");

    quote! {
        impl ::mcpkit::ToolHandler for #self_ty {
            fn list_tools(
                &self,
                _ctx: &::mcpkit::Context,
            ) -> impl std::future::Future<Output = Result<Vec<::mcpkit::types::Tool>, ::mcpkit::error::McpError>> + Send {
                async move {
                    Ok(vec![
                        #(#tool_defs),*
                    ])
                }
            }

            fn call_tool(
                &self,
                name: &str,
                args: ::serde_json::Value,
                _ctx: &::mcpkit::Context,
            ) -> impl std::future::Future<Output = Result<::mcpkit::types::ToolOutput, ::mcpkit::error::McpError>> + Send {
                // Convert args to a map for easier access
                let args_clone = args.clone();

                async move {
                    let args = match args_clone.as_object() {
                        Some(obj) => obj.clone(),
                        None => ::serde_json::Map::new(),
                    };

                    match name {
                        #(#dispatch_arms)*
                        _ => Err(::mcpkit::error::McpError::method_not_found_with_suggestions(
                            name,
                            vec![#(#tool_names.to_string()),*],
                        ))
                    }
                }
            }
        }
    }
}

/// Generate the `ResourceHandler` implementation.
fn generate_resource_handler(resources: &[ResourceMethod], self_ty: &syn::Type) -> TokenStream {
    // Generate static resource definitions (non-template URIs)
    let resource_defs: Vec<_> = resources
        .iter()
        .filter_map(|resource| {
            let uri = &resource.uri_pattern;
            let name = &resource.resource_name;
            let description = &resource.description;
            let mime_type = &resource.mime_type;

            // Only include non-template resources in list_resources
            if uri.contains('{') {
                None
            } else {
                Some(quote! {
                    ::mcpkit::types::Resource {
                        uri: #uri.to_string(),
                        name: #name.to_string(),
                        description: if #description.is_empty() { None } else { Some(#description.to_string()) },
                        mime_type: Some(#mime_type.to_string()),
                        size: None,
                        annotations: None,
                    },
                })
            }
        })
        .collect();

    // Generate template definitions (URIs with {param} placeholders)
    let template_defs: Vec<_> = resources
        .iter()
        .filter_map(|resource| {
            let uri = &resource.uri_pattern;
            let name = &resource.resource_name;
            let description = &resource.description;
            let mime_type = &resource.mime_type;

            // Only include template resources in list_resource_templates
            if uri.contains('{') {
                Some(quote! {
                    ::mcpkit::types::ResourceTemplate {
                        uri_template: #uri.to_string(),
                        name: #name.to_string(),
                        description: if #description.is_empty() { None } else { Some(#description.to_string()) },
                        mime_type: Some(#mime_type.to_string()),
                        annotations: None,
                    },
                })
            } else {
                None
            }
        })
        .collect();

    // Generate dispatch code for read_resource
    let dispatch_arms: Vec<_> = resources
        .iter()
        .map(|resource| {
            let method_name = &resource.name;
            let uri_pattern = &resource.uri_pattern;

            let call = if resource.is_async {
                quote!(self.#method_name(uri).await)
            } else {
                quote!(self.#method_name(uri))
            };

            // Check if this is a template pattern
            if uri_pattern.contains('{') {
                // Template matching - match by prefix
                let pattern_prefix = uri_pattern.split('{').next().unwrap_or("");
                if resource.returns_result {
                    // Method returns Result, use ? to propagate errors
                    quote! {
                        if uri.starts_with(#pattern_prefix) {
                            let result = #call?;
                            return Ok(vec![result]);
                        }
                    }
                } else {
                    // Method returns value directly, no need for ?
                    quote! {
                        if uri.starts_with(#pattern_prefix) {
                            let result = #call;
                            return Ok(vec![result]);
                        }
                    }
                }
            } else {
                // Exact URI match
                if resource.returns_result {
                    // Method returns Result, use ? to propagate errors
                    quote! {
                        if uri == #uri_pattern {
                            let result = #call?;
                            return Ok(vec![result]);
                        }
                    }
                } else {
                    // Method returns value directly, no need for ?
                    quote! {
                        if uri == #uri_pattern {
                            let result = #call;
                            return Ok(vec![result]);
                        }
                    }
                }
            }
        })
        .collect();

    // URI patterns are available for future error message enhancement
    let _uri_patterns: Vec<_> = resources.iter().map(|r| r.uri_pattern.as_str()).collect();

    quote! {
        impl ::mcpkit::ResourceHandler for #self_ty {
            fn list_resources(
                &self,
                _ctx: &::mcpkit::Context,
            ) -> impl std::future::Future<Output = Result<Vec<::mcpkit::types::Resource>, ::mcpkit::error::McpError>> + Send {
                async move {
                    Ok(vec![
                        #(#resource_defs)*
                    ])
                }
            }

            fn list_resource_templates(
                &self,
                _ctx: &::mcpkit::Context,
            ) -> impl std::future::Future<Output = Result<Vec<::mcpkit::types::ResourceTemplate>, ::mcpkit::error::McpError>> + Send {
                async move {
                    Ok(vec![
                        #(#template_defs)*
                    ])
                }
            }

            fn read_resource(
                &self,
                uri: &str,
                _ctx: &::mcpkit::Context,
            ) -> impl std::future::Future<Output = Result<Vec<::mcpkit::types::ResourceContents>, ::mcpkit::error::McpError>> + Send {
                let uri_owned = uri.to_string();

                async move {
                    let uri: &str = &uri_owned;
                    #(#dispatch_arms)*

                    Err(::mcpkit::error::McpError::resource_not_found(uri))
                }
            }
        }
    }
}

/// Generate the `PromptHandler` implementation.
fn generate_prompt_handler(prompts: &[PromptMethod], self_ty: &syn::Type) -> TokenStream {
    // Generate prompt definitions
    let prompt_defs: Vec<_> = prompts
        .iter()
        .map(|prompt| {
            let name = &prompt.prompt_name;
            let description = &prompt.description;

            // Generate arguments
            let arguments: Vec<_> = prompt
                .params
                .iter()
                .map(|param| {
                    let param_name = param.name.to_string();
                    let param_desc = param.doc.as_deref().unwrap_or("");
                    let required = !param.is_optional;

                    quote! {
                        ::mcpkit::types::PromptArgument {
                            name: #param_name.to_string(),
                            description: if #param_desc.is_empty() { None } else { Some(#param_desc.to_string()) },
                            required: Some(#required),
                        }
                    }
                })
                .collect();

            let arguments_expr = if arguments.is_empty() {
                quote!(None)
            } else {
                quote!(Some(vec![#(#arguments),*]))
            };

            quote! {
                ::mcpkit::types::Prompt {
                    name: #name.to_string(),
                    description: if #description.is_empty() { None } else { Some(#description.to_string()) },
                    arguments: #arguments_expr,
                }
            }
        })
        .collect();

    // Generate dispatch arms for get_prompt
    let dispatch_arms: Vec<_> = prompts
        .iter()
        .map(|prompt| {
            let method_name = &prompt.name;
            let prompt_name = &prompt.prompt_name;

            // Generate parameter extraction
            let param_extractions: Vec<_> = prompt
                .params
                .iter()
                .map(|param| {
                    let name = &param.name;
                    let name_str = name.to_string();
                    let ty = &param.ty;

                    if param.is_optional {
                        // For Option<T> types, we need to extract the inner type for turbofish
                        // The type is Option<InnerType>, and we need to parse InnerType
                        let inner_ty = extract_option_inner_type(ty);
                        quote! {
                            let #name: #ty = match arguments
                                .as_ref()
                                .and_then(|args| args.get(#name_str))
                            {
                                Some(v) => ::serde_json::from_value::<#inner_ty>(v.clone()).ok(),
                                None => None,
                            };
                        }
                    } else {
                        // For required parameters, use turbofish to explicitly specify the type
                        quote! {
                            let #name: #ty = {
                                let value = match arguments
                                    .as_ref()
                                    .and_then(|args| args.get(#name_str))
                                {
                                    Some(v) => v.clone(),
                                    None => return Err(::mcpkit::error::McpError::invalid_params(
                                        #prompt_name,
                                        format!("missing required argument: {}", #name_str),
                                    )),
                                };
                                match ::serde_json::from_value::<#ty>(value) {
                                    Ok(v) => v,
                                    Err(e) => return Err(::mcpkit::error::McpError::invalid_params(
                                        #prompt_name,
                                        format!("invalid argument '{}': {}", #name_str, e),
                                    )),
                                }
                            };
                        }
                    }
                })
                .collect();

            let param_names: Vec<_> = prompt.params.iter().map(|p| &p.name).collect();

            let call = if prompt.is_async {
                quote!(self.#method_name(#(#param_names),*).await)
            } else {
                quote!(self.#method_name(#(#param_names),*))
            };

            let call_with_conversion = if prompt.returns_result {
                quote!(#call)
            } else {
                quote!(Ok(#call))
            };

            quote! {
                #prompt_name => {
                    #(#param_extractions)*
                    #call_with_conversion
                }
            }
        })
        .collect();

    // Get the list of prompt names for error message
    let prompt_names: Vec<_> = prompts.iter().map(|p| p.prompt_name.as_str()).collect();

    quote! {
        impl ::mcpkit::PromptHandler for #self_ty {
            fn list_prompts(
                &self,
                _ctx: &::mcpkit::Context,
            ) -> impl std::future::Future<Output = Result<Vec<::mcpkit::types::Prompt>, ::mcpkit::error::McpError>> + Send {
                async move {
                    Ok(vec![
                        #(#prompt_defs),*
                    ])
                }
            }

            fn get_prompt(
                &self,
                name: &str,
                arguments: Option<::serde_json::Map<String, ::serde_json::Value>>,
                _ctx: &::mcpkit::Context,
            ) -> impl std::future::Future<Output = Result<::mcpkit::types::GetPromptResult, ::mcpkit::error::McpError>> + Send {
                let name = name.to_string();

                async move {
                    match name.as_str() {
                        #(#dispatch_arms)*
                        _ => Err(::mcpkit::error::McpError::method_not_found_with_suggestions(
                            &name,
                            vec![#(#prompt_names.to_string()),*],
                        ))
                    }
                }
            }
        }
    }
}

/// Generate convenience methods.
///
/// Generates an `into_server()` method that automatically wires up all handlers
/// defined via `#[tool]`, `#[resource]`, and `#[prompt]` attributes.
///
/// Uses `Arc` internally to share the handler across registrations, eliminating
/// the need for users to implement `Clone` on their handler types.
///
/// Note: We intentionally do NOT generate runtime-specific methods like `serve_stdio()`
/// because the SDK is runtime-agnostic. Users should create their own transport
/// and call `server.serve(transport)` directly.
fn generate_convenience_methods(
    self_ty: &syn::Type,
    has_tools: bool,
    has_resources: bool,
    has_prompts: bool,
) -> TokenStream {
    // Type alias for Arc<Self>
    let arc_self = quote!(::std::sync::Arc<Self>);

    // Determine the Server type parameters based on which handlers exist
    // Note: We use Arc<Self> as the handler type since we wrap in Arc internally
    let tools_ty = if has_tools {
        quote!(::mcpkit::server::Registered<#arc_self>)
    } else {
        quote!(::mcpkit::server::NotRegistered)
    };
    let resources_ty = if has_resources {
        quote!(::mcpkit::server::Registered<#arc_self>)
    } else {
        quote!(::mcpkit::server::NotRegistered)
    };
    let prompts_ty = if has_prompts {
        quote!(::mcpkit::server::Registered<#arc_self>)
    } else {
        quote!(::mcpkit::server::NotRegistered)
    };
    let tasks_ty = quote!(::mcpkit::server::NotRegistered);

    // Count handlers to determine if we need Arc
    let handler_count = [has_tools, has_resources, has_prompts]
        .iter()
        .filter(|&&x| x)
        .count();

    let builder_body = if handler_count == 0 {
        // No handlers - wrap in Arc for consistency
        quote! {
            let handler = ::std::sync::Arc::new(self);
            ::mcpkit::ServerBuilder::new(handler).build()
        }
    } else {
        // Generate the chain using Arc::clone()
        // Build the method chain: .with_tools(...).with_resources(...).with_prompts(...)
        let mut method_chain = quote!(::mcpkit::ServerBuilder::new(::std::sync::Arc::clone(
            &handler
        )));

        if has_tools {
            method_chain = quote!(#method_chain.with_tools(::std::sync::Arc::clone(&handler)));
        }
        if has_resources {
            method_chain = quote!(#method_chain.with_resources(::std::sync::Arc::clone(&handler)));
        }
        if has_prompts {
            method_chain = quote!(#method_chain.with_prompts(::std::sync::Arc::clone(&handler)));
        }

        quote! {
            let handler = ::std::sync::Arc::new(self);
            #method_chain.build()
        }
    };

    quote! {
        impl #self_ty {
            /// Convert this handler into a fully-configured MCP server.
            ///
            /// This method automatically registers all handlers defined on this type
            /// via `#[tool]`, `#[resource]`, and `#[prompt]` attributes.
            ///
            /// The handler is wrapped in `Arc` internally, so there's no need to
            /// implement `Clone` on your handler type.
            ///
            /// # Example
            ///
            /// ```ignore
            /// let server = MyServer::new().into_server();
            /// server.serve(transport).await?;
            /// ```
            #[must_use]
            pub fn into_server(self) -> ::mcpkit::server::Server<#arc_self, #tools_ty, #resources_ty, #prompts_ty, #tasks_ty>
            where
                Self: Send + Sync + 'static,
            {
                #builder_body
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_parse_server_attrs() {
        let tokens = quote!(name = "test", version = "1.0.0");
        let attrs = ServerAttrs::parse(tokens).unwrap();
        assert_eq!(attrs.name, "test");
        assert_eq!(attrs.version, "1.0.0");
    }
}
