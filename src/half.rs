use std::collections::HashMap;

use crate::{Order, PriceLevel, PriceSize, Result, Side};

#[derive(Debug)]
pub struct HalfBook {
    pub min_price: i64,
    pub max_price: i64,
    pub tick_size: i64,
    pub side: Side,
    orders: Vec<PriceLevel>,
    pub top_of_book: Option<usize>,
    arena: Vec<Order>,
    free_list: Vec<usize>,
    ids: HashMap<u64, usize>,
}

impl HalfBook {
    pub fn new(side: Side, max_price: i64, min_price: i64, tick_size: i64) -> Self {
        let ladder_size = ((max_price - min_price) / tick_size + 1) as usize;
        Self {
            min_price,
            max_price,
            tick_size,
            side,
            top_of_book: None,
            orders: (0..ladder_size).map(|_| Default::default()).collect(),
            arena: (0..ladder_size).map(|_| Default::default()).collect(),
            free_list: (0..ladder_size).collect(),
            ids: HashMap::with_capacity(1000),
        }
    }

    pub fn insert(&mut self, id: u64, price: i64, size: i64) -> Result<()> {
        if price <= 0 || size <= 0 {
            return Err(format!("Invalid order"));
        }
        // Compute price_index.
        let price_index = self.calculate_price_index(price);

        // Push new Order into arena → get index.
        let arena_index = match self.free_list.pop() {
            Some(arena_index) => arena_index,
            None => {
                let order = Order::default();
                self.arena.push(order);
                self.arena.len() - 1
            }
        };

        // Append to level tail.
        let Some(level) = self.orders.get_mut(price_index) else {
            return Err(format!(
                "Out of bounds on the price level somehow with {}",
                price_index
            ));
        };

        level.total_size += size;

        if level.head.is_none() {
            level.head = Some(arena_index);
        }

        let old_tail = level.tail;
        if let Some(tail_index) = old_tail {
            let Some(prev_order) = self.arena.get_mut(tail_index) else {
                return Err(format!(
                    "The tail cant be gotten from the arena {}",
                    tail_index
                ));
            };
            prev_order.next = Some(arena_index);
        }
        level.tail = Some(arena_index);

        let Some(order) = self.arena.get_mut(arena_index) else {
            return Err(format!(
                "We tried to get from arena index {} but it was out of bounds!",
                arena_index
            ));
        };
        order.overwrite(id, price_index, size, old_tail, None);

        // Insert into HashMap.
        self.ids.insert(id, arena_index);

        if matches!(self.side, Side::Buy) {
            match self.top_of_book {
                None => {
                    self.top_of_book = Some(price_index);
                }
                Some(tob) => {
                    if tob < price_index {
                        self.top_of_book = Some(price_index);
                    }
                }
            }
        } else {
            match self.top_of_book {
                None => {
                    self.top_of_book = Some(price_index);
                }
                Some(tob) => {
                    if tob > price_index {
                        self.top_of_book = Some(price_index);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn remove(&mut self, id: u64) -> Result<()> {
        // Lookup arena index via HashMap.
        let Some(arena_index) = self.ids.remove(&id) else {
            return Err(format!("This order with id {} is not in our ids map!", id));
        };

        let Some(order) = self.arena.get_mut(arena_index) else {
            return Err(format!(
                "This order with id {} is not in our arena at index {}!",
                id, arena_index
            ));
        };

        let Some(level) = self.orders.get_mut(order.price_index) else {
            return Err(format!(
                "This order with id {} is not in our orders at index {}!",
                id, order.price_index
            ));
        };
        // if we were the first, shift the head to our next
        if level.head.map(|h| h == arena_index).unwrap_or_default() {
            level.head = order.next;
        }
        // if we were the last, then the tail is our prev
        if level.tail.map(|t| t == arena_index).unwrap_or_default() {
            level.tail = order.prev;
        }

        level.total_size -= order.size;

        // these will prevent borrow issues
        let next = order.next;
        let prev = order.prev;
        let price_index = order.price_index;
        let total_size = level.total_size;

        self.remove_order_from_linked_list(prev, next)?;

        // if we are removing our TOB
        if let Some(tob) = self.top_of_book {
            if tob == price_index && total_size == 0 {
                self.top_of_book = self.find_next_best_level(tob);
            }
        }

        // Mark arena slot reusable.
        self.free_list.push(arena_index);

        Ok(())
    }

    pub fn modify(&mut self, id: u64, price: i64, size: i64) -> Result<()> {
        let price_index = self.calculate_price_index(price);
        let Some(arena_index) = self.ids.get(&id) else {
            return Err(format!("This order with id {} is not in our ids map!", id));
        };

        let Some(order) = self.arena.get_mut(*arena_index) else {
            return Err(format!(
                "This order with id {} is not in our arena map {}!",
                id, arena_index
            ));
        };

        if order.price_index != price_index {
            self.remove(id)?;
            self.insert(id, price, size)?;
        } else {
            let Some(level) = self.orders.get_mut(order.price_index) else {
                return Err(format!(
                    "This order with id {} is not in our orders at index {}!",
                    id, order.price_index
                ));
            };

            level.total_size -= order.size;
            level.total_size += size;
            order.size = size;
        }

        Ok(())
    }

    pub fn match_size(&mut self, mut size: i64) -> Result<i64> {
        if size == 0 {
            return Err(format!("Invalid order"));
        }

        let mut notional = 0;

        while size > 0 {
            let Some(tob) = self.top_of_book else {
                return Ok(notional);
            };

            // We repeatedly reborrow the price level in small scopes
            loop {
                let order_index = {
                    let Some(level) = self.orders.get_mut(tob) else {
                        return Err("Failed to get price level".into());
                    };

                    if size <= 0 || level.total_size <= 0 {
                        break;
                    }

                    level.head
                };

                let Some(order_index) = order_index else {
                    break;
                };

                // Now arena borrow is separate
                let (id, traded, order_empty) = {
                    let Some(order) = self.arena.get_mut(order_index) else {
                        return Err(format!("Arena access failed at {}", order_index));
                    };

                    let traded = size.min(order.size);
                    order.size -= traded;

                    (order.id, traded, order.size == 0)
                };

                // Now update size + price level again in fresh borrow
                {
                    let Some(level) = self.orders.get_mut(tob) else {
                        return Err("Failed to reborrow level".into());
                    };

                    level.total_size -= traded;
                }

                size -= traded;
                notional += traded * self.get_price_from_index(tob);

                if order_empty {
                    self.remove_head_of_price_level(tob)?;
                    self.ids.remove(&id);
                    self.free_list.push(order_index);
                }
            }

            // Fresh borrow again
            let empty = {
                let Some(level) = self.orders.get(tob) else {
                    return Err("Level missing".into());
                };
                level.total_size == 0
            };

            if empty {
                self.top_of_book = self.find_next_best_level(tob);
            }
        }

        Ok(notional)
    }

    pub fn get_total_liquidity(&self) -> i64 {
        self.orders
            .iter()
            .fold(0, |acc, order| acc + order.total_size)
    }

    pub fn get_top_of_book(&self) -> Option<PriceSize> {
        self.top_of_book.and_then(|tob| {
            self.orders.get(tob).map(|order| PriceSize {
                size: order.total_size,
                price: self.get_price_from_index(tob),
            })
        })
    }

    /// Given the side and the current top of book,
    /// scan for the nearest populated level
    fn find_next_best_level(&self, mut tob: usize) -> Option<usize> {
        if matches!(self.side, Side::Buy) {
            // best bids are towards the end of array
            // but we must look to the left for the next
            // price level that has a size
            if tob == 0 {
                return None;
            }

            while tob > 0 {
                tob -= 1;
                if let Some(price_level) = self.orders.get(tob) {
                    if price_level.total_size != 0 {
                        return Some(tob);
                    }
                }
            }

            return None;
        } else {
            // best asks are towards the front of array
            // but we must look to the right for the next
            // price level that has a size
            if tob == self.orders.len() {
                return None;
            }

            while tob < self.orders.len() {
                tob += 1;
                if let Some(price_level) = self.orders.get(tob) {
                    if price_level.total_size != 0 {
                        return Some(tob);
                    }
                }
            }

            return None;
        }
    }

    /// Given an orders previous and next order pointers,
    /// access those orders and connect them so that
    /// order.prev.next -> order.next
    /// order.next.prev -> order.prev
    fn remove_order_from_linked_list(
        &mut self,
        prev: Option<usize>,
        next: Option<usize>,
    ) -> Result<()> {
        // if we had a previous node
        if let Some(prev) = prev {
            let Some(prev_order) = self.arena.get_mut(prev) else {
                return Err(format!(
                    "The prev order with id {} is not in our arena!",
                    prev
                ));
            };

            // point it to our next
            prev_order.next = next;
        }

        // if we had a next node
        if let Some(next) = next {
            let Some(next_order) = self.arena.get_mut(next) else {
                return Err(format!(
                    "The next order with id {} is not in our arena!",
                    next
                ));
            };

            // point it to our prev
            next_order.prev = prev;
        }

        Ok(())
    }

    /// Given a price index, remove the head order
    /// and keep the order chain up to date
    fn remove_head_of_price_level(&mut self, index: usize) -> Result<()> {
        let Some(price_level) = self.orders.get_mut(index) else {
            return Err(format!(
                "Failed to access the price level for this index {}",
                index
            ));
        };

        if let Some(head_arena_index) = price_level.head {
            let Some(head_order) = self.arena.get_mut(head_arena_index) else {
                return Err(format!(
                    "Failed to access the price level for this index {}",
                    index
                ));
            };
            let prev = head_order.prev;
            let next = head_order.next;
            price_level.total_size -= head_order.size;

            if let Some(tail) = price_level.tail {
                if tail == head_arena_index {
                    price_level.tail = None;
                }
            }

            price_level.head = head_order.next;

            self.remove_order_from_linked_list(prev, next)?;
        }

        Ok(())
    }

    /// index = (price - min_price) / tick_size
    fn calculate_price_index(&self, price: i64) -> usize {
        ((price - self.min_price) / self.tick_size) as usize
    }

    fn get_price_from_index(&self, index: usize) -> i64 {
        index as i64 + self.min_price
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_PRICE: i64 = 1;
    const MAX_PRICE: i64 = 9;
    const TICK_SIZE: i64 = 1;
    const LADDER_SIZE: usize = ((MAX_PRICE - MIN_PRICE) / TICK_SIZE + 1) as usize;

    fn buy_book() -> HalfBook {
        HalfBook::new(Side::Buy, MAX_PRICE, MIN_PRICE, TICK_SIZE)
    }

    fn sell_book() -> HalfBook {
        HalfBook::new(Side::Sell, MAX_PRICE, MIN_PRICE, TICK_SIZE)
    }

    // --------------------------------------------------------
    // INSERT TESTS
    // --------------------------------------------------------

    #[test]
    fn insert_single_order() {
        let mut book = buy_book();
        let price = 3;
        book.insert(1, price, 100).unwrap();

        let level = &book.orders[book.calculate_price_index(3)];
        assert!(level.head.is_some());
        assert_eq!(level.head, level.tail);
        assert_eq!(book.ids.get(&1).is_some(), true);
    }

    #[test]
    fn insert_multiple_same_price_preserves_linked_list() {
        let mut book = buy_book();
        let price = 2;
        book.insert(1, price, 10).unwrap();
        book.insert(2, price, 20).unwrap();
        book.insert(3, price, 30).unwrap();

        let level = &book.orders[book.calculate_price_index(2)];

        assert!(level.head.is_some());
        assert!(level.tail.is_some());

        let head = level.head.unwrap();
        let tail = level.tail.unwrap();

        assert_ne!(head, tail);

        // Ensure tail has no next
        assert_eq!(book.arena[tail].next, None);

        // Ensure head has no prev
        assert_eq!(book.arena[head].prev, None);
    }

    #[test]
    fn insert_out_of_bounds_price_fails() {
        let mut book = buy_book();

        let result = book.insert(1, 999, 10);

        assert!(result.is_err());
    }

    #[test]
    fn insert_duplicate_id_overwrites_hashmap_entry() {
        let mut book = buy_book();

        book.insert(1, 1, 10).unwrap();
        book.insert(1, 1, 20).unwrap();

        // HashMap should contain only one entry
        assert_eq!(book.ids.len(), 1);
    }

    // --------------------------------------------------------
    // REMOVE TESTS
    // --------------------------------------------------------

    #[test]
    fn remove_non_existent_id_fails() {
        let mut book = buy_book();

        let result = book.remove(42);
        assert!(result.is_err());
    }

    #[test]
    fn remove_only_order_in_level() {
        let mut book = buy_book();

        book.insert(1, 4, 100).unwrap();
        book.remove(1).unwrap();

        let level = &book.orders[4];

        assert!(level.head.is_none());
        assert!(level.tail.is_none());
        assert!(book.ids.is_empty());
    }

    #[test]
    fn remove_head_of_multiple_orders() {
        let mut book = buy_book();
        let price = 5;
        book.insert(1, price, 10).unwrap();
        book.insert(2, price, 20).unwrap();

        let head_index = book.orders[book.calculate_price_index(price)].head.unwrap();
        let head_id = book.arena[head_index].id;

        book.remove(head_id).unwrap();

        let level = &book.orders[5];
        assert_ne!(level.head, Some(head_index));
        assert!(book.ids.get(&head_id).is_none());
    }

    #[test]
    fn remove_tail_of_multiple_orders() {
        let mut book = buy_book();
        let price = 6;
        book.insert(1, price, 10).unwrap();
        book.insert(2, price, 20).unwrap();

        let tail_index = book.orders[book.calculate_price_index(price)].tail.unwrap();
        let tail_id = book.arena[tail_index].id;

        book.remove(tail_id).unwrap();

        let level = &book.orders[book.calculate_price_index(price)];
        assert_ne!(level.tail, Some(tail_index));
        assert!(book.ids.get(&tail_id).is_none());
    }

    #[test]
    fn remove_middle_order_relinks_neighbors() {
        let mut book = buy_book();
        let price = 7;
        book.insert(1, price, 10).unwrap();
        book.insert(2, price, 20).unwrap();
        book.insert(3, price, 30).unwrap();

        book.remove(2).unwrap();

        let level = &book.orders[book.calculate_price_index(7)];
        let head = level.head.unwrap();
        let next = book.arena[head].next.unwrap();

        assert_eq!(book.arena[next].id, 3);
        assert_eq!(book.arena[next].prev, Some(head));
    }

    // --------------------------------------------------------
    // MODIFY TESTS
    // --------------------------------------------------------

    #[test]
    fn modify_size_same_price() {
        let mut book = buy_book();

        book.insert(1, 3, 50).unwrap();
        book.modify(1, 3, 100).unwrap();

        let arena_index = *book.ids.get(&1).unwrap();
        assert_eq!(book.arena[arena_index].size, 100);
    }

    #[test]
    fn modify_price_moves_order_between_levels() {
        let mut book = buy_book();

        book.insert(1, 1, 50).unwrap();
        book.modify(1, 2, 60).unwrap();

        assert!(book.orders[book.calculate_price_index(1)].head.is_none());
        assert!(book.orders[book.calculate_price_index(2)].head.is_some());
    }

    #[test]
    fn modify_non_existent_order_fails() {
        let mut book = buy_book();

        let result = book.modify(999, 1, 10);
        assert!(result.is_err());
    }

    // --------------------------------------------------------
    // FREE LIST / ARENA REUSE
    // --------------------------------------------------------

    #[test]
    fn arena_slot_reused_after_remove() {
        let mut book = buy_book();

        book.insert(1, 1, 10).unwrap();
        let arena_index = *book.ids.get(&1).unwrap();

        book.remove(1).unwrap();

        let free_len_before = book.free_list.len();

        book.insert(2, 1, 20).unwrap();

        let new_index = *book.ids.get(&2).unwrap();

        assert_eq!(arena_index, new_index);
        assert_eq!(book.free_list.len(), free_len_before - 1);
    }

    // --------------------------------------------------------
    // STRESS / EDGE
    // --------------------------------------------------------

    #[test]
    fn many_inserts_trigger_arena_growth() {
        let mut book = buy_book();

        let count = LADDER_SIZE + 10;

        for i in 0..count {
            book.insert(i as u64, 1, 1).unwrap();
        }

        assert!(book.arena.len() >= count);
    }

    #[test]
    fn remove_twice_should_fail_second_time() {
        let mut book = buy_book();

        book.insert(1, 1, 10).unwrap();
        book.remove(1).unwrap();

        assert!(book.remove(1).is_err());
    }

    #[test]
    fn top_of_book_works_for_bids() {
        let mut book = buy_book();

        book.insert(1, 1, 10).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.insert(2, 2, 10).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(2)));

        book.insert(3, 1, 10).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(2)));

        book.remove(2).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.remove(1).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.remove(3).unwrap();
        assert_eq!(book.top_of_book, None);

        assert!(book.remove(1).is_err());
    }

    #[test]
    fn top_of_book_works_for_asks() {
        let mut book = sell_book();

        book.insert(1, 1, 10).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.insert(2, 2, 10).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.insert(3, 1, 10).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.remove(2).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.remove(1).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(1)));

        book.remove(3).unwrap();

        assert_eq!(book.top_of_book, None);

        assert!(book.remove(1).is_err());
    }

