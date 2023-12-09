extern "C" {
    pub fn LLDELFLink(args: *const *const libc::c_char, size: libc::size_t) -> libc::c_int;
}
