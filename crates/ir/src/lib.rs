pub mod analysis;
pub mod cfg;
pub mod instruction;
pub mod pass;
pub mod symbol;

pub static POINTER_SIZE: usize = 32;

#[cfg(test)]
mod tests {

    #[test]
    fn it_works() {}
}
