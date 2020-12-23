fn main() {
    eprintln!("Images are:");
    for img in jamjar_examples::gen::Image::ALL {
        eprintln!("- {}", img);
    }

    eprintln!("Numbers are:");
    for num in jamjar_examples::gen::Number::ALL {
        eprintln!("- {}", num);
    }

    eprintln!("Numeri sono:");
    for num in jamjar_examples::gen::Numero::ALL {
        eprintln!("- {}", num);
    }

    eprintln!("Config is:");
    let file = include_str!("../../assets/config.toml");
    let config: jamjar_examples::gen::Config = toml::from_str(file).unwrap();
    eprintln!("{:?}", config);
}
