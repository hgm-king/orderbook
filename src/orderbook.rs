use crate::{
    Result,
    orders::{
        LimitOrderResponse, MarketOrderResponse, Order, OrderResponse, OrderTicket, OrderType, Side,
    },
};

const INITIAL_ORDERBOOK_SIZE: usize = 500;
pub const BTC: usize = 111;

#[derive(Debug)]
pub struct Orderbook {
    pub symbol: usize,

    /// Bids are in ascending order with the best bid at the end
    pub bids: Vec<Order>,
    /// Asks are in descending order with the best ask at the end
    pub asks: Vec<Order>,

    pub event_log: Vec<OrderTicket>,

    pub current_id: usize,
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            symbol: BTC,
            bids: Vec::with_capacity(INITIAL_ORDERBOOK_SIZE),
            asks: Vec::with_capacity(INITIAL_ORDERBOOK_SIZE),
            event_log: Vec::with_capacity(INITIAL_ORDERBOOK_SIZE * 3),
            current_id: 0,
        }
    }

    fn get_top_of_book(&self, side: Side) -> Option<&Order> {
        match side {
            Side::Buy => {
                if self.bids.len() > 0 {
                    self.bids.get(self.bids.len() - 1)
                } else {
                    None
                }
            }
            Side::Sell => {
                if self.asks.len() > 0 {
                    self.asks.get(self.asks.len() - 1)
                } else {
                    None
                }
            }
        }
    }

    pub fn get_best_bid(&self) -> Option<&Order> {
        self.get_top_of_book(Side::Buy)
    }

    pub fn get_best_ask(&self) -> Option<&Order> {
        self.get_top_of_book(Side::Sell)
    }

    pub fn total_liquidity(&self, side: Side) -> i64 {
        match side {
            Side::Buy => self.bids.iter().fold(0, |acc, order| acc + order.size),
            Side::Sell => self.asks.iter().fold(0, |acc, order| acc + order.size),
        }
    }

    pub fn accept_order(&mut self, order_ticket: OrderTicket) -> Result<OrderResponse> {
        let r = match order_ticket.order_type {
            OrderType::Market => self
                .handle_taker(order_ticket.side, order_ticket.size)
                .map(OrderResponse::Market),
            OrderType::Limit(price) => {
                let crosses_book = match order_ticket.side {
                    Side::Buy => self.get_best_ask().map(|order| order.price <= price).unwrap_or_default(),
                    Side::Sell => self.get_best_bid().map(|order| order.price >= price).unwrap_or_default(),
                };

                if crosses_book {
                    self.handle_taker(order_ticket.side, order_ticket.size)
                        .map(OrderResponse::Market)
                } else {
                    self.handle_maker(order_ticket.side, price, order_ticket.size)
                        .map(OrderResponse::Limit)
                }
            }
        };

        // println!("**** Orderbook after insert: {:?}", self);
        r
    }

    /// Taker is going to eat away all of the liquidity at the top of the orderbook,
    /// filling itself up until there are no more
    fn handle_taker(&mut self, side: Side, mut size: i64) -> Result<MarketOrderResponse> {
        if size <= 0 {
            return Err(format!("Invalid order"));
        }
        // takers buy from the asks and sell to the bids
        let half = match side {
            Side::Buy => &mut self.asks,
            Side::Sell => &mut self.bids,
        };

        let mut notional = 0;

        while size != 0 {
            if half.is_empty() {
                return Err(format!(
                    "Not able to fill this order anymore, need {} more but we're empty",
                    size
                ));
            }
            let elem_index = half.len() - 1;

            let Some(bbo) = half.get_mut(elem_index) else {
                return Err(format!("Failed to fill market order, orderbook is empty"));
            };

            // we can fill the whole order at this level
            if bbo.size > size {
                notional += size * bbo.price;
                bbo.size -= size;
                size = 0;

                // notify the maker
                // self.emit(exec_type::PARTIAL_FILL, bbo.id)
            }
            // we will have to remove this level and try the next
            else {
                notional += bbo.size * bbo.price;
                size -= bbo.size;
                half.remove(elem_index);

                // notify the maker
                // self.emit(exec_type::FILL, bbo.id)
            }
        }

        Ok(MarketOrderResponse { notional, size })
    }

    fn handle_maker(&mut self, side: Side, price: i64, size: i64) -> Result<LimitOrderResponse> {
        if size <= 0 || price <= 0 {
            return Err(format!("Invalid order"));
        }
        let id = self.get_next_id();

        let new_order = Order { id, price, size };
        // println!("\n\n_______________\nInserting {:?}", new_order);

        let response = LimitOrderResponse { id };

        let half = match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        if half.is_empty() {
            // println!("Empty case, inserting");
            half.push(new_order);
            return Ok(response);
        }

        let len = half.len();

        // counting backwards because the vecs are in reverse order
        for i in 1..(len + 1) {
            let index = len - i;
            // println!("({}/{}) {}", i, len, index);
            let Some(order) = half.get(index) else {
                return Err(format!(
                    "We have manged to get out of bounds with our insertion with index of {}",
                    i
                ));
            };

            // println!("assessing {:?}", order);

            // asks need to be descending and have the smallest ask at the end
            if matches!(side, Side::Sell) {
                // println!(
                //     "Searching until we find a record that is bigger! {} > {} is {}",
                //     order.price,
                //     new_order.price,
                //     order.price > new_order.price
                // );
                if order.price > new_order.price {
                    // println!("inserting at {}", index + 1);
                    if index + 1 > len {
                        half.push(new_order);
                    } else {
                        half.insert(index + 1, new_order);
                    }
                    return Ok(response);
                }
            }
            // bids need to be ascending and have the biggest at the end
            else {
                // println!(
                //     "Searching until we find a record that is smaller! {} < {} is {}",
                //     order.price,
                //     new_order.price,
                //     order.price < new_order.price
                // );
                if order.price < new_order.price {
                    // println!("inserting at {}", index + 1);
                    if index + 1 > len {
                        half.push(new_order);
                    } else {
                        half.insert(index + 1, new_order);
                    }
                    return Ok(response);
                }
            }
        }

        // we have made it to the end, insert at the front
        // println!("Pushing onto the front");
        half.insert(0, new_order);

        Ok(response)
    }

    fn get_next_id(&mut self) -> usize {
        let id = self.current_id;
        self.current_id += 1;
        id
    }
}

