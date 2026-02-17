use crate::{
    LimitOrderResponse, MarketOrderResponse, OrderResponse, OrderTicket, OrderType, PriceSize,
    Result, Side, half::HalfBook,
};

const MIN_PRICE: i64 = 1;
const MAX_PRICE: i64 = 999999;
const TICK_SIZE: i64 = 1;

#[derive(Debug)]
pub struct Orderbook {
    /// Bids are an arena
    pub bids: HalfBook,
    /// Asks are an arena
    pub asks: HalfBook,

    pub event_log: Vec<OrderTicket>,

    pub current_id: u64,
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            bids: HalfBook::new(Side::Buy, MAX_PRICE, MIN_PRICE, TICK_SIZE),
            asks: HalfBook::new(Side::Sell, MAX_PRICE, MIN_PRICE, TICK_SIZE),
            event_log: Vec::with_capacity(1000),
            current_id: 0,
        }
    }

    fn get_top_of_book(&self, side: Side) -> Option<PriceSize> {
        match side {
            Side::Sell => self.asks.get_top_of_book(),
            Side::Buy => self.bids.get_top_of_book(),
        }
    }

    pub fn get_best_bid(&self) -> Option<PriceSize> {
        self.get_top_of_book(Side::Buy)
    }

    pub fn get_best_ask(&self) -> Option<PriceSize> {
        self.get_top_of_book(Side::Sell)
    }

    pub fn total_liquidity(&self, side: Side) -> i64 {
        match side {
            Side::Sell => self.asks.get_total_liquidity(),
            Side::Buy => self.bids.get_total_liquidity(),
        }
    }

    pub fn accept_order(&mut self, order_ticket: OrderTicket) -> Result<OrderResponse> {
        match order_ticket.order_type {
            OrderType::Market => self
                .handle_taker(order_ticket.side, order_ticket.size)
                .map(OrderResponse::Market),
            OrderType::Limit(price) => {
                let crosses_book = match order_ticket.side {
                    Side::Buy => self
                        .get_best_ask()
                        .map(|order| order.price <= price)
                        .unwrap_or_default(),
                    Side::Sell => self
                        .get_best_bid()
                        .map(|order| order.price >= price)
                        .unwrap_or_default(),
                };

                if crosses_book {
                    self.handle_taker(order_ticket.side, order_ticket.size)
                        .map(OrderResponse::Market)
                } else {
                    self.handle_maker(order_ticket.side, price, order_ticket.size)
                        .map(OrderResponse::Limit)
                }
            }
        }
    }

    fn handle_taker(&mut self, side: Side, size: i64) -> Result<MarketOrderResponse> {
        let notional = match side {
            Side::Sell => self.bids.match_size(size)?,
            Side::Buy => self.asks.match_size(size)?,
        };

        Ok(MarketOrderResponse { notional, size })
    }

    fn handle_maker(&mut self, side: Side, price: i64, size: i64) -> Result<LimitOrderResponse> {
        let id = self.get_next_id();
        match side {
            Side::Sell => self.asks.insert(id, price, size)?,
            Side::Buy => self.bids.insert(id, price, size)?,
        };

        Ok(LimitOrderResponse { id })
    }

    fn get_next_id(&mut self) -> u64 {
        let id = self.current_id;
        self.current_id += 1;
        id
    }
}
