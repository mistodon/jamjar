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

# cherry

* means default

ShaderFlags
- YFlipped
- NoDepthWrite
- BlendAdd
- BackFaceOnly
- StencilAdd
- StencilSub

PushFlags
- *Transform
- ModelMatrix
- *AtlasUV

# shadow

1. If front face passes, add 1 to stencil
2. If back face passes, sub 1 from stencil

# Changing globals example

- Shader that glows if it's facing the view vector
- Draw one thing statically in the middle (it should always glow)
- _THEN_ draw a circle of things around the camera as the camera spins (so the one in front is always glowing)

Implementation:
1.  Each time a draw call occurs immediately after a camera change
    - Upload the old values to global bind buffer 0
    - Allocate a new global bind buffer if needed
    - Log the previous range:
        - global bind buffer = 0
        - opaque range = 0..10
        - trans range = 0..5
    - sort range 0..10 and range 0..5 of respective queues
2.  Then at draw time:
    - For each global buffer modified:
        - bind it
        - draw all the draw calls for those ranges

Note that this interleaves opaque and trans passes for each camera change. We might be able to change this by making a new render pass for successive draws? But I think it mostly won't matter.

Actually yeah, it seems easy to start a new pass and clear only depth/stencil on successive draws!

Note: This is all setup for local bind buffers
