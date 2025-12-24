//! Implementation of the `#[mcp_client]` attribute macro.
//!
//! This macro generates the `ClientHandler` implementation for MCP clients.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, ImplItem, ItemImpl, Result, parse2};

use crate::attrs::ClientAttrs;
use crate::codegen::is_result_type;

/// Information about a handler method extracted from the AST.
#[derive(Debug)]
struct HandlerMethod {
    /// The method name
    name: syn::Ident,
    /// Whether the method is async
    is_async: bool,
    /// Whether the return type is Result
    returns_result: bool,
}

/// Expand the `#[mcp_client]` attribute macro.
pub fn expand_mcp_client(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Parse attributes
    let attrs =
        ClientAttrs::parse(attr).map_err(|e| Error::new(proc_macro2::Span::call_site(), e))?;

    // Parse the impl block
    let mut impl_block: ItemImpl = parse2(item)?;

    // Find handler methods
    let sampling_method = find_and_remove_handler(&mut impl_block, "sampling");
    let elicitation_method = find_and_remove_handler(&mut impl_block, "elicitation");
    let roots_method = find_and_remove_handler(&mut impl_block, "roots");

    // Find lifecycle hooks
    let on_connected_method = find_and_remove_handler(&mut impl_block, "on_connected");
    let on_disconnected_method = find_and_remove_handler(&mut impl_block, "on_disconnected");

    // Find notification handlers
    let on_task_progress_method = find_and_remove_handler(&mut impl_block, "on_task_progress");
    let on_resource_updated_method =
        find_and_remove_handler(&mut impl_block, "on_resource_updated");
    let on_tools_list_changed_method =
        find_and_remove_handler(&mut impl_block, "on_tools_list_changed");
    let on_resources_list_changed_method =
        find_and_remove_handler(&mut impl_block, "on_resources_list_changed");
    let on_prompts_list_changed_method =
        find_and_remove_handler(&mut impl_block, "on_prompts_list_changed");

    // Extract the type name
    let self_ty = &impl_block.self_ty;

    // Generate ClientHandler impl
    let client_handler_impl = generate_client_handler(
        self_ty,
        sampling_method.as_ref(),
        elicitation_method.as_ref(),
        roots_method.as_ref(),
        on_connected_method.as_ref(),
        on_disconnected_method.as_ref(),
        on_task_progress_method.as_ref(),
        on_resource_updated_method.as_ref(),
        on_tools_list_changed_method.as_ref(),
        on_resources_list_changed_method.as_ref(),
        on_prompts_list_changed_method.as_ref(),
    );

    // Generate convenience methods
    let convenience_methods = generate_client_convenience_methods(
        self_ty,
        sampling_method.is_some(),
        elicitation_method.is_some(),
        roots_method.is_some(),
    );

    // Debug output if requested
    if attrs.debug_expand {
        eprintln!("=== Generated code for {} ===", quote!(#self_ty));
        eprintln!("{client_handler_impl}");
        eprintln!("{convenience_methods}");
        eprintln!("=== End generated code ===");
    }

    // Combine everything
    Ok(quote! {
        #impl_block

        #client_handler_impl

        #convenience_methods
    })
}

/// Find and remove a handler method from the impl block.
fn find_and_remove_handler(impl_block: &mut ItemImpl, handler_name: &str) -> Option<HandlerMethod> {
    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            if let Some(idx) = find_handler_attr(&method.attrs, handler_name) {
                // Remove the handler attribute
                method.attrs.remove(idx);

                let is_async = method.sig.asyncness.is_some();
                let returns_result = is_result_type(&method.sig.output);

                return Some(HandlerMethod {
                    name: method.sig.ident.clone(),
                    is_async,
                    returns_result,
                });
            }
        }
    }
    None
}

/// Find a handler attribute by name.
fn find_handler_attr(attrs: &[Attribute], name: &str) -> Option<usize> {
    attrs.iter().position(|attr| attr.path().is_ident(name))
}

