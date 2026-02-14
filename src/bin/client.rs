#[path = "../sketch.rs"]
mod sketch;

fn main() {
    let addr = std::env::args().nth(1);
    sketch::run(addr);
}
