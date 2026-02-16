use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use orderbook::{book::Orderbook, OrderTicket, OrderType, Side};

fn seed_book(ob: &mut Orderbook, levels: i64, size: i64) {
    for i in 0..levels {
        ob.accept_order(OrderTicket {
            side: Side::Buy,
            size,
            order_type: OrderType::Limit(10_000 - i),
        })
        .unwrap();

        ob.accept_order(OrderTicket {
            side: Side::Sell,
            size,
            order_type: OrderType::Limit(10_001 + i),
        })
        .unwrap();
    }
}

fn bench_market_sweeps(c: &mut Criterion) {
    c.bench_function("market_full_book_sweep_100_levels", |b| {
        b.iter_batched(
            || {
                let mut ob = Orderbook::new();
                seed_book(&mut ob, 100, 100);
                ob
            },
            |mut ob| {
                black_box(
                    ob.accept_order(OrderTicket {
                        side: Side::Buy,
                        size: 10_000, // sweep whole ask side
                        order_type: OrderType::Market,
                    })
                    .unwrap(),
                );
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_heavy_limit_insert(c: &mut Criterion) {
    c.bench_function("limit_insert_10k_orders", |b| {
        b.iter(|| {
            let mut ob = Orderbook::new();
            for i in 0..10_000 {
                black_box(
                    ob.accept_order(OrderTicket {
                        side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                        size: 1,
                        order_type: OrderType::Limit(10_000 + (i % 50) as i64),
                    })
                    .unwrap(),
                );
            }
        })
    });
}

fn bench_mixed_hft_flow(c: &mut Criterion) {
    c.bench_function("mixed_hft_50k_events", |b| {
        b.iter_batched(
            || {
                let mut ob = Orderbook::new();
                seed_book(&mut ob, 50, 50);
                ob
            },
            |mut ob| {
                for i in 0..50_000 {
                    let ticket = if i % 5 == 0 {
                        OrderTicket {
                            side: Side::Buy,
                            size: 5,
                            order_type: OrderType::Market,
                        }
                    } else if i % 5 == 1 {
                        OrderTicket {
                            side: Side::Sell,
                            size: 3,
                            order_type: OrderType::Market,
                        }
                    } else {
                        OrderTicket {
                            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                            size: 1,
                            order_type: OrderType::Limit(10_000 + (i % 20) as i64),
                        }
                    };

                    black_box(ob.accept_order(ticket).unwrap());
                }
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_fifo_queue_depth(c: &mut Criterion) {
    c.bench_function("deep_same_price_fifo_20k", |b| {
        b.iter_batched(
            || {
                let mut ob = Orderbook::new();

                // 20k orders at exact same price
                for _ in 0..20_000 {
                    ob.accept_order(OrderTicket {
                        side: Side::Sell,
                        size: 1,
                        order_type: OrderType::Limit(10_000),
                    })
                    .unwrap();
                }

                ob
            },
            |mut ob| {
                black_box(
                    ob.accept_order(OrderTicket {
                        side: Side::Buy,
                        size: 20_000,
                        order_type: OrderType::Market,
                    })
                    .unwrap(),
                );
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_large_steady_state(c: &mut Criterion) {
    c.bench_function("steady_state_200_levels_100k_events", |b| {
        b.iter_batched(
            || {
                let mut ob = Orderbook::new();
                seed_book(&mut ob, 200, 100);
                ob
            },
            |mut ob| {
                for i in 0..100_000 {
                    let ticket = if i % 7 == 0 {
                        OrderTicket {
                            side: Side::Buy,
                            size: 10,
                            order_type: OrderType::Market,
                        }
                    } else {
                        OrderTicket {
                            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                            size: 2,
                            order_type: OrderType::Limit(10_000 + (i % 100) as i64),
                        }
                    };

                    black_box(ob.accept_order(ticket).unwrap());
                }
            },
            BatchSize::LargeInput,
        )
    });
}

criterion_group!(
    benches,
    bench_market_sweeps,
    bench_heavy_limit_insert,
    bench_mixed_hft_flow,
    bench_fifo_queue_depth,
    bench_large_steady_state
);
criterion_main!(benches);
