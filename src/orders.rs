#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit(i64)
}

#[derive(Copy, Clone, Debug)]
pub enum Side {
    Buy,
    Sell
}

#[derive(Debug, Clone)]
pub struct OrderTicket {
    pub order_type: OrderType,
    pub size: i64,
    pub side: Side
}

#[derive(Debug, PartialEq)]
pub struct Order {
    pub id: usize,
    pub price: i64,
    pub size: i64
}

#[derive(Debug)]
pub enum OrderResponse {
    Market(MarketOrderResponse),
    Limit(LimitOrderResponse)
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
    pub id: usize,
}