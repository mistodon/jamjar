use std::collections::HashMap;

use resource::Resource;

pub fn map_resources<'a, T, I>(keys: &[T], resources: I) -> HashMap<T, Resource<[u8]>>
where
    T: std::hash::Hash + std::cmp::Eq + Copy,
    I: IntoIterator<Item = &'a (&'static str, Resource<[u8]>)>,
{
    keys.iter()
        .zip(resources)
        .map(|(&key, (_filename, res))| (key, res.clone()))
        .collect()
}

pub fn map_str_resources<'a, T, I>(keys: &[T], resources: I) -> HashMap<T, Resource<str>>
where
    T: std::hash::Hash + std::cmp::Eq + Copy,
    I: IntoIterator<Item = &'a (&'static str, Resource<str>)>,
{
    keys.iter()
        .zip(resources)
        .map(|(&key, (_filename, res))| (key, res.clone()))
        .collect()
}

#[cfg(feature = "audio")]
pub fn map_audio_resources<'a, T, I>(
    keys: &[T],
    resources: I,
) -> HashMap<T, crate::audio::AudioBytes>
where
    T: std::hash::Hash + std::cmp::Eq + Copy,
    I: IntoIterator<Item = &'a (&'static str, Resource<[u8]>)>,
{
    keys.iter()
        .zip(resources)
        .map(|(&key, (_filename, res))| (key, crate::audio::AudioBytes::new(res.clone().into())))
        .collect()
}
