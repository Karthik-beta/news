fn main() {
    let args: Vec<String> = std::env::args().collect();
    std::process::exit(board_spike::run(&args));
}
