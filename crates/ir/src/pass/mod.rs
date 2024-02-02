use crate::cfg::Program;

pub mod dead_code;
pub mod lift;

pub trait Pass: Default {
    fn run(&mut self, program: &mut Program);
}
