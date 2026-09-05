use vibeshop::{document::*, gpu::Engine};
fn engine() -> Engine {
    let instance=wgpu::Instance::default();
    let adapter=pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("GPU tests require a working adapter; CI installs Mesa Vulkan. Do not skip this test.");
    eprintln!("GPU adapter: {:?}",adapter.get_info());
    let (device,queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    Engine::new(device,queue)
}
fn layer(rgba: [u8;4],w:u32,h:u32) -> Layer { Layer::new("fixture",Source::new(w,h,rgba.repeat((w*h) as usize)).unwrap()) }
fn render(e:&mut Engine,d:&Document)->Vec<u8> {e.render(d).unwrap();e.readback().unwrap().finish().unwrap()}
fn close(actual:&[u8],expected:&[u8]) {assert_eq!(actual.len(),expected.len());for (a,b) in actual.iter().zip(expected){assert!(a.abs_diff(*b)<=2,"{actual:?} != {expected:?}");}}
#[test] fn gpu_pixels_export_and_resource_reuse() {
    let mut e=engine();
    let mut d=Document::new(layer([60,120,210,128],13,7));
    let bytes=render(&mut e,&d); for p in bytes.chunks_exact(4) {close(p,&[60,120,210,128]);}
    assert_eq!(e.uploads,1);
    d.layers[0].exposure=1.0; let brighter=render(&mut e,&d); assert!(brighter[0]>bytes[0]); assert_eq!(brighter[3],128); assert_eq!(e.uploads,1);
    d.layers[0].visible=false; assert!(render(&mut e,&d).iter().all(|p|*p==0));
    d=Document::new(layer([255,0,0,255],1,1)); let mut top=layer([0,0,255,255],1,1); top.opacity=0.5; d.layers.push(top);
    close(&render(&mut e,&d),&[188,0,188,255]);
    d.layers[1].blend=Blend::Multiply; close(&render(&mut e,&d),&[188,0,0,255]);
    d.layers[1].blend=Blend::Screen; close(&render(&mut e,&d),&[255,0,188,255]);
    d.layers[1].offset=[1,0]; close(&render(&mut e,&d),&[255,0,0,255]);
    d.layers.clear(); close(&render(&mut e,&d),&[0,0,0,0]);
}