/// Generate the `ClientHandler` implementation.
#[allow(clippy::too_many_arguments)]
fn generate_client_handler(
    self_ty: &syn::Type,
    sampling_method: Option<&HandlerMethod>,
    elicitation_method: Option<&HandlerMethod>,
    roots_method: Option<&HandlerMethod>,
    on_connected_method: Option<&HandlerMethod>,
    on_disconnected_method: Option<&HandlerMethod>,
    on_task_progress_method: Option<&HandlerMethod>,
    on_resource_updated_method: Option<&HandlerMethod>,
    on_tools_list_changed_method: Option<&HandlerMethod>,
    on_resources_list_changed_method: Option<&HandlerMethod>,
    on_prompts_list_changed_method: Option<&HandlerMethod>,
) -> TokenStream {
    // Generate create_message method
    let create_message_impl = if let Some(method) = sampling_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name(request).await)
        } else {
            quote!(self.#method_name(request))
        };
        if method.returns_result {
            quote! {
                fn create_message(
                    &self,
                    request: ::mcpkit::types::CreateMessageRequest,
                ) -> impl std::future::Future<Output = Result<::mcpkit::types::CreateMessageResult, ::mcpkit::error::McpError>> + Send {
                    async move {
                        #call
                    }
                }
            }
        } else {
            quote! {
                fn create_message(
                    &self,
                    request: ::mcpkit::types::CreateMessageRequest,
                ) -> impl std::future::Future<Output = Result<::mcpkit::types::CreateMessageResult, ::mcpkit::error::McpError>> + Send {
                    async move {
                        Ok(#call)
                    }
                }
            }
        }
    } else {
        quote!()
    };

    // Generate elicit method
    let elicit_impl = if let Some(method) = elicitation_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name(request).await)
        } else {
            quote!(self.#method_name(request))
        };
        if method.returns_result {
            quote! {
                fn elicit(
                    &self,
                    request: ::mcpkit::types::ElicitRequest,
                ) -> impl std::future::Future<Output = Result<::mcpkit::types::ElicitResult, ::mcpkit::error::McpError>> + Send {
                    async move {
                        #call
                    }
                }
            }
        } else {
            quote! {
                fn elicit(
                    &self,
                    request: ::mcpkit::types::ElicitRequest,
                ) -> impl std::future::Future<Output = Result<::mcpkit::types::ElicitResult, ::mcpkit::error::McpError>> + Send {
                    async move {
                        Ok(#call)
                    }
                }
            }
        }
    } else {
        quote!()
    };

    // Generate list_roots method
    let list_roots_impl = if let Some(method) = roots_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name().await)
        } else {
            quote!(self.#method_name())
        };
        if method.returns_result {
            quote! {
                fn list_roots(
                    &self,
                ) -> impl std::future::Future<Output = Result<Vec<::mcpkit::client::handler::Root>, ::mcpkit::error::McpError>> + Send {
                    async move {
                        #call
                    }
                }
            }
        } else {
            quote! {
                fn list_roots(
                    &self,
                ) -> impl std::future::Future<Output = Result<Vec<::mcpkit::client::handler::Root>, ::mcpkit::error::McpError>> + Send {
                    async move {
                        Ok(#call)
                    }
                }
            }
        }
    } else {
        quote!()
    };

    // Generate lifecycle hooks
    let on_connected_impl = if let Some(method) = on_connected_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name().await)
        } else {
            quote!(self.#method_name())
        };
        quote! {
            fn on_connected(&self) -> impl std::future::Future<Output = ()> + Send {
                async move {
                    #call
                }
            }
        }
    } else {
        quote!()
    };

    let on_disconnected_impl = if let Some(method) = on_disconnected_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name().await)
        } else {
            quote!(self.#method_name())
        };
        quote! {
            fn on_disconnected(&self) -> impl std::future::Future<Output = ()> + Send {
                async move {
                    #call
                }
            }
        }
    } else {
        quote!()
    };

    // Generate notification handlers
    let on_task_progress_impl = if let Some(method) = on_task_progress_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name(task_id, progress).await)
        } else {
            quote!(self.#method_name(task_id, progress))
        };
        quote! {
            fn on_task_progress(
                &self,
                task_id: ::mcpkit::types::TaskId,
                progress: ::mcpkit::types::TaskProgress,
            ) -> impl std::future::Future<Output = ()> + Send {
                async move {
                    #call
                }
            }
        }
    } else {
        quote!()
    };

    let on_resource_updated_impl = if let Some(method) = on_resource_updated_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name(uri).await)
        } else {
            quote!(self.#method_name(uri))
        };
        quote! {
            fn on_resource_updated(&self, uri: String) -> impl std::future::Future<Output = ()> + Send {
                async move {
                    #call
                }
            }
        }
    } else {
        quote!()
    };

    let on_tools_list_changed_impl = if let Some(method) = on_tools_list_changed_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name().await)
        } else {
            quote!(self.#method_name())
        };
        quote! {
            fn on_tools_list_changed(&self) -> impl std::future::Future<Output = ()> + Send {
                async move {
                    #call
                }
            }
        }
    } else {
        quote!()
    };

    let on_resources_list_changed_impl = if let Some(method) = on_resources_list_changed_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name().await)
        } else {
            quote!(self.#method_name())
        };
        quote! {
            fn on_resources_list_changed(&self) -> impl std::future::Future<Output = ()> + Send {
                async move {
                    #call
                }
            }
        }
    } else {
        quote!()
    };

    let on_prompts_list_changed_impl = if let Some(method) = on_prompts_list_changed_method {
        let method_name = &method.name;
        let call = if method.is_async {
            quote!(self.#method_name().await)
        } else {
            quote!(self.#method_name())
        };
        quote! {
            fn on_prompts_list_changed(&self) -> impl std::future::Future<Output = ()> + Send {
                async move {
                    #call
                }
            }
        }
    } else {
        quote!()
    };

    quote! {
        impl ::mcpkit::client::ClientHandler for #self_ty {
            #create_message_impl
            #elicit_impl
            #list_roots_impl
            #on_connected_impl
            #on_disconnected_impl
            #on_task_progress_impl
            #on_resource_updated_impl
            #on_tools_list_changed_impl
            #on_resources_list_changed_impl
            #on_prompts_list_changed_impl
        }
    }
}

