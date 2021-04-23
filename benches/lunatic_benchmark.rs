mod instance_creation;

use criterion::{criterion_group, criterion_main};

criterion_group!(benches, instance_creation::instance_creation);
criterion_main!(benches);
