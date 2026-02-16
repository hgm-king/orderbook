use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use orderbook::{orderbook::Orderbook, orders::{OrderTicket, OrderType, Side}};

const SCALE: i64 = 100;

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

//
// 1️⃣ Raw Limit Insert Throughput
//
fn bench_limit_inserts(c: &mut Criterion) {
    c.bench_function("limit_insert_1m", |b| {
        b.iter_batched(
            || Orderbook::new(),
            |mut book| {
                for i in 0..100000 {
                    let price = 10_000 * SCALE + (i % 100) as i64;
                    let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                    let _ = book.accept_order(black_box(limit(side, price, 10)));
                }
            },
            BatchSize::LargeInput,
        )
    });
}

//
// 2️⃣ Pure Market Sweeps (worst-case matching)
//
fn bench_full_sweep(c: &mut Criterion) {
    c.bench_function("market_full_sweep", |b| {
        b.iter_batched(
            || {
                let mut book = Orderbook::new();
                for _ in 0..100_000 {
                    book.accept_order(limit(Side::Sell, 10_000 * SCALE, 10)).unwrap();
                }
                book
            },
            |mut book| {
                let _ = book.accept_order(black_box(market(Side::Buy, 1_000_000)));
            },
            BatchSize::LargeInput,
        )
    });
}

//
// 3️⃣ Alternating Maker/Taker (realistic trading)
//
fn bench_realistic_flow(c: &mut Criterion) {
    c.bench_function("realistic_flow_500k", |b| {
        b.iter(|| {
            let mut book = Orderbook::new();

            for i in 0..500_000 {
                let price = 10_000 * SCALE + (i % 50) as i64;

                let _ = book.accept_order(limit(Side::Sell, price, 5));
                let _ = book.accept_order(limit(Side::Buy, price - 10, 5));

                if i % 3 == 0 {
                    let _ = book.accept_order(market(Side::Buy, 5));
                }

                if i % 5 == 0 {
                    let _ = book.accept_order(market(Side::Sell, 3));
                }
            }

            black_box(book);
        })
    });
}

//
// 4️⃣ Pathological Same-Price FIFO Stress
//
fn bench_fifo_pressure(c: &mut Criterion) {
    c.bench_function("fifo_pressure", |b| {
        b.iter(|| {
            let mut book = Orderbook::new();

            for _ in 0..200_000 {
                let _ = book.accept_order(limit(Side::Sell, 10_000 * SCALE, 1));
            }

            let _ = book.accept_order(market(Side::Buy, 200_000));

            black_box(book);
        })
    });
}

//
// 5️⃣ Deep Ladder Spread (many price levels)
//
fn bench_deep_ladder(c: &mut Criterion) {
    c.bench_function("deep_ladder_100k_levels", |b| {
        b.iter(|| {
            let mut book = Orderbook::new();

            for i in 0..100_000 {
                let price = 10_000 * SCALE + i as i64;
                let _ = book.accept_order(limit(Side::Sell, price, 1));
            }

            let _ = book.accept_order(market(Side::Buy, 100_000));

            black_box(book);
        })
    });
}

//
// 6️⃣ Million Mixed Operations (the brutality test)
//
fn bench_million_mixed(c: &mut Criterion) {
    c.bench_function("million_mixed_ops", |b| {
        b.iter(|| {
            let mut book = Orderbook::new();

            for i in 0..1_000_000 {
                let price = 10_000 * SCALE + (i % 100) as i64;
                let size = ((i % 10) + 1) as i64;

                match i % 4 {
                    0 => { let _ = book.accept_order(limit(Side::Buy, price, size)); }
                    1 => { let _ = book.accept_order(limit(Side::Sell, price + 10, size)); }
                    2 => { let _ = book.accept_order(market(Side::Buy, size)); }
                    _ => { let _ = book.accept_order(market(Side::Sell, size)); }
                }
            }

            black_box(book);
        })
    });
}

criterion_group!(
    benches,
    // bench_limit_inserts,
    bench_full_sweep,
    // bench_realistic_flow,
    // bench_fifo_pressure,
    // bench_deep_ladder,
    // bench_million_mixed
);

criterion_main!(benches);
