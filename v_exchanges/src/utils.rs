use v_utils::prelude::*;

#[macro_export]
macro_rules! recv_window_check {
	//NB: requires explicit arg provision, as otherwise `default` branch logic gets confused
	($recv_window:expr, $self:expr) => {{
		const MAX_RECV_WINDOW: std::time::Duration = std::time::Duration::from_secs(10 * 60); // 10 minutes

		// Check the provided recv_window first
		if let Some(rw) = $recv_window {
			if rw > MAX_RECV_WINDOW {
				return Err($crate::ExchangeError::Other(eyre::eyre!(
					"recv_window of {:?} exceeds maximum allowed duration of {:?}",
					rw,
					MAX_RECV_WINDOW
				)));
			}
		}

		// Check the client's default recv_window
		if let Some(rw) = $self.recv_window {
			if rw > MAX_RECV_WINDOW {
				return Err($crate::ExchangeError::Other(eyre::eyre!(
					"client's default recv_window of {:?} exceeds maximum allowed duration of {:?}",
					rw,
					MAX_RECV_WINDOW
				)));
			}
		}

		if $recv_window.is_none() && $self.recv_window.is_some() {
			tracing::warn!("called without recv_window, using global default (not recommended)");
		}
	}};
}

/// # Panics
/// Fine, because given prospected usages, theoretically only developer will see it.
pub fn join_params(a: Value, b: Value) -> Value {
	if let (Value::Object(mut a_map), Value::Object(b_map)) = (a, b) {
		a_map.extend(b_map);
		Value::Object(a_map)
	} else {
		panic!("Both inputs must be JSON objects");
	}
}

#[macro_export]
macro_rules! define_provider_timeframe {
	($struct_name:ident, $timeframes:expr) => {
		#[derive(derive_more::AsRef, Clone, Copy, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
		pub struct $struct_name(v_utils::trades::Timeframe);

		impl std::fmt::Display for $struct_name {
			fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
				const TIMEFRAMES: [&str; $timeframes.len()] = $timeframes;

				let s = self.0.try_as_predefined(&TIMEFRAMES).expect(concat!(
					"We can't create a ",
					stringify!($struct_name),
					" object if that doesn't succeed in the first place"
				));
				write!(f, "{s}")
			}
		}

		impl TryFrom<v_utils::trades::Timeframe> for $struct_name {
			type Error = $crate::UnsupportedTimeframeError;

			fn try_from(t: v_utils::trades::Timeframe) -> Result<Self, Self::Error> {
				const TIMEFRAMES: [&str; $timeframes.len()] = $timeframes;

				match t.try_as_predefined(&TIMEFRAMES) {
					Some(_) => Ok(Self(t)),
					_ => Err($crate::UnsupportedTimeframeError::new(t, TIMEFRAMES.iter().map(v_utils::trades::Timeframe::from).collect())),
				}
			}
		}
		impl From<&str> for $struct_name {
			fn from(s: &str) -> Self {
				Self(v_utils::trades::Timeframe::from(s))
			}
		}
	};
}
