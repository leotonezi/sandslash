use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use sandslash::{
    config::CrawlConfig,
    fetcher::{Fetcher, rate_limiter::HostRateLimiter},
};
use std::{num::NonZeroU32, sync::Arc};
use tokio::runtime::Runtime;
use url::Url;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

fn bench_fetch(c: &mut Criterion) {
    let rt = Runtime::new().expect("invariant: tokio runtime");

    let (server, fetcher, target) = rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string("<html><head><title>bench</title></head><body></body></html>"),
            )
            .mount(&server)
            .await;

        let root: Url = format!("{}/", server.uri())
            .parse()
            .expect("invariant: valid url");
        let config = CrawlConfig {
            root: root.clone(),
            depth: 0,
            concurrency: 32,
            rate_per_host: 10_000,
            redis_url: None,
            user_agent: CrawlConfig::DEFAULT_UA.to_owned(),
            timeout_secs: 10,
            max_pages: None,
            global_timeout_secs: None,
            respect_robots: false,
            validate_sitemap: false,
            quiet: true,
            no_color: true,
            verbose: false,
            output_json: None,
            check_external_links: false,
        };
        let limiter = Arc::new(HostRateLimiter::new(
            NonZeroU32::new(10_000).expect("invariant: nonzero"),
        ));
        let fetcher = Arc::new(Fetcher::new(&config, limiter).expect("invariant: fetcher"));
        (server, fetcher, root)
    });

    let mut group = c.benchmark_group("fetch_throughput");
    group.throughput(Throughput::Elements(1));

    for &conc in &[1usize, 4, 16, 32] {
        group.bench_with_input(BenchmarkId::from_parameter(conc), &conc, |b, &conc| {
            b.to_async(&rt).iter(|| {
                let fetcher = Arc::clone(&fetcher);
                let target = target.clone();
                async move {
                    let mut futs = FuturesUnordered::new();
                    for _ in 0..conc {
                        let f = Arc::clone(&fetcher);
                        let u = target.clone();
                        futs.push(async move { f.fetch(&u).await });
                    }
                    while let Some(r) = futs.next().await {
                        r.expect("invariant: mock fetch ok");
                    }
                }
            });
        });
    }

    group.finish();
    drop(server); // keep MockServer alive until all bench iterations complete
}

criterion_group!(benches, bench_fetch);
criterion_main!(benches);
