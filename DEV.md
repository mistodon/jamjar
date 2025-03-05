# Popup:

# Jamjar

## Refresh - I forgot what I was doing

- [x] cherry_lighting runs on Metal just fine
- [x] can we get cherry_lighting working on web?
- [ ] can we get cherry_lighting working on Android?

Must:

For update -> web -> android:
- [x] Validate a set of examples that work
- [x] And work on web
- [x] Then update wgpu and winit and make sure they still work
- [ ] Look into web-time and fix up all the timing stuff
- [ ] And still work on web
- [ ] Then look into Android examples

For cherry:
- [ ] Example of different scaling modes
- [ ] Changing canvas config between camera passes
    - [ ] Allow changing it permanently (on the context)
    - [ ] But also just changing it for the current frame (on the renderer)

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

# Thoughts about dynamic meshes:

Use cases:

1.  I generate geometry on the fly
2.  I post-process a mesh at runtime (outlines, etc.)
3.  I have procedurally, iteratively, generated meshes (like a minecraft map)

The first kind can easily be immediate-mode.
The second kind has to be retained but never modified.
The third kind needs to _live_ somewhere.

For case 1 (and maybe text) I could store/bind a second vertex buffer for immediate-mode stuff.

For case 2 and 3, it makes sense to use the same mesh atlas - but the key becomes an annoying thing. Plus I'd need a method to modify an existing mesh - so the atlas would have to deal with removals. Oh no, this puts us back in atlas-hell...

Two real problems left to solve:

1.  What's the key API for making builtin/filesystem/runtime meshes work?
2.  What are the update APIs for an atlas?
    - Remove? Replace (remove-then-add)? Update-in-place?

## Tasks

- [ ] Think about how sub-meshes should work
    - I think it's fine to have it like python regex groups
        - Drawing just The Mesh will draw all submeshes
        - But there can be a draw_submesh that lets you pick an index
        - They might be named too idk
        - So Mesh needs to store index ranges for each submesh
    - Variant meshes is harder. Where do we store the output of stitch_mesh()?
        - This feels like it should "live" in draw::cherry because it's not really a fundamental property of meshes OR mesh atlases.
        - Maybe draw_cherry even has a separate index buffer for stitched meshes?
            - So we don't have to do it manually - there's draw, draw_submesh, and draw_stitched_mesh (which takes an optional submesh index and populates the index buffer if it hasn't done that already)
        - But wait! Stitching might add vertices. Also our MeshAtlas doesn't account for submeshes, and cannot right now. Which means I need to re-work the MeshAtlas too...
            - I think we need new types:
                - MeshVertices - the actual vertices
                - SubMeshes - a list of index ranges, implicitly tied to a MeshVertices
            - Then we need to be able to load MeshVertices, and store arbitrary SubMeshes instances for that in our atlas.
            - MeshAtlas might even need new methods for insert_vertices(...) -> offset, and insert_submeshes(offset, ...)
            - No, better:
                - insert_vertices(key: TVertexKey, ...)
                - insert_submeshes(vertices: TVertexKey, submeshes: TSubmeshesKey, ...)
            - This lets us use Mesh as a key for insert_vertices
            - ... and (Mesh, Variant) as a key for insert_submeshes
- [ ] stitch_mesh() function that removes sharp edges
- [ ] shadow_volume() function that creates a shadow volume
