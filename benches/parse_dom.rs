use criterion::{Criterion, black_box, criterion_group, criterion_main};
use sandslash::parser::Dom;

const FIXTURE: &str = include_str!("../tests/fixtures/basic.html");

fn bench_parse_dom(c: &mut Criterion) {
    c.bench_function("dom_parse_basic", |b| {
        b.iter(|| Dom::parse(black_box(FIXTURE)))
    });
}

criterion_group!(benches, bench_parse_dom);
criterion_main!(benches);
