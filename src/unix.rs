use std::mem::MaybeUninit as MU;

static mut orig_tio: MU<libc::termios> = MU::uninit();

pub fn check_key() -> u16 {
    unsafe {
        let mut readfds = MU::uninit();
        libc::FD_ZERO(readfds.as_mut_ptr());
        libc::FD_SET(libc::STDIN_FILENO, readfds.as_mut_ptr());

        let mut timeout = libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        };

        (libc::select(1, readfds.as_mut_ptr(), 0 as _, 0 as _, &mut timeout) != 0) as u16
    }
}

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
