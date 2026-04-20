use std::env;

use jiff::Timestamp;
use serde::Deserialize;
use serde_json::json;
use v_exchanges::prelude::*;
use v_exchanges_adapters::bybit::{BybitHttpAuth, BybitOption};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryApiResponse {
	result: QueryApiResult,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryApiResult {
	expired_at: String,
	permissions: Permissions,
	read_only: u8,
	ips: Vec<String>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct Permissions {
	contract_trade: Vec<String>,
	spot: Vec<String>,
	wallet: Vec<String>,
	options: Vec<String>,
	derivatives: Vec<String>,
	exchange: Vec<String>,
	earn: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateApiResponse {
	ret_code: i64,
	ret_msg: String,
}

fn print_expiry(label: &str, expired_at: &str) {
	let now = Timestamp::now();
	match expired_at {
		"" | "0" => println!("{label}: never expires"),
		s => match s.parse::<Timestamp>() {
			Ok(t) => {
				let remaining = t - now;
				let total_secs = remaining.get_seconds();
				let days = total_secs / 86400;
				let hours = (total_secs % 86400) / 3600;
				println!("{label}: expires in {days}d {hours}h  (at {t})");
			}
			Err(e) => println!("{label}: failed to parse expiry {s:?}: {e}"),
		},
	}
}

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let (pub_, sec) = match (env::var("QUANTM_BYBIT_SUB_PUBKEY"), env::var("QUANTM_BYBIT_SUB_SECRET")) {
		//let (pub_, sec) = match (env::var("BYBIT_TESTSUB_PUBKEY"), env::var("BYBIT_TESTSUB_SECRET")) {
		(Ok(p), Ok(s)) => (p, s),
		_ => {
			panic!("one of the keys is not set");
		}
	};

	let mut c = Bybit::default();
	c.auth(pub_, sec.into());

	let auth_options = || vec![BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)];

	// --- before ---
	let before: QueryApiResponse = c.get_no_query("/v5/user/query-api", auth_options()).await.unwrap();
	print_expiry("before", &before.result.expired_at);
	let r = &before.result;
	println!("  read_only={} ips={:?}", r.read_only, r.ips);
	println!("  permissions: {:?}", r.permissions);

	// --- attempt to extend ---
	let body = json!({ "ips": "*" });
	let update: UpdateApiResponse = c.post("/v5/user/update-sub-api", body, auth_options()).await.unwrap();
	println!("update-sub-api => ret_code={} msg={:?}", update.ret_code, update.ret_msg);

	// --- after ---
	let after: QueryApiResponse = c.get_no_query("/v5/user/query-api", auth_options()).await.unwrap();
	print_expiry("after ", &after.result.expired_at);
}
