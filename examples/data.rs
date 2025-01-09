use v_exchanges::bitmex::Bitmex;

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));
	let bm = Bitmex::default();
	let bvol = bm.bvol(2).await.unwrap();
	dbg!(&bvol);
}
