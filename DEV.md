## Now:
- [ ] Atlas stuff
    - [ ] Reposition glyphs at draw-time
    - [ ] Layout text in font module
    - [ ] Layout text with optional word-wrapping

## Later:
Libs:
- [ ] Un-suppress warnings, fix them, clippy, etc.
- [ ] Include the original path for a Resource as a &'static str to allow safety checks when building resource maps
- [ ] Scope resource_list! properly so it can find the resource! macro
- [ ] Ignore unused imports in generated edres files
- [ ] Proper CI
- [ ] Can't have different features based on target, so find a better way to do the force-static etc.

CLI:
- [ ] Package README etc with app
- [ ] Include git hash, version number, etc in metadata somewhere?
- [ ] Include OS in filename
- [ ] Include optional runtime assets?
- [ ] CLI: Pipe stdout/stderr when calling external commands

## Long-term:
- [ ] Reimplement jambrush as another rendering API
- [ ] Port jambrush games to use jamjar

## Notes

### Canvas/resize/scaling modes

Most drawing APIs will have a notion of a canvas, an abstract surface to draw on. Call it "world space" if you want - these are the coordinates you have to think about when calling drawing methods on the API. The resize mode determines if/how the canvas changes size.

Canvas modes:
1. Scissored: things are drawn straight to the window, and scissored to fit the imaginary canvas
2. Intermediate: things are drawn to an actual size intermediate canvas then blitted to the window

Resize modes:
1. Set: it doesn't change unless you tell it to. Resizing the window will cause the canvas to just sit in the middle of the screen, surrounded by the clear color (it will be scissored so the borders will have nothing drawn in them).
2. Free: it changes to match the window size (accounting for DPI). There are no borders, no scissoring, you can always draw in the full window.
3. Aspect: it changes to fill the window size while retaining a set aspect ratio.

Scaling modes:
1. Set: you choose the scale factor (the API can tell you the largest that won't overflow the window)
2. Max: always the biggest scale that will fit the canvas in the window
3. MaxInteger: the biggest integer scale that will fit the canvas in the window

Given a window size, DPI, and CanvasConfig, jamjar should be able to tell you:
- For Direct mode:
    - A viewport and scissor rect, so drawing as thought the canvas is the entire window will squash it into the right place
- For Intermediate mode:
    - The desired dimensions of the intermediate canvas image
    - The size and position of the rect to blit

### Text rendering API

1.  If you want to do it all yourself:
    - Make your own font atlas
    - Write your own code to turn strings into a series of sprites
    - Render sprites as normal
2.  If you want to use ONLY the font module:
    - You can load and layout glyphs with it
    - Then you have to map those glyphs to sprites yourself
3.  If you want to use font and FontAtlas:
    - It's annoying - you have to layout text
    - Then compile the atlas
    - Then turn the glyphs into sprites
    - Then draw sprites
4.  If drawgroove had a built-in atlas:
    - Draw Glyphs "directly" and it'll be laid out for you in drop()
4b. If drawgroove had a built-in atlas but you didn't want to use it:
    - Idk dude. Multiple constructors requiring different features:
        - DrawContext(atlas_image)
        - DrawContext(image_atlas)
        - DrawContext(font_image_atlas)
    - Abstractify font rendering (feature gated ofc)
    - Abstractify additional sprite loading? (Panic if without atlas)
