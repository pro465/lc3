use lc3::*;
use std::env;

struct Term;

impl Drop for Term {
    fn drop(&mut self) {
        restore_input_buffering();
    }
}

fn handle_signal(_signal: libc::c_int) {
    restore_input_buffering();
    println!();
    std::process::exit(-2);
}

fn main() {
    let mut vm = Vm::new();

    let one_img = false;

    for i in env::args().skip(1) {
        vm.load_image(&i);
        one_img = true;
    }

    if !one_img {
        println!("usage: lc3 [psth] ...");
        return;
    }

    disable_input_buffering();

    let term = Term;

    unsafe {
        libc::signal(libc::SIGINT, handle_signal as _);
    }

    vm.run();

    drop(term)
}
