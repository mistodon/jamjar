use jamjar::jprintln;
use jamjar_examples::gen::data::*;

fn main() {
    loop {
        jprintln!("Static data:");
        jprintln!("{:#?}", &&**NUMBERS);
        jprintln!("{:#?}", &&**NUMERI);
        jprintln!("config = {:#?}", &&**CONFIG);

        jprintln!("You can edit assets/numbers.yaml or assets/numeri.toml and it'll live reload.");

        let wait = 5;
        jprintln!("Waiting {wait} seconds...");
        std::thread::sleep(std::time::Duration::from_millis(wait * 1000));

        unsafe {
            jamjar_examples::gen::data::reload_all();
        }
    }
}