/// Generate convenience methods for the client handler.
fn generate_client_convenience_methods(
    self_ty: &syn::Type,
    has_sampling: bool,
    has_elicitation: bool,
    has_roots: bool,
) -> TokenStream {
    // Build capabilities chain
    let mut capability_chain = vec![quote!(::mcpkit::capability::ClientCapabilities::default())];

    if has_sampling {
        capability_chain.push(quote!(.with_sampling()));
    }
    if has_elicitation {
        capability_chain.push(quote!(.with_elicitation()));
    }
    if has_roots {
        capability_chain.push(quote!(.with_roots()));
    }

    // Join the capability chain
    let capabilities = if capability_chain.len() == 1 {
        quote!(::mcpkit::capability::ClientCapabilities::default())
    } else {
        let mut result = capability_chain[0].clone();
        for cap in &capability_chain[1..] {
            result = quote!(#result #cap);
        }
        result
    };

    quote! {
        impl #self_ty {
            /// Get the capabilities provided by this handler.
            ///
            /// This returns the capabilities that should be advertised to servers
            /// based on the handler methods implemented.
            #[must_use]
            pub fn capabilities(&self) -> ::mcpkit::capability::ClientCapabilities {
                #capabilities
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_find_handler_attr() {
        let tokens = quote! {
            #[sampling]
            async fn handle_sampling(&self, request: CreateMessageRequest) -> CreateMessageResult {
                // ...
            }
        };

        let method: syn::ImplItemFn = syn::parse2(tokens).unwrap();
        let idx = find_handler_attr(&method.attrs, "sampling");
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn test_find_handler_attr_not_found() {
        let tokens = quote! {
            async fn handle_something(&self) {}
        };

        let method: syn::ImplItemFn = syn::parse2(tokens).unwrap();
        let idx = find_handler_attr(&method.attrs, "sampling");
        assert_eq!(idx, None);
    }

    #[test]
    fn test_find_handler_attr_multiple_attrs() {
        let tokens = quote! {
            #[doc = "Some docs"]
            #[elicitation]
            #[allow(unused)]
            async fn handle_elicit(&self, request: ElicitRequest) -> ElicitResult {
                // ...
            }
        };

        let method: syn::ImplItemFn = syn::parse2(tokens).unwrap();
        let idx = find_handler_attr(&method.attrs, "elicitation");
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn test_find_and_remove_handler() {
        let tokens = quote! {
            impl MyHandler {
                #[sampling]
                async fn handle_sampling(&self, request: CreateMessageRequest) -> Result<CreateMessageResult, McpError> {
                    Ok(CreateMessageResult::default())
                }
            }
        };

        let mut impl_block: ItemImpl = syn::parse2(tokens).unwrap();
        let handler = find_and_remove_handler(&mut impl_block, "sampling");

        assert!(handler.is_some());
        let handler = handler.unwrap();
        assert_eq!(handler.name.to_string(), "handle_sampling");
        assert!(handler.is_async);
        assert!(handler.returns_result);
    }

    #[test]
    fn test_find_and_remove_handler_sync() {
        let tokens = quote! {
            impl MyHandler {
                #[roots]
                fn get_roots(&self) -> Vec<Root> {
                    vec![]
                }
            }
        };

        let mut impl_block: ItemImpl = syn::parse2(tokens).unwrap();
        let handler = find_and_remove_handler(&mut impl_block, "roots");

        assert!(handler.is_some());
        let handler = handler.unwrap();
        assert_eq!(handler.name.to_string(), "get_roots");
        assert!(!handler.is_async);
        assert!(!handler.returns_result);
    }

    #[test]
    fn test_find_and_remove_handler_not_found() {
        let tokens = quote! {
            impl MyHandler {
                async fn regular_method(&self) {}
            }
        };

        let mut impl_block: ItemImpl = syn::parse2(tokens).unwrap();
        let handler = find_and_remove_handler(&mut impl_block, "sampling");

        assert!(handler.is_none());
    }

    #[test]
    fn test_expand_mcp_client_empty() {
        let attr = quote! {};
        let item = quote! {
            impl EmptyHandler {}
        };

        let result = expand_mcp_client(attr, item);
        assert!(result.is_ok());

        let output = result.unwrap().to_string();
        // Should contain ClientHandler impl
        assert!(output.contains("ClientHandler"));
        // Should contain capabilities method
        assert!(output.contains("capabilities"));
    }

    #[test]
    fn test_expand_mcp_client_with_sampling() {
        let attr = quote! {};
        let item = quote! {
            impl SamplingHandler {
                #[sampling]
                async fn handle(&self, request: CreateMessageRequest) -> Result<CreateMessageResult, McpError> {
                    Err(McpError::internal("test stub"))
                }
            }
        };

        let result = expand_mcp_client(attr, item);
        assert!(result.is_ok());

        let output = result.unwrap().to_string();
        // Should contain create_message implementation
        assert!(output.contains("create_message"));
        // Should have with_sampling in capabilities
        assert!(output.contains("with_sampling"));
    }

    #[test]
    fn test_expand_mcp_client_with_all_handlers() {
        let attr = quote! {};
        let item = quote! {
            impl FullHandler {
                #[sampling]
                async fn sampling(&self, request: CreateMessageRequest) -> Result<CreateMessageResult, McpError> {
                    Err(McpError::internal("test stub"))
                }

                #[elicitation]
                async fn elicit(&self, request: ElicitRequest) -> Result<ElicitResult, McpError> {
                    Err(McpError::internal("test stub"))
                }

                #[roots]
                async fn roots(&self) -> Result<Vec<Root>, McpError> {
                    Ok(vec![])
                }

                #[on_connected]
                async fn connected(&self) {}

                #[on_disconnected]
                async fn disconnected(&self) {}
            }
        };

        let result = expand_mcp_client(attr, item);
        assert!(result.is_ok());

        let output = result.unwrap().to_string();
        // Should contain all method implementations
        assert!(output.contains("create_message"));
        assert!(output.contains("elicit"));
        assert!(output.contains("list_roots"));
        assert!(output.contains("on_connected"));
        assert!(output.contains("on_disconnected"));
        // Should have all capabilities
        assert!(output.contains("with_sampling"));
        assert!(output.contains("with_elicitation"));
        assert!(output.contains("with_roots"));
    }

    #[test]
    fn test_expand_mcp_client_notification_handlers() {
        let attr = quote! {};
        let item = quote! {
            impl NotifyHandler {
                #[on_task_progress]
                async fn task_progress(&self, task_id: TaskId, progress: TaskProgress) {}

                #[on_resource_updated]
                async fn resource_updated(&self, uri: String) {}

                #[on_tools_list_changed]
                async fn tools_changed(&self) {}

                #[on_resources_list_changed]
                async fn resources_changed(&self) {}

                #[on_prompts_list_changed]
                async fn prompts_changed(&self) {}
            }
        };

        let result = expand_mcp_client(attr, item);
        assert!(result.is_ok());

        let output = result.unwrap().to_string();
        // Should contain all notification handlers
        assert!(output.contains("on_task_progress"));
        assert!(output.contains("on_resource_updated"));
        assert!(output.contains("on_tools_list_changed"));
        assert!(output.contains("on_resources_list_changed"));
        assert!(output.contains("on_prompts_list_changed"));
    }

    #[test]
    fn test_generate_client_convenience_methods_no_caps() {
        let self_ty: syn::Type = syn::parse2(quote!(MyHandler)).unwrap();
        let output = generate_client_convenience_methods(&self_ty, false, false, false);

        let output_str = output.to_string();
        assert!(output_str.contains("capabilities"));
        assert!(output_str.contains("ClientCapabilities :: default ()"));
        assert!(!output_str.contains("with_sampling"));
    }

    #[test]
    fn test_generate_client_convenience_methods_all_caps() {
        let self_ty: syn::Type = syn::parse2(quote!(MyHandler)).unwrap();
        let output = generate_client_convenience_methods(&self_ty, true, true, true);

        let output_str = output.to_string();
        assert!(output_str.contains("with_sampling"));
        assert!(output_str.contains("with_elicitation"));
        assert!(output_str.contains("with_roots"));
    }
}
