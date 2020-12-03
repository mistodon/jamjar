use jamjar_examples::gen::Image;

use jamjar::{resource_list, resource};

fn main() {
    let heart = include_bytes!("../../assets/images/heart.png");
    let star = include_bytes!("../../assets/images/star.png");
    let target = include_bytes!("../../assets/images/target.png");

    let images = jamjar::resources::map_resources(Image::ALL, &resource_list!("assets/images"));

    assert_eq!(&*images[&Image::Heart], heart);
    assert_eq!(&*images[&Image::Star], star);
    assert_eq!(&*images[&Image::Target], target);

    eprintln!("Resources loaded ok!");
}
