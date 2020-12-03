# Development

## Next
Lib:
- [x] Re-export winit through windowing
- [x] Build-time codegen (wrapping edres)
- [x] Easily importing that codegen stuff
- [x] Resource loading (wrapping resource)
- [x] Hot-reloading assets (wrapping dirty_static and helper macros)
- [x] Re-export resource, dirty_static, dymod, lazy_static
- [ ] Really simple drawing API
- [x] Audio module on separate thread
    - [ ] Or optionally same thread, if that doesn't work on wasm
- [ ] Test dymod with a billion configurations and then publish
- [ ] Include the original path for a Resource as a &'static str to allow safety checks when building resource maps
- [ ] Scope resource_list! properly so it can find the resource! macro

## Later
CLI:
- [ ] Package README etc with app
- [ ] Include git hash, version number, etc in metadata somewhere?
- [ ] Include OS in filename
- [ ] Include optional runtime assets?
