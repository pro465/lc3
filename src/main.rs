use crossterm::terminal;
use lc3::*;
use std::env;

struct Term;

impl Drop for Term {
    fn drop(&mut self) {
        terminal::disable_raw_mode().expect("could not disable raw mode");
    }
}

fn handle_signal(_signal: libc::c_int) {
    terminal::disable_raw_mode().expect("could not disable raw mode");
    println!();
    std::process::exit(-2);
}

fn main() {
    let mut vm = Vm::new();

    let mut one_img = false;

    for i in env::args().skip(1) {
        vm.load_image(&i);
        one_img = true;
    }

    if !one_img {
        println!("usage: lc3 (path)+");
        return;
    }

    terminal::enable_raw_mode().expect("could not enable raw mode");

    let term = Term;

    unsafe {
        libc::signal(libc::SIGINT, handle_signal as _);
    }

    vm.run();

    drop(term)
}
