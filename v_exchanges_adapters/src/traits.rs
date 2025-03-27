use std::fmt::Debug;

use v_exchanges_api_generics::{http, websocket, ws};

/// A `trait` that represents an option which can be set when creating handlers
pub trait HandlerOption: Default {
	type Options: HandlerOptions<OptionItem = Self>;
}

/// Set of [HandlerOption] s
pub trait HandlerOptions: Default + Clone + Debug {
	/// The element of this set
	type OptionItem: HandlerOption<Options = Self>;

	//Q: searched through impls, only differing options are HttpAuth and RecvWindow, (on unimportant exchanges at that), rest seem to have exact same types and uses. So maybe I could describe OptionItem procedurally + have part of the implementation for free? Really only problem would be the differing types and the websocket_url/http_url, which are effectively enums of `&'static str`
	fn update(&mut self, option: Self::OptionItem);
	fn is_authenticated(&self) -> bool;
}

/// A `trait` that shows the implementing type is able to create [http::RequestHandler]s
pub trait HttpOption<'a, R, B>: HandlerOption {
	type RequestHandler: http::RequestHandler<B>;

	fn request_handler(options: Self::Options) -> Self::RequestHandler;
}

#[deprecated(note = "switching to WsOption")]
/// A `trait` that shows the implementing type is able to create [websocket::WebSocketHandler]s
pub trait WebSocketOption<H>: HandlerOption {
	type WebSocketHandler: websocket::WebSocketHandler;

	fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler;
}

/// A `trait` that shows the implementing type is able to create [websocket::WebSocketHandler]s
pub trait WsOption<H>: HandlerOption {
	type WsHandler: ws::WsHandler;

	fn ws_handler(handler: H, options: Self::Options) -> Self::WsHandler;
}
