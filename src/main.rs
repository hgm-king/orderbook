use orderbook::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, world!");

    Ok(())
}
#[cfg(test)]
mod tests {
    use orderbook::{OrderResponse, OrderTicket, OrderType, Side, book::Orderbook};

    fn limit(side: Side, price: i64, size: i64) -> OrderTicket {
        OrderTicket {
            side,
            size,
            order_type: OrderType::Limit(price),
        }
    }

    fn market(side: Side, size: i64) -> OrderTicket {
        OrderTicket {
            side,
            size,
            order_type: OrderType::Market,
        }
    }

    #[test]
    fn test_basic_limit_insert_and_top_of_book() {
        let mut ob = Orderbook::new();

        // Add bids
        ob.accept_order(limit(Side::Buy, 100, 10)).unwrap();
        ob.accept_order(limit(Side::Buy, 101, 5)).unwrap();
        ob.accept_order(limit(Side::Buy, 99, 7)).unwrap();

        // Add asks
        ob.accept_order(limit(Side::Sell, 105, 8)).unwrap();
        ob.accept_order(limit(Side::Sell, 103, 12)).unwrap();

        let best_bid = ob.get_best_bid().unwrap();
        let best_ask = ob.get_best_ask().unwrap();

        assert_eq!(best_bid.price, 101);
        assert_eq!(best_bid.size, 5);

        assert_eq!(best_ask.price, 103);
        assert_eq!(best_ask.size, 12);

        assert!(best_bid.price < best_ask.price);
    }

    #[test]
    fn test_market_order_partial_fill() {
        let mut ob = Orderbook::new();

        // Build ask side
        ob.accept_order(limit(Side::Sell, 100, 10)).unwrap();
        ob.accept_order(limit(Side::Sell, 101, 20)).unwrap();

        // Market buy that consumes only first level
        let response = ob.accept_order(market(Side::Buy, 5)).unwrap();

        match response {
            OrderResponse::Market(m) => {
                assert_eq!(m.size, 5);
                assert_eq!(m.notional, 5 * 100);
            }
            _ => panic!("Expected market response"),
        }

        let best_ask = ob.get_best_ask().unwrap();
        assert_eq!(best_ask.price, 100);
        assert_eq!(best_ask.size, 5); // 10 - 5
    }

    #[test]
    fn test_market_order_multi_level_sweep() {
        let mut ob = Orderbook::new();

        ob.accept_order(limit(Side::Sell, 100, 10)).unwrap();
        ob.accept_order(limit(Side::Sell, 101, 10)).unwrap();
        ob.accept_order(limit(Side::Sell, 102, 10)).unwrap();

        let response = ob.accept_order(market(Side::Buy, 25)).unwrap();

        match response {
            OrderResponse::Market(m) => {
                assert_eq!(m.size, 25);
                // 10@100 + 10@101 + 5@102
                let expected = 10 * 100 + 10 * 101 + 5 * 102;
                assert_eq!(m.notional, expected);
            }
            _ => panic!("Expected market response"),
        }

        let best_ask = ob.get_best_ask().unwrap();
        assert_eq!(best_ask.price, 102);
        assert_eq!(best_ask.size, 5);
    }

    #[test]
    fn test_crossing_limit_becomes_taker() {
        let mut ob = Orderbook::new();

        ob.accept_order(limit(Side::Sell, 100, 10)).unwrap();

        // This buy limit crosses the book and should execute immediately
        let response = ob.accept_order(limit(Side::Buy, 105, 5)).unwrap();

        match response {
            OrderResponse::Market(m) => {
                assert_eq!(m.size, 5);
                assert_eq!(m.notional, 5 * 100);
            }
            _ => panic!("Crossing limit should execute as market"),
        }

        let best_ask = ob.get_best_ask().unwrap();
        assert_eq!(best_ask.size, 5);
    }

    #[test]
    fn test_liquidity_tracking() {
        let mut ob = Orderbook::new();

        ob.accept_order(limit(Side::Buy, 100, 10)).unwrap();
        ob.accept_order(limit(Side::Buy, 101, 5)).unwrap();
        ob.accept_order(limit(Side::Sell, 105, 20)).unwrap();

        assert_eq!(ob.total_liquidity(Side::Buy), 15);
        assert_eq!(ob.total_liquidity(Side::Sell), 20);

        ob.accept_order(market(Side::Sell, 8)).unwrap();

        assert_eq!(ob.total_liquidity(Side::Buy), 7);
    }

    #[test]
    fn test_deterministic_simulated_order_flow() {
        let mut ob = Orderbook::new();

        // Seed book
        for i in 0..10 {
            ob.accept_order(limit(Side::Buy, 100 - i, 10)).unwrap();
            ob.accept_order(limit(Side::Sell, 101 + i, 10)).unwrap();
        }

        // Deterministic traffic simulation
        for i in 0..100 {
            if i % 3 == 0 {
                ob.accept_order(market(Side::Buy, 3)).unwrap();
            } else if i % 3 == 1 {
                ob.accept_order(market(Side::Sell, 2)).unwrap();
            } else {
                let price = 100 + (i % 5) as i64;
                ob.accept_order(limit(Side::Buy, price, 1)).unwrap();
            }

            // Invariant: if both sides exist, no crossed book
            if let (Some(bid), Some(ask)) = (ob.get_best_bid(), ob.get_best_ask()) {
                assert!(bid.price < ask.price);
            }
        }

        // Final sanity checks
        let total_bid = ob.total_liquidity(Side::Buy);
        let total_ask = ob.total_liquidity(Side::Sell);

        assert!(total_bid >= 0);
        assert!(total_ask >= 0);
    }
}
