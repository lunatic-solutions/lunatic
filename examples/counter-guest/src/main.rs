fn main() {
    unsafe {
        assert_eq!(count(), 0);
        assert_eq!(increment(1), 1);
        assert_eq!(increment(5), 6);
        assert_eq!(decrement(3), 3);
        assert_eq!(count(), 3);
        println!("Final count is {}", count());
    }
}

#[link(wasm_import_module = "lunatic::counter")]
extern "C" {
    pub fn increment(amount: i32) -> i32;
    pub fn decrement(amount: i32) -> i32;
    pub fn count() -> i32;
}
