use lunatic::channel;
use std::time::Instant;

const MANY_ITERATIONS: u128 = 10_000;
const FEW_ITERATIONS: u128 = 100;

fn main() {
    let now = Instant::now();
    send_zero_bytes();
    println!(
        "Send zero bytes: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_8_bytes();
    println!(
        "Send 8 bytes: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_16_bytes();
    println!(
        "Send 16 bytes: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_32_bytes();
    println!(
        "Send 32 bytes: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_64_bytes();
    println!(
        "Send 64 bytes: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_128_bytes();
    println!(
        "Send 128 bytes: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_512_bytes();
    println!(
        "Send 512 bytes: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_1_kb();
    println!(
        "Send 1 Kb: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_2_kb();
    println!(
        "Send 2 Kb: {} ns",
        now.elapsed().as_nanos() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_4_kb();
    println!(
        "Send 4 Kb: {} us",
        now.elapsed().as_micros() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_8_kb();
    println!(
        "Send 8 Kb: {} us",
        now.elapsed().as_micros() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_16_kb();
    println!(
        "Send 16 Kb: {} us",
        now.elapsed().as_micros() / MANY_ITERATIONS
    );

    let now = Instant::now();
    send_1_mb();
    println!(
        "Send 1 Mb: {} ms",
        now.elapsed().as_millis() / FEW_ITERATIONS
    );

    let now = Instant::now();
    send_5_mb();
    println!(
        "Send 5 Mb: {} ms",
        now.elapsed().as_millis() / FEW_ITERATIONS
    );

    let now = Instant::now();
    send_20_mb();
    println!("Send 20 Mb: {} ms", now.elapsed().as_millis() / 10);
}

fn send_zero_bytes() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        sender.send(()).unwrap();
    }
}

fn send_8_bytes() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        sender.send(1337u64).unwrap();
    }
}

fn send_16_bytes() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: [u8; 16] = [0; 16];
        sender.send(data).unwrap();
    }
}

fn send_32_bytes() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data = String::from("This is a 32 byte long string!!!");
        sender.send(data).unwrap();
    }
}

fn send_64_bytes() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 64];
        sender.send(data).unwrap();
    }
}

fn send_128_bytes() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 128];
        sender.send(data).unwrap();
    }
}

fn send_512_bytes() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 512];
        sender.send(data).unwrap();
    }
}

fn send_1_kb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 1024];
        sender.send(data).unwrap();
    }
}

fn send_2_kb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 2 * 1024];
        sender.send(data).unwrap();
    }
}

fn send_4_kb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 4 * 1024];
        sender.send(data).unwrap();
    }
}

fn send_8_kb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 8 * 1024];
        sender.send(data).unwrap();
    }
}

fn send_16_kb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..MANY_ITERATIONS {
        let data: Vec<u8> = vec![0; 16 * 1024];
        sender.send(data).unwrap();
    }
}

fn send_1_mb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..FEW_ITERATIONS {
        let data: Vec<u8> = vec![0; 1024 * 1024];
        sender.send(data).unwrap();
    }
}

fn send_5_mb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..FEW_ITERATIONS {
        let data: Vec<u8> = vec![0; 5 * 1024 * 1024];
        sender.send(data).unwrap();
    }
}

fn send_20_mb() {
    let (sender, _receiver) = channel::unbounded();
    for _ in 0..10 {
        let data: Vec<u8> = vec![0; 20 * 1024 * 1024];
        sender.send(data).unwrap();
    }
}
