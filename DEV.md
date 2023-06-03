# Popup:

Must:

Should:
- [ ] Only upload changed portions of textures

Could:
- [ ] Fancier immediate-mode text rendering
- [ ] Add scale_trans method to MatN


# Thoughts about atlases:

The purpose of an atlas is to abstract away the process of uploading things to the GPU. So the flow is:

1. I add or remove things from my atlas. I don't care _how_ it happens, but I want these things to be available on the GPU.
2. My atlas arranges my things on the CPU side to optimize uploads to the GPU.
3. At some point before rendering, these uploads take place.
4. When rendering, I supply an identifier that points to my thing and the GPU knows where to find it to render it.

1. Modifying APIs: `insert, remove, retain, etc.`
2. Compile stage: `compile_into(cpu_side_container) -> modified range`
3. Upload stage: defined by consumer - use the modified range and the cpu_side_container to upload to GPU
4. Fetch APIs: `fetch(key) -> offset / region / etc.`
