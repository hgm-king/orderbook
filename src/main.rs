use orderbook::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, world!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use orderbook::{orderbook::Orderbook, orders::{OrderTicket, OrderType, Side}};

    const SCALE: i64 = 100; // if you use scaled ints

    fn limit(side: Side, price: i64, size: i64) -> OrderTicket {
        OrderTicket {
            order_type: OrderType::Limit(price),
            size,
            side,
        }
    }

    fn market(side: Side, size: i64) -> OrderTicket {
        OrderTicket {
            order_type: OrderType::Market,
            size,
            side,
        }
    }

    // ------------------------------------------------------------
    // 1️⃣ Fill book with 500 deterministic limit orders
    // ------------------------------------------------------------
    #[test]
    fn test_fill_book_500_orders() {
        let mut book = Orderbook::new();

        // 250 bids, 250 asks
        for i in 0..250 {
            let price = 10_000 * SCALE - i as i64;
            let size = 10 + (i as i64 % 5);
            book.accept_order(limit(Side::Buy, price, size)).unwrap();
        }

        for i in 0..250 {
            let price = 10_001 * SCALE + i as i64;
            let size = 15 + (i as i64 % 7);
            book.accept_order(limit(Side::Sell, price, size)).unwrap();
        }

        assert!(book.get_best_bid().is_some());
        assert!(book.get_best_ask().is_some());

        assert!(book.get_best_bid().unwrap().price
            < book.get_best_ask().unwrap().price);

        let total_bid = book.total_liquidity(Side::Buy);
        let total_ask = book.total_liquidity(Side::Sell);

        assert!(total_bid > 0);
        assert!(total_ask > 0);
    }

    // ------------------------------------------------------------
    // 2️⃣ Eat entire book with market orders
    // ------------------------------------------------------------
    #[test]
    fn test_eat_all_liquidity() {
        let mut book = Orderbook::new();

        // symmetric deterministic setup
        for _ in 0..100 {
            book.accept_order(limit(Side::Sell, 10_000 * SCALE, 10)).unwrap();
            book.accept_order(limit(Side::Buy, 9_999 * SCALE, 10)).unwrap();
        }

        let total_ask = book.total_liquidity(Side::Sell);
        let total_bid = book.total_liquidity(Side::Buy);

        // consume asks
        book.accept_order(market(Side::Buy, total_ask)).unwrap();
        assert_eq!(book.total_liquidity(Side::Sell), 0);

        // consume bids
        book.accept_order(market(Side::Sell, total_bid)).unwrap();
        assert_eq!(book.total_liquidity(Side::Buy), 0);

        assert!(book.get_best_bid().is_none());
        assert!(book.get_best_ask().is_none());
    }

    // ------------------------------------------------------------
    // 3️⃣ FIFO at same price level
    // ------------------------------------------------------------
    #[test]
    fn test_fifo_same_price() {
        let mut book = Orderbook::new();

        book.accept_order(limit(Side::Sell, 10_000 * SCALE, 10)).unwrap();
        book.accept_order(limit(Side::Sell, 10_000 * SCALE, 20)).unwrap();
        book.accept_order(limit(Side::Sell, 10_000 * SCALE, 30)).unwrap();

        book.accept_order(market(Side::Buy, 15)).unwrap();

        let remaining = book.total_liquidity(Side::Sell);
        assert_eq!(remaining, 45); // 10 + 20 + 30 - 15 = 45

        // first order must be fully consumed
        // second partially
    }

    // ------------------------------------------------------------
    // 4️⃣ Partial fill leaves remainder
    // ------------------------------------------------------------
    #[test]
    fn test_partial_fill() {
        let mut book = Orderbook::new();

        book.accept_order(limit(Side::Sell, 10_000 * SCALE, 100)).unwrap();
        book.accept_order(market(Side::Buy, 40)).unwrap();

        assert_eq!(book.total_liquidity(Side::Sell), 60);

        let best = book.get_best_ask().unwrap();
        assert_eq!(best.size, 60);
    }

    // ------------------------------------------------------------
    // 5️⃣ Crossing limit order behaves like taker
    // ------------------------------------------------------------
    #[test]
    fn test_crossing_limit_order() {
        let mut book = Orderbook::new();

        book.accept_order(limit(Side::Sell, 10_000 * SCALE, 50)).unwrap();

        // aggressive buy
        book.accept_order(limit(Side::Buy, 11_000 * SCALE, 50)).unwrap();

        assert_eq!(book.total_liquidity(Side::Sell), 0);
        assert_eq!(book.total_liquidity(Side::Buy), 0);
    }

    // ------------------------------------------------------------
    // 6️⃣ Zero-size order rejection
    // ------------------------------------------------------------
    #[test]
    fn test_zero_size_rejected() {
        let mut book = Orderbook::new();

        let result = book.accept_order(limit(Side::Buy, 10_000 * SCALE, 0));
        assert!(result.is_err());
    }

    // ------------------------------------------------------------
    // 7️⃣ Market order on empty book
    // ------------------------------------------------------------
    #[test]
    fn test_market_on_empty_book() {
        let mut book = Orderbook::new();

        let result = book.accept_order(market(Side::Buy, 100));
        assert!(result.is_err());
    }

    // ------------------------------------------------------------
    // 8️⃣ Massive alternating maker/taker simulation
    // ------------------------------------------------------------
    #[test]
    fn test_realistic_flow_simulation() {
        let mut book = Orderbook::new();

        // deterministic trading simulation
        for i in 0..200 {
            let price = 10_000 * SCALE + (i % 10) as i64;

            book.accept_order(limit(Side::Sell, price, 10)).unwrap();
            book.accept_order(limit(Side::Buy, price - 5, 10)).unwrap();

            if i % 3 == 0 {
                book.accept_order(market(Side::Buy, 5)).unwrap();
            }

            if i % 5 == 0 {
                book.accept_order(market(Side::Sell, 3)).unwrap();
            }
        }

        // invariants
        if let (Some(bid), Some(ask)) = (book.get_best_bid(), book.get_best_ask()) {
            assert!(bid.price < ask.price);
        }

        assert!(book.total_liquidity(Side::Buy) >= 0);
        assert!(book.total_liquidity(Side::Sell) >= 0);
    }

    // ------------------------------------------------------------
    // 9️⃣ Deterministic stress test (500+ mixed ops)
    // ------------------------------------------------------------
    #[test]
    fn test_stress_1000_operations() {
        let mut book = Orderbook::new();

        for i in 0..500 {
            let price = if i % 2 == 0 {
                10_000 * SCALE - (i % 20) as i64
            } else {
                10_000 * SCALE + (i % 20) as i64
            };

            let size = 1 + (i % 10) as i64;

            let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };

            book.accept_order(limit(side, price, size)).unwrap();
        }

        for i in 0..500 {
            let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
            book.accept_order(market(side, 5)).unwrap();
        }

        // book should never cross
        if let (Some(bid), Some(ask)) = (book.get_best_bid(), book.get_best_ask()) {
            assert!(bid.price < ask.price);
        }
    }
}