    // ------------------------------------------------------------
    // 1. Multi-level FIFO + partial market sweep (SELL book hit)
    // ------------------------------------------------------------
    #[test]
    fn test_market_sweep_multiple_price_levels_sell_book() {
        let mut book = sell_book();

        // Insert ascending ask prices (best ask = lowest price)
        book.insert(1, 2, 10).unwrap(); // 10 @ 2
        book.insert(2, 3, 5).unwrap(); // 5  @ 3
        book.insert(3, 4, 20).unwrap(); // 20 @ 4

        // Market buy of size 12
        let notional = book.match_size(12).unwrap();

        // Should consume:
        // 10 @ 2  = 20
        // 2  @ 3  = 6
        assert_eq!(notional, 26);

        // Remaining:
        // 3 @ 3
        // 20 @ 4
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(3)));
    }

    // ------------------------------------------------------------
    // 2. FIFO inside same level
    // ------------------------------------------------------------
    #[test]
    fn test_fifo_within_price_level() {
        let mut book = sell_book();

        book.insert(1, 5, 10).unwrap(); // first
        book.insert(2, 5, 15).unwrap(); // second

        // Match 12 -> should fully consume id=1 (10)
        // and partially id=2 (2)
        let notional = book.match_size(12).unwrap();

        assert_eq!(notional, 12 * 5);

        // Order 1 must be gone
        assert!(!book.ids.contains_key(&1));

        // Order 2 should still exist with 13 left
        let idx = book.ids.get(&2).unwrap();
        let order = &book.arena[*idx];
        assert_eq!(order.size, 13);
    }

    // ------------------------------------------------------------
    // 3. Full book sweep
    // ------------------------------------------------------------
    #[test]
    fn test_full_book_sweep_clears_top_of_book() {
        let mut book = sell_book();

        book.insert(1, 2, 5).unwrap();
        book.insert(2, 3, 5).unwrap();

        let notional = book.match_size(10).unwrap();

        assert_eq!(notional, 5 * 2 + 5 * 3);

        // Entire book empty
        assert!(book.top_of_book.is_none());
    }

    // ------------------------------------------------------------
    // 4. Buy book top-of-book transitions downward
    // ------------------------------------------------------------
    #[test]
    fn test_buy_book_tob_moves_down_after_match() {
        let mut book = buy_book();

        // For buys, higher price is better
        book.insert(1, 8, 10).unwrap(); // best bid
        book.insert(2, 6, 10).unwrap();

        assert_eq!(book.top_of_book, Some(book.calculate_price_index(8)));

        // Market sell hits best bid
        book.match_size(10).unwrap();

        // Now best bid should be 6
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(6)));
    }

    // ------------------------------------------------------------
    // 5. Modify across price levels
    // ------------------------------------------------------------
    #[test]
    fn test_modify_price_moves_order_between_levels() {
        let mut book = sell_book();

        book.insert(1, 5, 10).unwrap();
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(5)));

        // Move order to better ask (lower price)
        book.modify(1, 3, 10).unwrap();

        assert_eq!(book.top_of_book, Some(book.calculate_price_index(3)));

        // Old level should now be empty
        let old_idx = book.calculate_price_index(5);
        assert_eq!(book.orders[old_idx].total_size, 0);
    }

    // ------------------------------------------------------------
    // 6. Interleaved insert/remove/match scenario
    // ------------------------------------------------------------
    #[test]
    fn test_complex_sequence() {
        let mut book = sell_book();

        book.insert(1, 2, 10).unwrap();
        book.insert(2, 3, 10).unwrap();
        book.insert(3, 4, 10).unwrap();

        // Cancel middle level
        book.remove(2).unwrap();

        // Market buy 15
        let notional = book.match_size(15).unwrap();

        // Should take:
        // 10 @ 2 = 20
        // 5  @ 4 = 20
        assert_eq!(notional, 40);

        // Only 5 left at price 4
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(4)));
        let idx = book.ids.get(&3).unwrap();
        let order = &book.arena[*idx];
        assert_eq!(order.size, 5);
    }

    // ------------------------------------------------------------
    // 7. Partial match does NOT move top-of-book
    // ------------------------------------------------------------
    #[test]
    fn test_partial_match_keeps_same_tob() {
        let mut book = sell_book();

        book.insert(1, 2, 10).unwrap();

        // Match less than available
        let notional = book.match_size(5).unwrap();
        assert_eq!(notional, 10);

        // TOB should remain at price 2
        assert_eq!(book.top_of_book, Some(book.calculate_price_index(2)));

        let idx = book.ids.get(&1).unwrap();
        let order = &book.arena[*idx];
        assert_eq!(order.size, 5);
    }
}