#[cfg(test)]
mod test {
    use crate::{
        orderbook::Orderbook,
        orders::{OrderResponse, OrderTicket, OrderType, Side},
    };

    fn example_limit(side: Side, price: i64, size: i64) -> OrderTicket {
        OrderTicket {
            order_type: OrderType::Limit(price),
            size,
            side,
        }
    }

    fn example_market(side: Side, size: i64) -> OrderTicket {
        OrderTicket {
            order_type: OrderType::Market,
            size,
            side,
        }
    }

    #[test]
    fn sanity_check() {
        let x = [10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
        let len = x.len();
        for i in 1..(len + 1) {
            let index = len - i;
            println!("{}", x[index]);
        }
    }

    #[test]
    /// simple insert one limit
    /// and then match half and half
    fn test_orderbook_case_1() {
        let mut orderbook = Orderbook::new();
        let total_size = 10;

        // insert 1 order
        match orderbook.accept_order(example_limit(Side::Buy, 10, total_size)) {
            Ok(OrderResponse::Limit(res)) => {
                assert_eq!(res.id, 0);
            }
            Ok(OrderResponse::Market(_)) => {
                assert!(false, "We got a market response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 1);

        // fill half of the order
        match orderbook.accept_order(example_market(Side::Sell, 5)) {
            Ok(OrderResponse::Market(res)) => {
                // fill 5 size at 10 price
                assert_eq!(res.notional, 50);
            }
            Ok(OrderResponse::Limit(_)) => {
                assert!(false, "We got a limit response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 1);

        // fill last half of the order
        match orderbook.accept_order(example_market(Side::Sell, 5)) {
            Ok(OrderResponse::Market(res)) => {
                // fill 5 size at 10 price
                assert_eq!(res.notional, 50);
            }
            Ok(OrderResponse::Limit(_)) => {
                assert!(false, "We got a limit response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 0);
    }

    #[test]
    /// simple insert 2 limits
    /// and then match both
    fn test_orderbook_case_2() {
        let mut orderbook = Orderbook::new();

        // insert 1 order
        match orderbook.accept_order(example_limit(Side::Buy, 10, 5)) {
            Ok(OrderResponse::Limit(res)) => {
                assert_eq!(res.id, 0);
            }
            Ok(OrderResponse::Market(_)) => {
                assert!(false, "We got a market response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 1);

        // insert 1 order
        match orderbook.accept_order(example_limit(Side::Buy, 10, 5)) {
            Ok(OrderResponse::Limit(res)) => {
                assert_eq!(res.id, 1);
            }
            Ok(OrderResponse::Market(_)) => {
                assert!(false, "We got a market response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 2);

        // fill both of the orders
        match orderbook.accept_order(example_market(Side::Sell, 10)) {
            Ok(OrderResponse::Market(res)) => {
                assert_eq!(res.notional, 100);
            }
            Ok(OrderResponse::Limit(_)) => {
                assert!(false, "We got a limit response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 0);
    }

    #[test]
    /// simple insert 3 limits at different prices
    /// and then match 2 of them
    fn test_orderbook_case_3() {
        let mut orderbook = Orderbook::new();

        // insert 1 order
        match orderbook.accept_order(example_limit(Side::Buy, 100, 5)) {
            Ok(OrderResponse::Limit(res)) => {
                assert_eq!(res.id, 0);
            }
            Ok(OrderResponse::Market(_)) => {
                assert!(false, "We got a market response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 1);

        // insert 1 order
        match orderbook.accept_order(example_limit(Side::Buy, 120, 5)) {
            Ok(OrderResponse::Limit(res)) => {
                assert_eq!(res.id, 1);
            }
            Ok(OrderResponse::Market(_)) => {
                assert!(false, "We got a market response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 2);

        // insert 1 order
        match orderbook.accept_order(example_limit(Side::Buy, 110, 5)) {
            Ok(OrderResponse::Limit(res)) => {
                assert_eq!(res.id, 2);
            }
            Ok(OrderResponse::Market(_)) => {
                assert!(false, "We got a market response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 3);
        // println!("{:?}", orderbook.bids);

        let price = [100, 110, 120];
        for (order, price) in orderbook.bids.iter().zip(price) {
            assert_eq!(order.price, price);
        }

        // fill both of the orders
        match orderbook.accept_order(example_market(Side::Sell, 10)) {
            Ok(OrderResponse::Market(res)) => {
                // fill 0.5 size at 0.1 price
                assert_eq!(res.notional, 1150);
            }
            Ok(OrderResponse::Limit(_)) => {
                assert!(false, "We got a limit response?");
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
        assert_eq!(orderbook.bids.len(), 1);
    }

    // prove fifo
    #[test]
    fn test_orderbook_case_4() {
        let mut orderbook = Orderbook::new();
        let bid = Side::Buy;
        let ask = Side::Sell;

        // place order with id 0
        orderbook.accept_order(example_limit(bid, 2, 1)).unwrap();
        // placeorder with id 1
        orderbook.accept_order(example_limit(bid, 2, 1)).unwrap();
        // place order with id 2
        orderbook.accept_order(example_limit(bid, 2, 1)).unwrap();

        // first order is top of book
        assert_eq!(orderbook.get_best_bid().unwrap().id, 0);

        // fill order with id 0
        orderbook.accept_order(example_market(ask, 1)).unwrap();
        assert_eq!(orderbook.get_best_bid().unwrap().id, 1);

        // fill order with id 1
        orderbook.accept_order(example_market(ask, 1)).unwrap();
        assert_eq!(orderbook.get_best_bid().unwrap().id, 2);

        // fill order with id 2
        orderbook.accept_order(example_market(ask, 1)).unwrap();
        assert!(orderbook.get_best_bid().is_none());
        // we are cleared

        // order with id 3
        orderbook.accept_order(example_limit(bid, 2, 1)).unwrap();
        // order with id 4 (below bbo)
        orderbook.accept_order(example_limit(bid, 1, 1)).unwrap();
        // order with id 5
        orderbook.accept_order(example_limit(bid, 2, 1)).unwrap();

        // fill order with id 3
        orderbook.accept_order(example_market(ask, 1)).unwrap();
        assert_eq!(orderbook.get_best_bid().unwrap().id, 5);

        // fill order with id 5
        orderbook.accept_order(example_market(ask, 1)).unwrap();
        assert_eq!(orderbook.get_best_bid().unwrap().id, 4);
    }
}
