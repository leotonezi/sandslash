use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use sandslash::{audit::page_auditors, model::PageData, parser::Dom, score::score_page};
use std::collections::HashMap;
use url::Url;

const FIXTURE: &str = include_str!("../tests/fixtures/basic.html");

fn bench_page_audit(c: &mut Criterion) {
    let url: Url = "https://example.com/"
        .parse()
        .expect("invariant: valid url");
    let page = PageData {
        url: url.clone(),
        status: 200,
        redirect_chain: vec![],
        headers: HashMap::new(),
        html: FIXTURE.to_owned(),
        depth: 0,
    };
    let auditors = page_auditors();

    let mut group = c.benchmark_group("page_audit_pipeline");
    group.throughput(Throughput::Elements(1));
    group.bench_function(BenchmarkId::new("parse_audit_score", "basic"), |b| {
        b.iter(|| {
            let dom = Dom::parse(&page.html);
            let findings: Vec<_> = auditors
                .iter()
                .flat_map(|a| a.audit(black_box(&page), &dom))
                .collect();
            score_page(page.url.clone(), findings)
        })
    });
    group.finish();
}

criterion_group!(benches, bench_page_audit);
criterion_main!(benches);
