use jamjar_examples::gen::data::*;

fn main() {
    loop {
        eprintln!("Static data:");
        eprintln!("{:#?}", &&**NUMBERS);
        eprintln!("{:#?}", &&**NUMERI);
        eprintln!("config = {:#?}", &&**CONFIG);

        eprintln!("You can edit assets/numbers.yaml or assets/numeri.toml and it'll live reload.");

        let wait = 5;
        eprintln!("Waiting {} seconds...", wait);
        std::thread::sleep(std::time::Duration::from_millis(wait * 1000));

        unsafe { jamjar_examples::gen::data::reload_all(); }
    }
}
