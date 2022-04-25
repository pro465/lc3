use std::mem::MaybeUninit as MU;

static mut orig_tio: MU<libc::termios> = MU::uninit();

pub fn disable_input_buffering() {
    unsafe {
        libc::tcgetattr(libc::STDIN_FILENO, orig_tio.as_mut_ptr());
        let mut new_tio = orig_tio.assume_init();
        new_tio.c_lflag &= !libc::ICANON & !libc::ECHO;
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &new_tio);
    }
}

pub fn restore_input_buffering() {
    unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, orig_tio.as_ptr());
    }
}
