// Copyright (C) 2021-2022 The apca Developers
// SPDX-License-Identifier: GPL-3.0-or-later

use chrono::DateTime;
use chrono::Utc;

use num_decimal::Num;

use serde::Deserialize;
use serde::Serialize;
use serde_json::from_slice as from_json;
use serde_urlencoded::to_string as to_query;
use std::collections::HashMap;

use crate::data::v2::Feed;
use crate::data::DATA_BASE_URL;
use crate::Str;

/// A GET request to be made to the /v2/stocks/quotes/latest endpoint.
#[derive(Clone, Serialize, Eq, PartialEq, Debug)]
pub struct LastQuoteReq {
  /// Comma-separated list of symbols to retrieve the last quote for.
  pub symbols: String,
  /// The data feed to use.
  pub feed: Option<Feed>,
}

impl LastQuoteReq {
  /// Create a new LastQuoteReq with the given symbols.
  pub fn new(symbols: Vec<String>) -> Self {
    Self {
      symbols: symbols.join(",").into(),
      feed: None,
    }
  }
  /// Set the data feed to use.
  pub fn with_feed(mut self, feed: Feed) -> Self {
    self.feed = Some(feed);
    self
  }
}

/// A quote bar as returned by the /v2/stocks/quotes/latest endpoint.
/// See
/// https://alpaca.markets/docs/api-references/market-data-api/stock-pricing-data/historical/#latest-multi-quotes
// TODO: Not all fields are hooked up.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct Quote {
  /// The time stamp of this quote.
  pub time: DateTime<Utc>,
  /// The ask price.
  pub ask_price: Num,
  /// The ask size.
  pub ask_size: u64,
  /// The bid price.
  pub bid_price: Num,
  /// The bid size.
  pub bid_size: u64,
  /// Symbol of this quote
  pub symbol: String,
}

impl Quote {
  fn from(symbol: &str, point: QuoteDataPoint) -> Self {
    Self {
      time: point.t,
      ask_price: point.ap.clone(),
      ask_size: point.r#as,
      bid_price: point.bp.clone(),
      bid_size: point.bs,
      symbol: symbol.to_string(),
    }
  }

  fn parse(body: &[u8]) -> Result<Vec<Quote>, serde_json::Error> {
    from_json::<LastQuoteResponse>(body).map(|response| {
      response
        .quotes
        .into_iter()
        .map(|(sym, point)| Quote::from(&sym, point))
        .collect()
    })
  }
}

/// fields for individual data points in the response JSON
#[derive(Clone, Debug, Deserialize)]
pub struct QuoteDataPoint {
  t: DateTime<Utc>,
  ap: Num,
  r#as: u64,
  bp: Num,
  bs: u64,
}

/// A representation of the JSON data in the response
#[derive(Debug, Deserialize)]
pub struct LastQuoteResponse {
  quotes: HashMap<String, QuoteDataPoint>,
}

