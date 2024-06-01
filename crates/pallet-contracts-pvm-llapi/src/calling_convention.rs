use inkwell::{
    builder::Builder,
    context::Context,
    types::{BasicType, StructType},
    values::{BasicValue, PointerValue},
};

pub struct Spill<'a, 'ctx> {
    pointer: PointerValue<'ctx>,
    builder: &'a Builder<'ctx>,
    r#type: StructType<'ctx>,
    current_field: u32,
}

impl<'a, 'ctx> Spill<'a, 'ctx> {
    pub fn new(
        builder: &'a Builder<'ctx>,
        r#type: StructType<'ctx>,
        name: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            pointer: builder.build_alloca(r#type, name)?,
            builder,
            r#type,
            current_field: 0,
        })
    }

    pub fn next<V: BasicValue<'ctx>>(mut self, value: V) -> anyhow::Result<Self> {
        let field_pointer = self.builder.build_struct_gep(
            self.r#type,
            self.pointer,
            self.current_field,
            &format!("spill_parameter_{}", self.current_field),
        )?;
        self.builder.build_store(field_pointer, value)?;
        self.current_field += 1;
        Ok(self)
    }

    pub fn skip(mut self) -> Self {
        self.current_field += 1;
        self
    }

    pub fn done(self) -> PointerValue<'ctx> {
        assert!(
            self.r#type
                .get_field_type_at_index(self.current_field)
                .is_none(),
            "there must not be any missing parameters"
        );

        self.pointer
    }
}

pub fn instantiate(context: &Context) -> StructType {
    context.struct_type(
        &[
            // code_hash_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // ref_time_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // proof_size_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // deposit_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // value_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_len: u32,
            context.i32_type().as_basic_type_enum(),
            // address_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // address_len_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // output_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // output_len_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // salt_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // salt_len: u32
            context.i32_type().as_basic_type_enum(),
        ],
        true,
    )
}

pub fn call(context: &Context) -> StructType {
    context.struct_type(
        &[
            // flags: u32,
            context.i32_type().as_basic_type_enum(),
            // address_ptr:
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // ref_time_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // proof_size_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // deposit_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // value_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_len: u32,
            context.i32_type().as_basic_type_enum(),
            // output_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // output_len_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
        ],
        true,
    )
}
