fn main() {
    jamjar::codegen::create_data_structs(&[
        ("assets/config.toml", "src/gen/config.rs", "Config"),
    ]).unwrap();

    jamjar::codegen::create_files_enums(&[
        ("assets/images", "src/gen/images.rs", "Image"),
        ("assets/audio", "src/gen/audio.rs", "Audio"),
    ]).unwrap();

    jamjar::codegen::create_data_enums(&[
        ("assets/numbers.yaml", "src/gen/numbers.rs", "Number"),
        ("assets/numeri.toml", "src/gen/numeri.rs", "Numero"),
    ]).unwrap();
}
