use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::client::ClientBuilder;
use alloy::rpc::types::{
    BlockId, EIP1186AccountProofResponse, FeeHistory, Filter, FilterChanges, Log,
};
use alloy::transports::http::Http;
use alloy::transports::layers::{RetryBackoffLayer, RetryBackoffService};
use async_trait::async_trait;
use eyre::{eyre, Result};
use reqwest::Client;
use revm::primitives::AccessList;

use crate::errors::RpcError;
use crate::network_spec::NetworkSpec;
use crate::types::{Block, BlockTag};

use super::ExecutionRpc;

pub struct HttpRpc<N: NetworkSpec> {
    url: String,
    #[cfg(target_arch = "wasm32")]
    retry_config: RetryConfig,
    #[cfg(not(target_arch = "wasm32"))]
    provider: RootProvider<RetryBackoffService<Http<Client>>, N>,
    #[cfg(target_arch = "wasm32")]
    provider: RootProvider<Http<Client>, N>,
}

impl<N: NetworkSpec> Clone for HttpRpc<N> {
    fn clone(&self) -> Self {
        Self::new(&self.url).unwrap()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<N: NetworkSpec> ExecutionRpc<N> for HttpRpc<N> {
    fn new(rpc: &str) -> Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let client = ClientBuilder::default()
            .layer(RetryBackoffLayer::new(100, 50, 300))
            .http(rpc.parse().unwrap());

        #[cfg(target_arch = "wasm32")]
        let client = ClientBuilder::default().http(rpc.parse().unwrap());

        let provider = ProviderBuilder::new().network::<N>().on_client(client);

        Ok(HttpRpc {
            url: rpc.to_string(),
            #[cfg(target_arch = "wasm32")]
            retry_config: RetryConfig::default(),
            provider,
        })
    }

    async fn get_proof(
        &self,
        address: Address,
        slots: &[B256],
        block: BlockId,
    ) -> Result<EIP1186AccountProofResponse> {
        let proof_response = self
            .provider
            .get_proof(address, slots.to_vec())
            .block_id(block)
            .await
            .map_err(|e| RpcError::new("get_proof", e))?;

        Ok(proof_response)
    }

    async fn create_access_list(
        &self,
        tx: &N::TransactionRequest,
        block: BlockTag,
    ) -> Result<AccessList> {
        let block = match block {
            BlockTag::Latest => BlockId::latest(),
            BlockTag::Finalized => BlockId::finalized(),
            BlockTag::Number(num) => BlockId::number(num),
        };

        let list = self
            .provider
            .create_access_list(tx)
            .block_id(block)
            .await
            .map_err(|e| RpcError::new("create_access_list", e))?;

        Ok(list.access_list)
    }

    async fn get_code(&self, address: Address, block: u64) -> Result<Vec<u8>> {
        let code = self
            .provider
            .get_code_at(address)
            .block_id(block.into())
            .await
            .map_err(|e| RpcError::new("get_code", e))?;

        Ok(code.to_vec())
    }

    async fn send_raw_transaction(&self, bytes: &[u8]) -> Result<B256> {
        let tx = self
            .provider
            .send_raw_transaction(bytes)
            .await
            .map_err(|e| RpcError::new("send_raw_transaction", e))?;

        Ok(*tx.tx_hash())
    }

    async fn get_transaction_receipt(&self, tx_hash: B256) -> Result<Option<N::ReceiptResponse>> {
        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(|e| RpcError::new("get_transaction_receipt", e))?;

        Ok(receipt)
    }

    async fn get_block_receipts(&self, block: BlockTag) -> Result<Option<Vec<N::ReceiptResponse>>> {
        let block = match block {
            BlockTag::Latest => BlockNumberOrTag::Latest,
            BlockTag::Finalized => BlockNumberOrTag::Finalized,
            BlockTag::Number(num) => BlockNumberOrTag::Number(num),
        };

        let receipts = self
            .provider
            .get_block_receipts(block)
            .await
            .map_err(|e| RpcError::new("get_block_receipts", e))?;

        Ok(receipts)
    }

    async fn get_transaction(&self, tx_hash: B256) -> Result<Option<N::TransactionResponse>> {
        Ok(self
            .provider
            .get_transaction_by_hash(tx_hash)
            .await
            .map_err(|e| RpcError::new("get_transaction", e))?)
    }

    async fn get_logs(&self, filter: &Filter) -> Result<Vec<Log>> {
        Ok(self
            .provider
            .get_logs(filter)
            .await
            .map_err(|e| RpcError::new("get_logs", e))?)
    }

    async fn get_filter_changes(&self, filter_id: U256) -> Result<FilterChanges> {
        Ok(self
            .provider
            .get_filter_changes_dyn(filter_id)
            .await
            .map_err(|e| RpcError::new("get_filter_changes", e))?)
    }

    async fn get_filter_logs(&self, filter_id: U256) -> Result<Vec<Log>> {
        Ok(self
            .provider
            .raw_request("eth_getFilterLogs".into(), (filter_id,))
            .await
            .map_err(|e| RpcError::new("get_filter_logs", e))?)
    }

    async fn uninstall_filter(&self, filter_id: U256) -> Result<bool> {
        Ok(self
            .provider
            .raw_request("eth_uninstallFilter".into(), (filter_id,))
            .await
            .map_err(|e| RpcError::new("uninstall_filter", e))?)
    }

    async fn new_filter(&self, filter: &Filter) -> Result<U256> {
        Ok(self
            .provider
            .new_filter(filter)
            .await
            .map_err(|e| RpcError::new("new_filter", e))?)
    }

    async fn new_block_filter(&self) -> Result<U256> {
        Ok(self
            .provider
            .new_block_filter()
            .await
            .map_err(|e| RpcError::new("new_block_filter", e))?)
    }

    async fn new_pending_transaction_filter(&self) -> Result<U256> {
        Ok(self
            .provider
            .new_pending_transactions_filter(false)
            .await
            .map_err(|e| RpcError::new("new_pending_transaction_filter", e))?)
    }

    #[cfg(target_arch = "wasm32")]
    async fn chain_id(&self) -> Result<u64> {
        self.execute_with_retry(|| async {
            self.provider
                .get_chain_id()
                .await
                .map_err(|e| RpcError::new("chain_id", e))
        })
        .await
    }

    async fn get_fee_history(
        &self,
        block_count: u64,
        last_block: u64,
        reward_percentiles: &[f64],
    ) -> Result<FeeHistory> {
        Ok(self
            .provider
            .get_fee_history(block_count, last_block.into(), reward_percentiles)
            .await
            .map_err(|e| RpcError::new("fee_history", e))?)
    }

    async fn get_block(&self, hash: B256) -> Result<Block<N::TransactionResponse>> {
        self.provider
            .raw_request::<_, Option<Block<N::TransactionResponse>>>(
                "eth_getBlockByHash".into(),
                (hash, true),
            )
            .await?
            .ok_or(eyre!("block not found"))
    }
}

#[cfg(target_arch = "wasm32")]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use wasmtimer::tokio::sleep;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug)]
struct RetryConfig {
    max_attempts: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
}

#[cfg(target_arch = "wasm32")]
impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<N: NetworkSpec> HttpRpc<N> {
    async fn execute_with_retry<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut + Clone,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let config = RetryConfig::default();
        let mut attempts = 0;
        let mut backoff = config.initial_backoff;

        loop {
            attempts += 1;
            match operation().await {
                Ok(response) => return Ok(response),
                Err(err) => {
                    if !Self::should_retry(&err) || attempts >= config.max_attempts {
                        return Err(err);
                    }

                    sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, config.max_backoff);
                }
            }
        }
    }

    fn should_retry(err: &RpcError) -> bool {
        if let Some(source) = &err.source {
            let error_str = source.to_string().to_lowercase();
            error_str.contains("rate limit") ||
            error_str.contains("timeout") ||
            error_str.contains("connection") ||
            (error_str.contains("server") && error_str.contains("50"))
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{mock, Server};
    use std::time::Duration;

    #[tokio::test]
    #[cfg(target_arch = "wasm32")]
    async fn test_retry_mechanism() {
        let mut server = Server::new();
        
        // Test rate limit retry
        let mock = server.mock("POST", "/")
            .with_status(429)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "rate limit exceeded"}"#)
            .expect(3)
            .create();

        let provider = HttpRpc::<NetworkSpec>::new(&server.url()).unwrap();
        let result = provider.chain_id().await;
        
        assert!(result.is_err());
        mock.assert();

        // Test successful retry
        let mock = server.mock("POST", "/")
            .with_status(429)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "rate limit exceeded"}"#)
            .times(2)
            .create();

        let mock_success = server.mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"result": "0x1"}"#)
            .create();

        let result = provider.chain_id().await;
        assert!(result.is_ok());
        mock.assert();
        mock_success.assert();
    }
}