EndpointNoParse! {
  /// The representation of a GET request to the
  /// /v2/stocks/quotes/latest endpoint.
  pub Get(LastQuoteReq),
  Ok => Vec<Quote>, [
    /// The last quote was retrieved successfully.
    /* 200 */ OK,
  ],
  Err => GetError, [
    /// The provided symbol was invalid or not found or the data feed is
    /// not supported.
    /* 422 */ UNPROCESSABLE_ENTITY => InvalidInput,
  ]

  fn base_url() -> Option<Str> {
    Some(DATA_BASE_URL.into())
  }

  fn path(_input: &Self::Input) -> Str {
    format!("/v2/stocks/quotes/latest").into()
  }

  fn query(input: &Self::Input) -> Result<Option<Str>, Self::ConversionError> {
    Ok(Some(to_query(input)?.into()))
  }

  fn parse(body: &[u8]) -> Result<Self::Output, Self::ConversionError> {
    Quote::parse(body).map_err(Self::ConversionError::from)
  }

  fn parse_err(body: &[u8]) -> Result<Self::ApiError, Vec<u8>> {
    from_json::<Self::ApiError>(body).map_err(|_| body.to_vec())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use chrono::Duration;

  use test_log::test;

  use crate::api_info::ApiInfo;
  use crate::Client;
  use crate::RequestError;

  /// Check that we can parse the reference quote from the
  /// documentation.
  #[test]
  fn parse_reference_quote() {
    let response = br#"{
			"quotes": {
				"TSLA": {
					"t": "2022-04-12T17:26:45.009288296Z",
					"ax": "V",
					"ap": 1020,
					"as": 3,
					"bx": "V",
					"bp": 990,
					"bs": 5,
					"c": ["R"],
					"z": "C"
				},
				"AAPL": {
					"t": "2022-04-12T17:26:44.962998616Z",
					"ax": "V",
					"ap": 170,
					"as": 1,
					"bx": "V",
					"bp": 168.03,
					"bs": 1,
					"c": ["R"],
					"z": "C"
				}
			}
		}"#;

    let mut result = Quote::parse(response).unwrap();
    result.sort_by_key(|t| t.time);
    assert_eq!(result.len(), 2);
    assert_eq!(result[1].ask_price, Num::new(1020, 1));
    assert_eq!(result[1].ask_size, 3);
    assert_eq!(result[1].bid_price, Num::new(990, 1));
    assert_eq!(result[1].bid_size, 5);
    assert_eq!(result[1].symbol, "TSLA".to_string());
    assert_eq!(
      result[1].time,
      DateTime::parse_from_rfc3339("2022-04-12T17:26:45.009288296Z").unwrap()
    );
  }

  /// Verify that we can retrieve the last quote for an asset.
  #[test(tokio::test)]
  async fn request_last_quote() {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = LastQuoteReq::new(vec!["SPY".to_string()]);
    let quotes = client.issue::<Get>(&req).await.unwrap();
    // Just as a rough sanity check, we require that the reported time
    // is some time after two weeks before today. That should safely
    // account for any combination of holidays, weekends, etc.
    assert!(quotes[0].time >= Utc::now() - Duration::weeks(2));
  }

  /// Retrieve multiple symbols at once.
  #[test(tokio::test)]
  async fn request_last_quotes_multi() {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = LastQuoteReq::new(vec![
      "SPY".to_string(),
      "QQQ".to_string(),
      "MSFT".to_string(),
    ]);
    let quotes = client.issue::<Get>(&req).await.unwrap();
    assert_eq!(quotes.len(), 3);
    assert!(quotes[0].time >= Utc::now() - Duration::weeks(2));
  }

  /// Verify that we can specify the SIP feed as the data source to use.
  #[test(tokio::test)]
  async fn sip_feed() {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = LastQuoteReq::new(vec!["SPY".to_string()]).with_feed(Feed::SIP);

    let result = client.issue::<Get>(&req).await;
    // Unfortunately we can't really know whether the user has the
    // unlimited plan and can access the SIP feed. So really all we can
    // do here is accept both possible outcomes.
    match result {
      Ok(_) | Err(RequestError::Endpoint(GetError::InvalidInput(_))) => (),
      err => panic!("Received unexpected error: {:?}", err),
    }
  }

  /// Non-existent symbol is skipped in the result.
  #[test(tokio::test)]
  async fn nonexistent_symbol() {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = LastQuoteReq::new(vec!["SPY".to_string(), "NOSUCHSYMBOL".to_string()]);
    let quotes = client.issue::<Get>(&req).await.unwrap();
    assert_eq!(quotes.len(), 1);
  }

  /// Symbol with characters outside A-Z results in an error response from the server.
  #[test(tokio::test)]
  async fn bad_symbol() {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = LastQuoteReq::new(vec!["ABC123".to_string()]);
    let err = client.issue::<Get>(&req).await.unwrap_err();
    match err {
      RequestError::Endpoint(GetError::InvalidInput(_)) => (),
      _ => panic!("Received unexpected error: {:?}", err),
    };
  }
}
