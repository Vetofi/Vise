
#[cfg(target_os = "linux")]
pub mod wayland;

#[cfg(not(target_os = "macos"))]
pub fn open_vulkan_device(
    adapter: &wgpu::Adapter,
    desc: &wgpu::DeviceDescriptor<'_>,
) -> Option<Result<(wgpu::Device, wgpu::Queue), wgpu::RequestDeviceError>> {
    use wgpu::hal::api::Vulkan;
    let open = {
        let hal = unsafe { adapter.as_hal::<Vulkan>() }?;
        let instance = hal.shared_instance().raw_instance();
        let offered =
            unsafe { instance.enumerate_device_extension_properties(hal.raw_physical_device()) }
                .ok()?
                .iter()
                .any(|e| e.extension_name_as_c_str() == Ok(ash::ext::hdr_metadata::NAME));
        if !offered {
            return None;
        }
        unsafe {
            hal.open_with_callback(
                desc.required_features,
                &desc.required_limits,
                &desc.memory_hints,
                Some(Box::new(
                    |args: wgpu::hal::vulkan::CreateDeviceCallbackArgs| {
                        args.extensions.push(ash::ext::hdr_metadata::NAME);
                    },
                )),
            )
        }
        .ok()?
    };
    Some(unsafe { adapter.create_device_from_hal(open, desc) })
}

pub fn set_metadata(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'_>,
    paper_white: f32,
    peak: f32,
    black: f32,
) {
    #[cfg(not(target_os = "macos"))]
    {
        use wgpu::hal::api::Vulkan;
        let (Some(hal_device), Some(hal_surface)) =
            (unsafe { device.as_hal::<Vulkan>() }, unsafe {
                surface.as_hal::<Vulkan>()
            })
        else {
            return;
        };
        if !hal_device
            .enabled_device_extensions()
            .contains(&ash::ext::hdr_metadata::NAME)
        {
            return;
        }
        let Some(swapchain) = hal_surface.raw_native_swapchain() else {
            return;
        };
        let xy = |x, y| ash::vk::XYColorEXT { x, y };
        let metadata = ash::vk::HdrMetadataEXT::default()
            .display_primary_red(xy(0.64, 0.33))
            .display_primary_green(xy(0.30, 0.60))
            .display_primary_blue(xy(0.15, 0.06))
            .white_point(xy(0.3127, 0.3290))
            .max_luminance(peak)
            .min_luminance(black)
            .max_content_light_level(peak)
            .max_frame_average_light_level(paper_white.min(peak));
        let loader = ash::ext::hdr_metadata::Device::new(
            hal_device.shared_instance().raw_instance(),
            hal_device.raw_device(),
        );
        unsafe { loader.set_hdr_metadata(&[swapchain], &[metadata]) };
    }
    #[cfg(target_os = "macos")]
    let _ = (device, surface, paper_white, peak, black);
}
