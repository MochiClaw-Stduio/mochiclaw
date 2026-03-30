//! #[mochi_main] 宏 - 生成持久化执行的入口函数
//!
//! 此宏应用于用户定义的 handler 函数，生成使用 #[plugin_fn] 的胶水代码

use proc_macro::TokenStream;
use quote::quote;
use syn::ItemFn;

/// `#[mochi_main]` 宏 - 持久化执行入口
///
/// # 用法
/// ```ignore
/// #[mochi_main]
/// pub fn my_handler(ctx: &mut Context, action: Action, payload: &[u8]) -> Result<Vec<u8>, SuspendSignal> {
///     match action {
///         Action::Chat => { ... }
///         _ => { ... }
///     }
/// }
/// ```
///
/// 宏会生成使用 #[plugin_fn] 的入口函数
#[proc_macro_attribute]
pub fn mochi_main(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut item: ItemFn = syn::parse(input).expect("#[mochi_main] must be applied to a function");

    let func_name = &item.sig.ident;
    let inner_func_name = syn::Ident::new(&format!("{}_inner", func_name), func_name.span());

    // 将原函数重命名为 *_inner
    item.sig.ident = inner_func_name.clone();

    // 移除 #[mochi_main] 属性
    item.attrs.retain(|attr| !attr.path().is_ident("mochi_main"));

    let expanded = quote! {
        /// 用户定义的 handler 函数
        #item

        /// 入口函数，由 extism 调用
        #[extism_pdk::plugin_fn]
        pub fn lambda_main(input: mochiclaw_sdk::lambda::LambdaInput) -> extism_pdk::FnResult<mochiclaw_sdk::lambda::LambdaOutput> {
            use mochiclaw_sdk::lambda::{Context, LambdaOutput, SuspendSignal};

            // 创建 Context
            let mut ctx = Context::new(*input.history);

            // 调用用户 handler，传递 action 和 payload
            let result = #inner_func_name(&mut ctx, input.action, &input.payload);

            // 构建输出
            let output = match result {
                Ok(final_data) => LambdaOutput::Finished(final_data),
                Err(signal) => {
                    // 取出 pending effect 和 new_history
                    let (step_id, effect) = ctx.take_pending_effect().unwrap_or_else(|| {
                        (signal.step_id, Box::new(*signal.effect))
                    });
                    let new_history = ctx.into_new_history();

                    LambdaOutput::Suspended {
                        effect,
                        step_id,
                        new_history,
                    }
                }
            };

            Ok(output)
        }
    };

    TokenStream::from(expanded)
}
