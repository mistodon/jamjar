type SrcModuleType<'a> = (&'a str, &'a str, &'a str);

pub fn create_data_structs<'a, I: IntoIterator<Item = &'a SrcModuleType<'a>>>(
    file_mod_types: I,
) -> Result<(), edres::Error> {
    for &(filename, module, name) in file_mod_types {
        edres::create_struct(
            filename,
            module,
            &edres::StructOptions {
                generate_const: false,
                generate_load_fns: false,
                default_int_size: edres::IntSize::I32,
                default_float_size: edres::FloatSize::F32,
                max_array_size: 4,
                ..edres::StructOptions::serde_default()
            },
        )?;
    }

    Ok(())
}

pub fn create_files_enums<'a, I: IntoIterator<Item = &'a SrcModuleType<'a>>>(
    dir_mod_types: I,
) -> Result<(), edres::Error> {
    for &(dir, module, name) in dir_mod_types {
        edres::files_enum::create_files_enum(
            dir,
            module,
            &edres::EnumOptions {
                enum_name: name.to_owned(),
                first_variant_is_default: false,
                ..edres::EnumOptions::serde_default()
            },
        )?;
    }

    Ok(())
}

pub fn create_data_enums<'a, I: IntoIterator<Item = &'a SrcModuleType<'a>>>(
    file_mod_types: I,
) -> Result<(), edres::Error> {
    for &(filename, module, name) in file_mod_types {
        edres::create_enum(
            filename,
            module,
            &edres::EnumOptions {
                enum_name: name.to_owned(),
                ..edres::EnumOptions::serde_default()
            },
        )?;
    }

    Ok(())
}

// TODO: Maybe one day this could be a cool kid proc macro that finds
// the modules automatically.
#[macro_export]
macro_rules! generated_modules {
    ($($modname:ident),* $(,)?) => {
        $(mod $modname;)*
        $(pub use $modname::*;)*
    }
}
