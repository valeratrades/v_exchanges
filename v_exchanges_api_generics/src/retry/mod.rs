//! Generic retry mechanism for network operations.

pub mod backoff;

use std::{future::Future, marker::PhantomData, time::Duration};

pub use backoff::ExponentialBackoff;

/// Configuration for retry behavior.
#[derive(Clone, Debug, Default)]
pub struct RetryConfig {
	/// Maximum number of retry attempts (total attempts = 1 initial + `max_retries`).
	pub max_retries: u32 = 3,
	/// Initial delay between retries in milliseconds.
	pub initial_delay_ms: u64 = 500,
	/// Maximum delay between retries in milliseconds.
	pub max_delay_ms: u64 = 5_000,
	/// Backoff multiplier factor.
	pub backoff_factor: f64 = 2.0,
	/// Maximum jitter in milliseconds to add to delays.
	pub jitter_ms: u64 = 100,
	/// Whether the first retry should happen immediately without delay.
	/// Should be false for HTTP/order operations, true for connection operations.
	pub immediate_first: bool,
	/// Optional maximum total elapsed time for all retries in milliseconds.
	/// If exceeded, retries stop even if `max_retries` hasn't been reached.
	pub max_elapsed_ms: Option<u64>,
}

/// Generic retry manager for network operations.
///
/// Stateless and thread-safe - each operation maintains its own backoff state.
#[derive(Clone, Debug)]
pub struct RetryManager<E> {
	config: RetryConfig,
	_phantom: PhantomData<E>,
}

impl<E> RetryManager<E>
where
	E: std::error::Error,
{
	/// Creates a new retry manager with the given configuration.
	pub const fn new(config: RetryConfig) -> Self {
		Self { config, _phantom: PhantomData }
	}

	/// Executes an operation with retry logic.
	///
	/// # Errors
	///
	/// Returns an error if the operation fails after exhausting all retries,
	/// the budget is exceeded, or the backoff configuration is invalid.
	pub async fn execute_with_retry<F, Fut, T>(&self, operation_name: &str, mut operation: F, should_retry: impl Fn(&E) -> bool, create_error: impl Fn(String) -> E) -> Result<T, E>
	where
		F: FnMut() -> Fut,
		Fut: Future<Output = Result<T, E>>, {
		let mut backoff = ExponentialBackoff::try_from(&self.config).map_err(|e| create_error(format!("Invalid backoff configuration: {e}")))?;

		let mut attempt = 0;
		let start_time = tokio::time::Instant::now();

		loop {
			if let Some(max_elapsed_ms) = self.config.max_elapsed_ms {
				let elapsed = start_time.elapsed();
				if elapsed.as_millis() >= u128::from(max_elapsed_ms) {
					tracing::trace!(
						operation = %operation_name,
						attempts = attempt + 1,
						budget_ms = max_elapsed_ms,
						"Retry budget exceeded"
					);
					return Err(create_error("Budget exceeded".to_string()));
				}
			}

			match operation().await {
				Ok(success) => {
					if attempt > 0 {
						tracing::trace!(operation = %operation_name, attempts = attempt + 1, "Retry succeeded");
					}
					return Ok(success);
				}
				Err(e) => {
					if !should_retry(&e) {
						tracing::trace!(operation = %operation_name, error = %e, "Non-retryable error");
						return Err(e);
					}

					if attempt >= self.config.max_retries {
						tracing::trace!(operation = %operation_name, attempts = attempt + 1, error = %e, "Retries exhausted");
						return Err(e);
					}

					let mut delay = backoff.next_duration();

					if let Some(max_elapsed_ms) = self.config.max_elapsed_ms {
						let elapsed = start_time.elapsed();
						let remaining = Duration::from_millis(max_elapsed_ms).saturating_sub(elapsed);

						if remaining.is_zero() {
							tracing::trace!(
								operation = %operation_name,
								attempts = attempt + 1,
								budget_ms = max_elapsed_ms,
								"Retry budget exceeded"
							);
							return Err(create_error("Budget exceeded".to_string()));
						}

						delay = delay.min(remaining);
					}

					tracing::trace!(
						operation = %operation_name,
						attempt = attempt + 1,
						delay_ms = delay.as_millis() as u64,
						error = %e,
						"Retrying after failure"
					);

					// Yield even on zero-delay to avoid busy-wait loop
					if delay.is_zero() {
						tokio::task::yield_now().await;
						attempt += 1;
						continue;
					}

					tokio::time::sleep(delay).await;
					attempt += 1;
				}
			}
		}
	}
}

/// Create a RetryManager suitable for HTTP requests.
pub fn create_http_retry_manager<E: std::error::Error>() -> RetryManager<E> {
	RetryManager::new(RetryConfig {
		max_retries: 3,
		initial_delay_ms: 500,
		max_delay_ms: 5_000,
		backoff_factor: 2.0,
		jitter_ms: 100,
		immediate_first: false,
		max_elapsed_ms: None,
	})
}

/// Create a RetryManager suitable for WebSocket reconnections.
pub fn create_websocket_retry_manager<E: std::error::Error>() -> RetryManager<E> {
	RetryManager::new(RetryConfig {
		max_retries: 10,
		initial_delay_ms: 1_000,
		max_delay_ms: 30_000,
		backoff_factor: 2.0,
		jitter_ms: 500,
		immediate_first: true,
		max_elapsed_ms: None,
	})
}
