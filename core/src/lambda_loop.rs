//! Unified Lambda Loop - 持久化执行的插件调用引擎
//!
//! 插件通过返回 LambdaOutput 来控制流程：
//! - Finished(result) → 最终结果，结束 loop
//! - Suspended { effect, step_id, new_history } → 执行 effect，合并历史，继续 loop

use crate::error::Error;
use crate::history_store::HistoryStore;
use crate::http_executor::AsyncHttpExecutor;
use mochiclaw_lambda::LambdaHost;
use mochiclaw_sdk::lambda::{Action, EffectResult, LambdaInput, LambdaOutput};
use std::sync::Arc;

/// 使用持久化执行模型调用插件
///
/// # 参数
/// - `lambda_host`: 插件主机
/// - `http_executor`: HTTP 执行器
/// - `history_store`: 历史持久化存储
/// - `execution_id`: 唯一执行标识
/// - `lambda_name`: 插件名称
/// - `action`: 要执行的动作
/// - `payload`: action 对应的输入参数（msgpack 编码）
///
/// # 返回
/// - `Ok(result)`: 插件最终返回的结果（msgpack 编码）
pub async fn lambda_call(
    lambda_host: &LambdaHost,
    http_executor: Arc<AsyncHttpExecutor>,
    history_store: Arc<HistoryStore>,
    execution_id: &str,
    lambda_name: &str,
    action: Action,
    payload: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    // 加载历史（可能为空）
    let mut history = history_store.get(execution_id).await.unwrap_or_default();

    loop {
        let input = LambdaInput {
            version: 1,
            action: action.clone(),
            payload: payload.clone(),
            history: history.clone(),
        };

        let output: LambdaOutput = lambda_host
            .call(lambda_name, "lambda_main", &input)
            .map_err(|e| Error::Lambda(e.to_string()))?;

        match output {
            // 情况1: Finished → 执行完成，清理历史，返回结果
            LambdaOutput::Finished(result) => {
                history_store.clear(execution_id).await;
                return Ok(result);
            }

            // 情况2: Suspended → 需要执行 effect
            LambdaOutput::Suspended {
                effect,
                step_id,
                new_history,
            } => {
                // 合并新历史到存储
                history_store.merge(execution_id, new_history).await;

                tracing::debug!(
                    "executing effect for step '{}' (execution_id={})",
                    step_id,
                    execution_id
                );

                // 执行 effect，得到 HTTP 响应体字符串
                let response_str = http_executor
                    .execute_effect(lambda_name, &effect)
                    .await?;

                // 将 EffectResult 存入历史（key 为 step_id），lambda 下次会从历史中查找
                let effect_result = EffectResult {
                    success: true,
                    response: Some(response_str),
                    error: None,
                };
                let result_data =
                    rmp_serde::to_vec(&effect_result).map_err(|e| Error::Lambda(e.to_string()))?;
                history_store
                    .merge(execution_id, [(step_id, result_data)].into())
                    .await;

                // 重新加载历史（这样下次循环时能拿到最新的历史）
                history = history_store.get(execution_id).await.unwrap_or_default();
            }
        }
    }
}

/// 同上，但 payload 使用具体类型自动序列化
pub async fn lambda_call_typed<TPayload, TResult>(
    lambda_host: &LambdaHost,
    http_executor: Arc<AsyncHttpExecutor>,
    history_store: Arc<HistoryStore>,
    execution_id: &str,
    lambda_name: &str,
    action: Action,
    payload: &TPayload,
) -> Result<TResult, Error>
where
    TPayload: serde::Serialize,
    TResult: serde::de::DeserializeOwned,
{
    let payload_bytes = rmp_serde::to_vec(payload)
        .map_err(|e| Error::Lambda(format!("failed to serialize payload: {}", e)))?;

    let result_bytes = lambda_call(
        lambda_host,
        http_executor,
        history_store,
        execution_id,
        lambda_name,
        action,
        payload_bytes,
    )
    .await?;

    rmp_serde::from_slice(&result_bytes)
        .map_err(|e| Error::Lambda(format!("failed to deserialize result: {}", e)))
}
