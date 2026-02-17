# Orderbook
###### An example orderbook study using aggressive data structures and a few assumptions for simplicity and performance.

## Overview
I built a simple orderbook which holds bids and asks in memory and behaves fairly similar to any crypto orderbook you would see in a large exchange. There are limit and market orders that either make or take liquidity from the book,
and can be placed on either the bid or the ask side. 

Limit orders can be modified or cancelled. When modifying, if the price stays the same then the order keeps its place in queue and the size is altered. If the price is different then the order is cancelled and replaced at the new price. Orders are FIFO, so that the first order in line at a price gets filled first. It will be filled partially until the whole of the size is matched.

I added a test suite for the assertion of these behaviors as well as a benchmark to see the performance of the design. On my laptop, the orderbook can process over 11M orders in 1s, which I am pleased with.

```
Benchmarking one_million_event_simulation: Collecting 100 samples in estimated 12.502 
one_million_event_simulation
                        time:   [82.331 ms 85.494 ms 88.885 ms]
```

## Design
I decided on Rust as my language because it is fast enough to be realistically competative in performance and I would have less of a headache in solving bugs. I would've gone on to use the excellent Tokio suite of async tools but this project stayted within the scope.

### First Attempt
I originally wrote up a very simple design which held the bids and asks in a `Vec<Order>` where the top of book would be at the end. This was good because most action was at the top of book, and editing the array only had to shift a few orders at the rear of the array. I was getting 100k orders a second. I ran it through the AI and it told me rather rudely that was not acceptable for an order.

### Second Attempt
So I asked what data structures it recommended and it pointed me to a Price Ladder with an Arena. I coded this up based on its pseudocode for inserting, matching, modifying, and replacing orders. This proved to be a really effective set of data structures for this problem space.

### Data Structures
The reason it works so well is because of O(1) access for most operations. The arena is a Vec that holds all of the data and reuses memory by overwriting orders that arent used. This avoids allocation of memory when its supposed to be accepting orders.

The Price ladder is accessible by index and has a linked list so matching is almost constant time. Inserting and removing are constant time because insertion just pops the new order onto the tail of the queue, removing just closes a gap in the linked list.

Keeping the top of book on hand is also important because it allows you to jump right to the action for taker orders as well.

### Replayablility/Deterministic 
This was not very difficult to achieve because I made sure to use syncronous code by design. The orderbook is built using only pure functions as building blocks, making the entire behavior pure and deterministic as well. For example, orders are indexed to the price ladder with a pure mapping, and queues used in the price level as well as the arena `free_list` queue are both pure as well. Searching for the top of 

In order to replay any set of orders, just provide the orders as a list and enter each one into the book. This will result in the same state every time.

## Simplifications/Extensions
### Decimals
I did not want to use any decimals because they are imperfect on computers and rust_decimal is ideal for this but too heavy for the number crunching. I would ideally use it for serializing and deserializing the data over the network. For this I scaled up the decimals to a full integer to give me lovely indexes that I can use as pointers

### Events
I added comments where I would put events that need to be emitted to get the RECEIVED, FILLED, CANCELLED execution reports. I would ideally have some sort of channel that the book would send on without blocking and async system or thread would manage the nitty gritty there of mapping it back to subscribed users.

### ClientIds
I dont have the users provide client order id's but I would definitely want to handle that with a map of sorts.

### Further Optimizations
I would probably go on to remove the safe `.get` functions that I use to access the arrays. I did them originally becase I was preparing to have a battle with debugging and wanted to handle the case cleanly with errors. Once the logic is battle tested and has a more thorough test suite it would be safer to take those out.

I would probably also go deeper on my top of book calculations and my free_list push/pop and consider maybe bitmaps so that I can avoid scanning the arrays. I could also go very deep on the branching and inlining, so that can cut down on as many assembler instructions as possible.