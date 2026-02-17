pub mod book;
pub mod half;

pub type Error = String;
pub type Result<T> = std::result::Result<T, Error>;

pub struct PriceSize {
    pub price: i64,
    pub size: i64,
}

#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit(i64),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct OrderTicket {
    pub order_type: OrderType,
    pub size: i64,
    pub side: Side,
}

#[derive(Default, Debug)]
pub struct Order {
    pub id: u64,
    pub price_index: usize,
    pub size: i64,

    pub prev: Option<usize>,
    pub next: Option<usize>,
}

impl Order {
    pub fn new(
        id: u64,
        price_index: usize,
        size: i64,
        prev: Option<usize>,
        next: Option<usize>,
    ) -> Self {
        Self {
            id,
            price_index,
            size,
            prev,
            next,
        }
    }

    pub fn overwrite(
        &mut self,
        id: u64,
        price_index: usize,
        size: i64,
        prev: Option<usize>,
        next: Option<usize>,
    ) {
        self.id = id;
        self.price_index = price_index;
        self.size = size;
        self.prev = prev;
        self.next = next;
    }
}

#[derive(Debug, Default)]
pub struct PriceLevel {
    pub head: Option<usize>,
    pub tail: Option<usize>,
    pub total_size: i64,
}

#[derive(Debug)]
pub enum OrderResponse {
    Market(MarketOrderResponse),
    Limit(LimitOrderResponse),
}

/// tell the caller how much they bought and at what price
#[derive(Debug)]
pub struct MarketOrderResponse {
    pub notional: i64,
    pub size: i64,
}

/// tell the user their id so they can cancel or replace
#[derive(Debug)]
pub struct LimitOrderResponse {
    pub id: u64,
}