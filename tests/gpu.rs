use vibeshop::{document::*, gpu::Engine};
fn engine() -> Engine {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .expect("GPU tests require a working adapter; CI installs Mesa Vulkan. Do not skip this test.");
    eprintln!("GPU adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    Engine::new(device, queue)
}
fn layer(rgba: [u8; 4], w: u32, h: u32) -> Layer {
    Layer::new(
        "fixture",
        Source::new(w, h, rgba.repeat((w * h) as usize)).unwrap(),
    )
}
fn render(e: &mut Engine, d: &Document) -> Vec<u8> {
    e.render(d).unwrap();
    e.readback().unwrap().finish().unwrap()
}
fn close(actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!(a.abs_diff(*b) <= 2, "{actual:?} != {expected:?}");
    }
}
#[test]
fn gpu_pixels_export_and_resource_reuse() {
    let mut e = engine();
    let mut d = Document::new(layer([60, 120, 210, 128], 13, 7));
    let bytes = render(&mut e, &d);
    for p in bytes.as_chunks::<4>().0 {
        close(p, &[60, 120, 210, 128]);
    }
    assert_eq!(e.uploads, 1);
    d.layers[0].exposure = 1.0;
    let brighter = render(&mut e, &d);
    assert!(brighter[0] > bytes[0]);
    assert_eq!(brighter[3], 128);
    assert_eq!(e.uploads, 1);
    d.layers[0].visible = false;
    assert!(render(&mut e, &d).iter().all(|p| *p == 0));
    d = Document::new(layer([255, 0, 0, 255], 1, 1));
    let mut top = layer([0, 0, 255, 255], 1, 1);
    top.opacity = 0.5;
    d.layers.push(top);
    close(&render(&mut e, &d), &[188, 0, 188, 255]);
    d.layers[1].blend = Blend::Multiply;
    close(&render(&mut e, &d), &[188, 0, 0, 255]);
    d.layers[1].blend = Blend::Screen;
    close(&render(&mut e, &d), &[255, 0, 188, 255]);
    d.layers[1].offset = [1, 0];
    close(&render(&mut e, &d), &[255, 0, 0, 255]);
    d.layers.clear();
    close(&render(&mut e, &d), &[0, 0, 0, 0]);
}

#[test]
fn failed_render_cannot_export_previous_pixels() {
    let mut e = engine();
    let mut d = Document::new(layer([80, 90, 100, 255], 1, 1));
    close(&render(&mut e, &d), &[80, 90, 100, 255]);
    d.width = 0;
    assert!(e.render(&d).is_err());
    assert!(e.readback().is_err());
    d.width = 1;
    close(&render(&mut e, &d), &[80, 90, 100, 255]);
}

#[test]
fn tone_controls_produce_expected_pixels() {
    let mut e = engine();
    let mut d = Document::new(layer([255, 255, 255, 255], 1, 1));
    d.layers[0].exposure = -1.0;
    close(&render(&mut e, &d), &[188, 188, 188, 255]);
    d.layers[0].contrast = 0.0;
    close(&render(&mut e, &d), &[118, 118, 118, 255]);
    d = Document::new(layer([255, 0, 0, 128], 1, 1));
    d.layers[0].saturation = 0.0;
    close(&render(&mut e, &d), &[127, 127, 127, 128]);
    d.layers[0].reset_adjustments();
    close(&render(&mut e, &d), &[255, 0, 0, 128]);
}

