use std::hint::black_box;

use std::fs;

use camino::Utf8PathBuf;
use criterion::{Criterion, criterion_group, criterion_main};
use uniflowed_config::UniflowedConfig;
use uniflowed_router::{discover_routes, generate_router_flow};

fn bench_large_route_tree(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");

    for index in 0..1_000 {
        let route = root.join(format!("app/team/[teamId]/project/[projectId]/view{index}"));
        fs::create_dir_all(&route).expect("route dir");
        fs::write(route.join("_uf.page.flow"), "// @flow\n").expect("page");
    }

    let config = UniflowedConfig::default();
    c.bench_function("discover 1000 dynamic routes", |b| {
        b.iter(|| {
            let routes = discover_routes(&root, &config).expect("routes");
            black_box(routes)
        });
    });

    let routes = discover_routes(&root, &config).expect("routes");
    c.bench_function("generate router.flow for 1000 routes", |b| {
        b.iter(|| black_box(generate_router_flow(&routes)));
    });
}

criterion_group!(benches, bench_large_route_tree);
criterion_main!(benches);
