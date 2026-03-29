//! Unified Lambda Loop - 通用的插件循环调用引擎
//!
//! 插件通过返回 (effects, result) 来控制流程：
//! - result 非空 + effects 为空 → 最终结果，结束 loop
//! - result 非空 + effects 非空 → 并行执行 effects，返回 result，结束 loop
//! - effects 非空 → 并行执行所有 effect，收集结果，继续 loop
//! - effects 为空但 result 也为空 → 错误

use crate::error::Error;
use crate::http_executor::AsyncHttpExecutor;
use mochiclaw_plugin::PluginHost;
use mochiclaw_sdk::lambda::{Action, EffectResult, LambdaInput, LambdaOutput};
use std::sync::Arc;

/// 使用统一的 loop 引擎调用插件
///
/// # 参数
/// - `plugin_host`: 插件主机
/// - `http_executor`: HTTP 执行器
/// - `plugin_name`: 插件名称
/// - `action`: 要执行的动作
/// - `payload`: action 对应的输入参数（msgpack 编码）
/// - `initial_state`: 插件初始状态（msgpack 编码）
///
/// # 返回
/// - `Ok(result)`: 插件最终返回的结果（msgpack 编码）
pub async fn lambda_call(
    plugin_host: &PluginHost,
    http_executor: Arc<AsyncHttpExecutor>,
    plugin_name: &str,
    action: Action,
    payload: Vec<u8>,
    initial_state: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    let mut current_state = initial_state;
    let current_payload = payload;
    let mut effect_results: Vec<EffectResult> = Vec::new();

    loop {
        let input = LambdaInput {
            version: 1,
            action: action.clone(),
            state: current_state.clone(),
            payload: current_payload.clone(),
            effect_results: effect_results.clone(),
        };

        let output: LambdaOutput = plugin_host
            .call(plugin_name, "lambda_function", &input)
            .map_err(|e| Error::Plugin(e.to_string()))?;

        // 情况1: result 非空 + effects 为空 → 最终结果，结束 loop
        if !output.result.is_empty() && output.effects.is_empty() {
            return Ok(output.result);
        }

        // 情况2: result 非空 + effects 非空 → 并行执行 effects，返回 result，结束 loop
        if !output.result.is_empty() && !output.effects.is_empty() {
            let effects = output.effects;
            let result = output.result.clone();
            // 在后台执行 effects，不等待结果
            let http_executor = http_executor.clone();
            let plugin_name = plugin_name.to_string();
            tokio::spawn(async move {
                let _ = http_executor.execute_all(&plugin_name, effects).await;
            });
            return Ok(result);
        }

        // 情况3: effects 为空但 result 也为空 → 错误
        if output.effects.is_empty() {
            return Err(Error::Plugin(
                "plugin returned no effects and no result".to_string(),
            ));
        }

        // 情况4: effects 非空 → 并行执行，收集结果，继续 loop
        tracing::debug!(
            "executing {} effect(s) for action {:?}",
            output.effects.len(),
            action
        );

        effect_results = http_executor.execute_all(plugin_name, output.effects).await;

        // 如果有任何 effect 执行失败，记录 warning
        for (i, result) in effect_results.iter().enumerate() {
            if !result.success {
                tracing::warn!(
                    "effect {} failed: {:?}",
                    i,
                    result.error.as_deref().unwrap_or("unknown error")
                );
            }
        }

        // 更新状态，用于下一次调用
        current_state = output.new_state;

        // payload 保持不变，一直透传给插件
    }
}

/// 同上，但 payload 和 state 使用具体类型自动序列化/反序列化
pub async fn lambda_call_typed<TPayload, TResult>(
    plugin_host: &PluginHost,
    http_executor: Arc<AsyncHttpExecutor>,
    plugin_name: &str,
    action: Action,
    payload: &TPayload,
    initial_state: &[u8],
) -> Result<TResult, Error>
where
    TPayload: serde::Serialize,
    TResult: serde::de::DeserializeOwned,
{
    let payload_bytes = rmp_serde::to_vec(payload)
        .map_err(|e| Error::Plugin(format!("failed to serialize payload: {}", e)))?;

    let result_bytes = lambda_call(
        plugin_host,
        http_executor,
        plugin_name,
        action,
        payload_bytes,
        initial_state.to_vec(),
    )
    .await?;

    rmp_serde::from_slice(&result_bytes)
        .map_err(|e| Error::Plugin(format!("failed to deserialize result: {}", e)))
}
