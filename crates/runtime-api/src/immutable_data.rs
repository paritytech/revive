//! Allocates memory for the immutable data in a separate module.
//!
//! Because we only know how many immutable variables were set after
//! translating the whole contract code, we want to set the size at
//! last. However, array types need a size upon declaration.
//!
//! A simple work around is to replace it during link time.
//! To quote the [LLVM docs][0]:
//!
//! > For global variable declarations [..] the allocation size and
//! > alignment of the definition it resolves to must be greater than
//! > or equal to that of the declaration [..]
//!
//! To adhere to this we initially declare a length of 0 in
//! `revive-llvm-context`.
//!
//! [0]: https://llvm.org/docs/LangRef.html#global-variables

/// The immutable data module name.
pub static MODULE_NAME: &str = "__evm_immutables";
/// The immutable data global pointer.
pub static GLOBAL_IMMUTABLE_DATA_POINTER: &str = "__immutable_data_ptr";
/// The immutable data global size.
pub static GLOBAL_IMMUTABLE_DATA_SIZE: &str = "__immutable_data_size";
/// The immutable data maximum size in bytes.
pub static IMMUTABLE_DATA_MAX_SIZE: u32 = 4 * 1024;

/// Returns the immutable data global type.
pub fn data_type(context: &inkwell::context::Context, size: u32) -> inkwell::types::ArrayType<'_> {
    context
        .custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
        .array_type(size)
}

/// Returns the immutable data size global type.
pub fn size_type(context: &inkwell::context::Context) -> inkwell::types::IntType<'_> {
    context.custom_width_int_type(revive_common::BIT_LENGTH_X32 as u32)
}

/// Creates a LLVM module with the immutable data and its `size` in bytes.
pub fn module(context: &inkwell::context::Context, size: u32) -> inkwell::module::Module<'_> {
    let module = context.create_module(MODULE_NAME);
    let length = size / revive_common::BYTE_LENGTH_WORD as u32;

    let immutable_data = module.add_global(
        data_type(context, length),
        Default::default(),
        GLOBAL_IMMUTABLE_DATA_POINTER,
    );
    immutable_data.set_linkage(inkwell::module::Linkage::External);
    immutable_data.set_visibility(inkwell::GlobalVisibility::Default);
    immutable_data.set_initializer(&data_type(context, length).get_undef());

    let immutable_data_size = module.add_global(
        size_type(context),
        Default::default(),
        GLOBAL_IMMUTABLE_DATA_SIZE,
    );
    immutable_data_size.set_linkage(inkwell::module::Linkage::External);
    immutable_data_size.set_visibility(inkwell::GlobalVisibility::Default);
    immutable_data_size.set_initializer(&size_type(context).const_int(size as u64, false));

    module
}

#[cfg(test)]
mod tests {
    use crate::immutable_data::*;

    #[test]
    fn it_works() {
        inkwell::targets::Target::initialize_riscv(&Default::default());
        let context = inkwell::context::Context::create();
        let size = 512;
        let module = crate::immutable_data::module(&context, size);

        let immutable_data_pointer = module.get_global(GLOBAL_IMMUTABLE_DATA_POINTER).unwrap();
        assert_eq!(
            immutable_data_pointer.get_initializer().unwrap(),
            data_type(&context, size / 32).get_undef()
        );

        let immutable_data_size = module.get_global(GLOBAL_IMMUTABLE_DATA_SIZE).unwrap();
        assert_eq!(
            immutable_data_size.get_initializer().unwrap(),
            size_type(&context).const_int(size as u64, false)
        );
    }
}