#[test]
fn tiles_preserve_seams_and_only_recompute_changed_layer_bounds() {
    let mut e = engine();
    let mut d = Document::new(layer([60, 120, 210, 128], 1027, 515));
    let mut patch = layer([200, 90, 50, 192], 17, 13);
    patch.offset = [505, 505];
    d.layers.push(patch);
    let before = render(&mut e, &d);
    let initial_tiles = e.tiles_rendered;
    assert_eq!(initial_tiles, 6);
    assert_eq!(render(&mut e, &d), before);
    assert_eq!(e.tiles_rendered, initial_tiles);
    d.layers[1].name = "Renaming does not change pixels".into();
    assert_eq!(render(&mut e, &d), before);
    assert_eq!(e.tiles_rendered, initial_tiles);
    d.layers[1].exposure = 0.5;
    let edited = render(&mut e, &d);
    assert_eq!(e.tiles_rendered - initial_tiles, 4);
    assert_eq!(e.uploads, 2);
    // Compare the boundary-crossing area to an equivalent small composition.
    let mut reference = Document::new(layer([60, 120, 210, 128], 25, 20));
    reference.layers.push(d.layers[1].clone());
    reference.layers[1].offset = [5, 5];
    let small = render(&mut engine(), &reference);
    for y in 0..15 {
        for x in 0..25 {
            let at = ((y + 500) * 1027 + x + 500) * 4;
            let expected = (y * 25 + x) * 4;
            close(&edited[at..at + 4], &small[expected..expected + 4]);
        }
    }
    let tiles = e.tiles_rendered;
    d.layers[1].offset = [-8, -4];
    let moved = render(&mut e, &d);
    assert_eq!(e.tiles_rendered - tiles, 4);
    assert_ne!(moved, edited);
    // A fresh render must agree with cached tiles after move, hide and deletion.
    assert_eq!(moved, render(&mut engine(), &d));
    d.layers[1].visible = false;
    assert_eq!(render(&mut e, &d), render(&mut engine(), &d));
    d.layers.pop();
    assert_eq!(render(&mut e, &d), render(&mut engine(), &d));
}

#[test]
fn tile_cache_tracks_blend_order_resize_and_failed_render() {
    let mut e = engine();
    let mut d = Document::new(layer([80, 100, 200, 192], 513, 513));
    d.layers.push(layer([200, 100, 40, 128], 513, 513));
    for blend in [Blend::Normal, Blend::Multiply, Blend::Screen] {
        d.layers[1].blend = blend;
        let result = render(&mut e, &d);
        let mut small = Document::new(layer([80, 100, 200, 192], 1, 1));
        small.layers.push(layer([200, 100, 40, 128], 1, 1));
        small.layers[1].blend = blend;
        let expected = render(&mut engine(), &small);
        for pixel in result.as_chunks::<4>().0 {
            close(pixel, &expected);
        }
    }
    d.layers.swap(0, 1);
    assert_eq!(render(&mut e, &d), render(&mut engine(), &d));
    d.width = 0;
    assert!(e.render(&d).is_err());
    assert!(e.readback().is_err());
    d.width = 513;
    assert_eq!(render(&mut e, &d), render(&mut engine(), &d));
    d.width = 17;
    d.height = 11;
    assert_eq!(render(&mut e, &d), render(&mut engine(), &d));
}

#[test]
fn nonuniform_source_coordinates_survive_tile_boundaries_and_replacement() {
    let (width, height) = (1027, 515);
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[
                (x % 251) as u8,
                (y % 241) as u8,
                ((x / 251 * 53 + y / 241 * 17) % 256) as u8,
                if x % 7 == 0 { 0 } else { 192 },
            ]);
        }
    }
    let mut d = Document::new(Layer::new(
        "Generated spatial fixture",
        Source::new(width, height, pixels.clone()).unwrap(),
    ));
    let mut e = engine();
    let result = render(&mut e, &d);
    for (actual, expected) in result
        .as_chunks::<4>()
        .0
        .iter()
        .zip(pixels.as_chunks::<4>().0)
    {
        close(actual, if expected[3] == 0 { &[0; 4] } else { expected });
    }
    for p in pixels.as_chunks_mut::<4>().0 {
        p[0] = 255 - p[0];
    }
    d.layers[0].source = Source::new(width, height, pixels.clone()).unwrap();
    let replacement = render(&mut e, &d);
    assert_eq!(e.uploads, 2);
    assert_eq!(e.tiles_rendered, 12);
    for (actual, expected) in replacement
        .as_chunks::<4>()
        .0
        .iter()
        .zip(pixels.as_chunks::<4>().0)
    {
        close(actual, if expected[3] == 0 { &[0; 4] } else { expected });
    }
}

