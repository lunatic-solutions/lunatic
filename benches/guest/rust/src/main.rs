mod channel;
mod host_calls;

fn main() {
    println!("\n-- Channel --");
    channel::bench_channel();
    println!("\n-- Host calls --");
    host_calls::bench_host_calls();
}
