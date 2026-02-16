# Task Option 1: Deterministic Perpetual Order Book Core

## Objective
Design and implement a minimal off-chain perpetual order book with deterministic matching and replayability. Write it in the language you prefer and justify why you used the language in the design document.

## Problem Statement
Build a single-market perpetual order book supporting:
- Limit and market orders
- FIFO matching
- Partial fills
- Cancel/replace

The system must be deterministic under replay: given the same event log, it must always reconstruct identical states.

## Requirements

In-memory implementation is acceptable
Clear separation between:

- Order intake
- Matching logic
- State mutation

- Event log or journal abstraction
- Lightweight runnable demo (CLI or tests)

## Design Expectations

- How determinism is guaranteed
- How state is represented and replayed
- Tradeoffs between simplicity and extensibility

## Evaluation Signals

- Correct matching behavior
- Clean state transitions
- Explicit assumptions and questions raised
- Awareness of edge cases