#[test]
fn levels_curves_channel_order_and_alpha_match_independent_math() {
    use vibeshop::curves::Levels;
    let mut e = engine();
    let mut d = Document::new(layer([137, 188, 225, 128], 1, 1));
    d.layers[0].levels = Levels {
        black: 0.25,
        gamma: 1.0,
        white: 0.75,
    };
    close(&render(&mut e, &d), &[0, 188, 255, 128]);
    d.layers[0].levels = Levels {
        black: 0.0,
        gamma: 2.0,
        white: 1.0,
    };
    let linear = |v: u8| {
        let x = f32::from(v) / 255.0;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    let encode = |v: f32| {
        ((if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }) * 255.0)
            .round() as u8
    };
    close(
        &render(&mut e, &d),
        &[
            encode(linear(137).sqrt()),
            encode(linear(188).sqrt()),
            encode(linear(225).sqrt()),
            128,
        ],
    );
    d.layers[0].levels = Levels::default();
    d.layers[0].curves[0].set(16, 0.25).unwrap();
    d.layers[0].curves[1].set(16, 0.75).unwrap();
    let master = |x: f32| {
        if x <= 0.5 {
            x * 0.5
        } else {
            0.25 + (x - 0.5) * 1.5
        }
    };
    let red = |x: f32| {
        if x <= 0.5 {
            x * 1.5
        } else {
            0.75 + (x - 0.5) * 0.5
        }
    };
    close(
        &render(&mut e, &d),
        &[
            encode(red(master(linear(137)))),
            encode(master(linear(188))),
            encode(master(linear(225))),
            128,
        ],
    );
    let adjusted = render(&mut e, &d);
    e.set_compare(true);
    e.render(&d).unwrap();
    assert!(
        e.readback().is_err(),
        "comparison must never export as the edited document"
    );
    e.set_compare(false);
    assert_eq!(render(&mut e, &d), adjusted);
}

#[test]
fn curve_cache_and_tile_histograms_follow_edits_empty_tiles_and_transparency() {
    let mut e = engine();
    let mut d = Document::new(layer([188, 188, 188, 255], 513, 1));
    let mut right = layer([188, 188, 188, 128], 1, 1);
    right.offset = [512, 0];
    d.layers.push(right);
    d.layers[0].curves[0].set(16, 0.25).unwrap();
    d.layers[1].curves[0].set(16, 0.75).unwrap();
    let first = render(&mut e, &d);
    assert!(
        first[512 * 4] > first[0],
        "distinct layer LUTs must stay distinct"
    );
    let before = e.tiles_rendered;
    d.layers[1].curves[0].set(16, 0.5).unwrap();
    let second = render(&mut e, &d);
    assert_eq!(e.tiles_rendered - before, 1);
    assert_eq!(&first[..512 * 4], &second[..512 * 4]);
    assert_ne!(&first[512 * 4..], &second[512 * 4..]);
    let rows = e.histogram().unwrap().finish().unwrap();
    for row in rows {
        assert_eq!(row.iter().sum::<u32>(), 513);
    }
    d.layers[0].visible = false;
    render(&mut e, &d);
    let rows = e.histogram().unwrap().finish().unwrap();
    for row in rows {
        assert_eq!(row.iter().sum::<u32>(), 1);
    }
    d.layers[1].visible = false;
    render(&mut e, &d);
    assert_eq!(e.histogram().unwrap().finish().unwrap(), [[0; 256]; 4]);
    d = Document::new(layer([255, 0, 0, 255], 1, 1));
    render(&mut e, &d);
    let rows = e.histogram().unwrap().finish().unwrap();
    assert_eq!(rows[0][54], 1);
    assert_eq!(rows[1][255], 1);
    assert_eq!(rows[2][0], 1);
    assert_eq!(rows[3][0], 1);
    d = Document::new(layer([255, 255, 255, 0], 1, 1));
    render(&mut e, &d);
    assert_eq!(e.histogram().unwrap().finish().unwrap(), [[0; 256]; 4]);
}
