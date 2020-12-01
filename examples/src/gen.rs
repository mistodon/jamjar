use std::collections::HashMap;

jamjar::generated_modules! {
    config,
    images,
    numbers,
    numeri,
}

jamjar::static_data_mod! {
    pub mod data {
        static NUMBERS: HashMap<Number, usize> = load_numbers("assets/numbers.yaml");

        // NOTE: The `toml` crate doesn't allow enums as keys like above.
        static NUMERI: HashMap<String, usize> = carica_numeri("assets/numeri.toml");

        static CONFIG: Config = load_config("assets/config.toml");
    }
}
