use crate::{Error, Result};

/// The wgpu instance and device that everything renders with. Clones share the same device.
#[derive(Clone)]
pub(crate) struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// Picks an adapter (able to present to `compatible_surface`, if given) and opens a device.
    ///
    /// Honors wgpu's environment variables: `WGPU_ADAPTER_NAME` and `WGPU_POWER_PREF` here, and
    /// `WGPU_BACKEND` if `instance` was created with `InstanceDescriptor::with_env`.
    pub async fn new(
        instance: wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self> {
        let adapter =
            wgpu::util::initialize_adapter_from_env_or_default(&instance, compatible_surface)
                .await
                .map_err(|err| Error::RequestAdapter(err.into()))?;
        log::info!("using GPU adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|err| Error::RequestDevice(err.into()))?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }
}

/// Why `what`, a texture `width` by `height` texels, can't be made on `device`, if it can't: wgpu
/// panics on a texture that's empty or larger than the device takes, rather than failing.
pub(crate) fn texture_size_problem(
    device: &wgpu::Device,
    what: &str,
    [width, height]: [u32; 2],
) -> Option<String> {
    let limit = device.limits().max_texture_dimension_2d;
    let fits = |v: u32| (1..=limit).contains(&v);
    (!fits(width) || !fits(height))
        .then(|| format!("{what} is from 1x1 to {limit}x{limit} pixels, not {width}x{height}"))
}
