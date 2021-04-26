use easybench::bench_env;
use lunatic::channel;

pub fn bench_channel() {
    println!(
        "Send 0 bytes: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(())
            .unwrap())
    );
    println!(
        "Send 8 bytes: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(1337u64)
            .unwrap())
    );
    println!(
        "Send 16 bytes: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send([0u8; 16])
            .unwrap())
    );
    println!(
        "Send 32 bytes: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(String::from("This is a 32 byte long string!!!"))
            .unwrap())
    );
    println!(
        "Send 64 bytes: {}",
        bench_env(channel::unbounded(), move |(sender, _r)| sender
            .send(vec![0u8; 64])
            .unwrap())
    );
    println!(
        "Send 128 bytes: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 128])
            .unwrap())
    );
    println!(
        "Send 512 bytes: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 512])
            .unwrap())
    );
    println!(
        "Send 1 Kb: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 1024])
            .unwrap())
    );
    println!(
        "Send 2 Kb: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 2 * 1024])
            .unwrap())
    );
    println!(
        "Send 4 Kb: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 4 * 1024])
            .unwrap())
    );
    println!(
        "Send 16 Kb: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 16 * 1024])
            .unwrap())
    );
    println!(
        "Send 1 Mb: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 1024 * 1024])
            .unwrap())
    );
    println!(
        "Send 5 Mb: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 5 * 1024 * 1024])
            .unwrap())
    );
    println!(
        "Send 20 Mb: {}",
        bench_env(channel::unbounded(), |(sender, _r)| sender
            .send(vec![0u8; 20 * 1024 * 1024])
            .unwrap())
    );
}
